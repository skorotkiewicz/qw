use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use sha2::{Digest, Sha256};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "qw")]
#[command(about = "Spatial Grid Video Shuffler", long_about = None)]
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
            process_video(&input, &output, &seed, block_size, true)?;
        }
        Commands::Decrypt {
            input,
            output,
            seed,
            block_size,
        } => {
            process_video(&input, &output, &seed, block_size, false)?;
        }
    }
    Ok(())
}

fn get_video_info(path: &PathBuf) -> Result<(usize, usize, u64)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,nb_frames",
            "-of",
            "csv=s=x:p=0",
        ])
        .arg(path)
        .output()
        .context("Failed to run ffprobe. Ensure ffmpeg/ffprobe is installed.")?;

    let out_str = String::from_utf8(output.stdout)?;
    let parts: Vec<&str> = out_str.trim().split('x').collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid resolution: {}", out_str);
    }

    let w = parts[0].parse()?;
    let h = parts[1].parse()?;
    let frames = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    Ok((w, h, frames))
}

fn process_video(
    input_path: &PathBuf,
    output_path: &PathBuf,
    seed: &str,
    block_size: usize,
    encrypt: bool,
) -> Result<()> {
    let (width, height, total_frames) = get_video_info(input_path)?;
    println!(
        "Resolution: {}x{}, Total Frames: {}, Block Size: {}px",
        width, height, total_frames, block_size
    );

    // 1. Setup Pipes
    let mut decode_child = Command::new("ffmpeg")
        .args([
            "-i",
            input_path.to_str().unwrap(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-loglevel",
            "quiet",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let mut encode_child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{}x{}", width, height),
            "-i",
            "-",
            "-i",
            input_path.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-map",
            "1:a:0?",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "18",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
            output_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout_raw = decode_child
        .stdout
        .take()
        .context("Failed to open stdout")?;
    let stdin_raw = encode_child.stdin.take().context("Failed to open stdin")?;

    let mut stdout = BufReader::with_capacity(10 * 1024 * 1024, stdout_raw);
    let mut stdin = BufWriter::with_capacity(10 * 1024 * 1024, stdin_raw);

    // 2. Grid Logic
    let cols = width / block_size;
    let rows = height / block_size;
    let block_indices: Vec<(usize, usize)> = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .collect();

    let mut rng = get_rng(seed);
    let mut shuffled = block_indices.clone();
    shuffled.shuffle(&mut rng);

    let mapping = if encrypt {
        shuffled
    } else {
        let mut inv = vec![(0, 0); block_indices.len()];
        for (i, &s) in shuffled.iter().enumerate() {
            let target_idx = s.0 * cols + s.1;
            inv[target_idx] = (i / cols, i % cols);
        }
        inv
    };

    // 3. Loop
    let frame_size = width * height * 3;
    let mut input_frame = vec![0u8; frame_size];
    let mut output_frame = vec![0u8; frame_size];

    let pb = if total_frames > 0 {
        ProgressBar::new(total_frames)
    } else {
        ProgressBar::new_spinner()
    };
    pb.set_style(ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} frames (eta: {eta})",
    )?);

    while stdout.read_exact(&mut input_frame).is_ok() {
        for (i, &(src_r, src_c)) in mapping.iter().enumerate() {
            let dst_r = i / cols;
            let dst_c = i % cols;
            let line_len = block_size * 3;
            for bh in 0..block_size {
                let sy = src_r * block_size + bh;
                let dy = dst_r * block_size + bh;
                let src_off = (sy * width + src_c * block_size) * 3;
                let dst_off = (dy * width + dst_c * block_size) * 3;
                output_frame[dst_off..dst_off + line_len]
                    .copy_from_slice(&input_frame[src_off..src_off + line_len]);
            }
        }
        stdin.write_all(&output_frame)?;
        pb.inc(1);
    }

    pb.finish();
    drop(stdin);
    let _ = encode_child.wait()?;
    let _ = decode_child.wait()?;

    println!("Success. Saved to {}.", output_path.display());
    Ok(())
}
