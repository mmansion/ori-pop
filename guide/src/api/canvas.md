# Canvas, Persistence & Snapshots

## The persistent canvas

The canvas keeps its pixels between frames, exactly like Processing:

- **Call `background(...)` every frame** → classic clear-and-redraw.
- **Never call it** → content accumulates (generative drawing, growth
  algorithms, paint tools).
- **Call `background_a(..., alpha)` with a low alpha** → a translucent wash
  blends over the canvas each frame: the trail-fade idiom.

```rust
fn draw() {
    if frame_count() == 1 { background(8, 8, 14); }
    background_a(8, 8, 14, 10);   // ~4% fade per frame: long trails
    // ... draw the new frame's marks ...
}
```

The canvas is stored in a float texture, so even very low alphas fade
smoothly to the background instead of leaving quantization ghosts.

## High-resolution output (`pixel_density`)

```rust
pixel_density(2);   // 1-4, call before run()
```

The canvas renders at `density` × the window's physical resolution — the
window shows a downsampled view, and snapshots export at the full canvas
resolution. Because everything is vector-tessellated, this is true
high-resolution rendering, not upscaling.

## Snapshots (`save_frame`)

Freeze the canvas — including all accumulated content — to a PNG:

```rust
save_frame("snapshots/piece-####.png");
```

Runs of `#` become the zero-padded frame number (Processing's `saveFrame`
convention). Parent directories are created automatically. Resolution is
window physical pixels × `pixel_density`.

This is the sketch-side **bake**: the bridge from a live draw loop to a
fixed texture that can be printed, plotted, or mapped onto a flat.

## Offscreen canvases (`create_graphics`)

A `Graphics` is an independent drawing surface (Processing's `PGraphics`),
with the full drawing API as methods:

```rust
let mut g = create_graphics(220, 220);
g.background(30, 24, 48);        // clears g's content
g.stroke(255, 180, 90);
g.ellipse(110.0, 110.0, 120.0, 120.0);

image(&g, 40.0, 40.0);                      // place at native size
image_sized(&g, 0.0, 0.0, 110.0, 110.0);    // place scaled
```

`Graphics::background` *clears* recorded content; skip it to accumulate, or
use `background_a` for washes — the same persistence rules as the main
canvas. Placement quads respect the transform stack, so an offscreen canvas
can be drawn rotated, scaled, and repeated.
