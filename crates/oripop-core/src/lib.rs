use rand::{rngs::SmallRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

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
}

impl Default for Field {
    fn default() -> Self {
        Self {
            singularity: Singularity::default(),
            warp_amount: 0.05,
            warp_frequency: 6.0,
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

    (singularity + warp).clamp(0.0, 1.0)
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

pub fn run_window(width: u32, height: u32, title: &str) {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let _window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64))
        .build(&event_loop)
        .expect("failed to create window");

    event_loop
        .run(|event, target| {
            if let Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } = event
            {
                target.exit();
            }
        })
        .expect("event loop error");
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
    fn produces_exact_requested_count() {
        let mut params = Params::default();
        params.distribution.dot_count = 12_345;
        let dots = generate_dots(&params, 0.5);
        assert_eq!(dots.len(), params.distribution.dot_count as usize);
    }
}

