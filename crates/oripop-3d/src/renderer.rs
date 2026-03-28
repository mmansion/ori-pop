//! wgpu renderer that drives three passes per frame:
//!   1. Compute pass  — generative texture (domain-warped FBM, GPU only)
//!   2. 3D render pass — meshes with depth, MVP transform, texture sampling
//!   3. 2D overlay pass — alpha-blended 2D vertices from oripop-core

use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{mesh::{MeshKind, Vertex3D}, scene::Scene3D};

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
struct Uniforms2D {
    resolution: [f32; 2], // offset 0, 8 B
    _pad:       [f32; 2], // offset 8, 8 B
}

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
    surface_format: wgpu::TextureFormat,
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
    uniform_slot_size:    u64,

    // Pre-built mesh primitives
    meshes: [GpuMesh; 3], // indexed by MeshKind

    // 2D overlay pipeline (reuses oripop-core's WGSL shader)
    pipeline_2d:    wgpu::RenderPipeline,
    uniform_2d_buf: wgpu::Buffer,
    uniform_2d_bg:  wgpu::BindGroup,
}

const MAX_OBJECTS:    u64 = 256;
const GEN_TEX_SIZE:   u32 = 512;
const VERTEX_2D_STRIDE: u32 = 24; // [f32;2] position + [f32;4] color

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

impl Renderer {
    pub async fn init(
        window:    Arc<Window>,
        phys_w:    u32,
        phys_h:    u32,
        logical_w: u32,
        logical_h: u32,
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
            usage:                       wgpu::TextureUsages::RENDER_ATTACHMENT,
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

        // ── 2D overlay pipeline ────────────────────────────────────────────

        let shader_2d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("2d overlay shader"),
            source: wgpu::ShaderSource::Wgsl(oripop_core::draw::SHADER_2D_WGSL.into()),
        });

        let uniform_2d_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("2d uniforms"),
            contents: bytemuck::bytes_of(&Uniforms2D {
                resolution: [logical_w as f32, logical_h as f32],
                _pad:       [0.0; 2],
            }),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_2d_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("2d bg layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty:         wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        });

        let uniform_2d_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("2d bg"),
            layout:  &uniform_2d_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: uniform_2d_buf.as_entire_binding(),
            }],
        });

        let pipeline_2d_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("2d pipeline layout"),
            bind_group_layouts:   &[&uniform_2d_bg_layout],
            push_constant_ranges: &[],
        });

        let pipeline_2d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("2d overlay pipeline"),
            layout: Some(&pipeline_2d_layout),
            vertex: wgpu::VertexState {
                module:              &shader_2d,
                entry_point:         Some("vs_main"),
                buffers:             &[oripop_core::draw::vertex_2d_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader_2d,
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
            multisample:   wgpu::MultisampleState { count: msaa, mask: !0, alpha_to_coverage_enabled: false },
            multiview:     None,
            cache:         None,
        });

        // ── MSAA + depth ───────────────────────────────────────────────────

        let msaa_view  = create_msaa_view(&device, format, phys_w, phys_h, msaa);
        let depth_view = create_depth_view(&device, phys_w, phys_h, msaa);

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
            uniform_slot_size: slot_size,
            meshes,
            pipeline_2d,
            uniform_2d_buf,
            uniform_2d_bg,
        }
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

    pub fn update_2d_resolution(&self, logical_w: f32, logical_h: f32) {
        self.queue.write_buffer(
            &self.uniform_2d_buf,
            0,
            bytemuck::bytes_of(&Uniforms2D { resolution: [logical_w, logical_h], _pad: [0.0; 2] }),
        );
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    // ── Render ────────────────────────────────────────────────────────────────

    pub fn render(
        &self,
        scene:       &Scene3D,
        bg:          wgpu::Color,
        vertices_2d: &[u8],
    ) -> Result<(), wgpu::SurfaceError> {
        let output       = self.surface.get_current_texture()?;
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

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

        // ── Write per-object 3D uniforms ───────────────────────────────────

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

        // ── Command encoder ────────────────────────────────────────────────

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oripop-3d frame"),
        });

        // ── Pass 1: compute — generate texture ─────────────────────────────

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

        // ── Pass 2: 3D render ──────────────────────────────────────────────

        let (color_target_3d, resolve_3d) = match &self.msaa_view {
            Some(msaa) => (msaa as &wgpu::TextureView, None),
            None       => (&surface_view,              None),
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:                    Some("3d pass"),
                color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                    view:           color_target_3d,
                    resolve_target: resolve_3d,
                    ops:            wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(bg),
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
            pass.set_bind_group(1, &self.texture_bg, &[]);

            for (i, obj) in scene.objects.iter().enumerate() {
                let dyn_offset = (i as u64 * self.uniform_slot_size) as u32;
                pass.set_bind_group(0, &self.uniform_3d_bg, &[dyn_offset]);

                let mesh = &self.meshes[obj.mesh as usize];
                pass.set_vertex_buffer(0, mesh.vb.slice(..));
                pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        // ── Pass 3: 2D overlay + MSAA resolve ─────────────────────────────
        //
        // This pass always runs so that the MSAA content is resolved to the
        // surface even when there are no 2D draw calls.

        let vb_2d = (!vertices_2d.is_empty()).then(|| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("2d vb"),
                contents: vertices_2d,
                usage:    wgpu::BufferUsages::VERTEX,
            })
        });

        let (color_target_2d, resolve_2d) = match &self.msaa_view {
            Some(msaa) => (msaa as &wgpu::TextureView, Some(&surface_view)),
            None       => (&surface_view,              None),
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:                    Some("2d overlay + resolve"),
                color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                    view:           color_target_2d,
                    resolve_target: resolve_2d,
                    ops:            wgpu::Operations {
                        load:  wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });

            if let Some(ref vb) = vb_2d {
                let count = vertices_2d.len() as u32 / VERTEX_2D_STRIDE;
                pass.set_pipeline(&self.pipeline_2d);
                pass.set_bind_group(0, &self.uniform_2d_bg, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..count, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
