# Symmetric Media Scrambler Justfile
# https://github.com/casey/just

# Build the project in release mode
build:
    cargo build --release

# Run the project with sample arguments
run input output seed="secret" args="":
    cargo run --release -- -i {{input}} -o {{output}} --seed {{seed}} {{args}}

# Run all tests
test:
    cargo test

# Run clippy with strict warnings
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
    cargo fmt

# Clean build artifacts and temporary muxing files
clean:
    cargo clean
    rm -rf /tmp/qw_mux

# Full check (fmt + lint + test)
ready: fmt lint test
