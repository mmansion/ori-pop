//! Compile, load, and dispatch into a single texture cdylib at runtime.
//!
//! Each texture in `<project>/textures/<id>/` is a Cargo crate that builds a
//! `cdylib`. We invoke `cargo build -p <id>` to produce
//! `target/debug/<id-underscored>.dll`, copy that to a unique filename, and
//! load the copy via [`libloading`].
//!
//! The unique-filename trick lets us reload after the user edits + saves the
//! texture source: Windows will not return a fresh `HMODULE` for the same
//! file path while any handle is open, so we copy each build to a versioned
//! name.
//!
//! Drawing is exchanged through a C-ABI callback: the cdylib draws into its
//! own thread-local context and emits a serialized [`DrawFrame`] wire blob
//! (cartridge ABI v3) back into a host-owned buffer.

use std::cell::RefCell;
use std::fs;
use std::io;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::{Library, Symbol};
use oripop_canvas::cartridge::{decode_draw_frame, DecodedDrawFrame};
use oripop_canvas::draw::DrawFrame;

type EmitFn = unsafe extern "C" fn(
    emit_ctx:  *mut c_void,
    frame_ptr: *const u8,
    frame_len: usize,
);

type RenderFn = unsafe extern "C" fn(f32, *const u8, usize, EmitFn, *mut c_void);

type AbiVersionFn = unsafe extern "C" fn() -> u32;

pub struct Cartridge {
    #[allow(dead_code)]
    params_path:  PathBuf,
    params_cache: Vec<u8>,
    library:      Option<Library>,
    render_addr:  usize,
    frame:        RefCell<DecodedDrawFrame>,
}

impl Cartridge {
    pub fn build_and_load(
        workspace_root: &Path,
        texture_id: &str,
        params_path: PathBuf,
    ) -> io::Result<Self> {
        let lib_path = build_texture(workspace_root, texture_id)?;
        let staged = stage_copy(workspace_root, texture_id, &lib_path)?;
        let library = unsafe { Library::new(&staged) }.map_err(load_err)?;

        let expected = oripop_canvas::cartridge::CARTRIDGE_ABI_VERSION;
        let abi = unsafe {
            library
                .get::<AbiVersionFn>(b"oripop_texture_abi_version")
                .map(|sym| sym())
                .unwrap_or(1)
        };
        if abi != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "texture '{texture_id}' uses cartridge ABI v{abi}, host expects v{expected}; \
                     rebuild the texture against the current oripop-canvas \
                     (and add `oripop_canvas::export_cartridge_abi!();` to its lib.rs)"
                ),
            ));
        }

        let render_addr = unsafe {
            let sym: Symbol<RenderFn> =
                library.get(b"oripop_texture_render").map_err(load_err)?;
            *sym.into_raw() as usize
        };
        let params_cache = fs::read(&params_path)?;
        Ok(Self {
            params_path,
            params_cache,
            library: Some(library),
            render_addr,
            frame: RefCell::new(empty_frame()),
        })
    }

    pub fn params_bytes(&self) -> &[u8] {
        &self.params_cache
    }

    /// Call the texture's render entry. Returns a borrow of the host-owned
    /// frame that the cdylib emitted.
    pub fn render(&self, t: f32) -> std::cell::Ref<'_, DecodedDrawFrame> {
        *self.frame.borrow_mut() = empty_frame();
        let f: RenderFn = unsafe { std::mem::transmute(self.render_addr) };
        let frame_ptr = self.frame.as_ptr() as *mut c_void;
        unsafe {
            f(
                t,
                self.params_cache.as_ptr(),
                self.params_cache.len(),
                emit_thunk,
                frame_ptr,
            );
        }
        self.frame.borrow()
    }
}

impl Drop for Cartridge {
    fn drop(&mut self) {
        self.library.take();
    }
}

fn empty_frame() -> DecodedDrawFrame {
    DecodedDrawFrame {
        frame: DrawFrame {
            bg:       wgpu::Color::BLACK,
            clear:    false,
            density:  1,
            vertices: Vec::new(),
            runs:     Vec::new(),
            graphics: Vec::new(),
        },
        resolved: oripop_canvas::draw::ResolvedCanvasFormat::Srgb,
    }
}

unsafe extern "C" fn emit_thunk(
    emit_ctx:  *mut c_void,
    frame_ptr: *const u8,
    frame_len: usize,
) {
    if emit_ctx.is_null() || frame_ptr.is_null() || frame_len == 0 {
        return;
    }
    let src = std::slice::from_raw_parts(frame_ptr, frame_len);
    if let Some(decoded) = decode_draw_frame(src) {
        *(emit_ctx as *mut DecodedDrawFrame) = decoded;
    }
}

fn build_texture(workspace_root: &Path, texture_id: &str) -> io::Result<PathBuf> {
    let status = Command::new("cargo")
        .current_dir(workspace_root)
        .args(["build", "-p", texture_id, "--lib"])
        .status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("cargo build -p {texture_id} failed"),
        ));
    }
    let crate_name = texture_id.replace('-', "_");
    let target = workspace_root
        .join("target")
        .join("debug")
        .join(platform_lib_name(&crate_name));
    if !target.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("cdylib not found: {}", target.display()),
        ));
    }
    Ok(target)
}

fn stage_copy(workspace_root: &Path, texture_id: &str, src: &Path) -> io::Result<PathBuf> {
    let stage_dir = workspace_root.join("target").join("debug").join(".oripop");
    fs::create_dir_all(&stage_dir)?;
    let suffix = unique_suffix();
    let crate_name = texture_id.replace('-', "_");
    let staged = stage_dir.join(format!(
        "{}-{}.{}",
        crate_name,
        suffix,
        lib_extension()
    ));
    fs::copy(src, &staged)?;
    Ok(staged)
}

fn unique_suffix() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{ms}-{n}")
}

fn platform_lib_name(crate_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{crate_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{crate_name}.dylib")
    } else {
        format!("lib{crate_name}.so")
    }
}

fn lib_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    }
}

fn load_err(e: libloading::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}
