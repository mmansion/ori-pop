//! Screenshot and frame-sequence capture.
//!
//! # Keys
//! - **S** — save one PNG to the working directory with a timestamp filename.
//! - **R** — toggle frame-sequence recording.  Each frame is saved as a
//!   numbered PNG inside a timestamped folder.  Stop recording, then stitch
//!   with ffmpeg:
//!   ```sh
//!   ffmpeg -r 60 -i recording_1234567890/frame_%06d.png -c:v libx264 -pix_fmt yuv420p out.mp4
//!   ```
//!
//! # Platform notes
//! Capture requires `COPY_SRC` on the surface texture.  This is enabled in
//! [`crate::renderer::Renderer`]'s surface configuration.  On platforms that
//! do not support `COPY_SRC` on the surface the API call will silently fail.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── State ─────────────────────────────────────────────────────────────────────

/// Capture state managed by the runner.
///
/// Check [`CaptureState::needs_capture`] each frame and call
/// [`crate::renderer::Renderer::capture_frame`] when it returns `Some`.
#[derive(Default)]
pub struct CaptureState {
    screenshot_pending: bool,
    pub recording:      bool,
    recording_dir:      Option<PathBuf>,
    pub frame_count:    u64,
}

impl CaptureState {
    /// Request a single screenshot on the next frame.
    pub fn request_screenshot(&mut self) {
        self.screenshot_pending = true;
    }

    /// Toggle recording on or off.
    pub fn toggle_recording(&mut self) {
        if self.recording {
            self.recording     = false;
            self.recording_dir = None;
            self.frame_count   = 0;
            log::info!("Recording stopped.");
        } else {
            let dir = PathBuf::from(format!("recording_{}", timestamp()));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::error!("Could not create recording dir: {e}");
                return;
            }
            log::info!("Recording started → {}", dir.display());
            self.recording_dir = Some(dir);
            self.recording     = true;
            self.frame_count   = 0;
        }
    }

    /// Returns the path to save this frame to, if capture is needed.
    ///
    /// Call once per frame.  For a screenshot this returns `Some` exactly
    /// once; for recording it returns `Some` every frame while active.
    pub fn needs_capture(&mut self) -> Option<PathBuf> {
        if self.screenshot_pending {
            self.screenshot_pending = false;
            let path = PathBuf::from(format!("screenshot_{}.png", timestamp()));
            return Some(path);
        }
        if self.recording {
            if let Some(ref dir) = self.recording_dir {
                let path = dir.join(format!("frame_{:06}.png", self.frame_count));
                self.frame_count += 1;
                return Some(path);
            }
        }
        None
    }
}

// ── GPU capture ───────────────────────────────────────────────────────────────

/// Copy the contents of a GPU texture to a PNG file on disk.
///
/// `bytes_per_row` must be aligned to [`wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`].
/// The surface format is used to determine whether B and R channels need to
/// be swapped (wgpu on Windows typically uses `Bgra8UnormSrgb`).
///
/// Blocks the calling thread while waiting for GPU readback — acceptable for
/// occasional screenshots and moderate-framerate recordings.  For high-speed
/// recording consider an async variant that queues the readback.
pub fn save_frame(
    device: &wgpu::Device,
    queue:  &wgpu::Queue,
    texture: &wgpu::Texture,
    width:  u32,
    height: u32,
    format: wgpu::TextureFormat,
    path:   &Path,
) {
    // bytes_per_row must be a multiple of COPY_BYTES_PER_ROW_ALIGNMENT (256).
    let raw_bpr    = width * 4;
    let align      = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes_per_row = (raw_bpr + align - 1) / align * align;

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("capture staging"),
        size:               (bytes_per_row * height) as u64,
        usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("capture") },
    );
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin:    wgpu::Origin3d::ZERO,
            aspect:    wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset:         0,
                bytes_per_row:  Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(enc.finish()));

    // Block until the GPU has finished writing to the staging buffer.
    staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();

    let raw  = staging.slice(..).get_mapped_range();
    let swap = is_bgra(format);

    // De-pad rows and optionally swap B↔R to produce RGBA bytes.
    let mut rgba: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let src = (row * bytes_per_row) as usize;
        for col in 0..width as usize {
            let px = src + col * 4;
            if swap {
                rgba.extend_from_slice(&[raw[px+2], raw[px+1], raw[px], raw[px+3]]);
            } else {
                rgba.extend_from_slice(&raw[px..px+4]);
            }
        }
    }
    drop(raw);

    match image::RgbaImage::from_raw(width, height, rgba) {
        Some(img) => match img.save(path) {
            Ok(())   => log::info!("Saved: {}", path.display()),
            Err(e)   => log::error!("Save failed: {e}"),
        },
        None => log::error!("Failed to create image buffer for capture."),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_bgra(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
