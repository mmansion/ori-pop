use oripop_core::prelude::*;
use oripop_core::{generate_dots, Force, Line, Params, Point};

fn main() {
    size(800, 800);
    title("forces");
    smooth(4);
    run(draw);
}

fn draw() {
    background(12, 10, 18);

    let mut params = Params::default();
    params.canvas.width = 800.0;
    params.canvas.height = 800.0;
    params.distribution.dot_count = 30_000;
    params.distribution.min_radius = 0.001;
    params.distribution.max_radius = 0.0028;
    params.field.singularity.strength = 0.0;
    params.field.warp_amount = 0.02;
    params.field.forces = vec![
        Force::Attractor {
            center: Point::new(0.3, 0.35),
            strength: 0.9,
            falloff: 12.0,
        },
        Force::Attractor {
            center: Point::new(1.0, 0.01),
            strength: 0.6,
            falloff: 8.0,
        },
        Force::Gradient {
            along: Line::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0)),
            strength: 0.3,
        },
        Force::Compression {
            axis: Line::new(Point::new(0.2, 0.8), Point::new(0.8, 0.2)),
            width: 0.08,
            strength: 0.7,
        },
    ];

    let dots = generate_dots(&params, 0.0);

    no_stroke();
    for dot in &dots {
        let lum = (dot.w * 220.0 + 35.0).min(255.0) as u8;
        fill_a(lum, lum, lum, 160);
        let s = dot.r * params.canvas.width * 2.0;
        rect(dot.x - s * 0.5, dot.y - s * 0.5, s, s);
    }
}
