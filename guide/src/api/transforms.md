# Transforms

The 2D transform system works like Processing's `pushMatrix()` / `popMatrix()`, but shortened to `push()` / `pop()`.

## Push & Pop

Save and restore the current transform:

```rust
push();
// ... transforms and drawing here ...
pop();
// back to the previous transform
```

## Translate

Move the origin:

```rust
translate(dx, dy);
```

## Rotate

Rotate around the current origin, in **radians**:

```rust
rotate(angle);
```

Positive angle rotates counter-clockwise. Use `std::f32::consts::TAU` for a full turn, `FRAC_PI_2` for 90 degrees, etc.

## Scale

Scale the coordinate system:

```rust
scale(sx, sy);
```

Stroke weight is **not** scaled by transforms — it stays in canvas pixels,
so outlines keep a consistent width under `scale()`.

## Shear

Skew along an axis by an angle in radians:

```rust
shear_x(angle);
shear_y(angle);
```

## Reset

Replace the current transform with the identity mid-frame:

```rust
reset_matrix();
```

## Order matters

Transforms apply in the order you call them. This is the same as Processing:

```rust
push();
translate(400.0, 300.0); // move origin to center
rotate(0.5);             // rotate around center
rect(-25.0, -25.0, 50.0, 50.0); // draw at origin
pop();
```

## Per-frame reset

The transform resets to identity at the start of each frame, so there is no
carry-over between frames.

## Style stack

`push()`/`pop()` manage the *transform* only (Processing's
`pushMatrix`/`popMatrix`). For colors, weight, and modes there is a separate
style stack:

```rust
push_style();
stroke(255, 0, 0);
stroke_weight(8.0);
// ...
pop_style();   // colors, weight, modes restored
```
