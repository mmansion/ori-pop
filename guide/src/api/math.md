# Math, Random & Noise

The standard creative-coding math kit, in the prelude.

## Ranges & interpolation

```rust
map(v, 0.0, 10.0, 0.0, 100.0)  // re-map between ranges
lerp(a, b, t)                  // linear interpolation
norm(v, min, max)              // normalize into [0, 1]
constrain(v, min, max)         // clamp
dist(x1, y1, x2, y2)           // distance
mag(x, y)                      // vector magnitude
sq(v)                          // v * v
radians(deg)  /  degrees(rad)
```

## Random

Seeded and reproducible per thread:

```rust
random_seed(42);               // same sequence every run
random(10.0)                   // f32 in [0, 10)
random_range(-5.0, 5.0)        // f32 in [low, high)
random_gaussian()              // mean 0, std-dev 1
```

## Perlin noise

Classic octaved Perlin, returning values in [0, 1]:

```rust
noise_seed(7);                 // reproducible field
noise_detail(4, 0.5);          // octaves, falloff (defaults)
noise(x)                       // 1D
noise2(x, y)                   // 2D
noise3(x, y, z)                // 3D — drift z with time for animated fields
```

The flowfield idiom:

```rust
let angle = noise3(x * 0.002, y * 0.002, t) * TAU * 2.0;
let (dx, dy) = (angle.cos(), angle.sin());
```
