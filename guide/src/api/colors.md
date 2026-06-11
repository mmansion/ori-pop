# Colors & Background

All color channels use the 0–255 range, matching Processing.

## Background — clears and washes

```rust
background(30, 30, 40);        // opaque: hard-clears the canvas
background_a(30, 30, 40, 10);  // translucent: blends a wash (trails!)
background_gray(30);
background_color(c);           // from a Color value
```

The canvas is **persistent**: if you never call `background()`, content
accumulates across frames, exactly like Processing. A translucent
`background_a` is the idiomatic trail fade — see
[Canvas & Snapshots](./canvas.md).

## Stroke & fill

```rust
stroke(255, 100, 50);          stroke_a(255, 100, 50, 128);
stroke_gray(200);              stroke_color(c);
no_stroke();

fill(60, 200, 120);            fill_a(60, 200, 120, 80);
fill_gray(40);                 fill_color(c);
no_fill();

stroke_weight(3.0);            // logical pixels
```

## Color mode (HSB)

```rust
color_mode(ColorMode::Hsb);    // channels become hue, saturation, brightness
fill(128, 200, 255);           // teal-ish hue at full brightness
color_mode(ColorMode::Rgb);    // back to default
```

All channels stay 0–255 in either mode. Gray shorthands are always literal
RGB grays.

## Color values & interpolation

`Color` is a resolved RGBA value (Processing's `color()`):

```rust
let warm = color(255, 140, 60);          // resolved under current color_mode
let cool = color_a(80, 150, 255, 200);   // with alpha
let mid  = lerp_color(warm, cool, 0.5);  // per-channel interpolation
fill_color(mid);
```

Multi-stop palettes are just chained lerps:

```rust
fn palette(k: f32) -> Color {
    let (a, b, c) = (color(28, 44, 90), color(70, 170, 160), color(250, 240, 210));
    if k < 0.5 { lerp_color(a, b, k * 2.0) } else { lerp_color(b, c, (k - 0.5) * 2.0) }
}
```

## Style stack

Save and restore the full drawing style (colors, weight, modes, caps) —
independent of the transform stack:

```rust
push_style();
// ... temporary style changes ...
pop_style();
```

## Anti-aliasing

```rust
smooth(4); // MSAA samples: 1 (off), 2, 4 (default), 8 — call before run()
```
