//! Standalone runtime: opens its own window and renders the texture each frame.
//!
//! Run with `cargo run -p coral-stipple` from the workspace root.

use std::path::PathBuf;
use std::sync::OnceLock;

use coral_stipple::draw;
use oripop_canvas::Params;
use oripop_runtime::prelude::*;

static PARAMS: OnceLock<Params> = OnceLock::new();

fn main() {
    let params = load_params();
    let w = params.canvas.width.max(1.0) as u32;
    let h = params.canvas.height.max(1.0) as u32;
    PARAMS.set(params).expect("params init");
    size(w, h);
    title("coral-stipple");
    smooth(4);
    run(draw_frame);
}

fn load_params() -> Params {
    let dir = std::env::var("ORIPOP_TEXTURE_DIR").unwrap_or_else(|_| {
        env!("CARGO_MANIFEST_DIR").to_string()
    });
    let path = PathBuf::from(dir).join("params.json");
    let text = std::fs::read_to_string(&path).expect("read params.json");
    serde_json::from_str(&text).expect("parse params.json")
}

fn draw_frame() {
    let params = PARAMS.get().expect("params");
    let t = frame_count() as f32 / 60.0;
    draw(params, t);
}
