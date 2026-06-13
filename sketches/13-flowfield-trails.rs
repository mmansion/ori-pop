//! Perlin flowfield with persistent trails.
//!
//! Proves: noise2, seeded random, HSB color, persistent canvas (background
//! is only drawn on the first frame; a translucent fade rect creates trails).

use std::cell::RefCell;

use oripop_runtime::prelude::*;

const W: f32 = 1100.0;
const H: f32 = 700.0;
const N: usize = 900;
const NOISE_SCALE: f32 = 0.0016;

struct Particle {
    x: f32,
    y: f32,
}

thread_local! {
    static PARTICLES: RefCell<Vec<Particle>> = const { RefCell::new(Vec::new()) };
}

fn main() {
    size(W as u32, H as u32);
    title("13-flowfield-trails — Perlin field, persistent canvas");
    smooth(4);
    run(draw);
}

fn respawn() -> Particle {
    Particle { x: random(W), y: random(H) }
}

fn draw() {
    let t = frame_count() as f32 * 0.002;

    if frame_count() == 1 {
        background(8, 8, 1);
        random_seed(11);
        noise_seed(7);
        PARTICLES.with(|p| {
            p.borrow_mut().extend((0..N).map(|_| respawn()));
        });
    }

    // p5-style translucent background: blends a wash instead of clearing,
    // so old strokes sink away gradually. Lower alpha = longer trails.
    background_a(8, 8, 14, 5);

    color_mode(ColorMode::Hsb);
    PARTICLES.with(|p| {
        let mut particles = p.borrow_mut();
        for part in particles.iter_mut() {
            // Field angle from 2D noise, drifting slowly through a third axis.
            let a = noise3(part.x * NOISE_SCALE, part.y * NOISE_SCALE, t)
                * std::f32::consts::TAU
                * 2.0;
            let nx = part.x + a.cos() * 2.2;
            let ny = part.y + a.sin() * 2.2;

            // Hue follows the flow direction; brightness follows speed nothing
            // fancy — direction alone gives the field visible structure.
            let hue = map(a.rem_euclid(std::f32::consts::TAU), 0.0, std::f32::consts::TAU, 110.0, 230.0);
            stroke_a(hue as u8, 170, 255, 36);
            stroke_weight(1.2);
            line(part.x, part.y, nx, ny);

            part.x = nx;
            part.y = ny;
            if part.x < 0.0 || part.x > W || part.y < 0.0 || part.y > H || random(1.0) < 0.002 {
                *part = respawn();
            }
        }
    });
    color_mode(ColorMode::Rgb);
}
