//! Rasterize [`DrawFrame`] content into a GPU canvas texture (plane sampling source).
//!
//! Handles offscreen [`Graphics`](oripop_canvas::graphics::Graphics) targets and
//! textured draw runs — the same path previously owned by `Runner2D` in
//! `oripop-canvas`.

use std::collections::HashMap;

use oripop_canvas::draw::{
    DrawFrame, GraphicsFrame, ResolvedCanvasFormat, Vertex, VERTEX_2D_STRIDE,
};

use wgpu::util::DeviceExt;

const GFX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const MSAA: u32 = 4;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms2D {
    resolution: [f32; 2],
    _pad:       [f32; 2],
}

struct GfxTarget {
    width:       u32,
    height:      u32,
    color_view:  wgpu::TextureView,
    msaa_view:   wgpu::TextureView,
    render_bind: wgpu::BindGroup,
    sample_bind: wgpu::BindGroup,
    vbuf:        Option<wgpu::Buffer>,
    vbuf_cap:    u64,
}

pub struct CanvasRaster {
    device:              wgpu::Device,
    queue:               wgpu::Queue,
    layout:              wgpu::BindGroupLayout,
    white_view:          wgpu::TextureView,
    sampler:             wgpu::Sampler,
    uniform_buf:         wgpu::Buffer,
    uniform_bind:        wgpu::BindGroup,
    gfx_pipeline:        wgpu::RenderPipeline,
    canvas_pipeline:     wgpu::RenderPipeline,
    canvas_texture:      wgpu::Texture,
    canvas_texture_view: wgpu::TextureView,
    canvas_msaa_view:    wgpu::TextureView,
    canvas_tex_w:        u32,
    canvas_tex_h:        u32,
    canvas_gpu_format:   wgpu::TextureFormat,
    canvas_init:         bool,
    gfx_targets:         HashMap<u64, GfxTarget>,
    vertex_buf:          Option<wgpu::Buffer>,
    vertex_buf_cap:      u64,
}

fn resolved_to_gpu(resolved: ResolvedCanvasFormat) -> wgpu::TextureFormat {
    match resolved {
        ResolvedCanvasFormat::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        ResolvedCanvasFormat::Float => wgpu::TextureFormat::Rgba16Float,
    }
}

fn create_msaa_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label:           Some("canvas msaa"),
            size:            wgpu::Extent3d {
                width:                 w.max(1),
                height:                h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count:    MSAA,
            dimension:       wgpu::TextureDimension::D2,
            format,
            usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats:    &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    label: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:  Some(label),
        source: wgpu::ShaderSource::Wgsl(oripop_canvas::draw::SHADER_2D_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some(label),
        bind_group_layouts:   &[layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:  Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module:              &shader,
            entry_point:         Some("vs_main"),
            buffers:             &[oripop_canvas::draw::vertex_2d_buffer_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module:              &shader,
            entry_point:         Some("fs_main"),
            targets:             &[Some(wgpu::ColorTargetState {
                format,
                blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology:           wgpu::PrimitiveTopology::TriangleList,
            front_face:         wgpu::FrontFace::Ccw,
            cull_mode:          None,
            strip_index_format: None,
            polygon_mode:       wgpu::PolygonMode::Fill,
            unclipped_depth:    false,
            conservative:       false,
        },
        depth_stencil: None,
        multisample:   wgpu::MultisampleState {
            count:                     MSAA,
            mask:                      !0,
            alpha_to_coverage_enabled: false,
        },
        multiview:     None,
        cache:         None,
    })
}

impl CanvasRaster {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("canvas raster layout"),
            entries: &oripop_canvas::draw::bind_group_layout_entries_2d(),
        });
        let (white_view, sampler) = oripop_canvas::draw::create_white_texture_2d(&device, &queue);
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("canvas raster uniforms"),
            size:               std::mem::size_of::<Uniforms2D>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("canvas raster uniform bind"),
            layout:  &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::TextureView(&white_view),
                },
                wgpu::BindGroupEntry {
                    binding:  2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let gfx_pipeline = build_pipeline(&device, &layout, "gfx raster", GFX_FORMAT);
        let canvas_gpu_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let canvas_pipeline =
            build_pipeline(&device, &layout, "canvas raster", canvas_gpu_format);
        let (canvas_texture, canvas_texture_view) =
            create_canvas_texture(&device, 1, 1, canvas_gpu_format);
        let canvas_msaa_view = create_msaa_view(&device, canvas_gpu_format, 1, 1);
        Self {
            device,
            queue,
            layout,
            white_view,
            sampler,
            uniform_buf,
            uniform_bind,
            gfx_pipeline,
            canvas_pipeline,
            canvas_texture,
            canvas_texture_view,
            canvas_msaa_view,
            canvas_tex_w: 1,
            canvas_tex_h: 1,
            canvas_gpu_format,
            canvas_init: false,
            gfx_targets: HashMap::new(),
            vertex_buf: None,
            vertex_buf_cap: 0,
        }
    }

    pub fn canvas_texture_view(&self) -> &wgpu::TextureView {
        &self.canvas_texture_view
    }

    pub fn canvas_texture(&self) -> &wgpu::Texture {
        &self.canvas_texture
    }

    pub fn ensure_canvas(&mut self, width: u32, height: u32, resolved: ResolvedCanvasFormat) {
        let format = resolved_to_gpu(resolved);
        let w = width.max(1);
        let h = height.max(1);
        if self.canvas_tex_w == w && self.canvas_tex_h == h && self.canvas_gpu_format == format {
            return;
        }
        self.canvas_tex_w = w;
        self.canvas_tex_h = h;
        self.canvas_gpu_format = format;
        self.canvas_init = false;
        let (tex, view) = create_canvas_texture(&self.device, w, h, format);
        self.canvas_texture = tex;
        self.canvas_texture_view = view;
        self.canvas_msaa_view = create_msaa_view(&self.device, format, w, h);
        if self.canvas_gpu_format != wgpu::TextureFormat::Rgba8UnormSrgb {
            self.canvas_pipeline =
                build_pipeline(&self.device, &self.layout, "canvas raster", format);
        }
    }

    fn set_resolution(&self, w: f32, h: f32) {
        self.queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&Uniforms2D {
                resolution: [w, h],
                _pad:       [0.0; 2],
            }),
        );
    }

    fn prepare_gfx(&mut self, gf: &GraphicsFrame) {
        let needs_new = self
            .gfx_targets
            .get(&gf.id)
            .map(|t| t.width != gf.width || t.height != gf.height)
            .unwrap_or(true);
        if needs_new {
            let extent = wgpu::Extent3d {
                width:                 gf.width.max(1),
                height:                gf.height.max(1),
                depth_or_array_layers: 1,
            };
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label:           Some("gfx color"),
                size:            extent,
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          GFX_FORMAT,
                usage:           wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats:    &[],
            });
            let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = create_msaa_view(&self.device, GFX_FORMAT, gf.width, gf.height);
            let uniform = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("gfx uniforms"),
                contents: bytemuck::bytes_of(&Uniforms2D {
                    resolution: [gf.width as f32, gf.height as f32],
                    _pad:       [0.0; 2],
                }),
                usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let render_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label:   Some("gfx render bind"),
                layout:  &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding:  0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding:  1,
                        resource: wgpu::BindingResource::TextureView(&self.white_view),
                    },
                    wgpu::BindGroupEntry {
                        binding:  2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let sample_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label:   Some("gfx sample bind"),
                layout:  &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding:  0,
                        resource: self.uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding:  1,
                        resource: wgpu::BindingResource::TextureView(&color_view),
                    },
                    wgpu::BindGroupEntry {
                        binding:  2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.gfx_targets.insert(
                gf.id,
                GfxTarget {
                    width: gf.width,
                    height: gf.height,
                    color_view,
                    msaa_view,
                    render_bind,
                    sample_bind,
                    vbuf: None,
                    vbuf_cap: 0,
                },
            );
        }
        let target = self.gfx_targets.get_mut(&gf.id).expect("gfx");
        let bytes = bytemuck::cast_slice::<Vertex, u8>(&gf.vertices);
        if bytes.is_empty() {
            return;
        }
        let needed = bytes.len() as u64;
        if target.vbuf_cap < needed {
            let cap = needed.next_power_of_two().max(4096);
            target.vbuf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label:              Some("gfx vb"),
                size:               cap,
                usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            target.vbuf_cap = cap;
        }
        self.queue
            .write_buffer(target.vbuf.as_ref().unwrap(), 0, bytes);
    }

    /// Raster `frame` into the internal canvas texture.
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &DrawFrame,
        canvas_w: f32,
        canvas_h: f32,
        accumulate: bool,
    ) {
        for gf in &frame.graphics {
            self.prepare_gfx(gf);
        }

        let bytes = bytemuck::cast_slice::<Vertex, u8>(&frame.vertices);
        let has_verts = !bytes.is_empty();
        if has_verts {
            let needed = bytes.len() as u64;
            if self.vertex_buf_cap < needed {
                let cap = needed.next_power_of_two().max(4096);
                self.vertex_buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label:              Some("canvas vb"),
                    size:               cap,
                    usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                self.vertex_buf_cap = cap;
            }
            self.queue
                .write_buffer(self.vertex_buf.as_ref().unwrap(), 0, bytes);
        }

        self.set_resolution(canvas_w, canvas_h);

        let load = if accumulate {
            wgpu::LoadOp::Load
        } else if frame.clear || !self.canvas_init {
            wgpu::LoadOp::Clear(frame.bg)
        } else {
            wgpu::LoadOp::Load
        };
        self.canvas_init = true;

        for gf in &frame.graphics {
            let Some(target) = self.gfx_targets.get(&gf.id) else {
                continue;
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:                    Some("gfx raster"),
                color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                    view:           &target.msaa_view,
                    resolve_target: Some(&target.color_view),
                    ops:            wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(gf.bg),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            if let Some(vb) = &target.vbuf {
                if !gf.vertices.is_empty() {
                    pass.set_pipeline(&self.gfx_pipeline);
                    pass.set_bind_group(0, &target.render_bind, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.draw(0..gf.vertices.len() as u32, 0..1);
                }
            }
        }

        if has_verts || frame.clear || accumulate {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:                    Some("canvas raster"),
                color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                    view:           &self.canvas_msaa_view,
                    resolve_target: Some(&self.canvas_texture_view),
                    ops:            wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            if has_verts {
                pass.set_pipeline(&self.canvas_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buf.as_ref().unwrap().slice(..));
                let runs = if frame.runs.is_empty() {
                    None
                } else {
                    Some(&frame.runs[..])
                };
                if let Some(runs) = runs {
                    for run in runs {
                        let bind = if run.tex == 0 {
                            &self.uniform_bind
                        } else {
                            match self.gfx_targets.get(&run.tex) {
                                Some(t) => &t.sample_bind,
                                None => continue,
                            }
                        };
                        pass.set_bind_group(0, bind, &[]);
                        pass.draw(run.start..run.start + run.count, 0..1);
                    }
                } else {
                    pass.set_bind_group(0, &self.uniform_bind, &[]);
                    let count = bytes.len() as u32 / VERTEX_2D_STRIDE as u32;
                    pass.draw(0..count, 0..1);
                }
            }
        }
    }
}

fn create_canvas_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label:           Some("canvas plane texture"),
        size:            wgpu::Extent3d {
            width:                 width.max(1),
            height:                height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format,
        usage:           wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats:    &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}
