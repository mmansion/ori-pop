use oripop_canvas::prelude::*;
use oripop_canvas::{generate_dots, Force, Line, Params, Point};

fn main() {
    size(800, 800);
    title("interactive");
    smooth(4);
    run(draw);
}

fn draw() {
    background(10, 10, 16);

    let mx = mouse_x() / 800.0;
    let my = mouse_y() / 800.0;

    let mut params = Params::default();
    params.canvas.width = 800.0;
    params.canvas.height = 800.0;
    params.distribution.dot_count = 25_000;
    params.distribution.min_radius = 0.001;
    params.distribution.max_radius = 0.003;
    params.field.singularity.strength = 0.0;
    params.field.warp_amount = 0.03;
    params.field.forces = vec![
        Force::Attractor {
            center: Point::new(mx, my),
            strength: 1.0,
            falloff: 14.0,
        },
        Force::Attractor {
            center: Point::new(1.0 - mx, 1.0 - my),
            strength: 0.5,
            falloff: 10.0,
        },
        Force::Compression {
            axis: Line::new(Point::new(mx, 0.0), Point::new(1.0 - mx, 1.0)),
            width: 0.06,
            strength: if mouse_pressed() { 0.9 } else { 0.3 },
        },
    ];

    let dots = generate_dots(&params, 0.0);

    no_stroke();
    for dot in &dots {
        let w = dot.w;
        let r = (w * 180.0 + 40.0).min(255.0) as u8;
        let g = (w * 140.0 + 30.0).min(255.0) as u8;
        let b = (w * 220.0 + 50.0).min(255.0) as u8;
        fill_a(r, g, b, (w * 180.0 + 40.0).min(255.0) as u8);
        let s = dot.r * 800.0 * 2.0;
        rect(dot.x - s * 0.5, dot.y - s * 0.5, s, s);
    }
}
