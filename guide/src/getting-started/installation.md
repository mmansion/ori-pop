# Installation

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain, 1.80+)
- A GPU that supports Vulkan, Metal, or DX12

## Clone and build

```bash
git clone https://github.com/mmansion/ori-pop.git
cd ori-pop
cargo build
```

That's it. All dependencies are managed through `Cargo.toml`.

## Run a sketch

```bash
cargo run -p sketches --bin hello-ori-pop
```

## Generate API docs

```bash
cargo doc --no-deps -p oripop-canvas --open
```
