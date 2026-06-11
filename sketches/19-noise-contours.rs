//! Topographic contour lines from drifting 3D noise (marching squares).
//!
//! Proves: complex geometry extracted from a scalar field, multi-stop
//! palette via lerp_color, smooth field animation. Run with --release for
//! the densest grids.
//!
//! A noise field is sampled on a grid; for each of several iso-levels,
//! marching squares emits the line segments where the field crosses the
//! level — a living elevation map.

use oripop_canvas::prelude::*;

const W: f32 = 1100.0;
const H: f32 = 720.0;
const COLS: usize = 110;
const ROWS: usize = 72;
const LEVELS: usize = 9;
const NOISE_SCALE: f32 = 0.012;

fn main() {
    size(W as u32, H as u32);
    title("19-noise-contours — marching squares over Perlin");
    smooth(4);
    run(draw);
}

/// Three-stop palette: deep water -> shore -> peak.
fn palette(k: f32) -> Color {
    let deep = Color::rgb(28, 44, 90);
    let shore = Color::rgb(70, 170, 160);
    let peak = Color::rgb(250, 240, 210);
    if k < 0.5 {
        lerp_color(deep, shore, k * 2.0)
    } else {
        lerp_color(shore, peak, (k - 0.5) * 2.0)
    }
}

/// Interpolate the crossing point of `iso` between two corner samples.
fn cross(a: f32, b: f32, iso: f32) -> f32 {
    if (b - a).abs() < 1e-6 {
        0.5
    } else {
        ((iso - a) / (b - a)).clamp(0.0, 1.0)
    }
}

fn draw() {
    background(12, 14, 22);
    let t = millis() as f32 * 0.00006;

    // Sample the field once per frame.
    let cw = W / COLS as f32;
    let ch = H / ROWS as f32;
    let mut field = vec![0.0f32; (COLS + 1) * (ROWS + 1)];
    for j in 0..=ROWS {
        for i in 0..=COLS {
            field[j * (COLS + 1) + i] =
                noise3(i as f32 * cw * NOISE_SCALE, j as f32 * ch * NOISE_SCALE, t);
        }
    }
    let sample = |i: usize, j: usize| field[j * (COLS + 1) + i];

    for l in 0..LEVELS {
        let k = (l as f32 + 1.0) / (LEVELS as f32 + 1.0);
        let iso = map(k, 0.0, 1.0, 0.32, 0.68); // noise is centered near 0.5
        stroke_color(palette(k));
        stroke_weight(if l % 3 == 0 { 2.2 } else { 1.0 });

        for j in 0..ROWS {
            for i in 0..COLS {
                let x = i as f32 * cw;
                let y = j as f32 * ch;
                let (tl, tr) = (sample(i, j), sample(i + 1, j));
                let (bl, br) = (sample(i, j + 1), sample(i + 1, j + 1));

                // Marching-squares case from the four corners.
                let case = (usize::from(tl > iso) << 3)
                    | (usize::from(tr > iso) << 2)
                    | (usize::from(br > iso) << 1)
                    | usize::from(bl > iso);
                if case == 0 || case == 15 {
                    continue;
                }

                // Edge crossing points (top, right, bottom, left).
                let top = (x + cw * cross(tl, tr, iso), y);
                let right = (x + cw, y + ch * cross(tr, br, iso));
                let bottom = (x + cw * cross(bl, br, iso), y + ch);
                let left = (x, y + ch * cross(tl, bl, iso));

                let seg = |a: (f32, f32), b: (f32, f32)| line(a.0, a.1, b.0, b.1);
                match case {
                    1 | 14 => seg(left, bottom),
                    2 | 13 => seg(bottom, right),
                    3 | 12 => seg(left, right),
                    4 | 11 => seg(top, right),
                    6 | 9 => seg(top, bottom),
                    7 | 8 => seg(left, top),
                    5 => {
                        seg(left, top);
                        seg(bottom, right);
                    }
                    10 => {
                        seg(top, right);
                        seg(left, bottom);
                    }
                    _ => {}
                }
            }
        }
    }
}
