//! Standalone runtime: run with `cargo run -p flowfield-ink` from the workspace.

use std::path::PathBuf;
use std::sync::OnceLock;

use flowfield_ink::{draw, FlowfieldParams};
use oripop_runtime::prelude::*;

static PARAMS: OnceLock<FlowfieldParams> = OnceLock::new();

fn main() {
    let params = load_params();
    let w = params.canvas.width.max(1.0) as u32;
    let h = params.canvas.height.max(1.0) as u32;
    PARAMS.set(params).expect("params init");
    size(w, h);
    title("flowfield-ink");
    smooth(4);
    run(draw_frame);
}

fn load_params() -> FlowfieldParams {
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
