//! Offscreen canvas demo — `create_graphics` + `image`.
//!
//! One `Graphics` surface is redrawn each frame and placed on the main
//! canvas three times: at native size, scaled down, and rotating.

use std::cell::RefCell;

use oripop_canvas::prelude::*;

thread_local! {
    static OFFSCREEN: RefCell<Option<Graphics>> = const { RefCell::new(None) };
}

fn main() {
    size(900, 600);
    title("12-graphics-demo — offscreen canvases");
    smooth(4);
    run(draw);
}

fn draw() {
    background(16, 16, 22);
    let t = frame_count() as f32 * 0.02;

    OFFSCREEN.with(|cell| {
        let mut slot = cell.borrow_mut();
        let g = slot.get_or_insert_with(|| create_graphics(220, 220));

        // Redraw the offscreen surface from scratch each frame.
        g.background(30, 24, 48);
        g.no_fill();
        g.stroke_weight(3.0);
        for i in 0..8 {
            let phase = t + i as f32 * 0.7;
            let r = 36.0 + i as f32 * 22.0 + phase.sin() * 12.0;
            let lum = 120 + (i * 16) as u8;
            g.stroke(lum, 180, 255 - lum);
            g.ellipse(110.0, 110.0, r, r);
        }
        g.stroke(255, 220, 120);
        g.stroke_weight(2.0);
        g.line(110.0, 110.0, 110.0 + t.cos() * 90.0, 110.0 + t.sin() * 90.0);

        // Place it three ways.
        image(g, 40.0, 40.0);
        image_sized(g, 320.0, 40.0, 110.0, 110.0);

        push();
        translate(640.0, 360.0);
        rotate(t * 0.7);
        image_sized(g, -90.0, -90.0, 180.0, 180.0);
        pop();
    });

    // Solid geometry interleaved after textured runs.
    stroke(90, 90, 110);
    stroke_weight(1.0);
    line(0.0, 300.0, 900.0, 300.0);
    no_stroke();
    fill(255, 120, 90);
    ellipse(120.0 + t.sin() * 60.0, 420.0, 36.0, 36.0);
}
