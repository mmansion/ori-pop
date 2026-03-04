pub mod bezier;
pub mod draw;
pub mod line;
pub mod point;
pub mod prelude;

pub use bezier::Bezier;
pub use line::Line;
pub use point::Point;

use rand::{rngs::SmallRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Params {
    pub seed: u64,
    pub canvas: Canvas,
    pub field: Field,
    pub distribution: Distribution,
    pub render: Render,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            seed: 1,
            canvas: Canvas::default(),
            field: Field::default(),
            distribution: Distribution::default(),
            render: Render::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Canvas {
    pub width: f32,
    pub height: f32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub singularity: Singularity,
    pub warp_amount: f32,
    pub warp_frequency: f32,
    #[serde(default)]
    pub forces: Vec<Force>,
}

impl Default for Field {
    fn default() -> Self {
        Self {
            singularity: Singularity::default(),
            warp_amount: 0.05,
            warp_frequency: 6.0,
            forces: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Singularity {
    pub cx: f32,
    pub cy: f32,
    pub falloff: f32,
    pub strength: f32,
}

impl Default for Singularity {
    fn default() -> Self {
        Self {
            cx: 0.5,
            cy: 0.5,
            falloff: 14.0,
            strength: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Distribution {
    pub dot_count: u32,
    pub density_pow: f32,
    pub jitter: f32,
    pub min_radius: f32,
    pub max_radius: f32,
    pub fixed_radius: Option<f32>,
}

impl Default for Distribution {
    fn default() -> Self {
        Self {
            dot_count: 35_000,
            density_pow: 1.4,
            jitter: 0.002,
            min_radius: 0.0011,
            max_radius: 0.0028,
            fixed_radius: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Render {
    pub invert: bool,
    pub threshold: Option<f32>,
}

impl Default for Render {
    fn default() -> Self {
        Self {
            invert: false,
            threshold: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum Force {
    Attractor {
        center: Point,
        strength: f32,
        falloff: f32,
    },
    Gradient {
        along: Line,
        strength: f32,
    },
    Compression {
        axis: Line,
        width: f32,
        strength: f32,
    },
}

pub fn eval_force(force: &Force, x: f32, y: f32) -> f32 {
    let p = Point::new(x, y);
    match force {
        Force::Attractor {
            center,
            strength,
            falloff,
        } => {
            let d2 = p.dist_sq(center);
            strength / (1.0 + falloff * d2)
        }
        Force::Gradient { along, strength } => {
            let dx = along.b.x - along.a.x;
            let dy = along.b.y - along.a.y;
            let len_sq = dx * dx + dy * dy;
            if len_sq < 1e-10 {
                return 0.0;
            }
            let t = ((p.x - along.a.x) * dx + (p.y - along.a.y) * dy) / len_sq;
            t.clamp(0.0, 1.0) * strength
        }
        Force::Compression {
            axis,
            width,
            strength,
        } => {
            let dist = axis.distance(&p);
            let t = (dist / width.max(0.0001)).min(1.0);
            strength * (1.0 - smoothstep(t))
        }
    }
}

pub fn field_at(forces: &[Force], x: f32, y: f32) -> f32 {
    let mut sum = 0.0_f32;
    for f in forces {
        sum += eval_force(f, x, y);
    }
    sum.clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub w: f32,
}

pub fn generate_dots(params: &Params, t: f32) -> Vec<Dot> {
    let frame = (t * 60.0).round() as i64;
    let mut rng = SmallRng::seed_from_u64(mix_seed(params.seed, frame));
    let mut dots = Vec::with_capacity(params.distribution.dot_count as usize);

    while dots.len() < params.distribution.dot_count as usize {
        let mut nx = rng.gen::<f32>();
        let mut ny = rng.gen::<f32>();

        if params.distribution.jitter > 0.0 {
            let jx = rng.gen_range(-params.distribution.jitter..=params.distribution.jitter);
            let jy = rng.gen_range(-params.distribution.jitter..=params.distribution.jitter);
            nx = (nx + jx).clamp(0.0, 1.0);
            ny = (ny + jy).clamp(0.0, 1.0);
        }

        let weight = density_at(params, nx, ny, frame).clamp(0.0, 1.0);
        let accept = weight
            .max(0.001)
            .powf(params.distribution.density_pow.max(0.01));
        if rng.gen::<f32>() > accept {
            continue;
        }

        let radius = if let Some(fixed) = params.distribution.fixed_radius {
            fixed
        } else {
            rng.gen_range(params.distribution.min_radius..=params.distribution.max_radius)
        }
        .max(0.000_001);

        dots.push(Dot {
            x: nx * params.canvas.width,
            y: ny * params.canvas.height,
            r: radius,
            w: weight,
        });
    }

    dots
}

pub fn density_at(params: &Params, nx: f32, ny: f32, frame: i64) -> f32 {
    let dx = nx - params.field.singularity.cx;
    let dy = ny - params.field.singularity.cy;
    let d2 = dx * dx + dy * dy;

    let singularity = (params.field.singularity.strength
        / (1.0 + params.field.singularity.falloff * d2))
        .clamp(0.0, 1.0);

    let warp = if params.field.warp_amount > 0.0 {
        let n = value_noise(
            nx * params.field.warp_frequency + frame as f32 * 0.013,
            ny * params.field.warp_frequency - frame as f32 * 0.011,
            mix_seed(params.seed ^ 0x9E37_79B9_7F4A_7C15, frame),
        );
        (n - 0.5) * 2.0 * params.field.warp_amount
    } else {
        0.0
    };

    let mut forces_sum = 0.0_f32;
    for f in &params.field.forces {
        forces_sum += eval_force(f, nx, ny);
    }

    (singularity + warp + forces_sum).clamp(0.0, 1.0)
}

fn value_noise(x: f32, y: f32, seed: u64) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let tx = x.fract();
    let ty = y.fract();

    let v00 = hash01(xi, yi, seed);
    let v10 = hash01(xi + 1, yi, seed);
    let v01 = hash01(xi, yi + 1, seed);
    let v11 = hash01(xi + 1, yi + 1, seed);

    let sx = smoothstep(tx);
    let sy = smoothstep(ty);
    let a = lerp(v00, v10, sx);
    let b = lerp(v01, v11, sx);
    lerp(a, b, sy)
}

fn hash01(x: i32, y: i32, seed: u64) -> f32 {
    let mut z = seed ^ ((x as u64) << 32) ^ (y as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z >> 8) as u32) as f32 / (u32::MAX as f32)
}

fn mix_seed(seed: u64, frame: i64) -> u64 {
    let mut z = seed ^ (frame as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    z
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_same_params_same_time() {
        let params = Params::default();
        let a = generate_dots(&params, 1.25);
        let b = generate_dots(&params, 1.25);
        assert_eq!(a, b);
    }

    #[test]
    fn dots_within_canvas_bounds() {
        let params = Params::default();
        let dots = generate_dots(&params, 0.0);
        for d in dots {
            assert!(d.x >= 0.0 && d.x <= params.canvas.width);
            assert!(d.y >= 0.0 && d.y <= params.canvas.height);
            assert!(d.r > 0.0);
        }
    }

    #[test]
    fn attractor_peaks_at_center() {
        let f = Force::Attractor {
            center: Point::new(0.5, 0.5),
            strength: 1.0,
            falloff: 10.0,
        };
        let center = eval_force(&f, 0.5, 0.5);
        let edge = eval_force(&f, 0.0, 0.0);
        assert!((center - 1.0).abs() < 1e-6);
        assert!(edge < center);
    }

    #[test]
    fn gradient_increases_along_direction() {
        let f = Force::Gradient {
            along: Line::new(Point::new(0.0, 0.5), Point::new(1.0, 0.5)),
            strength: 1.0,
        };
        let lo = eval_force(&f, 0.1, 0.5);
        let hi = eval_force(&f, 0.9, 0.5);
        assert!(hi > lo);
    }

    #[test]
    fn compression_peaks_on_center_line() {
        let f = Force::Compression {
            axis: Line::new(Point::new(0.0, 0.5), Point::new(1.0, 0.5)),
            width: 0.1,
            strength: 1.0,
        };
        let on_line = eval_force(&f, 0.5, 0.5);
        let off_line = eval_force(&f, 0.5, 0.0);
        assert!((on_line - 1.0).abs() < 1e-6);
        assert!(off_line < on_line);
    }

    #[test]
    fn field_at_clamps_to_unit() {
        let forces = vec![
            Force::Attractor { center: Point::new(0.5, 0.5), strength: 0.8, falloff: 1.0 },
            Force::Attractor { center: Point::new(0.5, 0.5), strength: 0.8, falloff: 1.0 },
        ];
        let val = field_at(&forces, 0.5, 0.5);
        assert!((val - 1.0).abs() < 1e-6);
    }

    #[test]
    fn forces_affect_density() {
        let mut params = Params::default();
        params.field.singularity.strength = 0.0;
        params.field.warp_amount = 0.0;
        let empty_density = density_at(&params, 0.5, 0.5, 0);

        params.field.forces = vec![Force::Attractor {
            center: Point::new(0.5, 0.5),
            strength: 1.0,
            falloff: 10.0,
        }];
        let with_force = density_at(&params, 0.5, 0.5, 0);
        assert!(with_force > empty_density);
    }

    #[test]
    fn produces_exact_requested_count() {
        let mut params = Params::default();
        params.distribution.dot_count = 12_345;
        let dots = generate_dots(&params, 0.5);
        assert_eq!(dots.len(), params.distribution.dot_count as usize);
    }
}
