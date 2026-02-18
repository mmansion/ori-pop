# ORI-POP
MANSION’s personal art-making toolset and hyper-minimal creative-coding workspace in Rust. Built around the tension between order and emergence, it draws on ori—fold as structure—and pop—points in space—to explore how discrete particles organize within invisible fields. Beginning with force rather than image, MANSION employs scalar gradients, attractors, and compression zones to generate geometric landscapes through density and flow—compositions that feel both constructed and discovered.

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
