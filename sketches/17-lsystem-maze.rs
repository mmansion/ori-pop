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
const FRAMES_TO_FILL: f32 = 540.0; // ~9s per pass at 60fps
const HOLD_FRAMES: u32 = 150;      // pause on the finished maze

struct Growth {
    segments: Vec<[f32; 4]>,
    drawn: usize,
    per_frame: usize,
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

/// Walk the expanded string with a turtle, returning scaled segments.
fn build_segments(order: usize) -> Vec<[f32; 4]> {
    let cells = (1usize << order) - 1; // 2^order - 1 steps per axis
    let span = (W.min(H) - MARGIN * 2.0) / cells as f32;
    let ox = (W - cells as f32 * span) * 0.5;
    let oy = (H - cells as f32 * span) * 0.5;

    let (mut x, mut y) = (ox, oy);
    let (mut dx, mut dy) = (1.0f32, 0.0f32); // heading: +x

    let mut segments = Vec::with_capacity(1 << (2 * order));
    for c in expand_hilbert(order).chars() {
        match c {
            'F' => {
                let nx = x + dx * span;
                let ny = y + dy * span;
                segments.push([x, y, nx, ny]);
                x = nx;
                y = ny;
            }
            // y grows down, so this pair is a consistent left/right swap.
            '+' => (dx, dy) = (dy, -dx),
            '-' => (dx, dy) = (-dy, dx),
            _ => {}
        }
    }
    segments
}

fn start_pass(order_idx: usize) -> Growth {
    background(12, 12, 16);
    let segments = build_segments(ORDERS[order_idx]);
    let per_frame = ((segments.len() as f32 / FRAMES_TO_FILL).ceil() as usize).max(2);
    Growth { segments, drawn: 0, per_frame, order_idx, hold: HOLD_FRAMES }
}

/// Path color along the curve: ember -> magenta -> ice.
fn palette(k: f32) -> Color {
    let a = Color::rgb(245, 160, 70);
    let b = Color::rgb(205, 80, 160);
    let c = Color::rgb(120, 200, 245);
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

        // Grow: draw only the new segments; the canvas keeps the rest.
        let order = ORDERS[growth.order_idx];
        let weight = match order {
            5 => 6.0,
            6 => 3.4,
            _ => 1.8,
        };
        stroke_cap(StrokeCap::Round);
        stroke_weight(weight);

        let end = (growth.drawn + growth.per_frame).min(total);
        for i in growth.drawn..end {
            let k = i as f32 / total as f32;
            stroke_color(palette(k));
            let [x1, y1, x2, y2] = growth.segments[i];
            line(x1, y1, x2, y2);
        }
        growth.drawn = end;
    });
}
