//! L-system garden — four species of turtle-interpreted plants.
//!
//! Proves: recursive/grammar-generated geometry through the transform stack
//! (push/pop/rotate/translate as a turtle), lerp_color along branch depth,
//! stroke weight tapering, noise-driven wind sway.

use std::cell::RefCell;

use oripop_canvas::prelude::*;

const W: f32 = 1200.0;
const H: f32 = 800.0;

struct Species {
    expanded: String,
    angle: f32,
    seg_len: f32,
    trunk: Color,
    tip: Color,
    x: f32,
    sway_seed: f32,
}

thread_local! {
    static GARDEN: RefCell<Vec<Species>> = const { RefCell::new(Vec::new()) };
}

/// Expand an L-system: apply `rules` to `axiom` for `n` iterations.
/// Guards against runaway growth.
fn expand(axiom: &str, rules: &[(char, &str)], n: usize) -> String {
    let mut s = axiom.to_string();
    for _ in 0..n {
        let mut next = String::with_capacity(s.len() * 4);
        for c in s.chars() {
            match rules.iter().find(|(from, _)| *from == c) {
                Some((_, to)) => next.push_str(to),
                None => next.push(c),
            }
        }
        s = next;
        if s.len() > 20_000 {
            break;
        }
    }
    s
}

fn build_garden() -> Vec<Species> {
    vec![
        Species {
            // Bushy fern.
            expanded: expand("F", &[('F', "FF+[+F-F-F]-[-F+F+F]")], 3),
            angle: radians(22.0),
            seg_len: 7.5,
            trunk: Color::rgb(80, 58, 42),
            tip: Color::rgb(120, 215, 110),
            x: W * 0.16,
            sway_seed: 11.0,
        },
        Species {
            // Slender weed.
            expanded: expand("F", &[('F', "F[+F]F[-F]F")], 4),
            angle: radians(25.7),
            seg_len: 3.2,
            trunk: Color::rgb(70, 70, 50),
            tip: Color::rgb(230, 220, 120),
            x: W * 0.42,
            sway_seed: 47.0,
        },
        Species {
            // Classic branching tree (node-rewriting).
            expanded: expand("X", &[('X', "F[+X][-X]FX"), ('F', "FF")], 5),
            angle: radians(25.0),
            seg_len: 5.0,
            trunk: Color::rgb(92, 60, 50),
            tip: Color::rgb(235, 140, 180),
            x: W * 0.66,
            sway_seed: 83.0,
        },
        Species {
            // Coral-like shrub.
            expanded: expand("F", &[('F', "F[+FF][-FF]F[-F][+F]F")], 2),
            angle: radians(31.0),
            seg_len: 9.0,
            trunk: Color::rgb(50, 70, 80),
            tip: Color::rgb(110, 220, 230),
            x: W * 0.88,
            sway_seed: 129.0,
        },
    ]
}

fn draw_plant(s: &Species, t: f32) {
    // Wind: the branch angle breathes with noise, stronger near the tips.
    let wind = (noise2(s.sway_seed, t) - 0.5) * 0.35;

    push();
    translate(s.x, H);

    let mut depth: i32 = 0;
    let max_depth = 7.0;
    for c in s.expanded.chars() {
        match c {
            'F' => {
                let k = constrain(depth as f32 / max_depth, 0.0, 1.0);
                stroke_color(lerp_color(s.trunk, s.tip, k));
                stroke_weight(map(k, 0.0, 1.0, 4.5, 0.9));
                line(0.0, 0.0, 0.0, -s.seg_len);
                translate(0.0, -s.seg_len);
            }
            '+' => rotate(s.angle + wind * (1.0 + depth as f32 * 0.25)),
            '-' => rotate(-s.angle + wind * (1.0 + depth as f32 * 0.25)),
            '[' => {
                push();
                depth += 1;
            }
            ']' => {
                pop();
                depth -= 1;
            }
            _ => {}
        }
    }

    pop();
}

fn main() {
    size(W as u32, H as u32);
    title("17-lsystem-garden — grammar-grown plants");
    smooth(4);
    run(draw);
}

fn draw() {
    GARDEN.with(|g| {
        let mut garden = g.borrow_mut();
        if garden.is_empty() {
            noise_seed(5);
            *garden = build_garden();
        }

        background(15, 14, 18);
        // Faint ground line.
        stroke_a(255, 255, 255, 26);
        stroke_weight(1.0);
        line(0.0, H - 1.5, W, H - 1.5);

        let t = millis() as f32 * 0.00018;
        for s in garden.iter() {
            draw_plant(s, t);
        }
    });
}
