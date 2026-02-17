# ori-pop

Hyper-minimal creative-coding workspace in Rust.

## Current Setup

- `crates/oripop-core`: the framework layer.
  - deterministic dot-field generation APIs.
  - minimal window bootstrapping via `run_window(width, height, title)`.
- `sketches`: sketchbook crate.
  - sketches depend only on `oripop-core`.
  - each sketch is its own app (`fn main`) as a raw `.rs` file.

## Workspace Layout

- `crates/oripop-core`
- `sketches`
- `presets`

## Hello Sketch

File:
- `sketches/hello-ori-pop.rs`

Code:

```rust
use oripop_core::run_window;

fn main() {
    run_window(900, 700, "hello-ori-pop");
}
```

Run:

```bash
cargo run -p sketches --bin hello-ori-pop
```

## Add A New Sketch

1. Create `sketches/my-sketch.rs` with its own `fn main()`.
2. Add a `[[bin]]` entry in `sketches/Cargo.toml`.
3. Run with:

```bash
cargo run -p sketches --bin my-sketch
```
