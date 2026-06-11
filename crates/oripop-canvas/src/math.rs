//! Processing-style math helpers: interpolation, range mapping, seeded
//! random numbers, and Perlin noise.
//!
//! Random and noise state are thread-local (like the drawing context), so
//! sketches get deterministic results after `random_seed` / `noise_seed`.

use std::cell::RefCell;

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

// ── Interpolation & ranges ──────────────────────────────

/// Re-map `value` from one range to another (no clamping).
pub fn map(value: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    out_min + (out_max - out_min) * ((value - in_min) / (in_max - in_min))
}

/// Linear interpolation between `a` and `b` by `t`.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Normalize `value` from a range into [0, 1] (no clamping).
pub fn norm(value: f32, min: f32, max: f32) -> f32 {
    (value - min) / (max - min)
}

/// Constrain `value` to [min, max].
pub fn constrain(value: f32, min: f32, max: f32) -> f32 {
    value.clamp(min, max)
}

/// Distance between two points.
pub fn dist(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    ((x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1)).sqrt()
}

/// Magnitude of a 2D vector.
pub fn mag(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Square of a number.
pub fn sq(v: f32) -> f32 {
    v * v
}

/// Degrees → radians.
pub fn radians(degrees: f32) -> f32 {
    degrees.to_radians()
}

/// Radians → degrees.
pub fn degrees(radians: f32) -> f32 {
    radians.to_degrees()
}

// ── Random ──────────────────────────────────────────────

thread_local! {
    static RNG: RefCell<SmallRng> = RefCell::new(SmallRng::from_os_rng());
}

/// Seed the random number generator for reproducible sequences.
pub fn random_seed(seed: u64) {
    RNG.with(|r| *r.borrow_mut() = SmallRng::seed_from_u64(seed));
}

/// Random `f32` in [0, high).
pub fn random(high: f32) -> f32 {
    RNG.with(|r| r.borrow_mut().random_range(0.0..high.max(f32::MIN_POSITIVE)))
}

/// Random `f32` in [low, high).
pub fn random_range(low: f32, high: f32) -> f32 {
    if high <= low {
        return low;
    }
    RNG.with(|r| r.borrow_mut().random_range(low..high))
}

/// Sample from a Gaussian distribution with mean 0 and standard deviation 1
/// (Box-Muller).
pub fn random_gaussian() -> f32 {
    RNG.with(|r| {
        let mut rng = r.borrow_mut();
        let u1: f32 = rng.random_range(f32::MIN_POSITIVE..1.0);
        let u2: f32 = rng.random_range(0.0..1.0);
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    })
}

// ── Perlin noise ────────────────────────────────────────

struct NoiseState {
    perm: [u8; 512],
    octaves: u32,
    falloff: f32,
}

impl NoiseState {
    fn new(seed: u64) -> Self {
        let mut perm = [0u8; 512];
        let mut base: [u8; 256] = std::array::from_fn(|i| i as u8);
        // Fisher-Yates with a dedicated rng so noise_seed is independent of
        // random_seed.
        let mut rng = SmallRng::seed_from_u64(seed);
        for i in (1..256).rev() {
            let j = rng.random_range(0..=i);
            base.swap(i, j);
        }
        for i in 0..512 {
            perm[i] = base[i & 255];
        }
        Self { perm, octaves: 4, falloff: 0.5 }
    }
}

thread_local! {
    static NOISE: RefCell<NoiseState> = RefCell::new(NoiseState::new(0x6F72_6970));
}

/// Seed the noise field for reproducible patterns.
pub fn noise_seed(seed: u64) {
    NOISE.with(|n| {
        let (octaves, falloff) = {
            let s = n.borrow();
            (s.octaves, s.falloff)
        };
        let mut fresh = NoiseState::new(seed);
        fresh.octaves = octaves;
        fresh.falloff = falloff;
        *n.borrow_mut() = fresh;
    });
}

/// Set the number of octaves and per-octave amplitude falloff
/// (Processing's `noiseDetail`). Defaults: 4 octaves, 0.5 falloff.
pub fn noise_detail(octaves: u32, falloff: f32) {
    NOISE.with(|n| {
        let mut s = n.borrow_mut();
        s.octaves = octaves.max(1);
        s.falloff = falloff.clamp(0.0, 1.0);
    });
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn grad(hash: u8, x: f32, y: f32, z: f32) -> f32 {
    // 12 gradient directions, Perlin's reference scheme.
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

fn perlin3(perm: &[u8; 512], x: f32, y: f32, z: f32) -> f32 {
    let xi = (x.floor() as i32 & 255) as usize;
    let yi = (y.floor() as i32 & 255) as usize;
    let zi = (z.floor() as i32 & 255) as usize;
    let xf = x - x.floor();
    let yf = y - y.floor();
    let zf = z - z.floor();
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));

    let a = perm[xi] as usize + yi;
    let aa = perm[a] as usize + zi;
    let ab = perm[a + 1] as usize + zi;
    let b = perm[xi + 1] as usize + yi;
    let ba = perm[b] as usize + zi;
    let bb = perm[b + 1] as usize + zi;

    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;

    l(
        l(
            l(grad(perm[aa], xf, yf, zf), grad(perm[ba], xf - 1.0, yf, zf), u),
            l(grad(perm[ab], xf, yf - 1.0, zf), grad(perm[bb], xf - 1.0, yf - 1.0, zf), u),
            v,
        ),
        l(
            l(grad(perm[aa + 1], xf, yf, zf - 1.0), grad(perm[ba + 1], xf - 1.0, yf, zf - 1.0), u),
            l(
                grad(perm[ab + 1], xf, yf - 1.0, zf - 1.0),
                grad(perm[bb + 1], xf - 1.0, yf - 1.0, zf - 1.0),
                u,
            ),
            v,
        ),
        w,
    )
}

fn noise_impl(x: f32, y: f32, z: f32) -> f32 {
    NOISE.with(|n| {
        let s = n.borrow();
        let mut total = 0.0;
        let mut freq = 1.0;
        let mut amp = 1.0;
        let mut max = 0.0;
        for _ in 0..s.octaves {
            total += (perlin3(&s.perm, x * freq, y * freq, z * freq) * 0.5 + 0.5) * amp;
            max += amp;
            amp *= s.falloff;
            freq *= 2.0;
        }
        total / max
    })
}

/// 1D Perlin noise in [0, 1].
pub fn noise(x: f32) -> f32 {
    noise_impl(x, 0.0, 0.0)
}

/// 2D Perlin noise in [0, 1].
pub fn noise2(x: f32, y: f32) -> f32 {
    noise_impl(x, y, 0.0)
}

/// 3D Perlin noise in [0, 1].
pub fn noise3(x: f32, y: f32, z: f32) -> f32 {
    noise_impl(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_and_friends() {
        assert_eq!(map(5.0, 0.0, 10.0, 0.0, 100.0), 50.0);
        assert_eq!(lerp(0.0, 10.0, 0.25), 2.5);
        assert_eq!(norm(25.0, 0.0, 100.0), 0.25);
        assert_eq!(constrain(11.0, 0.0, 10.0), 10.0);
        assert_eq!(dist(0.0, 0.0, 3.0, 4.0), 5.0);
        assert_eq!(mag(3.0, 4.0), 5.0);
        assert_eq!(sq(4.0), 16.0);
    }

    #[test]
    fn random_is_seedable_and_bounded() {
        random_seed(42);
        let a: Vec<f32> = (0..8).map(|_| random(10.0)).collect();
        random_seed(42);
        let b: Vec<f32> = (0..8).map(|_| random(10.0)).collect();
        assert_eq!(a, b);
        assert!(a.iter().all(|v| (0.0..10.0).contains(v)));
        random_seed(43);
        let c: Vec<f32> = (0..8).map(|_| random(10.0)).collect();
        assert_ne!(a, c);
    }

    #[test]
    fn noise_is_deterministic_smooth_and_bounded() {
        noise_seed(7);
        let a = noise2(1.5, 2.5);
        noise_seed(7);
        let b = noise2(1.5, 2.5);
        assert_eq!(a, b);
        for i in 0..100 {
            let v = noise(i as f32 * 0.13);
            assert!((0.0..=1.0).contains(&v), "noise out of range: {v}");
        }
        // Nearby samples stay close (smoothness sanity check).
        let d = (noise(3.0) - noise(3.001)).abs();
        assert!(d < 0.05, "noise not smooth: delta {d}");
    }
}
