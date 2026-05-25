//! Coral stipple field — single-texture cdylib loaded by the studio.
//!
//! The studio calls [`oripop_texture_render`] each frame with the current
//! `params.json` payload. The standalone binary entry point lives in `bin.rs`.

use std::os::raw::c_void;

use oripop_canvas::cartridge::{dispatch, EmitFn};
use oripop_canvas::prelude::*;
use oripop_canvas::{generate_dots, Params};

/// Studio entry point. Reads params from a JSON byte slice and emits the
/// resulting frame (background + tessellated vertices) to the host through
/// `emit`.
///
/// # Safety
/// `params_ptr`/`params_len` must describe a valid UTF-8 buffer for the
/// lifetime of this call. `emit` must remain valid until it returns.
#[unsafe(no_mangle)]
pub extern "C" fn oripop_texture_render(
    t:          f32,
    params_ptr: *const u8,
    params_len: usize,
    emit:       EmitFn,
    emit_ctx:   *mut c_void,
) {
    let bytes = unsafe { std::slice::from_raw_parts(params_ptr, params_len) };
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return,
    };
    let params: Params = match serde_json::from_str(text) {
        Ok(p) => p,
        Err(_) => return,
    };
    dispatch(emit, emit_ctx, || draw(&params, t));
}

pub fn draw(params: &Params, t: f32) {
    background(10, 10, 14);
    no_stroke();
    let dots = generate_dots(params, t);
    for dot in &dots {
        let lum = (dot.w * 220.0 + 35.0).min(255.0) as u8;
        fill_a(lum, lum, lum.saturating_add(6), 255);
        let s = dot.r * params.canvas.width * 2.0;
        rect(dot.x - s * 0.5, dot.y - s * 0.5, s, s);
    }
}
