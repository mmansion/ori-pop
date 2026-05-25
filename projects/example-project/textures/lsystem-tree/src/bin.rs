//! Standalone runtime: run with `cargo run -p lsystem-tree` from the workspace.

use std::path::PathBuf;
use std::sync::OnceLock;

use lsystem_tree::{draw, LsystemParams};
use oripop_runtime::prelude::*;

static PARAMS: OnceLock<LsystemParams> = OnceLock::new();

fn main() {
    let params = load_params();
    let w = params.canvas.width.max(1.0) as u32;
    let h = params.canvas.height.max(1.0) as u32;
    PARAMS.set(params).expect("params init");
    size(w, h);
    title("lsystem-tree");
    smooth(4);
    run(draw_frame);
}

fn load_params() -> LsystemParams {
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
