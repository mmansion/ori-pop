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

/// Version of the cartridge ABI: the emit-callback signature **and** the
/// vertex byte layout it carries (see [`crate::draw::VERTEX_2D_STRIDE`]).
///
/// v1 — 24-byte vertices (position + color).
/// v2 — 36-byte vertices (position + color + uv + texture slot).
///
/// Textures export this via [`crate::export_cartridge_abi!`]; the studio
/// host refuses to load a cartridge whose version does not match.
pub const CARTRIDGE_ABI_VERSION: u32 = 2;

/// Export the cartridge ABI version symbol from a texture cdylib.
///
/// Place one invocation in the texture's `lib.rs`:
///
/// ```ignore
/// oripop_canvas::export_cartridge_abi!();
/// ```
#[macro_export]
macro_rules! export_cartridge_abi {
    () => {
        #[unsafe(no_mangle)]
        pub extern "C" fn oripop_texture_abi_version() -> u32 {
            $crate::cartridge::CARTRIDGE_ABI_VERSION
        }
    };
}

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
///
/// `emit_ctx` is never dereferenced here; it is an opaque host pointer passed
/// straight back to the host-supplied `emit` callback.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
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
