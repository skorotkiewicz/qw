# qw - Symmetric Media Scrambler 📼

A high-performance media scrambler that transforms video and audio into a secure, deterministic visual puzzle.

`qw` uses symmetric spatial grid shuffling and audio block scrambling. Because the operation is self-inverting (based on mathematical involutions), you use the same command both to scramble and to restore your media.

## 📼 Demo

| Scrambled | Restored |
| :---: | :---: |
| ![Scrambled](assets/scrambled.mp4) | ![Restored](assets/restored.mp4) |

## Installation

Requires **FFmpeg** and **FFprobe** installed on your system.

```bash
git clone https://github.com/skorotkiewicz/qw.git
cd qw
# Use cargo:
cargo build --release
# Or use just:
just build
```

## Usage

### Flip
The same command toggles between original and scrambled states.
```bash
# Scramble a video
./target/release/qw -i video.mp4 -o scrambled.mp4 --seed "your-secret"

# Restore it (using the same command and seed)
./target/release/qw -i scrambled.mp4 -o restored.mp4 --seed "your-secret"
```

### Options
- `--seed`: The secret key for shuffling.
- `--block-size`: Size of the video grid blocks (default: 16).
- `--audio-block-ms`: Audio shuffle grain (default: 100). Use `10000` for 10s chunks.
- `--max-frames`: Limit processing to the first N frames (useful for testing).

## Features

- **Universal Input**: Supports H.264, H.265, MJPEG, VP9, and any other format handled by FFmpeg.
- **Symmetric Flip**: Running the command twice with the same seed perfectly restores the file (Bit-perfect order).
- **Spatial Shuffling**: Video frames are divided into a grid and swapped using a deterministic involution.
- **Robust Audio Shuffling**: Audio is divided into temporal blocks and shuffled, ensuring the scrambling survives modern compression (AAC).
- **High Compatibility**: Outputs standard MP4 files (H.264/AAC). Playable in VLC, mpv, or any modern player.

---
*Built with Rust and FFmpeg.*
