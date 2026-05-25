//! Example library design — field stipple loaded from `params.json`.
//!
//! Play sets `ORIPOP_DESIGN_DIR` to this folder.

use std::path::PathBuf;
use std::sync::OnceLock;

use oripop_runtime::prelude::*;
use oripop_canvas::{draw_stipple, Params};

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
    let t = frame_count() as f32 / 60.0;
    draw_stipple(params, t);
}
