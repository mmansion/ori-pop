use oripop_core::prelude::*;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::sync::atomic::{AtomicU64, Ordering};

const W: f32 = 1200.0;
const H: f32 = 800.0;

static SEED: AtomicU64 = AtomicU64::new(0);

fn main() {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    SEED.store(t, Ordering::Relaxed);
    size(W as u32, H as u32);
    title("primitives-demo");
    smooth(4);
    run(draw);
}

fn rand_color(rng: &mut SmallRng) -> (u8, u8, u8) {
    (rng.gen_range(40..=255), rng.gen_range(40..=255), rng.gen_range(40..=255))
}

fn draw() {
    background(12, 12, 18);

    let mut rng = SmallRng::seed_from_u64(SEED.load(Ordering::Relaxed));

    for _ in 0..10 {
        let (r, g, b) = rand_color(&mut rng);
        stroke(r, g, b);
        stroke_weight(rng.gen_range(1.0..5.0));
        no_fill();
        line(
            rng.gen_range(-50.0..W + 50.0), rng.gen_range(-50.0..H + 50.0),
            rng.gen_range(-50.0..W + 50.0), rng.gen_range(-50.0..H + 50.0),
        );
    }

    for _ in 0..6 {
        let (r, g, b) = rand_color(&mut rng);
        fill(r, g, b);
        let (sr, sg, sb) = rand_color(&mut rng);
        stroke(sr, sg, sb);
        stroke_weight(rng.gen_range(1.0..2.5));
        let w = rng.gen_range(60.0..300.0);
        let h = rng.gen_range(60.0..250.0);
        rect(
            rng.gen_range(-w * 0.5..W - w * 0.5),
            rng.gen_range(-h * 0.5..H - h * 0.5),
            w, h,
        );
    }

    for _ in 0..6 {
        let (r, g, b) = rand_color(&mut rng);
        fill(r, g, b);
        let (sr, sg, sb) = rand_color(&mut rng);
        stroke(sr, sg, sb);
        stroke_weight(rng.gen_range(1.0..2.0));
        let w = rng.gen_range(60.0..350.0);
        let h = rng.gen_range(60.0..350.0);
        ellipse(
            rng.gen_range(-w * 0.5..W - w * 0.5),
            rng.gen_range(-h * 0.5..H - h * 0.5),
            w, h,
        );
    }

    for _ in 0..6 {
        let (r, g, b) = rand_color(&mut rng);
        fill(r, g, b);
        let (sr, sg, sb) = rand_color(&mut rng);
        stroke(sr, sg, sb);
        stroke_weight(rng.gen_range(1.0..2.5));
        let cx = rng.gen_range(0.0..W);
        let cy = rng.gen_range(0.0..H);
        let spread = rng.gen_range(80.0..250.0);
        triangle(
            cx + rng.gen_range(-spread..spread), cy + rng.gen_range(-spread..spread),
            cx + rng.gen_range(-spread..spread), cy + rng.gen_range(-spread..spread),
            cx + rng.gen_range(-spread..spread), cy + rng.gen_range(-spread..spread),
        );
    }

    for _ in 0..30 {
        let (r, g, b) = rand_color(&mut rng);
        stroke(r, g, b);
        stroke_weight(rng.gen_range(2.0..10.0));
        point(rng.gen_range(0.0..W), rng.gen_range(0.0..H));
    }
}
