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

## Order matters

Transforms apply in the order you call them. This is the same as Processing:

```rust
push();
translate(400.0, 300.0); // move origin to center
rotate(0.5);             // rotate around center
rect(-25.0, -25.0, 50.0, 50.0); // draw at origin
pop();
```

## Reset

The transform resets to identity at the start of each frame, so there is no carry-over between frames.
