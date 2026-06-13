//! Truchet arc tiling with animated tile flips.
//!
//! Proves: arcs as tiling primitives, seeded reproducible randomness,
//! lerp_color gradients across the grid, per-tile transform animation.
//!
//! Each tile holds two quarter-circle arcs joining edge midpoints; the two
//! orientations connect into endless meandering loops. Tiles occasionally
//! flip with a smooth quarter-turn.

use std::cell::RefCell;

use oripop_runtime::prelude::*;

const COLS: usize = 20;
const ROWS: usize = 13;
const TILE: f32 = 58.0;
const W: f32 = COLS as f32 * TILE;
const H: f32 = ROWS as f32 * TILE;

struct Tile {
    /// 0.0 or 1.0 when settled; in between while flipping.
    orient: f32,
    flipping: bool,
}

thread_local! {
    static TILES: RefCell<Vec<Tile>> = const { RefCell::new(Vec::new()) };
}

fn main() {
    size(W as u32, H as u32);
    title("18-truchet-arcs — meandering tile loops");
    smooth(4);
    run(draw);
}

fn draw() {
    background(14, 13, 17);

    TILES.with(|cell| {
        let mut tiles = cell.borrow_mut();
        if tiles.is_empty() {
            random_seed(2026);
            *tiles = (0..COLS * ROWS)
                .map(|_| Tile { orient: if random(1.0) < 0.5 { 0.0 } else { 1.0 }, flipping: false })
                .collect();
        }

        let deep = color(60, 90, 220);
        let glow = color(255, 170, 90);
        let t = millis() as f32 * 0.001;

        for row in 0..ROWS {
            for col in 0..COLS {
                let tile = &mut tiles[row * COLS + col];

                // Rarely start a flip; animate it as a slow quarter turn so
                // at most a couple of tiles are ever in motion.
                if !tile.flipping && random(1.0) < 0.0001 {
                    tile.flipping = true;
                }
                if tile.flipping {
                    let target = if tile.orient < 0.5 { 1.0 } else { 0.0 };
                    tile.orient += (target - tile.orient) * 0.04;
                    if (tile.orient - target).abs() < 0.01 {
                        tile.orient = target;
                        tile.flipping = false;
                    }
                }

                // Color drifts along the diagonal and breathes with time.
                let k = (col as f32 / COLS as f32 + row as f32 / ROWS as f32) * 0.5;
                let c = lerp_color(deep, glow, (k + (t * 0.25).sin() * 0.15).clamp(0.0, 1.0));

                push();
                translate(col as f32 * TILE + TILE * 0.5, row as f32 * TILE + TILE * 0.5);
                rotate(tile.orient * std::f32::consts::FRAC_PI_2);

                no_fill();
                stroke_color(c);
                // Steady weight, varying only across space — the motion
                // should come from the loops and flips, not from throbbing.
                stroke_weight(3.5 + k * 2.0);
                stroke_cap(StrokeCap::Round);

                // Two quarter arcs centered on opposite tile corners.
                let h = TILE * 0.5;
                arc(-h, -h, TILE, TILE, 0.0, std::f32::consts::FRAC_PI_2);
                arc(h, h, TILE, TILE, std::f32::consts::PI, std::f32::consts::PI * 1.5);

                pop();
            }
        }
    });
}
