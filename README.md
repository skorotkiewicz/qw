# qw 📼

A high-performance media scrambler that transforms video and audio into a secure, deterministic visual puzzle.

`qw` uses spatial grid shuffling and bit-level audio scrambling to secure your media. The output remains a fully playable MP4 file—visually a glitched puzzle and audibly digital noise—until restored with the correct seed.

## Installation

Requires **FFmpeg**.

```bash
git clone https://github.com/your-username/qw.git
cd qw
cargo build --release
```

## Usage

### 🧩 Encrypt
```bash
./target/release/qw encrypt -i video.mp4 -o scrambled.mp4 --seed "my-secret"
```

### 🔓 Decrypt
```bash
./target/release/qw decrypt -i scrambled.mp4 -o restored.mp4 --seed "my-secret"
```

## Features

- **Spatial Shuffling**: Frames are divided into a 16px grid and shuffled.
- **Audio Scrambling**: Audio is XOR-masked in parallel for total security.
- **Playable**: Standard MP4 output. Play it, scrub it, stream it.
- **Reversible**: Bit-perfect restoration using a seeded Fisher-Yates shuffle.

---
*Built with Rust and FFmpeg.*
