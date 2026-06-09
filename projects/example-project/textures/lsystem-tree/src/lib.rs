//! L-system fractal branches — single-texture cdylib loaded by the studio.
//!
//! Symbols recognized by the turtle:
//! - `F` move forward, drawing a line
//! - `f` move forward without drawing
//! - `+` turn left by `angle_deg`
//! - `-` turn right by `angle_deg`
//! - `[` push turtle state
//! - `]` pop turtle state

use std::os::raw::c_void;

use oripop_canvas::cartridge::{dispatch, EmitFn};
use oripop_canvas::prelude::*;
use serde::{Deserialize, Serialize};

oripop_canvas::export_cartridge_abi!();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LsystemParams {
    pub canvas:          Canvas,
    pub axiom:           String,
    pub rule_in:         String,
    pub rule_out:        String,
    pub iterations:      u32,
    pub angle_deg:       f32,
    pub angle_wobble:    f32,
    pub step:            f32,
    pub start_x:         f32,
    pub start_y:         f32,
    pub start_angle_deg: f32,
    pub stroke:          [u8; 4],
    pub stroke_weight:   f32,
    pub background:      [u8; 3],
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
    let params: LsystemParams = match serde_json::from_str(text) {
        Ok(p) => p,
        Err(_) => return,
    };
    dispatch(emit, emit_ctx, || draw(&params, t));
}

pub fn draw(params: &LsystemParams, t: f32) {
    let [br, bg, bb] = params.background;
    background(br, bg, bb);
    let [sr, sg, sb, sa] = params.stroke;
    stroke_a(sr, sg, sb, sa);
    stroke_weight(params.stroke_weight);

    let sentence = expand(&params.axiom, &params.rule_in, &params.rule_out, params.iterations);
    let wobble = params.angle_wobble * (t * 0.8).sin();
    let angle_rad = (params.angle_deg + wobble).to_radians();

    walk(
        &sentence,
        params.start_x,
        params.start_y,
        params.start_angle_deg.to_radians(),
        params.step,
        angle_rad,
    );
}

fn expand(axiom: &str, rule_in: &str, rule_out: &str, iterations: u32) -> String {
    if rule_in.is_empty() {
        return axiom.to_string();
    }
    let mut s = axiom.to_string();
    for _ in 0..iterations {
        s = s.replace(rule_in, rule_out);
    }
    s
}

fn walk(sentence: &str, start_x: f32, start_y: f32, start_angle: f32, step: f32, angle_rad: f32) {
    let mut x = start_x;
    let mut y = start_y;
    let mut a = start_angle;
    let mut stack: Vec<(f32, f32, f32)> = Vec::new();
    for c in sentence.chars() {
        match c {
            'F' => {
                let nx = x + a.cos() * step;
                let ny = y + a.sin() * step;
                line(x, y, nx, ny);
                x = nx;
                y = ny;
            }
            'f' => {
                x += a.cos() * step;
                y += a.sin() * step;
            }
            '+' => a += angle_rad,
            '-' => a -= angle_rad,
            '[' => stack.push((x, y, a)),
            ']' => {
                if let Some((sx, sy, sa)) = stack.pop() {
                    x = sx;
                    y = sy;
                    a = sa;
                }
            }
            _ => {}
        }
    }
}
