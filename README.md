# qw 📼

A high-performance media scrambler that transforms video and audio into a secure, deterministic visual puzzle.

`qw` uses symmetric spatial grid shuffling and bit-level audio scrambling. Because the operation is self-inverting, you use the same command to both scramble and restore your media.

## Installation

Requires **FFmpeg**.

```bash
git clone https://github.com/skorotkiewicz/qw.git
cd qw
cargo build --release
```

## Usage

### 🧩 Flip
The same command toggles between original and scrambled states.
```bash
./target/release/qw -i video.mp4 -o output.mp4 --seed "your-secret" --block-size 16
```

## Features

- **Symmetric Flip**: Running the command twice with the same seed perfectly restores the file.
- **Spatial Shuffling**: Frames are divided into a grid and swapped using a deterministic involution.
- **Audio Scrambling**: Audio is XOR-masked in parallel for total security.
- **Playable**: Standard MP4 output. Play it, scrub it, stream it.

---
*Built with Rust and FFmpeg.*
