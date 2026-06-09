//! Flowfield ink traces — single-texture cdylib loaded by the studio.

use std::os::raw::c_void;

use oripop_canvas::cartridge::{dispatch, EmitFn};
use oripop_canvas::prelude::*;
use rand::{rngs::SmallRng, Rng, SeedableRng};

oripop_canvas::export_cartridge_abi!();
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowfieldParams {
    pub canvas:        Canvas,
    pub seed:          u64,
    pub particles:     u32,
    pub max_steps:     u32,
    pub step_length:   f32,
    pub noise_scale:   f32,
    pub noise_octaves: u32,
    pub time_warp:     f32,
    pub stroke:        [u8; 4],
    pub stroke_weight: f32,
    pub background:    [u8; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Canvas {
    pub width:  f32,
    pub height: f32,
}

/// Studio entry point.
///
/// # Safety
/// `params_ptr`/`params_len` must describe a valid UTF-8 buffer for the
/// lifetime of this call. `emit` must remain valid until it returns.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
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
    let params: FlowfieldParams = match serde_json::from_str(text) {
        Ok(p) => p,
        Err(_) => return,
    };
    dispatch(emit, emit_ctx, || draw(&params, t));
}

pub fn draw(params: &FlowfieldParams, t: f32) {
    let [br, bg, bb] = params.background;
    background(br, bg, bb);
    let [sr, sg, sb, sa] = params.stroke;
    stroke_a(sr, sg, sb, sa);
    stroke_weight(params.stroke_weight);

    let w = params.canvas.width;
    let h = params.canvas.height;
    let drift = t * params.time_warp;

    let mut rng = SmallRng::seed_from_u64(params.seed);
    for _ in 0..params.particles {
        let mut x: f32 = rng.random::<f32>() * w;
        let mut y: f32 = rng.random::<f32>() * h;
        for _ in 0..params.max_steps {
            let angle = field_angle(x, y, params.noise_scale, params.noise_octaves, drift);
            let nx = x + angle.cos() * params.step_length;
            let ny = y + angle.sin() * params.step_length;
            if nx < 0.0 || ny < 0.0 || nx >= w || ny >= h {
                break;
            }
            line(x, y, nx, ny);
            x = nx;
            y = ny;
        }
    }
}

fn field_angle(x: f32, y: f32, scale: f32, octaves: u32, drift: f32) -> f32 {
    let mut sum = 0.0f32;
    let mut amp = 1.0f32;
    let mut freq = scale;
    for k in 0..octaves.max(1) {
        let phase = k as f32 * 1.3 + drift;
        sum += amp * ((x * freq + phase).sin() + (y * freq * 1.13 + phase * 0.7).cos());
        amp *= 0.55;
        freq *= 2.0;
    }
    sum * std::f32::consts::PI
}
