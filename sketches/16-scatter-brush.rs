//! Interactive scatter brush on a persistent canvas.
//!
//! Proves: pmouse, mouse buttons, mouse wheel, gaussian random, millis,
//! persistent canvas (nothing is ever cleared unless you press 'c').
//!
//! - Drag with the left button: airbrush strokes along the mouse path.
//! - Drag with the right button: dark eraser-ish strokes.
//! - Scroll wheel: brush radius.
//! - Press 'c': clear the canvas.

use std::cell::RefCell;

use oripop_canvas::prelude::*;

const W: f32 = 1100.0;
const H: f32 = 700.0;

thread_local! {
    static RADIUS: RefCell<f32> = const { RefCell::new(28.0) };
}

fn main() {
    size(W as u32, H as u32);
    title("16-scatter-brush — drag to paint, wheel for size, 'c' clears");
    smooth(4);
    run(draw);
}

fn draw() {
    if frame_count() == 1 || (key_pressed() && key() == 'c') {
        background(16, 15, 19);
    }

    let radius = RADIUS.with(|r| {
        let mut r = r.borrow_mut();
        *r = constrain(*r + mouse_wheel() * 3.0, 4.0, 120.0);
        *r
    });

    if mouse_pressed() {
        let (mx, my) = (mouse_x(), mouse_y());
        let (px, py) = (pmouse_x(), pmouse_y());
        let speed = dist(px, py, mx, my);
        // Hue drifts slowly with time; right button paints shadow.
        let erase = mouse_button() == Some(MouseButton::Right);
        let hue = ((millis() as f32 * 0.01) % 255.0) as u8;

        color_mode(ColorMode::Hsb);
        // Scatter gaussian dots along the segment from pmouse to mouse.
        let steps = (speed.max(1.0) as usize).min(40);
        for i in 0..=steps {
            let t = i as f32 / steps.max(1) as f32;
            let bx = lerp(px, mx, t);
            let by = lerp(py, my, t);
            for _ in 0..6 {
                let ox = random_gaussian() * radius * 0.4;
                let oy = random_gaussian() * radius * 0.4;
                let d = mag(ox, oy);
                let falloff = constrain(1.0 - d / radius, 0.0, 1.0);
                no_stroke();
                if erase {
                    fill_a(16, 15, 19, (falloff * 90.0) as u8 + 8);
                } else {
                    fill_a(hue, 160, 255, (falloff * 60.0) as u8 + 6);
                }
                circle(bx + ox, by + oy, 2.0 + falloff * 5.0);
            }
        }
        color_mode(ColorMode::Rgb);
    }

    // Brush cursor ring (drawn every frame; it leaves a faint trace, which
    // reads as part of the medium on a persistent canvas).
    no_fill();
    stroke_a(255, 255, 255, 18);
    stroke_weight(1.0);
    circle(mouse_x(), mouse_y(), radius * 2.0);
}
