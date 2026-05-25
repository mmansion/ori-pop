//! Built-in field-stipple draw routine.
//!
//! Shared between the standalone runtime (called from a design's `main.rs`) and
//! the studio's in-process GPU preview, so both render the same vertices.

use crate::draw::{background, fill_a, no_stroke, rect};
use crate::field::{generate_dots, Params};

/// Draw a stipple field for `params` at time `t` (seconds) using the
/// Processing-style drawing API. Call after `begin_frame` / inside `draw_fn`.
pub fn draw_stipple(params: &Params, t: f32) {
    background(10, 10, 14);
    no_stroke();
    let dots = generate_dots(params, t);
    for dot in &dots {
        let lum = (dot.w * 220.0 + 35.0).min(255.0) as u8;
        fill_a(lum, lum, lum.saturating_add(6), 255);
        let s = dot.r * params.canvas.width * 2.0;
        rect(dot.x - s * 0.5, dot.y - s * 0.5, s, s);
    }
}
