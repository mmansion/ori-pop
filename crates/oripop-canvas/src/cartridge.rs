//! Helpers for textures shipped as `cdylib` and loaded by `oripop-studio`.
//!
//! Each texture exposes a single `extern "C"` entry point that, per frame:
//! 1. resets the cdylib's drawing context ([`begin_frame`]),
//! 2. invokes the user-provided closure that issues `oripop-canvas` calls,
//! 3. emits a serialized [`DrawFrame`](crate::draw::DrawFrame) to the host
//!    through a function-pointer callback.
//!
//! The callback model avoids sharing a thread-local context between the host
//! and the dynamically-loaded library (which the platform / linker make
//! difficult on Windows). The host owns the GPU pipeline and decodes the
//! emitted bytes into its own buffer.

use std::os::raw::c_void;

use crate::draw::{
    begin_frame, resolved_canvas_format, take_draw_frame, DrawFrame, GraphicsFrame,
    ResolvedCanvasFormat, Vertex, VERTEX_2D_STRIDE,
};

/// Version of the cartridge ABI: the emit-callback signature **and** the
/// serialized frame layout (see [`encode_draw_frame`]).
///
/// v1 — 24-byte vertices (position + color).
/// v2 — 36-byte vertices (position + color + uv + texture slot), flat stream.
/// v3 — full [`DrawFrame`] wire blob (vertices + runs + graphics).
///
/// Textures export this via [`crate::export_cartridge_abi!`]; the studio
/// host refuses to load a cartridge whose version does not match.
pub const CARTRIDGE_ABI_VERSION: u32 = 3;

const WIRE_MAGIC: u32 = 0x3349_524F; // "ORI3" little-endian

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
/// referenced by `frame_ptr` are only valid for the duration of the call.
pub type EmitFn = unsafe extern "C" fn(
    emit_ctx:  *mut c_void,
    frame_ptr: *const u8,
    frame_len: usize,
);

/// Decoded host-side frame plus resolved canvas format from the wire blob.
pub struct DecodedDrawFrame {
    pub frame:    DrawFrame,
    pub resolved: ResolvedCanvasFormat,
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_f64(buf: &mut Vec<u8>, v: f64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn read_u32(bytes: &[u8], off: &mut usize) -> Option<u32> {
    let slice = bytes.get(*off..*off + 4)?;
    *off += 4;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn read_u64(bytes: &[u8], off: &mut usize) -> Option<u64> {
    let slice = bytes.get(*off..*off + 8)?;
    *off += 8;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

fn read_f64(bytes: &[u8], off: &mut usize) -> Option<f64> {
    let slice = bytes.get(*off..*off + 8)?;
    *off += 8;
    Some(f64::from_le_bytes(slice.try_into().ok()?))
}

fn write_color(buf: &mut Vec<u8>, c: wgpu::Color) {
    write_f64(buf, c.r);
    write_f64(buf, c.g);
    write_f64(buf, c.b);
    write_f64(buf, c.a);
}

fn read_color(bytes: &[u8], off: &mut usize) -> Option<wgpu::Color> {
    Some(wgpu::Color {
        r: read_f64(bytes, off)?,
        g: read_f64(bytes, off)?,
        b: read_f64(bytes, off)?,
        a: read_f64(bytes, off)?,
    })
}

/// Serialize a [`DrawFrame`] for the cartridge host.
pub fn encode_draw_frame(frame: &DrawFrame, resolved: ResolvedCanvasFormat) -> Vec<u8> {
    let mut out = Vec::new();
    write_u32(&mut out, WIRE_MAGIC);
    write_u32(&mut out, CARTRIDGE_ABI_VERSION);
    let format_flag = match resolved {
        ResolvedCanvasFormat::Srgb => 0u32,
        ResolvedCanvasFormat::Float => 1u32,
    };
    let flags = (u32::from(frame.clear)) | (format_flag << 1);
    write_u32(&mut out, flags);
    write_u32(&mut out, frame.density);
    write_color(&mut out, frame.bg);

    let vert_bytes = bytemuck::cast_slice::<Vertex, u8>(&frame.vertices);
    write_u32(&mut out, vert_bytes.len() as u32);
    out.extend_from_slice(vert_bytes);

    write_u32(&mut out, frame.runs.len() as u32);
    for run in &frame.runs {
        write_u64(&mut out, run.tex);
        write_u32(&mut out, run.start);
        write_u32(&mut out, run.count);
    }

    write_u32(&mut out, frame.graphics.len() as u32);
    for gf in &frame.graphics {
        write_u64(&mut out, gf.id);
        write_u32(&mut out, gf.width);
        write_u32(&mut out, gf.height);
        write_color(&mut out, gf.bg);
        let gv = bytemuck::cast_slice::<Vertex, u8>(&gf.vertices);
        write_u32(&mut out, gv.len() as u32);
        out.extend_from_slice(gv);
    }
    out
}

/// Deserialize a cartridge wire blob into a host-owned [`DecodedDrawFrame`].
pub fn decode_draw_frame(bytes: &[u8]) -> Option<DecodedDrawFrame> {
    let mut off = 0usize;
    if read_u32(bytes, &mut off)? != WIRE_MAGIC {
        return None;
    }
    if read_u32(bytes, &mut off)? != CARTRIDGE_ABI_VERSION {
        return None;
    }
    let flags = read_u32(bytes, &mut off)?;
    let clear = flags & 1 != 0;
    let resolved = match (flags >> 1) & 1 {
        1 => ResolvedCanvasFormat::Float,
        _ => ResolvedCanvasFormat::Srgb,
    };
    let density = read_u32(bytes, &mut off)?;
    let bg = read_color(bytes, &mut off)?;

    let vert_len = read_u32(bytes, &mut off)? as usize;
    if vert_len % VERTEX_2D_STRIDE != 0 {
        return None;
    }
    let vert_slice = bytes.get(off..off + vert_len)?;
    off += vert_len;
    let vertices: Vec<Vertex> = bytemuck::cast_slice(vert_slice).to_vec();

    let run_count = read_u32(bytes, &mut off)? as usize;
    let mut runs = Vec::with_capacity(run_count);
    for _ in 0..run_count {
        runs.push(crate::draw::DrawRun {
            tex:   read_u64(bytes, &mut off)?,
            start: read_u32(bytes, &mut off)?,
            count: read_u32(bytes, &mut off)?,
        });
    }

    let gfx_count = read_u32(bytes, &mut off)? as usize;
    let mut graphics = Vec::with_capacity(gfx_count);
    for _ in 0..gfx_count {
        let id = read_u64(bytes, &mut off)?;
        let width = read_u32(bytes, &mut off)?;
        let height = read_u32(bytes, &mut off)?;
        let gfx_bg = read_color(bytes, &mut off)?;
        let gv_len = read_u32(bytes, &mut off)? as usize;
        if gv_len % VERTEX_2D_STRIDE != 0 {
            return None;
        }
        let gv_slice = bytes.get(off..off + gv_len)?;
        off += gv_len;
        graphics.push(GraphicsFrame {
            id,
            width,
            height,
            bg: gfx_bg,
            vertices: bytemuck::cast_slice(gv_slice).to_vec(),
        });
    }

    if off != bytes.len() {
        return None;
    }

    Some(DecodedDrawFrame {
        frame: DrawFrame {
            bg,
            clear,
            density,
            vertices,
            runs,
            graphics,
        },
        resolved,
    })
}

/// Run the texture's draw closure and emit the captured frame to the host.
///
/// `emit_ctx` is never dereferenced here; it is an opaque host pointer passed
/// straight back to the host-supplied `emit` callback.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn dispatch<F: FnOnce()>(emit: EmitFn, emit_ctx: *mut c_void, draw: F) {
    begin_frame();
    draw();
    let frame = take_draw_frame();
    let resolved = resolved_canvas_format();
    let bytes = encode_draw_frame(&frame, resolved);
    unsafe {
        emit(emit_ctx, bytes.as_ptr(), bytes.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{begin_frame, background, fill, rect, take_draw_frame};
    use crate::graphics::create_graphics;

    #[test]
    fn draw_frame_wire_round_trip() {
        begin_frame();
        background(20, 30, 40);
        let mut g = create_graphics(32, 32);
        g.fill(200, 100, 50);
        g.rect(4.0, 4.0, 12.0, 12.0);
        crate::draw::image(&g, 10.0, 10.0);
        fill(80, 80, 80);
        rect(50.0, 50.0, 20.0, 20.0);
        let frame = take_draw_frame();
        let resolved = crate::draw::resolved_canvas_format();
        let bytes = encode_draw_frame(&frame, resolved);
        let decoded = decode_draw_frame(&bytes).expect("decode");
        assert_eq!(decoded.frame.vertices.len(), frame.vertices.len());
        assert_eq!(decoded.frame.runs.len(), frame.runs.len());
        assert!(!decoded.frame.graphics.is_empty());
    }
}
