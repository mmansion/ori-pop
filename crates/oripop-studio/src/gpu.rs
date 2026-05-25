//! Off-screen wgpu renderer that shares its device with eframe.
//!
//! Builds the same 2D pipeline as the standalone runtime against an
//! `Rgba8UnormSrgb` texture, then hands the texture to egui for display.
//! Bake reuses the same pipeline and reads the texture back into RGBA8 pixels.

use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};
use eframe::egui;
use oripop_canvas::draw::{vertex_2d_buffer_layout, SHADER_2D_WGSL};

use crate::cartridge::Cartridge;

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const MSAA_SAMPLES:  u32 = 4;
const COPY_ALIGN:    u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
const VERTEX_STRIDE: usize = 24;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    _pad:       [f32; 2],
}

pub struct PreviewGpu {
    device:         wgpu::Device,
    queue:          wgpu::Queue,
    egui:           Option<egui_wgpu::RenderState>,
    pipeline:       wgpu::RenderPipeline,
    layout:         wgpu::BindGroupLayout,
    vertex_buf:     Option<wgpu::Buffer>,
    vertex_buf_cap: u64,
    target:         Option<PreviewTarget>,
}

struct PreviewTarget {
    width:      u32,
    height:     u32,
    color_view: wgpu::TextureView,
    msaa_view:  wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    egui_id:    Option<egui::TextureId>,
}

impl PreviewGpu {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let (pipeline, layout) = build_pipeline(&rs.device);
        Self {
            device: rs.device.clone(),
            queue: rs.queue.clone(),
            egui: Some(rs.clone()),
            pipeline,
            layout,
            vertex_buf: None,
            vertex_buf_cap: 0,
            target: None,
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
            label:                 Some("oripop-studio headless"),
            required_features:     wgpu::Features::empty(),
            required_limits:       wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints:          Default::default(),
            trace:                 wgpu::Trace::Off,
        }))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let (pipeline, layout) = build_pipeline(&device);
        Ok(Self {
            device,
            queue,
            egui: None,
            pipeline,
            layout,
            vertex_buf: None,
            vertex_buf_cap: 0,
            target: None,
        })
    }

    pub fn invalidate_target(&mut self) {
        if let Some(mut t) = self.target.take() {
            if let (Some(egui), Some(id)) = (&self.egui, t.egui_id.take()) {
                egui.renderer.write().free_texture(&id);
            }
        }
    }

    /// Render `cartridge` at time `t` (seconds) into the cached preview
    /// texture and return its egui texture id for display.
    pub fn render(
        &mut self,
        cartridge: &Cartridge,
        t: f32,
        width: u32,
        height: u32,
    ) -> Option<egui::TextureId> {
        self.ensure_target(width, height, /*register*/ true);

        let frame = cartridge.render(t);
        self.upload_vertices(&frame.vertices);

        let target = self.target.as_ref().expect("target");
        encode_render(
            &self.device,
            &self.queue,
            &self.pipeline,
            self.vertex_buf.as_ref(),
            &target.bind_group,
            &target.color_view,
            &target.msaa_view,
            frame.bg,
            frame.vertices.len(),
        );
        target.egui_id
    }

    /// Render once at the requested time and return tightly-packed RGBA8 pixels (no padding).
    pub fn bake_rgba(
        &mut self,
        cartridge: &Cartridge,
        t: f32,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let RenderTargets {
            color,
            color_view,
            msaa_view,
            uniform,
            bind_group,
        } = create_render_targets(&self.device, &self.layout, width, height);
        self.queue.write_buffer(
            &uniform,
            0,
            bytemuck::cast_slice(&[Uniforms {
                resolution: [width as f32, height as f32],
                _pad:       [0.0; 2],
            }]),
        );

        let frame = cartridge.render(t);
        self.upload_vertices(&frame.vertices);
        encode_render(
            &self.device,
            &self.queue,
            &self.pipeline,
            self.vertex_buf.as_ref(),
            &bind_group,
            &color_view,
            &msaa_view,
            frame.bg,
            frame.vertices.len(),
        );
        drop(frame);

        let bytes_per_pixel = 4u32;
        let unpadded_row = width * bytes_per_pixel;
        let padded_row   = unpadded_row.div_ceil(COPY_ALIGN) * COPY_ALIGN;
        let buf_size     = (padded_row * height) as u64;

        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("oripop-studio bake readback"),
            size:               buf_size,
            usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oripop-studio bake copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture:   &color,
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
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll");

        let view = slice.get_mapped_range();
        let mut out = Vec::with_capacity((unpadded_row * height) as usize);
        for row in 0..height {
            let start = (row * padded_row) as usize;
            let end   = start + unpadded_row as usize;
            out.extend_from_slice(&view[start..end]);
        }
        drop(view);
        readback.unmap();
        out
    }

    fn ensure_target(&mut self, width: u32, height: u32, register: bool) {
        if self
            .target
            .as_ref()
            .map(|t| t.width == width && t.height == height)
            .unwrap_or(false)
        {
            return;
        }
        self.invalidate_target();

        let RenderTargets {
            color: _color,
            color_view,
            msaa_view,
            uniform,
            bind_group,
        } = create_render_targets(&self.device, &self.layout, width, height);
        self.queue.write_buffer(
            &uniform,
            0,
            bytemuck::cast_slice(&[Uniforms {
                resolution: [width as f32, height as f32],
                _pad:       [0.0; 2],
            }]),
        );

        let egui_id = if register {
            self.egui.as_ref().map(|egui| {
                egui.renderer.write().register_native_texture(
                    &self.device,
                    &color_view,
                    wgpu::FilterMode::Linear,
                )
            })
        } else {
            None
        };

        self.target = Some(PreviewTarget {
            width,
            height,
            color_view,
            msaa_view,
            bind_group,
            egui_id,
        });
    }

    fn upload_vertices(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let needed = bytes.len() as u64;
        if self.vertex_buf_cap < needed {
            let cap = needed.next_power_of_two().max(4096);
            self.vertex_buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label:              Some("oripop-studio vertex buffer"),
                size:               cap,
                usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.vertex_buf_cap = cap;
        }
        self.queue
            .write_buffer(self.vertex_buf.as_ref().unwrap(), 0, bytes);
    }
}

fn encode_render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    vertex_buf: Option<&wgpu::Buffer>,
    bind_group: &wgpu::BindGroup,
    color_view: &wgpu::TextureView,
    msaa_view: &wgpu::TextureView,
    bg: wgpu::Color,
    vertex_bytes_len: usize,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("oripop-studio render"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:                    Some("oripop-studio pass"),
            color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                view:           msaa_view,
                resolve_target: Some(color_view),
                ops:            wgpu::Operations {
                    load:  wgpu::LoadOp::Clear(bg),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice:    None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes:         None,
            occlusion_query_set:      None,
        });
        if vertex_bytes_len > 0 {
            if let Some(vb) = vertex_buf {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                let vertex_count = (vertex_bytes_len / VERTEX_STRIDE) as u32;
                pass.draw(0..vertex_count, 0..1);
            }
        }
    }
    queue.submit(std::iter::once(encoder.finish()));
}

fn build_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:  Some("oripop-studio 2d shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_2D_WGSL.into()),
    });

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:   Some("oripop-studio bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding:    0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty:         wgpu::BindingType::Buffer {
                ty:                 wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size:   Some(NonZeroU64::new(16).unwrap()),
            },
            count:      None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some("oripop-studio pipeline layout"),
        bind_group_layouts:   &[&layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:         Some("oripop-studio pipeline"),
        layout:        Some(&pipeline_layout),
        vertex:        wgpu::VertexState {
            module:              &shader,
            entry_point:         Some("vs_main"),
            buffers:             &[vertex_2d_buffer_layout()],
            compilation_options: Default::default(),
        },
        fragment:      Some(wgpu::FragmentState {
            module:              &shader,
            entry_point:         Some("fs_main"),
            targets:             &[Some(wgpu::ColorTargetState {
                format:     TARGET_FORMAT,
                blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive:     wgpu::PrimitiveState {
            topology:           wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face:         wgpu::FrontFace::Ccw,
            cull_mode:          None,
            polygon_mode:       wgpu::PolygonMode::Fill,
            unclipped_depth:    false,
            conservative:       false,
        },
        depth_stencil: None,
        multisample:   wgpu::MultisampleState {
            count:                     MSAA_SAMPLES,
            mask:                      !0,
            alpha_to_coverage_enabled: false,
        },
        multiview:     None,
        cache:         None,
    });

    (pipeline, layout)
}

struct RenderTargets {
    color:      wgpu::Texture,
    color_view: wgpu::TextureView,
    msaa_view:  wgpu::TextureView,
    uniform:    wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

fn create_render_targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    width: u32,
    height: u32,
) -> RenderTargets {
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label:           Some("oripop-studio color"),
        size:            wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          TARGET_FORMAT,
        usage:           wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats:    &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());

    let msaa = device.create_texture(&wgpu::TextureDescriptor {
        label:           Some("oripop-studio msaa"),
        size:            wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count:    MSAA_SAMPLES,
        dimension:       wgpu::TextureDimension::D2,
        format:          TARGET_FORMAT,
        usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats:    &[],
    });
    let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());

    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("oripop-studio uniform"),
        size:               std::mem::size_of::<Uniforms>() as u64,
        usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label:   Some("oripop-studio bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding:  0,
            resource: uniform.as_entire_binding(),
        }],
    });

    RenderTargets {
        color,
        color_view,
        msaa_view,
        uniform,
        bind_group,
    }
}
