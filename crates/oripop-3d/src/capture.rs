//! Screenshot and real-time video recording.
//!
//! # Keys
//! - **S** — save a PNG screenshot to `output/`.
//! - **R** — toggle recording.
//!
//! # Recording modes
//!
//! When **ffmpeg** is on `PATH` (recommended), raw BGRA frames are piped directly
//! to ffmpeg from a background thread.  Output is `output/recording_<ts>.mp4`.
//! The render thread only pays for the GPU readback (~1 ms at 1080p); all encoding
//! happens in a separate process using hardware acceleration.
//!
//! When ffmpeg is **not** found, the system falls back to a numbered PNG sequence
//! in `output/recording_<ts>/`.  Stop recording, then stitch with:
//! ```sh
//! ffmpeg -r 60 -i output/recording_xxx/frame_%06d.png -c:v libx264 -pix_fmt yuv420p out.mp4
//! ```
//!
//! # Platform notes
//! Capture requires `COPY_SRC` on the surface texture — enabled in the renderer's
//! surface configuration.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

// ── State ─────────────────────────────────────────────────────────────────────

/// Capture state owned by the runner.
///
/// Call [`update_surface`] after init and on every window resize so the
/// capture functions know the current dimensions and pixel format.
#[derive(Default)]
pub struct CaptureState {
    pub screenshot_pending: bool,
    pub recording:          bool,
    pub frame_count:        u64,

    // Set in update_surface; used by all capture calls.
    width:     u32,
    height:    u32,
    swap_bgra: bool, // true when surface format is BGRA (swap B↔R for PNG)

    // One of these is active when recording:
    recorder: Option<FfmpegRecorder>, // fast path — ffmpeg on PATH
    png_dir:  Option<PathBuf>,        // fallback — PNG sequence
}

impl CaptureState {
    /// Sync surface info from the renderer — call after init and on resize.
    pub fn update_surface(&mut self, width: u32, height: u32, format: wgpu::TextureFormat) {
        self.width     = width;
        self.height    = height;
        self.swap_bgra = is_bgra(format);
    }

    /// Request a single PNG screenshot on the next frame.
    pub fn request_screenshot(&mut self) {
        self.screenshot_pending = true;
    }

    /// Start or stop recording.  Pass current surface dimensions and format.
    pub fn toggle_recording(&mut self) {
        if self.recording {
            self.stop();
        } else {
            self.start();
        }
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn start(&mut self) {
        let ts  = timestamp();
        let w   = self.width;
        let h   = self.height;
        let fmt = if self.swap_bgra { "bgra" } else { "rgba" };
        let out = output_dir().join(format!("recording_{ts}.mp4"));

        // Attempt to spawn ffmpeg.
        let spawn = Command::new("ffmpeg")
            .args([
                "-f", "rawvideo",
                "-pixel_format", fmt,
                "-video_size", &format!("{w}x{h}"),
                "-framerate", "60",
                "-i", "pipe:0",
                "-c:v", "libx264",
                "-preset", "fast",
                "-pix_fmt", "yuv420p",
                "-y",
                out.to_str().unwrap_or("recording.mp4"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match spawn {
            Ok(mut child) => {
                let stdin = child.stdin.take().expect("ffmpeg stdin");
                // Bounded channel — 8 frames of buffer before back-pressure.
                let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(8);

                let out_display = out.display().to_string();
                let thread = std::thread::spawn(move || {
                    let mut bw = std::io::BufWriter::new(stdin);
                    for frame in rx {
                        if bw.write_all(&frame).is_err() { break; }
                    }
                    drop(bw); // flushes + closes stdin → ffmpeg receives EOF
                    child.wait().ok();
                    log::info!("Video saved: {}", out.display());
                });

                self.recorder    = Some(FfmpegRecorder { tx: Some(tx), thread: Some(thread) });
                self.recording   = true;
                self.frame_count = 0;
                log::info!("Recording started (ffmpeg) → {out_display}");
            }
            Err(_) => {
                // No ffmpeg — fall back to PNG sequence.
                let dir = output_dir().join(format!("recording_{ts}"));
                std::fs::create_dir_all(&dir).ok();
                log::warn!("ffmpeg not found on PATH — recording PNG sequence to {}", dir.display());
                log::warn!("Install ffmpeg from https://ffmpeg.org for fast MP4 output.");
                self.png_dir     = Some(dir);
                self.recording   = true;
                self.frame_count = 0;
            }
        }
    }

    fn stop(&mut self) {
        self.recording = false;
        if let Some(mut rec) = self.recorder.take() {
            rec.stop(); // drops sender → background thread finishes → ffmpeg encodes
        }
        self.png_dir     = None;
        self.frame_count = 0;
        log::info!("Recording stopped.");
    }

    // ── Frame operations ──────────────────────────────────────────────────────

    /// Capture one PNG screenshot.  Clears `screenshot_pending`.
    pub fn take_screenshot(
        &mut self,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
        texture: &wgpu::Texture,
    ) {
        self.screenshot_pending = false;
        let pixels = readback(device, queue, texture, self.width, self.height);
        let path   = output_dir().join(format!("screenshot_{}.png", timestamp()));
        save_png(&pixels, self.width, self.height, self.swap_bgra, &path);
    }

    /// Record the current frame.  Only call when `self.recording` is true.
    ///
    /// For the ffmpeg path: sends raw BGRA bytes to the background write thread
    /// (fast — just a channel send).  For the PNG fallback: encodes a PNG on
    /// the calling thread (slow).
    pub fn record_frame(
        &mut self,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
        texture: &wgpu::Texture,
    ) {
        let pixels = readback(device, queue, texture, self.width, self.height);

        if let Some(ref rec) = self.recorder {
            // Fast path — raw BGRA to ffmpeg (no conversion, just a channel send).
            if let Some(ref tx) = rec.tx {
                // If the channel is full (encoder can't keep up), drop the frame
                // rather than stalling the render thread.
                tx.try_send(pixels).ok();
            }
        } else if let Some(ref dir) = self.png_dir {
            // Slow fallback — PNG per frame.
            let path = dir.join(format!("frame_{:06}.png", self.frame_count));
            save_png(&pixels, self.width, self.height, self.swap_bgra, &path);
        }

        self.frame_count += 1;
    }
}

// ── ffmpeg recorder ───────────────────────────────────────────────────────────

struct FfmpegRecorder {
    tx:     Option<SyncSender<Vec<u8>>>,
    thread: Option<JoinHandle<()>>,
}

impl FfmpegRecorder {
    fn stop(&mut self) {
        drop(self.tx.take()); // disconnect channel — background thread exits loop
        if let Some(t) = self.thread.take() {
            t.join().ok(); // wait for ffmpeg to finish encoding
        }
    }
}

// ── GPU readback ──────────────────────────────────────────────────────────────

/// Copy GPU texture pixels to CPU memory.
///
/// Returns de-padded raw bytes in the surface's native format (BGRA or RGBA).
/// Row stride is exactly `width × 4` — no alignment padding.
///
/// Blocks until the GPU has finished the copy (`device.poll(wait_indefinitely)`).
/// This is the fundamental cost of CPU readback; all other work (encoding,
/// writing) is done on a background thread to keep this cost isolated.
fn readback(
    device:  &wgpu::Device,
    queue:   &wgpu::Queue,
    texture: &wgpu::Texture,
    width:   u32,
    height:  u32,
) -> Vec<u8> {
    let raw_bpr = width * 4;
    let align   = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded  = (raw_bpr + align - 1) / align * align;

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("capture staging"),
        size:               (padded * height) as u64,
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
                bytes_per_row:  Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(enc.finish()));

    staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();

    let raw     = staging.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((raw_bpr * height) as usize);
    for row in 0..height {
        let src = (row * padded) as usize;
        out.extend_from_slice(&raw[src..src + raw_bpr as usize]);
    }
    drop(raw);
    out
}

// ── PNG helper ────────────────────────────────────────────────────────────────

fn save_png(pixels: &[u8], width: u32, height: u32, swap: bool, path: &Path) {
    let rgba: Vec<u8> = if swap {
        pixels.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect()
    } else {
        pixels.to_vec()
    };
    match image::RgbaImage::from_raw(width, height, rgba) {
        Some(img) => match img.save(path) {
            Ok(())  => log::info!("Saved: {}", path.display()),
            Err(e)  => log::error!("Save failed: {e}"),
        },
        None => log::error!("Image buffer error during PNG save."),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_bgra(fmt: wgpu::TextureFormat) -> bool {
    matches!(fmt, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb)
}

fn timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from("output");
    std::fs::create_dir_all(&dir).ok();
    dir
}
