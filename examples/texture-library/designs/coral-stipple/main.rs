//! Example library design — field stipple loaded from `params.json`.
//!
//! Play sets `ORIPOP_DESIGN_DIR` to this folder.

use std::path::PathBuf;
use std::sync::OnceLock;

use oripop_runtime::prelude::*;
use oripop_canvas::{generate_dots, Params};

static PARAMS: OnceLock<Params> = OnceLock::new();

fn main() {
    let params = load_params();
    let w = params.canvas.width.max(1.0) as u32;
    let h = params.canvas.height.max(1.0) as u32;
    PARAMS.set(params).expect("params init");
    size(w, h);
    title("coral-stipple");
    smooth(4);
    run(draw);
}

fn load_params() -> Params {
    let dir = std::env::var("ORIPOP_DESIGN_DIR").expect("ORIPOP_DESIGN_DIR must be set by oripop-studio play");
    let path = PathBuf::from(dir).join("params.json");
    let text = std::fs::read_to_string(&path).expect("read params.json");
    serde_json::from_str(&text).expect("parse params.json")
}

fn draw() {
    let params = PARAMS.get().expect("params");
    background(10, 10, 14);

    let t = frame_count() as f32 / 60.0;
    let dots = generate_dots(params, t);

    no_stroke();
    for dot in &dots {
        let lum = (dot.w * 220.0 + 35.0).min(255.0) as u8;
        fill_a(lum, lum, lum.saturating_add(6), 255);
        let s = dot.r * params.canvas.width * 2.0;
        rect(dot.x - s * 0.5, dot.y - s * 0.5, s, s);
    }
}
