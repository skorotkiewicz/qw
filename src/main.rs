use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

#[derive(Parser)]
#[command(name = "qw")]
#[command(about = "Spatial and Temporal Media Shuffler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Encrypt {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long)]
        seed: String,
        #[arg(long, default_value = "16")]
        block_size: usize,
    },
    Decrypt {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long)]
        seed: String,
        #[arg(long, default_value = "16")]
        block_size: usize,
    },
}

fn get_rng(seed_str: &str) -> ChaCha8Rng {
    let mut hasher = Sha256::new();
    hasher.update(seed_str.as_bytes());
    let result = hasher.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&result);
    ChaCha8Rng::from_seed(seed)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Encrypt {
            input,
            output,
            seed,
            block_size,
        } => {
            process_media(&input, &output, &seed, block_size, true)?;
        }
        Commands::Decrypt {
            input,
            output,
            seed,
            block_size,
        } => {
            process_media(&input, &output, &seed, block_size, false)?;
        }
    }
    Ok(())
}

fn get_media_info(path: &PathBuf) -> Result<(usize, usize, u64, u32, u32)> {
    let v_output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,nb_frames", "-of", "csv=s=x:p=0"])
        .arg(path).output().context("ffprobe video failed")?;
    let v_str = String::from_utf8(v_output.stdout)?;
    let vp = v_str.trim().split('x').collect::<Vec<_>>();
    if vp.len() < 2 { anyhow::bail!("Invalid video"); }
    let w = vp[0].parse()?;
    let h = vp[1].parse()?;
    let frames = vp.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    
    let a_output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=sample_rate,channels", "-of", "csv=s=x:p=0"])
        .arg(path).output().ok();
    let (rate, chans) = if let Some(ao) = a_output {
        let as_str = String::from_utf8(ao.stdout).unwrap_or_default();
        let ap = as_str.trim().split('x').collect::<Vec<_>>();
        (ap.get(0).and_then(|&s| s.parse().ok()).unwrap_or(48000), ap.get(1).and_then(|&s| s.parse().ok()).unwrap_or(2))
    } else { (48000, 2) };
    Ok((w, h, frames, rate, chans))
}

fn process_media(
    input_path: &PathBuf,
    output_path: &PathBuf,
    seed_str: &str,
    block_size: usize,
    encrypt: bool,
) -> Result<()> {
    let (width, height, total_frames, sample_rate, channels) = get_media_info(input_path)?;
    println!("Media: {}x{} @ {}Hz", width, height, sample_rate);

    let mut hasher = Sha256::new();
    hasher.update(seed_str.as_bytes());
    let hash = hex::encode(hasher.finalize());
    let fifo_path = format!("/tmp/qw_audio_{}.raw", &hash[..8]);
    let _ = std::fs::remove_file(&fifo_path);
    let _ = Command::new("mkfifo").arg(&fifo_path).status();

    // Spawn decoders
    let mut v_dec = Command::new("ffmpeg").args(["-i", input_path.to_str().unwrap(), "-f", "rawvideo", "-pix_fmt", "rgb24", "-loglevel", "quiet", "-"]).stdout(Stdio::piped()).spawn()?;
    let mut a_dec = Command::new("ffmpeg").args(["-i", input_path.to_str().unwrap(), "-f", "s16le", "-acodec", "pcm_s16le", "-loglevel", "quiet", "-"]).stdout(Stdio::piped()).spawn()?;

    // Encoder: Open FIFO FIRST, then stdin
    let mut enc = Command::new("ffmpeg")
        .args([
            "-y",
            "-f", "s16le", "-ar", &sample_rate.to_string(), "-ac", &channels.to_string(), "-i", &fifo_path,
            "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{}x{}", width, height), "-i", "-",
            "-map", "1:v:0", "-map", "0:a:0?",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "22",
            "-c:a", "aac", "-b:a", "128k",
            output_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped()).stderr(Stdio::null()).spawn()?;

    let v_out = v_dec.stdout.take().unwrap();
    let a_out = a_dec.stdout.take().unwrap();
    let enc_in = enc.stdin.take().unwrap();

    let mut rng_a = get_rng(seed_str);
    let fp = fifo_path.clone();
    let a_handle = thread::spawn(move || -> Result<()> {
        let mut reader = BufReader::new(a_out);
        let mut writer = BufWriter::new(File::create(fp).context("FIFO open failed")?);
        let mut buf = vec![0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 { break; }
            let mut xor = vec![0u8; n];
            rng_a.fill(&mut xor);
            for i in 0..n { buf[i] ^= xor[i]; }
            writer.write_all(&buf[..n])?;
        }
        Ok(())
    });

    let mut v_reader = BufReader::with_capacity(10*1024*1024, v_out);
    let mut v_writer = BufWriter::with_capacity(10*1024*1024, enc_in);

    let cols = width / block_size;
    let rows = height / block_size;
    let grid: Vec<(usize, usize)> = (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))).collect();
    let mut rng_v = get_rng(seed_str);
    let mut shuff = grid.clone();
    shuff.shuffle(&mut rng_v);
    let map = if encrypt { shuff } else {
        let mut inv = vec![(0, 0); grid.len()];
        for (i, &s) in shuff.iter().enumerate() { inv[s.0 * cols + s.1] = (i / cols, i % cols); }
        inv
    };

    let frame_sz = width * height * 3;
    let mut b_in = vec![0u8; frame_sz];
    let mut b_out = vec![0u8; frame_sz];
    let pb = if total_frames > 0 { ProgressBar::new(total_frames) } else { ProgressBar::new_spinner() };
    pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} [{elapsed_precise}] {pos}/{len} Grid: {msg}")?);
    pb.set_message(format!("{}px", block_size));

    while v_reader.read_exact(&mut b_in).is_ok() {
        for (i, &(sr, sc)) in map.iter().enumerate() {
            let (dr, dc) = (i / cols, i % cols);
            let len = block_size * 3;
            for bh in 0..block_size {
                let sy = sr * block_size + bh;
                let dy = dr * block_size + bh;
                let s_off = (sy * width + sc * block_size) * 3;
                let d_off = (dy * width + dc * block_size) * 3;
                b_out[d_off..d_off + len].copy_from_slice(&b_in[s_off..s_off + len]);
            }
        }
        v_writer.write_all(&b_out)?;
        pb.inc(1);
    }

    pb.finish();
    drop(v_writer);
    let _ = a_handle.join();
    let _ = enc.wait();
    let _ = v_dec.wait();
    let _ = a_dec.wait();
    let _ = std::fs::remove_file(&fifo_path);

    println!("Success.");
    Ok(())
}
