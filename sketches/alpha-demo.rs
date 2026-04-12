use oripop_canvas::prelude::*;

fn main() {
    size(600, 400);
    title("alpha-demo");
    smooth(4);
    run(draw);
}

fn draw() {
    background(30, 30, 40);

    // Three overlapping circles centered on canvas (Venn diagram)
    let cx = 300.0_f32;
    let cy = 200.0_f32;
    let d = 180.0;
    let spread = 60.0;
    no_stroke();

    fill_a(255, 60, 60, 140);
    ellipse(cx - spread - d / 2.0, cy - spread * 0.5 - d / 2.0, d, d);

    fill_a(60, 200, 60, 140);
    ellipse(cx + spread - d / 2.0, cy - spread * 0.5 - d / 2.0, d, d);

    fill_a(60, 60, 255, 140);
    ellipse(cx - d / 2.0, cy + spread * 0.5 - d / 2.0, d, d);

    // Semi-transparent white stroke over everything
    stroke_a(255, 255, 255, 80);
    stroke_weight(2.0);
    no_fill();
    for i in 0..10 {
        let y = 50.0 + i as f32 * 35.0;
        line(50.0, y, 550.0, y);
    }
}
