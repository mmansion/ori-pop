//! Breathing noise blobs with holes.
//!
//! Proves: begin_shape / curve_vertex / contours, noise-driven organic form,
//! HSB color sweeps, layered alpha over a soft fade.

use oripop_canvas::prelude::*;

const W: f32 = 1000.0;
const H: f32 = 750.0;

fn main() {
    size(W as u32, H as u32);
    title("15-noise-blobs — curve_vertex shapes with contours");
    smooth(4);
    run(draw);
}

/// One closed Catmull-Rom ring whose radius is modulated by noise.
fn blob_ring(cx: f32, cy: f32, base_r: f32, wobble: f32, seed: f32, t: f32) {
    const STEPS: usize = 24;
    // Catmull-Rom needs lead-in/lead-out control points: wrap three extra.
    for i in 0..STEPS + 3 {
        let k = (i % STEPS) as f32 / STEPS as f32;
        let a = k * std::f32::consts::TAU;
        let r = base_r
            * (1.0 + wobble * (noise3(a.cos() * 0.7 + seed, a.sin() * 0.7, t) - 0.5) * 2.0);
        curve_vertex(cx + a.cos() * r, cy + a.sin() * r);
    }
}

fn draw() {
    if frame_count() == 1 {
        background(14, 12, 20);
        noise_seed(21);
    }
    // Gentle fade so the blobs leave breathing ghosts.
    no_stroke();
    fill_a(14, 12, 20, 24);
    rect(0.0, 0.0, W, H);

    let t = millis() as f32 * 0.00025;
    color_mode(ColorMode::Hsb);

    for i in 0..5 {
        let fi = i as f32;
        let seed = fi * 13.7;
        // Each blob drifts on its own slow noise orbit.
        let cx = map(noise2(seed, t * 0.6), 0.0, 1.0, W * 0.18, W * 0.82);
        let cy = map(noise2(seed + 40.0, t * 0.6), 0.0, 1.0, H * 0.2, H * 0.8);
        let r = 60.0 + fi * 22.0;

        let hue = ((t * 30.0 + fi * 36.0) % 255.0) as u8;
        fill_a(hue, 150, 230, 120);
        stroke_a(hue, 190, 255, 200);
        stroke_weight(1.6);

        begin_shape();
        blob_ring(cx, cy, r, 0.36, seed, t);
        // Hole: a smaller counter-wobbling ring inside.
        begin_contour();
        blob_ring(cx, cy, r * 0.45, 0.5, seed + 99.0, t + 5.0);
        end_contour();
        end_shape_close();
    }

    color_mode(ColorMode::Rgb);
}
