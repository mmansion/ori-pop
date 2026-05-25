//! GPU bake: same render path as preview, read back to RGBA + PNG.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oripop_project::{BakeLock, BakeManifest, CanvasKind, Project};

use crate::cartridge::Cartridge;
use crate::gpu::PreviewGpu;

pub struct BakeOptions {
    pub time:  f32,
    pub frame: u64,
}

impl Default for BakeOptions {
    fn default() -> Self {
        Self { time: 0.0, frame: 0 }
    }
}

pub fn bake(
    project: &Project,
    texture_id: &str,
    cartridge: &Cartridge,
    width: u32,
    height: u32,
    gpu: &mut PreviewGpu,
    opts: BakeOptions,
) -> io::Result<(PathBuf, PathBuf)> {
    let rgba = gpu.bake_rgba(cartridge, opts.time, width, height);

    let bakes_dir = project.root.join("bakes").join(texture_id);
    fs::create_dir_all(&bakes_dir)?;

    let stamp = timestamp();
    let png_name  = format!("{stamp}.png");
    let json_name = format!("{stamp}.bake.json");
    let png_path  = bakes_dir.join(&png_name);
    let json_path = bakes_dir.join(&json_name);

    write_png(&png_path, width, height, &rgba)?;

    let mut manifest = BakeManifest::new(
        texture_id,
        &iso_timestamp(),
        &png_name,
        width,
        height,
        true,
    );
    manifest.lock = Some(BakeLock {
        frame: Some(opts.frame),
        time:  Some(opts.time),
        seed:  None,
    });
    let params_snapshot: serde_json::Value =
        serde_json::from_slice(cartridge.params_bytes()).unwrap_or(serde_json::Value::Null);
    manifest.params_snapshot = Some(params_snapshot);
    manifest.canvas_kind = canvas_kind_label(project, texture_id)?;

    fs::write(&json_path, serde_json::to_string_pretty(&manifest).unwrap())?;

    Ok((png_path, json_path))
}

fn canvas_kind_label(project: &Project, texture_id: &str) -> io::Result<String> {
    let (_, manifest) = project.texture(texture_id)?;
    Ok(match manifest.canvas {
        CanvasKind::Provisional { .. } => "provisional".to_string(),
        CanvasKind::PrimitiveUv { .. } => "primitive_uv".to_string(),
        CanvasKind::Atlas { .. }       => "atlas".to_string(),
    })
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "invalid RGBA buffer size")
    })?;
    img.save(path).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

fn timestamp() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("bake-{ms}")
}

fn iso_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
