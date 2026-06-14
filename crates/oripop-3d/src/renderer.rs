//! wgpu renderer that drives GPU passes per frame:
//!   1. Canvas raster   — DrawFrame → plane texture (graphics + textured runs)
//!   2. Compute pass    — generative texture (domain-warped FBM, GPU only)
//!   3. 3D render pass  — meshes with depth, MVP transform, texture sampling
//!
//! A fourth egui pass is driven externally by `lib.rs` via `render_egui()`,
//! keeping the egui renderer separate from the core pipeline.

use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{
    mesh::{MeshKind, Vertex3D},
    scene::{ObjectTexture, Scene3D, STIPPLE_CANVAS_SIZE},
};

use oripop_canvas::draw::{DrawFrame, ResolvedCanvasFormat};

use crate::canvas_raster::CanvasRaster;

// ── GPU-side uniform structs ─────────────────────────────────────────────────
//
// Layout is verified to match the WGSL structs in shader3d.wgsl and
// texture_gen.wgsl.  Every `vec3` in WGSL becomes `[f32; 4]` here so that
// the 16-byte alignment of vec3 (in uniform blocks) is satisfied without
// relying on the Rust compiler inserting padding.

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms3D {
    mvp:        [[f32; 4]; 4], // offset   0, 64 B
    model:      [[f32; 4]; 4], // offset  64, 64 B
    camera_pos: [f32; 4],      // offset 128, 16 B  (.xyz used)
    light_dir:  [f32; 4],      // offset 144, 16 B  (.xyz used)
    time:       f32,           // offset 160,  4 B
    _pad:       [f32; 3],      // offset 164, 12 B
}                              // total: 176 B

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GenParamsGpu {
    time:          f32, // offset  0
    seed:          f32, // offset  4
    frequency:     f32, // offset  8
    octaves:       u32, // offset 12
    warp_strength: f32, // offset 16
    _pad0:         f32, // offset 20
    _pad1:         f32, // offset 24
    _pad2:         f32, // offset 28
}                       // total: 32 B

// ── GpuMesh (pre-uploaded buffer pair) ───────────────────────────────────────

struct GpuMesh {
    vb:          wgpu::Buffer,
    ib:          wgpu::Buffer,
    index_count: u32,
}

impl GpuMesh {
    fn upload(device: &wgpu::Device, mesh: &crate::mesh::Mesh) -> Self {
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("mesh vb"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage:    wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("mesh ib"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage:    wgpu::BufferUsages::INDEX,
        });
        Self { vb, ib, index_count: mesh.indices.len() as u32 }
    }
}

// ── Main renderer ─────────────────────────────────────────────────────────────

#[allow(dead_code)] // fields kept for ownership / GPU resource lifetime
pub(crate) struct Renderer {
    // Surface
    surface:        wgpu::Surface<'static>,
    pub device:     wgpu::Device,
    pub queue:      wgpu::Queue,
    config:         wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,
    pub scale_factor: f64,

    // MSAA
    msaa_view:    Option<wgpu::TextureView>,
    msaa_samples: u32,

    // Depth buffer (used by the 3D pass only)
    depth_view: wgpu::TextureView,

    // Generative texture (written by compute, read by 3D pass)
    gen_texture_view: wgpu::TextureView,
    gen_params_buf:   wgpu::Buffer,
    compute_pipeline: wgpu::ComputePipeline,
    compute_bg:       wgpu::BindGroup,
    gen_tex_size:     u32,

    // 3D render pipeline
    pipeline_3d:          wgpu::RenderPipeline,
    uniform_3d_buf:       wgpu::Buffer,
    uniform_3d_bg_layout: wgpu::BindGroupLayout,
    uniform_3d_bg:        wgpu::BindGroup,
    texture_bg:           wgpu::BindGroup,
    texture_bg_layout:    wgpu::BindGroupLayout,
    sampler:              wgpu::Sampler,
    /// Bind group sampling the canvas plane texture ([`ObjectTexture::Canvas`]).
    texture_bg_canvas:    wgpu::BindGroup,
    canvas_tex_w:         u32,
    canvas_tex_h:         u32,
    canvas_gpu_format:    wgpu::TextureFormat,
    uniform_slot_size:    u64,

    // Pre-built mesh primitives
    meshes: [GpuMesh; 3], // indexed by MeshKind

    /// Full canvas raster path (graphics targets + textured draw runs).
    canvas_raster: CanvasRaster,
}

const MAX_OBJECTS:    u64 = 256;
const GEN_TEX_SIZE:   u32 = 512;

fn depth_format() -> wgpu::TextureFormat { wgpu::TextureFormat::Depth32Float }

fn create_depth_view(device: &wgpu::Device, w: u32, h: u32, samples: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label:                 Some("depth"),
            size:                  wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count:       1,
            sample_count:          samples,
            dimension:             wgpu::TextureDimension::D2,
            format:                depth_format(),
            usage:                 wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats:          &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_msaa_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    w: u32, h: u32,
    samples: u32,
) -> Option<wgpu::TextureView> {
    if samples <= 1 { return None; }
    Some(
        device
            .create_texture(&wgpu::TextureDescriptor {
                label:           Some("msaa"),
                size:            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    samples,
                dimension:       wgpu::TextureDimension::D2,
                format,
                usage:           wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats:    &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default()),
    )
}

fn resolved_to_gpu_format(resolved: ResolvedCanvasFormat) -> wgpu::TextureFormat {
    match resolved {
        ResolvedCanvasFormat::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        ResolvedCanvasFormat::Float => wgpu::TextureFormat::Rgba16Float,
    }
}

fn create_canvas_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    canvas_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label:   Some("texture bg canvas"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding:  0,
                resource: wgpu::BindingResource::TextureView(canvas_view),
            },
            wgpu::BindGroupEntry {
                binding:  1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

impl Renderer {
    pub async fn init(
        window:    Arc<Window>,
        phys_w:    u32,
        phys_h:    u32,
        logical_w: u32,
        _logical_h: u32,
        msaa:      u32,
    ) -> Self {
        let instance = wgpu::Instance::default();
        let surface  = instance.create_surface(window).expect("create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("request adapter");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label:                None,
                required_features:    wgpu::Features::empty(),
                required_limits:      wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints:         Default::default(),
                trace:                wgpu::Trace::Off,
            })
            .await
            .expect("request device");

        let caps   = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let present_mode = caps.present_modes.iter().copied()
            .find(|&m| m == wgpu::PresentMode::Fifo)
            .unwrap_or(caps.present_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            // COPY_SRC allows reading the surface texture back to CPU for
            // screenshot and video recording capture.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width:                       phys_w,
            height:                      phys_h,
            present_mode,
            alpha_mode:                  caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats:                vec![],
        };
        surface.configure(&device, &config);

        let scale_factor = phys_w as f64 / logical_w.max(1) as f64;

        // ── Generative texture ─────────────────────────────────────────────

        let gen_texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("gen texture"),
            size:            wgpu::Extent3d { width: GEN_TEX_SIZE, height: GEN_TEX_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba16Float,
            usage:           wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats:    &[],
        });
        let gen_texture_view = gen_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let gen_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("gen params"),
            size:               std::mem::size_of::<GenParamsGpu>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Compute pipeline ───────────────────────────────────────────────

        let compute_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("compute bg layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty:         wgpu::BindingType::StorageTexture {
                        access:         wgpu::StorageTextureAccess::WriteOnly,
                        format:         wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty:         wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   wgpu::BufferSize::new(std::mem::size_of::<GenParamsGpu>() as u64),
                    },
                    count: None,
                },
            ],
        });

        let compute_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("compute bg"),
            layout:  &compute_bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&gen_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: gen_params_buf.as_entire_binding(),
                },
            ],
        });

        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("texture gen shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("texture_gen.wgsl").into()),
        });

        let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("compute layout"),
            bind_group_layouts:   &[&compute_bg_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label:               Some("texture gen"),
            layout:              Some(&compute_pipeline_layout),
            module:              &compute_shader,
            entry_point:         Some("cs_main"),
            compilation_options: Default::default(),
            cache:               None,
        });

        // ── Sampler + texture bind group (for the 3D render pipeline) ──────

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:         Some("gen sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter:    wgpu::FilterMode::Linear,
            min_filter:    wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("texture bg layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty:         wgpu::BindingType::Texture {
                        multisampled:  false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type:   wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count:      None,
                },
            ],
        });

        let texture_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("texture bg"),
            layout:  &texture_bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&gen_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // ── 3D uniform buffer (dynamic, one slot per object) ───────────────

        let alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
        let struct_sz = std::mem::size_of::<Uniforms3D>() as u64;
        let slot_size = (struct_sz + alignment - 1) / alignment * alignment;

        let uniform_3d_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("3d uniforms"),
            size:               slot_size * MAX_OBJECTS,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_3d_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("3d uniform bg layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty:         wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size:   wgpu::BufferSize::new(struct_sz),
                },
                count: None,
            }],
        });

        let uniform_3d_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("3d uniform bg"),
            layout:  &uniform_3d_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_3d_buf,
                    offset: 0,
                    size:   wgpu::BufferSize::new(struct_sz),
                }),
            }],
        });

        // ── 3D render pipeline ─────────────────────────────────────────────

        let shader_3d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("3d shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader3d.wgsl").into()),
        });

        let pipeline_3d_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("3d pipeline layout"),
            bind_group_layouts:   &[&uniform_3d_bg_layout, &texture_bg_layout],
            push_constant_ranges: &[],
        });

        let pipeline_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("3d pipeline"),
            layout: Some(&pipeline_3d_layout),
            vertex: wgpu::VertexState {
                module:              &shader_3d,
                entry_point:         Some("vs_main"),
                buffers:             &[Vertex3D::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader_3d,
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
                cull_mode:          Some(wgpu::Face::Back),
                strip_index_format: None,
                polygon_mode:       wgpu::PolygonMode::Fill,
                unclipped_depth:    false,
                conservative:       false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format:              depth_format(),
                depth_write_enabled: true,
                depth_compare:       wgpu::CompareFunction::Less,
                stencil:             wgpu::StencilState::default(),
                bias:                wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: msaa, mask: !0, alpha_to_coverage_enabled: false },
            multiview:   None,
            cache:       None,
        });

        // ── Pre-built mesh primitives ───────────────────────────────────────

        let meshes = [
            GpuMesh::upload(&device, &MeshKind::Sphere.build()),
            GpuMesh::upload(&device, &MeshKind::Cube.build()),
            GpuMesh::upload(&device, &MeshKind::Plane.build()),
        ];

        let canvas_raster = CanvasRaster::new(device.clone(), queue.clone());
        let canvas_gpu_format = wgpu::TextureFormat::Rgba8UnormSrgb;

        // ── MSAA + depth ───────────────────────────────────────────────────

        let msaa_view  = create_msaa_view(&device, format, phys_w, phys_h, msaa);
        let depth_view = create_depth_view(&device, phys_w, phys_h, msaa);

        let texture_bg_canvas = create_canvas_bind_group(
            &device,
            &texture_bg_layout,
            &sampler,
            canvas_raster.canvas_texture_view(),
        );

        Self {
            surface,
            device,
            queue,
            config,
            surface_format: format,
            scale_factor,
            msaa_view,
            msaa_samples: msaa,
            depth_view,
            gen_texture_view,
            gen_params_buf,
            compute_pipeline,
            compute_bg,
            gen_tex_size: GEN_TEX_SIZE,
            pipeline_3d,
            uniform_3d_buf,
            uniform_3d_bg_layout,
            uniform_3d_bg,
            texture_bg,
            texture_bg_layout,
            sampler,
            texture_bg_canvas,
            canvas_tex_w: 1,
            canvas_tex_h: 1,
            canvas_gpu_format,
            uniform_slot_size: slot_size,
            meshes,
            canvas_raster,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Physical framebuffer size in pixels (updated on every resize).
    pub fn phys_size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    // ── Resize ────────────────────────────────────────────────────────────────

    pub fn resize(&mut self, phys_w: u32, phys_h: u32) {
        let w = phys_w.max(2);
        let h = phys_h.max(2);
        self.config.width  = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.msaa_view  = create_msaa_view(&self.device, self.surface_format, w, h, self.msaa_samples);
        self.depth_view = create_depth_view(&self.device, w, h, self.msaa_samples);
    }

    fn ensure_canvas_target(&mut self, width: u32, height: u32, resolved: ResolvedCanvasFormat) {
        let format = resolved_to_gpu_format(resolved);
        let w = width.max(1);
        let h = height.max(1);
        if self.canvas_tex_w == w && self.canvas_tex_h == h && self.canvas_gpu_format == format {
            return;
        }
        self.canvas_tex_w = w;
        self.canvas_tex_h = h;
        self.canvas_gpu_format = format;
        self.canvas_raster.ensure_canvas(w, h, resolved);
        self.texture_bg_canvas = create_canvas_bind_group(
            &self.device,
            &self.texture_bg_layout,
            &self.sampler,
            self.canvas_raster.canvas_texture_view(),
        );
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    // ── Render ────────────────────────────────────────────────────────────────

    /// Render one frame: canvas raster → compute → 3D (with MSAA resolve).
    ///
    /// Returns the acquired [`wgpu::SurfaceTexture`] **without presenting** it,
    /// so the caller can composite additional passes (e.g. egui) before calling
    /// `output.present()` exactly once per frame.
    pub fn render(
        &mut self,
        scene: &Scene3D,
        frame: &DrawFrame,
    ) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        let output       = self.surface.get_current_texture()?;
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let resolved = oripop_canvas::draw::resolved_canvas_format();
        let (lw, lh, _, _, density) = oripop_canvas::draw::settings();
        let canvas_w = (lw as f32 * density as f32).max(1.0);
        let canvas_h = (lh as f32 * density as f32).max(1.0);
        let tex_w = canvas_w as u32;
        let tex_h = canvas_h as u32;

        let need_canvas = scene.canvas_plane
            || scene
                .objects
                .iter()
                .any(|o| o.visible && o.texture == ObjectTexture::Canvas)
            || !frame.vertices.is_empty()
            || !frame.graphics.is_empty()
            || frame.clear;

        if need_canvas {
            self.ensure_canvas_target(tex_w, tex_h, resolved);
        }

        // ── Write gen-params uniform ───────────────────────────────────────

        let gp = GenParamsGpu {
            time:          scene.time,
            seed:          scene.gen.seed,
            frequency:     scene.gen.frequency,
            octaves:       scene.gen.octaves,
            warp_strength: scene.gen.warp_strength,
            _pad0: 0.0, _pad1: 0.0, _pad2: 0.0,
        };
        self.queue.write_buffer(&self.gen_params_buf, 0, bytemuck::bytes_of(&gp));

        // Legacy manual CPU stipple upload (migration path for sketch 10).
        let legacy_bytes = (STIPPLE_CANVAS_SIZE * STIPPLE_CANVAS_SIZE * 4) as usize;
        let stipple_modified = scene.stipple_canvas.len() == legacy_bytes
            && scene
                .stipple_canvas
                .chunks_exact(4)
                .any(|px| px != [10, 10, 14, 255]);
        let need_legacy = stipple_modified
            && scene
                .objects
                .iter()
                .any(|o| o.visible && o.texture == ObjectTexture::Canvas);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oripop-3d frame"),
        });

        if need_legacy {
            self.ensure_canvas_target(STIPPLE_CANVAS_SIZE, STIPPLE_CANVAS_SIZE, ResolvedCanvasFormat::Srgb);
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture:   self.canvas_raster.canvas_texture(),
                    mip_level: 0,
                    origin:    wgpu::Origin3d::ZERO,
                    aspect:    wgpu::TextureAspect::All,
                },
                &scene.stipple_canvas,
                wgpu::TexelCopyBufferLayout {
                    offset:         0,
                    bytes_per_row:  Some(4 * STIPPLE_CANVAS_SIZE),
                    rows_per_image: Some(STIPPLE_CANVAS_SIZE),
                },
                wgpu::Extent3d {
                    width:                 STIPPLE_CANVAS_SIZE,
                    height:                STIPPLE_CANVAS_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }

        if need_canvas {
            self.canvas_raster.encode(
                &mut encoder,
                frame,
                canvas_w,
                canvas_h,
                need_legacy,
            );
        }

        // ── Write gen-params uniform ───────────────────────────────────────

        let aspect    = scene.aspect();
        let view_proj = scene.camera.view_proj(aspect);
        let n_objects = scene.objects.len();

        if n_objects > 0 {
            let buf_size = self.uniform_slot_size as usize * n_objects;
            let mut data = vec![0u8; buf_size];

            for (i, obj) in scene.objects.iter().enumerate() {
                let mvp   = view_proj * obj.transform;
                let eye   = scene.camera.eye;
                let light = scene.light_dir;
                let u = Uniforms3D {
                    mvp:        mvp.to_cols_array_2d(),
                    model:      obj.transform.to_cols_array_2d(),
                    camera_pos: [eye.x,   eye.y,   eye.z,   1.0],
                    light_dir:  [light.x, light.y, light.z, 0.0],
                    time:       scene.time,
                    _pad:       [0.0; 3],
                };
                let src    = bytemuck::bytes_of(&u);
                let offset = i * self.uniform_slot_size as usize;
                data[offset..offset + src.len()].copy_from_slice(src);
            }
            self.queue.write_buffer(&self.uniform_3d_buf, 0, &data);
        }

        let clear_bg = if frame.clear {
            frame.bg
        } else {
            wgpu::Color { r: 0.04, g: 0.04, b: 0.055, a: 1.0 }
        };

        // ── Write per-object 3D uniforms ───────────────────────────────────

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label:            Some("texture gen"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.compute_bg, &[]);
            let tiles = (self.gen_tex_size + 7) / 8;
            cpass.dispatch_workgroups(tiles, tiles, 1);
        }

        // ── Pass 2: 3D render + MSAA resolve ───────────────────────────────

        let (color_target_3d, resolve_3d) = match &self.msaa_view {
            Some(msaa) => (msaa as &wgpu::TextureView, Some(&surface_view)),
            None       => (&surface_view,              None),
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:                    Some("3d pass"),
                color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                    view:           color_target_3d,
                    resolve_target: resolve_3d,
                    ops:            wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(clear_bg),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view:       &self.depth_view,
                    depth_ops:  Some(wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes:    None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline_3d);

            for (i, obj) in scene.objects.iter().enumerate() {
                if !obj.visible {
                    continue;
                }
                let tex_bg = match obj.texture {
                    ObjectTexture::Gen => &self.texture_bg,
                    ObjectTexture::Canvas => &self.texture_bg_canvas,
                };
                pass.set_bind_group(1, tex_bg, &[]);

                let dyn_offset = (i as u64 * self.uniform_slot_size) as u32;
                pass.set_bind_group(0, &self.uniform_3d_bg, &[dyn_offset]);

                let mesh = &self.meshes[obj.mesh as usize];
                pass.set_vertex_buffer(0, mesh.vb.slice(..));
                pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        Ok(output)
    }

    /// Composite an egui frame onto `output` acquired by a preceding [`render`] call.
    ///
    /// Takes **owned** tessellated data so that all internal references have the
    /// function's own lifetime — this satisfies egui_wgpu 0.33's `RenderPass<'static>`
    /// requirement without any caller-side `'static` constraint.
    ///
    /// Does **not** call `output.present()`.  The caller presents exactly once,
    /// after all passes for the frame have been submitted.
    pub fn render_egui(
        &self,
        output:         &wgpu::SurfaceTexture,
        egui_renderer:  &mut egui_wgpu::Renderer,
        paint_jobs:      Vec<egui::ClippedPrimitive>,
        textures_delta:  egui::TexturesDelta,
        screen:          egui_wgpu::ScreenDescriptor,
    ) {
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        for (id, delta) in &textures_delta.set {
            egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }

        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("egui frame") },
        );

        // update_buffers returns extra command buffers generated by any
        // CallbackTrait::prepare/finish_prepare implementations (paint callbacks).
        // Submit them alongside the main encoder so their GPU work completes first.
        let staging_cmds = egui_renderer.update_buffers(
            &self.device, &self.queue, &mut encoder, &paint_jobs, &screen,
        );

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            // egui_wgpu 0.33 requires RenderPass<'static>; wgpu 27 provides
            // forget_lifetime() for exactly this use case.  The explicit drop below
            // ensures the pass is released before encoder.finish().
            let mut pass_static = pass.forget_lifetime();
            egui_renderer.render(&mut pass_static, &paint_jobs, &screen);
            drop(pass_static);
        }

        // Submit staging buffers first (from callbacks), then the render encoder.
        self.queue.submit(staging_cmds.into_iter().chain(std::iter::once(encoder.finish())));

        for id in &textures_delta.free {
            egui_renderer.free_texture(id);
        }
    }
}
