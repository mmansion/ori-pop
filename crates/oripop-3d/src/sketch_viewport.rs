//! Offscreen sketch viewport — rasterize [`DrawFrame`] to a GPU texture for
//! egui embedding or headless PNG bakes (studio preview / CLI).

use oripop_canvas::draw::{DrawFrame, ResolvedCanvasFormat};

use crate::canvas_raster::CanvasRaster;

const COPY_ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

/// Rasterizes canvas-authored frames offscreen using the same path as the
/// 3D player sketch mode.
pub struct SketchViewport {
    device:  wgpu::Device,
    queue:   wgpu::Queue,
    raster:  CanvasRaster,
    egui:    Option<egui_wgpu::RenderState>,
    tex_w:   u32,
    tex_h:   u32,
    egui_id: Option<egui::TextureId>,
}

impl SketchViewport {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let device = rs.device.clone();
        let queue = rs.queue.clone();
        let raster = CanvasRaster::new(device.clone(), queue.clone());
        Self {
            device,
            queue,
            raster,
            egui: Some(rs.clone()),
            tex_w: 0,
            tex_h: 0,
            egui_id: None,
        }
    }

    /// Headless device for CLI bakes (no eframe present).
    pub fn new_headless() -> std::io::Result<Self> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference:       wgpu::PowerPreference::default(),
            compatible_surface:     None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("no wgpu adapter: {e}"))
        })?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label:                 Some("oripop sketch viewport"),
            required_features:     wgpu::Features::empty(),
            required_limits:       wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints:          Default::default(),
            trace:                 wgpu::Trace::Off,
        }))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let raster = CanvasRaster::new(device.clone(), queue.clone());
        Ok(Self {
            device,
            queue,
            raster,
            egui: None,
            tex_w: 0,
            tex_h: 0,
            egui_id: None,
        })
    }

    pub fn invalidate_target(&mut self) {
        if let (Some(egui), Some(id)) = (&self.egui, self.egui_id.take()) {
            egui.renderer.write().free_texture(&id);
        }
        self.tex_w = 0;
        self.tex_h = 0;
    }

    /// Raster `frame` and return an egui texture id when embedded in eframe.
    pub fn render(
        &mut self,
        frame: &DrawFrame,
        resolved: ResolvedCanvasFormat,
        logical_w: u32,
        logical_h: u32,
    ) -> Option<egui::TextureId> {
        self.encode_frame(frame, resolved, logical_w, logical_h, true);
        self.egui_id
    }

    /// Raster once and return tightly-packed RGBA8 pixels (no row padding).
    pub fn bake_rgba(
        &mut self,
        frame: &DrawFrame,
        resolved: ResolvedCanvasFormat,
        logical_w: u32,
        logical_h: u32,
    ) -> std::io::Result<Vec<u8>> {
        self.encode_frame(frame, resolved, logical_w, logical_h, false);
        let tex_w = logical_w.saturating_mul(frame.density.max(1));
        let tex_h = logical_h.saturating_mul(frame.density.max(1));
        read_texture_rgba8(&self.device, &self.queue, self.raster.canvas_texture(), tex_w, tex_h)
    }

    fn encode_frame(
        &mut self,
        frame: &DrawFrame,
        resolved: ResolvedCanvasFormat,
        logical_w: u32,
        logical_h: u32,
        register_egui: bool,
    ) {
        let density = frame.density.max(1);
        let tex_w = logical_w.saturating_mul(density);
        let tex_h = logical_h.saturating_mul(density);
        self.ensure_target(tex_w, tex_h, resolved, register_egui);

        let canvas_w = logical_w as f32 * density as f32;
        let canvas_h = logical_h as f32 * density as f32;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sketch viewport"),
        });
        self.raster
            .encode(&mut encoder, frame, canvas_w, canvas_h, false);
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn ensure_target(
        &mut self,
        tex_w: u32,
        tex_h: u32,
        resolved: ResolvedCanvasFormat,
        register_egui: bool,
    ) {
        if self.tex_w == tex_w && self.tex_h == tex_h && self.egui_id.is_some() == register_egui {
            return;
        }
        if self.tex_w != tex_w || self.tex_h != tex_h {
            self.invalidate_target();
            self.raster.ensure_canvas(tex_w.max(1), tex_h.max(1), resolved);
            self.tex_w = tex_w;
            self.tex_h = tex_h;
        }
        if register_egui && self.egui_id.is_none() {
            if let Some(egui) = &self.egui {
                self.egui_id = Some(egui.renderer.write().register_native_texture(
                    &self.device,
                    self.raster.canvas_texture_view(),
                    wgpu::FilterMode::Linear,
                ));
            }
        }
    }
}

fn read_texture_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> std::io::Result<Vec<u8>> {
    let bytes_per_pixel = 4u32;
    let unpadded_row = width * bytes_per_pixel;
    let padded_row = unpadded_row.div_ceil(COPY_ALIGN) * COPY_ALIGN;
    let buf_size = (padded_row * height) as u64;

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("sketch viewport readback"),
        size:               buf_size,
        usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("sketch viewport readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin:    wgpu::Origin3d::ZERO,
            aspect:    wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset:         0,
                bytes_per_row:  Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let view = slice.get_mapped_range();
    let mut out = Vec::with_capacity((unpadded_row * height) as usize);
    for row in 0..height {
        let start = (row * padded_row) as usize;
        let end = start + unpadded_row as usize;
        out.extend_from_slice(&view[start..end]);
    }
    drop(view);
    readback.unmap();
    Ok(out)
}
