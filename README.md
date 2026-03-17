# qw 📼

A high-performance spatial video scrambler that transforms your media into a secure, deterministic visual puzzle.

## Overview

`qw` is not just another encryption tool. It uses a **Spatial Grid Shuffle** algorithm to reorder the pixels of every video frame into a scrambled grid. 

The result is a "pixelized" or "glitched" video that remains **100% playable** in any standard media player, but is visually unwatchable without the correct seed. Because it processes at the frame level, it maintains perfect audio synchronization and can be perfectly reversed to restore the original file.

## Features

- **Puzzle Scrambling**: Every frame is divided into a grid and shuffled using a seeded PRNG.
- **Playable Encrypt**: The output is a valid MP4 file. You can play it, scrub through it, and see the chaos in real-time.
- **Perfect Audio Sync**: Preserves the original audio stream while the visuals are scrambled.
- **Deterministic & Reversible**: Use the same seed and block size to restore your media exactly as it was.
- **Hybrid Performance**: Combines the safety of Rust with the power of FFmpeg for blazing-fast processing.

## Prerequisites

`qw` requires **FFmpeg** and **FFprobe** to be installed on your system.

```bash
# Ubuntu/Debian
sudo apt install ffmpeg

# macOS
brew install ffmpeg
```

## Installation

```bash
git clone https://github.com/your-username/qw.git
cd qw
cargo build --release
```

## Usage

### 🧩 Encrypt
Scramble a video into a 16px grid puzzle.
```bash
./target/release/qw encrypt -i original.mp4 -o scrambled.mp4 --seed "my-secret-passphrase"
```

### 🔓 Decrypt
Restore the original video from the scrambled file.
```bash
./target/release/qw decrypt -i scrambled.mp4 -o restored.mp4 --seed "my-secret-passphrase"
```

### ⚙️ Options
- `--block-size <PX>`: Set the grid granularity (default: 16). Smaller = finer noise, Larger = bigger puzzle pieces.

## How it works

1. **Decoding**: Decoding the video into raw RGB24 frames using FFmpeg pipes.
2. **Grid Shuffling**: Rust divides each frame into `N x N` pixel blocks and performs a deterministic Fisher-Yates shuffle based on a SHA-256 derived seed.
3. **Encoding**: Shuffled frames are piped back into FFmpeg to be re-encoded with the original audio stream copied bit-for-bit.
4. **Reversibility**: The decryption process applies the exact inverse permutation, returning pixels to their original coordinates.

---
*Built with Rust and FFmpeg.*
