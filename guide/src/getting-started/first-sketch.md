# Your First Sketch

## Sketch structure

Every sketch is a standalone Rust binary with two parts:

1. **`main()`** — configure the window (like Processing's `setup()`).
2. **`draw()`** — called every frame.

```rust
use oripop_canvas::prelude::*;

fn main() {
    size(800, 600);
    title("my sketch");
    smooth(4);
    run(draw);
}

fn draw() {
    background(20, 20, 30);
    stroke(255, 200, 100);
    stroke_weight(3.0);
    line(100.0, 100.0, 700.0, 500.0);
}
```

## Adding a new sketch

1. Create `sketches/my-sketch.rs` with `fn main()`.
2. Add a `[[bin]]` entry in `sketches/Cargo.toml`:

```toml
[[bin]]
name = "my-sketch"
path = "my-sketch.rs"
```

3. Run it:

```bash
cargo run -p sketches --bin my-sketch
```

## Coordinate system

- Origin is **top-left** (0, 0).
- **x** grows to the right.
- **y** grows downward.
- All values are in **logical pixels** (DPI-independent).
