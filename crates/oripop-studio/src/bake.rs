//! Headless bake: params.json → stipple raster → PNG + manifest.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oripop_canvas::{generate_dots, raster_dots, Params};
use oripop_project::{BakeLock, BakeManifest, CanvasKind, TextureLibrary};

pub struct BakeOptions {
    pub time: f32,
    pub frame: u64,
}

impl Default for BakeOptions {
    fn default() -> Self {
        Self {
            time: 0.0,
            frame: 0,
        }
    }
}

pub fn bake(
    library: &TextureLibrary,
    design_id: &str,
    opts: BakeOptions,
) -> io::Result<(PathBuf, PathBuf)> {
    let (design_dir, design) = library.design(design_id)?;
    let (width, height) = canvas_size(&design.canvas)?;

    let params_path = design_dir.join(&design.params);
    let text = fs::read_to_string(&params_path)?;
    let mut params: Params = serde_json::from_str(&text).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, e)
    })?;
    params.canvas.width = width as f32;
    params.canvas.height = height as f32;

    let dots = generate_dots(&params, opts.time);
    let mut buf = vec![0u8; (width * height * 4) as usize];
    raster_dots(
        &mut buf,
        width,
        height,
        params.canvas.width,
        params.canvas.height,
        &dots,
        [10, 10, 14, 255],
    );

    let bakes_dir = library.root.join("bakes").join(design_id);
    fs::create_dir_all(&bakes_dir)?;

    let stamp = timestamp();
    let png_name = format!("{stamp}.png");
    let json_name = format!("{stamp}.bake.json");
    let png_path = bakes_dir.join(&png_name);
    let json_path = bakes_dir.join(&json_name);

    write_png(&png_path, width, height, &buf)?;

    let mut manifest = BakeManifest::new(
        design_id,
        &iso_timestamp(),
        &png_name,
        width,
        height,
        true,
    );
    manifest.lock = Some(BakeLock {
        frame: Some(opts.frame),
        time: Some(opts.time),
        seed: Some(params.seed),
    });
    manifest.params_snapshot = Some(serde_json::to_value(&params).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, e)
    })?);
    manifest.canvas_kind = canvas_kind_label(&design.canvas).to_string();

    fs::write(&json_path, serde_json::to_string_pretty(&manifest).unwrap())?;

    Ok((png_path, json_path))
}

fn canvas_size(canvas: &CanvasKind) -> io::Result<(u32, u32)> {
    match canvas {
        CanvasKind::Provisional { width, height } => Ok((*width, *height)),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "bake v0 supports provisional canvas only",
        )),
    }
}

fn canvas_kind_label(canvas: &CanvasKind) -> &'static str {
    match canvas {
        CanvasKind::Provisional { .. } => "provisional",
        CanvasKind::PrimitiveUv { .. } => "primitive_uv",
        CanvasKind::Atlas { .. } => "atlas",
    }
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
    // Simple UTC-ish stamp without extra deps.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
