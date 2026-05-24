use oripop_canvas::prelude::*;

const W: u32 = 800;
const H: u32 = 600;

fn main() {
    size(W, H);
    title("transform-demo");
    smooth(4);
    run(draw);
}

fn draw() {
    background(25, 25, 35);

    let cx = W as f32 / 2.0;
    let cy = H as f32 / 2.0;
    let t = frame_count() as f32 * 0.01;

    // Spinning rays: draw in screen coords to verify lines work (transform applied in line() may be wrong)
    let tau = std::f32::consts::TAU;
    stroke(255, 180, 80);
    stroke_weight(3.0);
    no_fill();
    for i in 0..24 {
        let angle = t + i as f32 * tau / 24.0;
        let x2 = cx + 140.0 * angle.cos();
        let y2 = cy + 140.0 * angle.sin();
        line(cx, cy, x2, y2);
    }

    // Inner rotating rect (smaller, different color)
    stroke(100, 200, 255);
    no_fill();
    push();
    translate(cx, cy);
    rotate(-t * 1.5);
    rect(-30.0, -30.0, 60.0, 60.0);
    pop();

    // Scaled copies in corners using push/pop
    let corners = [(80.0, 80.0), (W as f32 - 80.0, 80.0), (W as f32 - 80.0, H as f32 - 80.0), (80.0, H as f32 - 80.0)];
    fill(180, 255, 150);
    no_stroke();
    for (i, (x, y)) in corners.iter().enumerate() {
        push();
        translate(*x, *y);
        rotate(t + i as f32 * std::f32::consts::FRAC_PI_2);
        scale(0.8 + 0.2 * (t * 2.0).sin(), 0.8 + 0.2 * (t * 2.0).sin());
        rect(-20.0, -20.0, 40.0, 40.0);
        pop();
    }
}
