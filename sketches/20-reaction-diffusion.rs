//! Gray-Scott reaction-diffusion, the canonical pattern-forming PDE.
//!
//! Proves: simulation-driven generative pattern (PHILOSOPHY.md names
//! reaction-diffusion as a core surface-pattern generator), value-mapped
//! lerp_color rendering. Run with --release; the sim is CPU-side.
//!
//! Two virtual chemicals U and V diffuse and react: V eats U and
//! reproduces, U feeds in, V dies off. Coral/fingerprint structure
//! self-organizes from a few random seeds. Press 'r' to reseed.

use std::cell::RefCell;

use oripop_runtime::prelude::*;

const COLS: usize = 140;
const ROWS: usize = 90;
const CELL: f32 = 8.0;
const W: f32 = COLS as f32 * CELL;
const H: f32 = ROWS as f32 * CELL;

// Gray-Scott parameters (coral growth regime).
const DU: f32 = 1.0;
const DV: f32 = 0.5;
const FEED: f32 = 0.0545;
const KILL: f32 = 0.062;
const STEPS_PER_FRAME: usize = 10;

struct Sim {
    u: Vec<f32>,
    v: Vec<f32>,
    u2: Vec<f32>,
    v2: Vec<f32>,
}

impl Sim {
    fn new() -> Self {
        let n = COLS * ROWS;
        let mut sim = Sim {
            u: vec![1.0; n],
            v: vec![0.0; n],
            u2: vec![1.0; n],
            v2: vec![0.0; n],
        };
        sim.seed();
        sim
    }

    /// Drop a handful of V islands into a clean U sea.
    fn seed(&mut self) {
        self.u.fill(1.0);
        self.v.fill(0.0);
        for _ in 0..14 {
            let cx = random_range(8.0, (COLS - 8) as f32) as usize;
            let cy = random_range(8.0, (ROWS - 8) as f32) as usize;
            for dy in 0..4 {
                for dx in 0..4 {
                    let i = (cy + dy) * COLS + cx + dx;
                    self.v[i] = 1.0;
                    self.u[i] = 0.5;
                }
            }
        }
    }

    fn step(&mut self) {
        // 3x3 Laplacian: center -1, orthogonal 0.2, diagonal 0.05.
        // Toroidal wrap keeps the field seamless (tiling-friendly).
        for y in 0..ROWS {
            let up = (y + ROWS - 1) % ROWS;
            let dn = (y + 1) % ROWS;
            for x in 0..COLS {
                let lf = (x + COLS - 1) % COLS;
                let rt = (x + 1) % COLS;
                let i = y * COLS + x;

                let lap = |g: &Vec<f32>| -> f32 {
                    -g[i] + 0.2 * (g[y * COLS + lf] + g[y * COLS + rt] + g[up * COLS + x] + g[dn * COLS + x])
                        + 0.05
                            * (g[up * COLS + lf]
                                + g[up * COLS + rt]
                                + g[dn * COLS + lf]
                                + g[dn * COLS + rt])
                };

                let u = self.u[i];
                let v = self.v[i];
                let uvv = u * v * v;
                self.u2[i] = (u + DU * lap(&self.u) - uvv + FEED * (1.0 - u)).clamp(0.0, 1.0);
                self.v2[i] = (v + DV * lap(&self.v) + uvv - (KILL + FEED) * v).clamp(0.0, 1.0);
            }
        }
        std::mem::swap(&mut self.u, &mut self.u2);
        std::mem::swap(&mut self.v, &mut self.v2);
    }
}

thread_local! {
    static SIM: RefCell<Option<Sim>> = const { RefCell::new(None) };
}

fn main() {
    size(W as u32, H as u32);
    title("20-reaction-diffusion — Gray-Scott ('r' reseeds)");
    smooth(1);
    run(draw);
}

fn draw() {
    background(10, 8, 14);

    SIM.with(|cell| {
        let mut slot = cell.borrow_mut();
        let sim = slot.get_or_insert_with(|| {
            random_seed(99);
            Sim::new()
        });

        if key_pressed() && key() == 'r' {
            sim.seed();
        }

        for _ in 0..STEPS_PER_FRAME {
            sim.step();
        }

        // Render V concentration as squares, color-lerped through the
        // membrane: dark substrate -> violet body -> hot edge.
        let body = Color::rgb(110, 60, 200);
        let edge = Color::rgb(255, 200, 120);
        no_stroke();
        for y in 0..ROWS {
            for x in 0..COLS {
                let v = sim.v[y * COLS + x];
                if v < 0.12 {
                    continue;
                }
                let k = constrain(map(v, 0.12, 0.4, 0.0, 1.0), 0.0, 1.0);
                fill_color(lerp_color(body, edge, k));
                let s = CELL * map(k, 0.0, 1.0, 0.55, 1.0);
                rect(
                    x as f32 * CELL + (CELL - s) * 0.5,
                    y as f32 * CELL + (CELL - s) * 0.5,
                    s,
                    s,
                );
            }
        }
    });
}
