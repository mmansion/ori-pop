# Colors & Background

All color values use the 0–255 range, matching Processing.

## Background

Clear the canvas each frame:

```rust
background(30, 30, 40);        // opaque RGB
background_a(30, 30, 40, 128); // with alpha
```

## Stroke

Set the outline color for shapes and lines:

```rust
stroke(255, 100, 50);          // opaque
stroke_a(255, 100, 50, 128);   // semi-transparent
no_stroke();                   // disable outlines
```

## Fill

Set the interior color for shapes:

```rust
fill(60, 200, 120);            // opaque
fill_a(60, 200, 120, 80);     // semi-transparent
no_fill();                     // disable fill (outlines only)
```

## Stroke weight

Control line and outline thickness:

```rust
stroke_weight(3.0); // 3 logical pixels wide
```

## Anti-aliasing

Configure MSAA sample count before `run()`. Valid values: 1 (off), 2, 4, 8.

```rust
smooth(4); // 4x MSAA (default)
```
