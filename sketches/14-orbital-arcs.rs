//! Orbital arc composition.
//!
//! Proves: arc modes (open/chord/pie), stroke caps, lerp_color, push_style /
//! pop_style, shear, frame_rate, transforms.

use oripop_canvas::prelude::*;

const W: f32 = 900.0;
const H: f32 = 900.0;

fn main() {
    size(W as u32, H as u32);
    title("14-orbital-arcs — arc modes and styles");
    smooth(4);
    run(draw);
}

fn draw() {
    background(12, 10, 18);
    frame_rate(60.0);
    let t = millis() as f32 * 0.001;

    let warm = color(255, 140, 60);
    let cool = color(80, 150, 255);

    push();
    translate(W * 0.5, H * 0.5);

    // Concentric rings of arc segments, alternating direction.
    no_fill();
    for ring in 0..14 {
        let r = 60.0 + ring as f32 * 26.0;
        let dir = if ring % 2 == 0 { 1.0 } else { -1.0 };
        let segments = 3 + ring % 4;
        let span = std::f32::consts::TAU / segments as f32;
        let blend = ring as f32 / 13.0;
        let c = lerp_color(warm, cool, blend);

        push_style();
        stroke_color(c);
        stroke_weight(map((t * 0.7 + blend * std::f32::consts::TAU).sin(), -1.0, 1.0, 2.0, 9.0));
        stroke_cap(if ring % 3 == 0 { StrokeCap::Square } else { StrokeCap::Round });
        for s in 0..segments {
            let a0 = t * dir * (0.2 + blend * 0.5) + s as f32 * span;
            arc(0.0, 0.0, r * 2.0, r * 2.0, a0, a0 + span * 0.62);
        }
        pop_style();
    }

    // A slow pie/chord pair in the middle, sheared for a little depth.
    push();
    shear_x((t * 0.4).sin() * 0.18);
    no_stroke();
    fill_a(255, 200, 120, 70);
    arc_with_mode(0.0, 0.0, 96.0, 96.0, t, t + 4.2, ArcMode::Pie);
    fill_a(120, 200, 255, 70);
    arc_with_mode(0.0, 0.0, 96.0, 96.0, t + 3.4, t + 5.9, ArcMode::Chord);
    pop();

    pop();
}
