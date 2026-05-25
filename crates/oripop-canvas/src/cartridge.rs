//! Helpers for textures shipped as `cdylib` and loaded by `oripop-studio`.
//!
//! Each texture exposes a single `extern "C"` entry point that, per frame:
//! 1. resets the cdylib's drawing context ([`begin_frame`]),
//! 2. invokes the user-provided closure that issues `oripop-canvas` calls,
//! 3. emits the resulting background color + tessellated vertex bytes to the
//!    host through a function-pointer callback.
//!
//! The callback model avoids sharing a thread-local context between the host
//! and the dynamically-loaded library (which the platform / linker make
//! difficult on Windows). The host owns the GPU pipeline and copies the
//! emitted bytes into its own buffer.

use std::os::raw::c_void;

use crate::draw::{begin_frame, take_2d_vertices};

/// Signature of the emit callback that the host (`oripop-studio`) passes in.
///
/// The callback must finish copying the data before returning; the bytes
/// referenced by `vert_ptr` are only valid for the duration of the call.
pub type EmitFn = unsafe extern "C" fn(
    emit_ctx: *mut c_void,
    bg_r:     f64,
    bg_g:     f64,
    bg_b:     f64,
    bg_a:     f64,
    vert_ptr: *const u8,
    vert_len: usize,
);

/// Run the texture's draw closure and emit the captured frame to the host.
pub fn dispatch<F: FnOnce()>(emit: EmitFn, emit_ctx: *mut c_void, draw: F) {
    begin_frame();
    draw();
    let (bg, vbytes) = take_2d_vertices();
    unsafe {
        emit(
            emit_ctx,
            bg.r,
            bg.g,
            bg.b,
            bg.a,
            vbytes.as_ptr(),
            vbytes.len(),
        );
    }
}
