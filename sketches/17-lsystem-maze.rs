//! L-system maze growth — a Hilbert curve drawing itself.
//!
//! The Hilbert space-filling curve *is* an L-system:
//!
//! ```text
//! axiom: A
//! A -> +BF-AFA-FB+
//! B -> -AF+BFB+FA-      (F forward, +/- turn 90 degrees)
//! ```
//!
//! The expanded string is walked by a turtle into a segment list, then the
//! curve grows a few segments per frame onto the persistent canvas until it
//! has filled the whole frame like a maze. Each completed pass restarts at
//! the next depth (5 -> 6 -> 7), so the maze refills ever finer.
//!
//! Proves: L-system expansion, space-filling geometry, persistent-canvas
//! incremental drawing, multi-stop lerp_color along the path.

use std::cell::RefCell;

use oripop_canvas::prelude::*;

const W: f32 = 900.0;
const H: f32 = 900.0;
const MARGIN: f32 = 40.0;
const ORDERS: [usize; 3] = [5, 6, 7];
/// Seconds each pass takes to draw, per order (at ~60fps).
const PASS_SECONDS: [f32; 3] = [12.0, 16.0, 24.0];
const HOLD_FRAMES: u32 = 180; // pause on the finished maze

struct Growth {
    segments: Vec<[f32; 4]>,
    /// Fully drawn segments.
    drawn: usize,
    /// Pen distance already drawn into the current segment.
    dist_in_seg: f32,
    /// Pen speed in pixels per frame.
    speed: f32,
    order_idx: usize,
    hold: u32,
}

thread_local! {
    static GROWTH: RefCell<Option<Growth>> = const { RefCell::new(None) };
}

/// Expand the Hilbert L-system to `order` iterations.
fn expand_hilbert(order: usize) -> String {
    let mut s = String::from("A");
    for _ in 0..order {
        let mut next = String::with_capacity(s.len() * 9);
        for c in s.chars() {
            match c {
                'A' => next.push_str("+BF-AFA-FB+"),
                'B' => next.push_str("-AF+BFB+FA-"),
                other => next.push(other),
            }
        }
        s = next;
    }
    s
}

/// Walk the expanded string with a turtle in unit steps, then normalize the
/// path's bounding box onto the canvas — so the curve always fills the
/// frame regardless of where the turtle wanders.
fn build_segments(order: usize) -> Vec<[f32; 4]> {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut dx, mut dy) = (1.0f32, 0.0f32);

    let mut points = vec![[0.0f32, 0.0f32]];
    for c in expand_hilbert(order).chars() {
        match c {
            'F' => {
                x += dx;
                y += dy;
                points.push([x, y]);
            }
            '+' => (dx, dy) = (dy, -dx),
            '-' => (dx, dy) = (-dy, dx),
            _ => {}
        }
    }

    // Fit the path's bounds into the canvas, centered, uniform scale.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in &points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    let extent = (max_x - min_x).max(max_y - min_y).max(1.0);
    let scale = (W.min(H) - MARGIN * 2.0) / extent;
    let ox = (W - (max_x - min_x) * scale) * 0.5 - min_x * scale;
    let oy = (H - (max_y - min_y) * scale) * 0.5 - min_y * scale;

    points
        .windows(2)
        .map(|w| {
            [
                w[0][0] * scale + ox,
                w[0][1] * scale + oy,
                w[1][0] * scale + ox,
                w[1][1] * scale + oy,
            ]
        })
        .collect()
}

fn start_pass(order_idx: usize) -> Growth {
    // Plotter paper.
    background(237, 232, 220);
    let segments = build_segments(ORDERS[order_idx]);
    let total_len: f32 = segments
        .iter()
        .map(|s| dist(s[0], s[1], s[2], s[3]))
        .sum();
    let speed = (total_len / (PASS_SECONDS[order_idx] * 60.0)).max(1.0);
    Growth { segments, drawn: 0, dist_in_seg: 0.0, speed, order_idx, hold: HOLD_FRAMES }
}

/// Pen inks along the path: indigo -> crimson -> teal.
fn palette(k: f32) -> Color {
    let a = Color::rgb(45, 60, 140);
    let b = Color::rgb(170, 45, 70);
    let c = Color::rgb(20, 115, 105);
    if k < 0.5 {
        lerp_color(a, b, k * 2.0)
    } else {
        lerp_color(b, c, (k - 0.5) * 2.0)
    }
}

fn main() {
    size(W as u32, H as u32);
    title("17-lsystem-maze — Hilbert curve growing itself");
    smooth(4);
    run(draw);
}

fn draw() {
    GROWTH.with(|cell| {
        let mut slot = cell.borrow_mut();
        let growth = slot.get_or_insert_with(|| start_pass(0));

        let total = growth.segments.len();
        if growth.drawn >= total {
            // Finished: hold, then restart one order deeper.
            growth.hold = growth.hold.saturating_sub(1);
            if growth.hold == 0 {
                let next = (growth.order_idx + 1) % ORDERS.len();
                *growth = start_pass(next);
            }
            return;
        }

        // Move the pen a fixed distance this frame, drawing through as many
        // (partial) segments as the budget covers — continuous pen motion,
        // every line visibly traced. The canvas keeps everything drawn.
        let order = ORDERS[growth.order_idx];
        let weight = match order {
            5 => 6.0,
            6 => 3.4,
            _ => 1.8,
        };
        stroke_cap(StrokeCap::Round);
        stroke_weight(weight);

        let mut budget = growth.speed;
        while budget > 0.0 && growth.drawn < total {
            let [x1, y1, x2, y2] = growth.segments[growth.drawn];
            let seg_len = dist(x1, y1, x2, y2).max(1e-4);
            let from_t = growth.dist_in_seg / seg_len;
            let remaining = seg_len - growth.dist_in_seg;

            stroke_color(palette(growth.drawn as f32 / total as f32));
            if budget >= remaining {
                // Finish this segment and roll into the next.
                line(lerp(x1, x2, from_t), lerp(y1, y2, from_t), x2, y2);
                budget -= remaining;
                growth.drawn += 1;
                growth.dist_in_seg = 0.0;
            } else {
                // Pen stops mid-segment this frame.
                let to_t = (growth.dist_in_seg + budget) / seg_len;
                line(
                    lerp(x1, x2, from_t),
                    lerp(y1, y2, from_t),
                    lerp(x1, x2, to_t),
                    lerp(y1, y2, to_t),
                );
                growth.dist_in_seg += budget;
                budget = 0.0;
            }
        }
    });
}
