# Build Commands

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable, 1.75+)
- `cargo` available in `PATH`

## Build the CLI binary

```bash
# Debug build (fast compile, unoptimised)
cargo build --bin rustpi

# Release build (optimised, slower compile)
cargo build --release --bin rustpi
```

Binaries are written to:
- Debug: `target/debug/rustpi`
- Release: `target/release/rustpi`

## Build the entire workspace

```bash
# Debug
cargo build

# Release
cargo build --release
```

## Build a specific crate

```bash
cargo build -p cli
cargo build -p rpc-api
cargo build -p model-adapters
```

## Run tests

```bash
# All crates
cargo test --workspace

# Single crate
cargo test -p cli
cargo test -p rpc-api

# Single test by name
cargo test -p cli test_run_with_prompt
```

## Check (no binary output — fastest feedback)

```bash
# Whole workspace
cargo check

# Single crate
cargo check -p rpc-api
```

## Lint

```bash
cargo clippy --workspace
```

## Clean build artefacts

```bash
cargo clean
```

## Run directly without installing

```bash
cargo run --bin rustpi -- run "hello"
cargo run --release --bin rustpi -- run "hello"
```

## Install to `~/.cargo/bin`

```bash
cargo install --path crates/cli
```
