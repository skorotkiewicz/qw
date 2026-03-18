use anyhow::{Context, Result, anyhow};
use clap::Parser;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
#[command(name = "qw", version, about = "Symmetric media shuffler", long_about = None)]
struct Args {
    /// Input media file
    #[arg(short, long)]
    input: PathBuf,

    /// Output media file
    #[arg(short, long)]
    output: PathBuf,

    /// Secret seed for shuffling
    #[arg(short, long)]
    seed: String,

    /// Size of the pixel grid blocks (default 16)
    #[arg(long, default_value_t = 16)]
    block_size: usize,

    /// Audio block size in milliseconds (default 100). Use 10000 for 10sec.
    #[arg(long, default_value_t = 100)]
    audio_block_ms: u64,

    /// Maximum frames to process (for testing)
    #[arg(long)]
    max_frames: Option<usize>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    run(args)
}

fn run(args: Args) -> Result<()> {
    println!("Starting QW with seed: {}", args.seed);
    println!("Input: {:?}", args.input);
    println!("Output: {:?}", args.output);
    println!("Block size (Video): {}", args.block_size);
    println!("Block size (Audio): {}ms", args.audio_block_ms);

    // Initializing FFmpegReader
    let mut reader = FFmpegReader::new(&args.input)?;

    println!("Decoding video frames (via FFmpeg)...");
    let mut frames = reader.read_frames(args.max_frames.unwrap_or(usize::MAX))?;
    println!("Total decoded video frames: {}", frames.len());

    if frames.is_empty() {
        println!("No frames decoded. Check input file format.");
        return Ok(());
    }

    let width = frames[0].width;
    let height = frames[0].height;
    println!("Detected Resolution: {}x{}", width, height);

    println!("Shuffling video frames...");
    let grid = BlockGrid::new(width, height, args.block_size);
    let inv_map = generate_involution_map(grid.total_blocks(), &args.seed);

    frames.par_iter_mut().for_each(|frame| {
        shuffle_pixels(&mut frame.pixels, &grid, &inv_map);
    });
    println!("Video shuffling complete.");

    let mut audio_data = None;
    if reader.audio_meta.is_some() {
        println!("Shuffling audio blocks...");
        let mut audio = reader.read_audio()?;
        println!("Total audio samples: {}", audio.samples.len());

        let scrambler = AudioScrambler::new(
            &args.seed,
            audio.sample_rate,
            audio.channels,
            args.audio_block_ms,
        );
        scrambler.scramble(&mut audio.samples);
        println!("Audio scrambling complete.");
        audio_data = Some(audio);
    }

    println!("Writing output to {:?}...", args.output);
    write_mp4(&args.output, frames, audio_data)?;
    println!("Done!");

    Ok(())
}

pub struct RawFrame {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

pub struct RawAudio {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u8,
}

pub struct VideoMeta {
    pub width: usize,
    pub height: usize,
}

pub struct AudioMeta {
    pub sample_rate: u32,
    pub channels: u8,
}

pub struct FFmpegReader {
    input_path: PathBuf,
    pub video_meta: VideoMeta,
    pub audio_meta: Option<AudioMeta>,
}

impl FFmpegReader {
    pub fn new(path: &Path) -> Result<Self> {
        let path_str = path.to_str().context("invalid path")?;

        // Get Video Metadata
        let v_output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height",
                "-of",
                "csv=s=x:p=0",
                path_str,
            ])
            .output()
            .context("failed to execute ffprobe for video")?;

        let v_str = String::from_utf8_lossy(&v_output.stdout).trim().to_string();
        let dims: Vec<&str> = v_str.split('x').collect();
        if dims.len() < 2 {
            return Err(anyhow!(
                "failed to parse video dimensions or file not found: {}",
                v_str
            ));
        }
        let width = dims[0].parse()?;
        let height = dims[1].parse()?;

        // Get Audio Metadata
        let a_output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=sample_rate,channels",
                "-of",
                "csv=p=0",
                path_str,
            ])
            .output()
            .context("failed to execute ffprobe for audio")?;

        let a_str = String::from_utf8_lossy(&a_output.stdout).trim().to_string();
        let a_meta = if !a_str.is_empty() {
            let parts: Vec<&str> = a_str.split(',').collect();
            if parts.len() >= 2 {
                Some(AudioMeta {
                    sample_rate: parts[0].parse()?,
                    channels: parts[1].parse()?,
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            input_path: path.to_path_buf(),
            video_meta: VideoMeta { width, height },
            audio_meta: a_meta,
        })
    }

    pub fn read_frames(&mut self, max_frames: usize) -> Result<Vec<RawFrame>> {
        let frame_size = self.video_meta.width * self.video_meta.height * 3;
        let mut child = Command::new("ffmpeg")
            .args([
                "-i",
                self.input_path.to_str().unwrap(),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-vcodec",
                "rawvideo",
                "-loglevel",
                "error",
                "-",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .context("failed to spawn ffmpeg for video reading")?;

        let mut stdout = child.stdout.take().unwrap();
        let mut frames = Vec::new();

        use indicatif::ProgressBar;
        let pb = ProgressBar::new_spinner();
        pb.set_message("Decoding frames...");

        loop {
            if frames.len() >= max_frames {
                break;
            }
            let mut buf = vec![0u8; frame_size];
            match stdout.read_exact(&mut buf) {
                Ok(_) => {
                    frames.push(RawFrame {
                        pixels: buf,
                        width: self.video_meta.width,
                        height: self.video_meta.height,
                    });
                    pb.set_message(format!("Decoded {} frames", frames.len()));
                }
                Err(_) => break, // EOF
            }
        }
        let _ = child.kill();
        pb.finish_with_message(format!("Decoded {} frames", frames.len()));
        Ok(frames)
    }

    pub fn read_audio(&mut self) -> Result<RawAudio> {
        let meta = self.audio_meta.as_ref().context("no audio")?;
        let mut child = Command::new("ffmpeg")
            .args([
                "-i",
                self.input_path.to_str().unwrap(),
                "-f",
                "s16le",
                "-ac",
                meta.channels.to_string().as_str(),
                "-ar",
                meta.sample_rate.to_string().as_str(),
                "-loglevel",
                "error",
                "-",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .context("failed to spawn ffmpeg for audio reading")?;

        let mut stdout = child.stdout.take().unwrap();
        let mut samples = Vec::new();
        let mut buf = [0u8; 4096];

        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            for i in 0..(n / 2) {
                let s = i16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]);
                samples.push(s);
            }
        }

        Ok(RawAudio {
            samples,
            sample_rate: meta.sample_rate,
            channels: meta.channels,
        })
    }
}

pub struct AudioScrambler {
    seed: String,
    block_samples: usize,
}

impl AudioScrambler {
    pub fn new(seed: &str, sample_rate: u32, channels: u8, block_ms: u64) -> Self {
        let block_samples =
            (sample_rate as f64 * channels as f64 * (block_ms as f64 / 1000.0)) as usize;
        Self {
            seed: seed.to_string(),
            block_samples,
        }
    }

    pub fn scramble(&self, samples: &mut [i16]) {
        if self.block_samples == 0 || samples.len() < self.block_samples {
            return;
        }
        let total_blocks = samples.len() / self.block_samples;
        if total_blocks < 2 {
            return;
        }

        let map = generate_involution_map(total_blocks, &self.seed);
        let original = samples.to_vec();

        for (block_idx, &target_idx) in map.iter().enumerate().take(total_blocks) {
            if target_idx == block_idx {
                continue;
            }

            let src_start = block_idx * self.block_samples;
            let dst_start = target_idx * self.block_samples;

            samples[dst_start..self.block_samples + dst_start]
                .copy_from_slice(&original[src_start..self.block_samples + src_start]);
        }
    }
}

pub fn get_rng(seed_str: &str) -> ChaCha8Rng {
    let mut hasher = Sha256::new();
    hasher.update(seed_str.as_bytes());
    let result = hasher.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&result);
    ChaCha8Rng::from_seed(seed)
}

pub fn generate_involution_map(count: usize, seed_str: &str) -> Vec<usize> {
    let mut rng = get_rng(seed_str);
    let mut indices: Vec<usize> = (0..count).collect();
    indices.shuffle(&mut rng);

    let mut map = vec![0; count];
    let mut paired = vec![false; count];

    for i in 0..(count / 2) {
        let a = indices[i * 2];
        let b = indices[i * 2 + 1];
        map[a] = b;
        map[b] = a;
        paired[a] = true;
        paired[b] = true;
    }

    for i in 0..count {
        if !paired[i] {
            map[i] = i;
        }
    }

    map
}

pub fn shuffle_pixels(pixels: &mut [u8], grid: &BlockGrid, inv_map: &[usize]) {
    let original = pixels.to_vec();
    let channels = 3; // RGB

    for (block_idx, &target_idx) in inv_map.iter().enumerate() {
        if target_idx == block_idx {
            continue;
        }

        let (src_x, src_y, src_w, src_h) = grid.block_rect(block_idx);
        let (dst_x, dst_y, _dst_w, _dst_h) = grid.block_rect(target_idx);

        for dy in 0..src_h {
            let s_offset = ((src_y + dy) * grid.width + src_x) * channels;
            let d_offset = ((dst_y + dy) * grid.width + dst_x) * channels;
            let row_bytes = src_w * channels;

            pixels[d_offset..row_bytes + d_offset]
                .copy_from_slice(&original[s_offset..row_bytes + s_offset]);
        }
    }
}

fn write_mp4(path: &Path, frames: Vec<RawFrame>, audio: Option<RawAudio>) -> Result<()> {
    use std::fs;
    let tmp_dir = PathBuf::from("/tmp/qw_mux");
    fs::create_dir_all(&tmp_dir).ok();

    let video_pipe = tmp_dir.join("video.mjpeg");
    let mut v_file = File::create(&video_pipe)?;

    use indicatif::ProgressBar;
    let pb = ProgressBar::new(frames.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) video mux",
            )?
            .progress_chars("#>-"),
    );

    for frame in frames {
        let mut jpeg = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut jpeg, 90);
        encoder
            .encode(
                &frame.pixels,
                frame.width as u16,
                frame.height as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .map_err(|e| anyhow!("JPEG encode error: {:?}", e))?;
        v_file.write_all(&jpeg)?;
        pb.inc(1);
    }
    pb.finish();

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y").arg("-i").arg(&video_pipe);

    let mut a_file_path = None;
    if let Some(a) = audio {
        let a_path = tmp_dir.join("audio.raw");
        let mut a_file = File::create(&a_path)?;
        let bytes: Vec<u8> = a
            .samples
            .iter()
            .flat_map(|s| s.to_le_bytes().to_vec())
            .collect();
        a_file.write_all(&bytes)?;

        cmd.arg("-f")
            .arg("s16le")
            .arg("-ar")
            .arg(a.sample_rate.to_string())
            .arg("-ac")
            .arg(a.channels.to_string())
            .arg("-i")
            .arg(&a_path);
        a_file_path = Some(a_path);
    }

    // We mux back to H264 for universal compatibility.
    cmd.arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("ultrafast")
        .arg("-pix_fmt")
        .arg("yuv420p");

    if a_file_path.is_some() {
        cmd.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
    }

    cmd.arg("-shortest").arg(path);

    let status = cmd.status().context("failed to execute ffmpeg muxer")?;
    if !status.success() {
        return Err(anyhow!("ffmpeg muxing failed with status: {}", status));
    }

    // Cleanup
    fs::remove_file(video_pipe).ok();
    if let Some(p) = a_file_path {
        fs::remove_file(p).ok();
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct BlockGrid {
    pub width: usize,
    pub height: usize,
    pub block_size: usize,
    pub cols: usize,
    pub rows: usize,
}

impl BlockGrid {
    pub fn new(width: usize, height: usize, block_size: usize) -> Self {
        let cols = width.div_ceil(block_size);
        let rows = height.div_ceil(block_size);
        Self {
            width,
            height,
            block_size,
            cols,
            rows,
        }
    }

    pub fn total_blocks(&self) -> usize {
        self.cols * self.rows
    }

    pub fn block_coords(&self, block_idx: usize) -> (usize, usize) {
        let row = block_idx / self.cols;
        let col = block_idx % self.cols;
        (col, row)
    }

    pub fn block_rect(&self, block_idx: usize) -> (usize, usize, usize, usize) {
        let (col, row) = self.block_coords(block_idx);
        let x = col * self.block_size;
        let y = row * self.block_size;
        let w = if x + self.block_size > self.width {
            self.width - x
        } else {
            self.block_size
        };
        let h = if y + self.block_size > self.height {
            self.height - y
        } else {
            self.block_size
        };
        (x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_involution_property() {
        let seeds = ["abc", "123", "secret-key"];
        let sizes = [0, 1, 2, 10, 100, 101];

        for seed in seeds {
            for size in sizes.iter().cloned() {
                let map = generate_involution_map(size, seed);
                assert_eq!(map.len(), size);
                for i in 0..size {
                    let j = map[i];
                    assert_eq!(map[j], i);
                }
            }
        }
    }

    #[test]
    fn test_block_grid_edges() {
        let grid = BlockGrid::new(30, 30, 16);
        assert_eq!(grid.total_blocks(), 4);
        assert_eq!(grid.block_rect(3), (16, 16, 14, 14));
    }

    #[test]
    fn test_audio_block_shuffling() {
        let mut samples = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let original = samples.clone();

        let sc = AudioScrambler {
            seed: "test".to_string(),
            block_samples: 2,
        };

        sc.scramble(&mut samples);
        assert_ne!(samples, original);

        sc.scramble(&mut samples);
        assert_eq!(samples, original);
    }
}
