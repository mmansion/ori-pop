//! Processing-style drawing API backed by wgpu.
//!
//! Import everything via [`crate::prelude`] and write sketches that look like
//! Processing: call [`size`], [`title`], optionally [`smooth`] in `main()`,
//! then [`run`] with a `draw` function that is called every frame.
//!
//! # Coordinate system
//!
//! Origin is **top-left**, x grows right, y grows down — same as Processing.
//! All positions are in **logical pixels** (DPI-independent).
//!
//! # Example
//!
//! ```no_run
//! use oripop_core::prelude::*;
//!
//! fn main() {
//!     size(800, 600);
//!     title("my sketch");
//!     smooth(4);
//!     run(draw);
//! }
//!
//! fn draw() {
//!     background(20, 20, 30);
//!     stroke(255, 200, 100);
//!     line(100.0, 100.0, 700.0, 500.0);
//! }
//! ```

use std::cell::RefCell;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowId},
};

// ── Vertex ──────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 8,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    _pad: [f32; 2],
}

// ── Transform (2D affine, row-major 2x3) ────────────────
// x' = m00*x + m01*y + m02
// y' = m10*x + m11*y + m12

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn transform_point(m: &[f32; 6], x: f32, y: f32) -> [f32; 2] {
    [
        m[0] * x + m[1] * y + m[2],
        m[3] * x + m[4] * y + m[5],
    ]
}

fn mat_mult(m: &[f32; 6], a: &[f32; 6]) -> [f32; 6] {
    [
        m[0] * a[0] + m[1] * a[3],
        m[0] * a[1] + m[1] * a[4],
        m[0] * a[2] + m[1] * a[5] + m[2],
        m[3] * a[0] + m[4] * a[3],
        m[3] * a[1] + m[4] * a[4],
        m[3] * a[2] + m[4] * a[5] + m[5],
    ]
}

// ── Draw State ──────────────────────────────────────────

struct DrawState {
    stroke_color: [f32; 4],
    stroke_weight: f32,
    has_stroke: bool,
    fill_color: [f32; 4],
    has_fill: bool,
}

impl Default for DrawState {
    fn default() -> Self {
        Self {
            stroke_color: [1.0, 1.0, 1.0, 1.0],
            stroke_weight: 1.0,
            has_stroke: true,
            fill_color: [1.0, 1.0, 1.0, 1.0],
            has_fill: true,
        }
    }
}

// ── Context ─────────────────────────────────────────────

struct Context {
    width: u32,
    height: u32,
    title: String,
    msaa_samples: u32,
    state: DrawState,
    vertices: Vec<Vertex>,
    bg: wgpu::Color,
    frame_count: u64,
    matrix: [f32; 6],
    matrix_stack: Vec<[f32; 6]>,
    mouse_x: f32,
    mouse_y: f32,
    mouse_pressed: bool,
    key_pressed: bool,
    key_code: char,
}

impl Context {
    fn new() -> Self {
        Self {
            width: 400,
            height: 400,
            title: String::from("ori-pop"),
            msaa_samples: 4,
            state: DrawState::default(),
            vertices: Vec::new(),
            bg: wgpu::Color::BLACK,
            frame_count: 0,
            matrix: IDENTITY,
            matrix_stack: Vec::new(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_pressed: false,
            key_pressed: false,
            key_code: '\0',
        }
    }

    fn reset_frame(&mut self) {
        self.vertices.clear();
        self.frame_count += 1;
        self.matrix = IDENTITY;
        self.matrix_stack.clear();
    }

    fn transform_pt(&self, x: f32, y: f32) -> [f32; 2] {
        transform_point(&self.matrix, x, y)
    }
}

thread_local! {
    static CTX: RefCell<Context> = RefCell::new(Context::new());
}

fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
    CTX.with(|c| f(&mut c.borrow_mut()))
}

// ── Public API ──────────────────────────────────────────

/// Set the window dimensions in logical pixels. Call before [`run`].
///
/// Defaults to 400 x 400 if not called.
pub fn size(width: u32, height: u32) {
    with_ctx(|ctx| {
        ctx.width = width;
        ctx.height = height;
    });
}

/// Set the window title. Call before [`run`].
pub fn title(t: &str) {
    with_ctx(|ctx| ctx.title = t.to_string());
}

/// Set anti-aliasing sample count. Valid values: 1 (off), 2, 4, 8.
/// Default is 4. Call before run().
pub fn smooth(samples: u32) {
    let s = match samples {
        0 | 1 => 1,
        2 => 2,
        3..=4 => 4,
        _ => 8,
    };
    with_ctx(|ctx| ctx.msaa_samples = s);
}

/// Clear the canvas to an opaque RGB color. Called each frame before drawing.
///
/// Values are 0–255 per channel.
pub fn background(r: u8, g: u8, b: u8) {
    background_a(r, g, b, 255);
}

/// Clear the canvas to an RGBA color. The alpha channel is 0 (transparent)
/// to 255 (opaque).
pub fn background_a(r: u8, g: u8, b: u8, a: u8) {
    with_ctx(|ctx| {
        ctx.bg = wgpu::Color {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        };
    });
}

/// Set the stroke (outline) color to an opaque RGB value and enable stroke.
pub fn stroke(r: u8, g: u8, b: u8) {
    stroke_a(r, g, b, 255);
}

/// Set the stroke color with alpha transparency and enable stroke.
///
/// Alpha is 0 (fully transparent) to 255 (fully opaque).
pub fn stroke_a(r: u8, g: u8, b: u8, a: u8) {
    with_ctx(|ctx| {
        ctx.state.stroke_color = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0];
        ctx.state.has_stroke = true;
    });
}

/// Disable stroke for subsequent shapes.
pub fn no_stroke() {
    with_ctx(|ctx| ctx.state.has_stroke = false);
}

/// Set the fill color to an opaque RGB value and enable fill.
pub fn fill(r: u8, g: u8, b: u8) {
    fill_a(r, g, b, 255);
}

/// Set the fill color with alpha transparency and enable fill.
///
/// Alpha is 0 (fully transparent) to 255 (fully opaque).
pub fn fill_a(r: u8, g: u8, b: u8, a: u8) {
    with_ctx(|ctx| {
        ctx.state.fill_color = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0];
        ctx.state.has_fill = true;
    });
}

/// Disable fill for subsequent shapes (outlines only).
pub fn no_fill() {
    with_ctx(|ctx| ctx.state.has_fill = false);
}

/// Set the stroke thickness in logical pixels. Default is 1.0.
pub fn stroke_weight(w: f32) {
    with_ctx(|ctx| ctx.state.stroke_weight = w);
}

/// Return how many frames have been drawn since [`run`] started.
///
/// Starts at 1 on the first call to `draw()`.
pub fn frame_count() -> u64 {
    with_ctx(|ctx| ctx.frame_count)
}

/// Current horizontal position of the mouse in logical pixels.
pub fn mouse_x() -> f32 {
    with_ctx(|ctx| ctx.mouse_x)
}

/// Current vertical position of the mouse in logical pixels.
pub fn mouse_y() -> f32 {
    with_ctx(|ctx| ctx.mouse_y)
}

/// True while any mouse button is held down.
pub fn mouse_pressed() -> bool {
    with_ctx(|ctx| ctx.mouse_pressed)
}

/// True while any key is held down.
pub fn key_pressed() -> bool {
    with_ctx(|ctx| ctx.key_pressed)
}

/// The character of the most recently pressed key, or `'\0'` if none.
pub fn key() -> char {
    with_ctx(|ctx| ctx.key_code)
}

/// Push current transform onto the stack. Pair with pop().
pub fn push() {
    with_ctx(|ctx| ctx.matrix_stack.push(ctx.matrix));
}

/// Pop transform from the stack. Pair with push().
pub fn pop() {
    with_ctx(|ctx| {
        if let Some(m) = ctx.matrix_stack.pop() {
            ctx.matrix = m;
        }
    });
}

/// Translate by (dx, dy) in current units.
pub fn translate(dx: f32, dy: f32) {
    with_ctx(|ctx| {
        let t = [1.0, 0.0, dx, 0.0, 1.0, dy];
        ctx.matrix = mat_mult(&ctx.matrix, &t);
    });
}

/// Rotate by angle in radians (counter-clockwise).
pub fn rotate(angle: f32) {
    with_ctx(|ctx| {
        let c = angle.cos();
        let s = angle.sin();
        let r = [c, -s, 0.0, s, c, 0.0];
        ctx.matrix = mat_mult(&ctx.matrix, &r);
    });
}

/// Scale by (sx, sy). Use scale(s) for uniform scale.
pub fn scale(sx: f32, sy: f32) {
    with_ctx(|ctx| {
        let s = [sx, 0.0, 0.0, 0.0, sy, 0.0];
        ctx.matrix = mat_mult(&ctx.matrix, &s);
    });
}

/// Draw a line from (`x1`, `y1`) to (`x2`, `y2`) using the current stroke
/// color and weight. Does nothing if stroke is disabled.
pub fn line(x1: f32, y1: f32, x2: f32, y2: f32) {
    with_ctx(|ctx| {
        if !ctx.state.has_stroke {
            return;
        }
        let color = ctx.state.stroke_color;
        let weight = ctx.state.stroke_weight;

        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.0001 {
            return;
        }
        let nx = -dy / len * weight * 0.5;
        let ny = dx / len * weight * 0.5;

        let p0 = [x1 + nx, y1 + ny];
        let p1 = [x1 - nx, y1 - ny];
        let p2 = [x2 + nx, y2 + ny];
        let p3 = [x2 - nx, y2 - ny];

        ctx.vertices.extend_from_slice(&[
            Vertex { position: p0, color },
            Vertex { position: p1, color },
            Vertex { position: p2, color },
            Vertex { position: p1, color },
            Vertex { position: p3, color },
            Vertex { position: p2, color },
        ]);
    });
}

/// Draw a single point at (`x`, `y`) as a small filled square whose size
/// equals the current stroke weight.
pub fn point(x: f32, y: f32) {
    with_ctx(|ctx| {
        if !ctx.state.has_stroke {
            return;
        }
        let color = ctx.state.stroke_color;
        let half = ctx.state.stroke_weight * 0.5;
        push_filled_rect(ctx, x - half, y - half, ctx.state.stroke_weight, ctx.state.stroke_weight, color);
    });
}

/// Draw a rectangle with its top-left corner at (`x`, `y`) and the given
/// width and height. Respects current fill and stroke settings.
pub fn rect(x: f32, y: f32, w: f32, h: f32) {
    with_ctx(|ctx| {
        if ctx.state.has_fill {
            push_filled_rect(ctx, x, y, w, h, ctx.state.fill_color);
        }
        if ctx.state.has_stroke {
            let color = ctx.state.stroke_color;
            let sw = ctx.state.stroke_weight;
            push_line(ctx, x, y, x + w, y, sw, color);
            push_line(ctx, x + w, y, x + w, y + h, sw, color);
            push_line(ctx, x + w, y + h, x, y + h, sw, color);
            push_line(ctx, x, y + h, x, y, sw, color);
        }
    });
}

/// Draw an ellipse bounded by the rectangle at (`x`, `y`) with the given
/// width and height. Respects current fill and stroke settings.
pub fn ellipse(x: f32, y: f32, w: f32, h: f32) {
    const SEGMENTS: usize = 64;
    with_ctx(|ctx| {
        let rx = w * 0.5;
        let ry = h * 0.5;
        let cx = x + rx;
        let cy = y + ry;

        if ctx.state.has_fill {
            let color = ctx.state.fill_color;
            let step = std::f32::consts::TAU / SEGMENTS as f32;
            let center = ctx.transform_pt(cx, cy);
            for i in 0..SEGMENTS {
                let a0 = step * i as f32;
                let a1 = step * (i + 1) as f32;
                let p0 = ctx.transform_pt(cx + rx * a0.cos(), cy + ry * a0.sin());
                let p1 = ctx.transform_pt(cx + rx * a1.cos(), cy + ry * a1.sin());
                ctx.vertices.extend_from_slice(&[
                    Vertex { position: center, color },
                    Vertex { position: p0, color },
                    Vertex { position: p1, color },
                ]);
            }
        }
        if ctx.state.has_stroke {
            let color = ctx.state.stroke_color;
            let sw = ctx.state.stroke_weight;
            let step = std::f32::consts::TAU / SEGMENTS as f32;
            for i in 0..SEGMENTS {
                let a0 = step * i as f32;
                let a1 = step * (i + 1) as f32;
                let x0 = cx + rx * a0.cos();
                let y0 = cy + ry * a0.sin();
                let x1 = cx + rx * a1.cos();
                let y1 = cy + ry * a1.sin();
                push_line(ctx, x0, y0, x1, y1, sw, color);
            }
        }
    });
}

/// Draw a triangle with vertices at (`x1`,`y1`), (`x2`,`y2`), (`x3`,`y3`).
/// Respects current fill and stroke settings.
pub fn triangle(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
    with_ctx(|ctx| {
        if ctx.state.has_fill {
            let color = ctx.state.fill_color;
            let p1 = ctx.transform_pt(x1, y1);
            let p2 = ctx.transform_pt(x2, y2);
            let p3 = ctx.transform_pt(x3, y3);
            ctx.vertices.extend_from_slice(&[
                Vertex { position: p1, color },
                Vertex { position: p2, color },
                Vertex { position: p3, color },
            ]);
        }
        if ctx.state.has_stroke {
            let color = ctx.state.stroke_color;
            let sw = ctx.state.stroke_weight;
            push_line(ctx, x1, y1, x2, y2, sw, color);
            push_line(ctx, x2, y2, x3, y3, sw, color);
            push_line(ctx, x3, y3, x1, y1, sw, color);
        }
    });
}

fn push_filled_rect(ctx: &mut Context, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let tl = ctx.transform_pt(x, y);
    let tr = ctx.transform_pt(x + w, y);
    let br = ctx.transform_pt(x + w, y + h);
    let bl = ctx.transform_pt(x, y + h);
    ctx.vertices.extend_from_slice(&[
        Vertex { position: tl, color },
        Vertex { position: tr, color },
        Vertex { position: br, color },
        Vertex { position: tl, color },
        Vertex { position: br, color },
        Vertex { position: bl, color },
    ]);
}

fn push_line(ctx: &mut Context, x1: f32, y1: f32, x2: f32, y2: f32, weight: f32, color: [f32; 4]) {
    let [x1, y1] = ctx.transform_pt(x1, y1);
    let [x2, y2] = ctx.transform_pt(x2, y2);
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.0001 {
        return;
    }
    let nx = -dy / len * weight * 0.5;
    let ny = dx / len * weight * 0.5;
    let p0 = [x1 + nx, y1 + ny];
    let p1 = [x1 - nx, y1 - ny];
    let p2 = [x2 + nx, y2 + ny];
    let p3 = [x2 - nx, y2 - ny];
    ctx.vertices.extend_from_slice(&[
        Vertex { position: p0, color },
        Vertex { position: p1, color },
        Vertex { position: p2, color },
        Vertex { position: p1, color },
        Vertex { position: p3, color },
        Vertex { position: p2, color },
    ]);
}

// ── GPU ─────────────────────────────────────────────────

const MIN_SURFACE_PIXELS: u32 = 2;

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    msaa_view: Option<wgpu::TextureView>,
    msaa_samples: u32,
    surface_format: wgpu::TextureFormat,
    scale_factor: f64,
}

fn create_msaa_texture(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32, samples: u32) -> Option<wgpu::TextureView> {
    if samples <= 1 {
        return None;
    }
    Some(
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa texture"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default()),
    )
}

impl Gpu {
    fn resize(&mut self, phys_w: u32, phys_h: u32) {
        let w = phys_w.max(MIN_SURFACE_PIXELS);
        let h = phys_h.max(MIN_SURFACE_PIXELS);
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.msaa_view = create_msaa_texture(&self.device, self.surface_format, w, h, self.msaa_samples);
        let logical_w = w as f64 / self.scale_factor;
        let logical_h = h as f64 / self.scale_factor;
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[Uniforms {
                resolution: [logical_w as f32, logical_h as f32],
                _pad: [0.0; 2],
            }]),
        );
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    fn render(&self, bg: wgpu::Color, vertices: &[Vertex]) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let (target_view, resolve_target) = match &self.msaa_view {
            Some(msaa) => (msaa, Some(&surface_view)),
            None => (&surface_view, None),
        };

        let vertex_buffer = if !vertices.is_empty() {
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex buffer"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }))
        } else {
            None
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oripop render"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oripop pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(bg),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some(ref vb) = vertex_buffer {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

async fn init_gpu(window: Arc<Window>, phys_w: u32, phys_h: u32, logical_w: u32, logical_h: u32, msaa_samples: u32) -> Gpu {
    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(window).expect("create surface");

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .expect("request adapter");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("request device");

    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(caps.formats[0]);
    let present_mode = caps
        .present_modes
        .iter()
        .copied()
        .find(|&m| m == wgpu::PresentMode::Fifo)
        .unwrap_or(caps.present_modes[0]);

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: phys_w,
        height: phys_h,
        present_mode,
        alpha_mode: caps.alpha_modes[0],
        desired_maximum_frame_latency: 2,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("oripop shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    let uniforms = Uniforms {
        resolution: [logical_w as f32, logical_h as f32],
        _pad: [0.0; 2],
    };
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniform buffer"),
        contents: bytemuck::cast_slice(&[uniforms]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oripop bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oripop bind group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("oripop pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("oripop pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::LAYOUT],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: msaa_samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    let msaa_view = create_msaa_texture(&device, format, phys_w, phys_h, msaa_samples);

    let scale_factor = phys_w as f64 / logical_w.max(1) as f64;

    Gpu {
        surface,
        device,
        queue,
        config,
        pipeline,
        uniform_buffer,
        uniform_bind_group,
        msaa_view,
        msaa_samples,
        surface_format: format,
        scale_factor,
    }
}

// ── Logger ──────────────────────────────────────────────

struct Logger;

static LOGGER: Logger = Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!("[{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

// ── Integration API for oripop-3d ───────────────────────
//
// These functions are intentionally NOT re-exported from `prelude`.
// They exist so that oripop-3d's combined runner can share the same
// thread-local drawing state as oripop-core's 2D API.

/// The WGSL source of the 2D drawing shader.
/// oripop-3d embeds this to create a compatible 2D overlay pipeline.
pub const SHADER_2D_WGSL: &str = include_str!("shader.wgsl");

/// Returns the vertex buffer layout for the 2D drawing pipeline.
/// oripop-3d uses this to wire up a matching vertex buffer in its renderer.
pub fn vertex_2d_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    Vertex::LAYOUT
}

/// Read the configured window settings without starting the event loop.
/// Returns `(width, height, title, msaa_samples)`.
pub fn settings() -> (u32, u32, String, u32) {
    with_ctx(|ctx| (ctx.width, ctx.height, ctx.title.clone(), ctx.msaa_samples))
}

/// Reset per-frame 2D state (vertex list, frame counter, matrix stack).
/// Call once at the start of each frame in oripop-3d's event loop.
pub fn begin_frame() {
    with_ctx(|ctx| ctx.reset_frame());
}

/// Drain the accumulated 2D draw data for this frame.
///
/// Returns the background clear colour and raw vertex bytes.
/// Each vertex is 24 bytes: `[f32; 2]` position at offset 0,
/// `[f32; 4]` RGBA colour at offset 8.
pub fn take_2d_vertices() -> (wgpu::Color, Vec<u8>) {
    with_ctx(|ctx| {
        let bg    = ctx.bg;
        let bytes = bytemuck::cast_slice(&ctx.vertices).to_vec();
        ctx.vertices.clear();
        (bg, bytes)
    })
}

/// Update mouse position and button state.
/// Call from oripop-3d's event loop so that `mouse_x()`, `mouse_y()`,
/// and `mouse_pressed()` return correct values inside draw callbacks.
pub fn set_mouse(x: f32, y: f32, pressed: bool) {
    with_ctx(|ctx| {
        ctx.mouse_x       = x;
        ctx.mouse_y       = y;
        ctx.mouse_pressed = pressed;
    });
}

/// Update keyboard state.
/// Call from oripop-3d's event loop so that `key_pressed()` and `key()`
/// return correct values inside draw callbacks.
pub fn set_key(pressed: bool, code: char) {
    with_ctx(|ctx| {
        ctx.key_pressed = pressed;
        if pressed { ctx.key_code = code; }
    });
}

// ── run() ───────────────────────────────────────────────

struct Runner2D {
    draw_fn:      fn(),
    window_attrs: winit::window::WindowAttributes,
    msaa:         u32,
    window:       Option<Arc<Window>>,
    gpu:          Option<Gpu>,
}

impl ApplicationHandler for Runner2D {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(self.window_attrs.clone())
                .expect("create window"),
        );
        let phys    = window.inner_size();
        let (w, h)  = with_ctx(|ctx| (ctx.width, ctx.height));
        self.gpu    = Some(pollster::block_on(init_gpu(
            Arc::clone(&window), phys.width, phys.height, w, h, self.msaa,
        )));
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: WindowId,
        event: WindowEvent,
    ) {
        let (Some(window), Some(gpu)) = (self.window.as_ref(), self.gpu.as_mut()) else { return };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(sz) => {
                gpu.resize(sz.width, sz.height);
                window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                with_ctx(|ctx| ctx.reset_frame());
                (self.draw_fn)();
                let (bg, vertices) =
                    with_ctx(|ctx| (ctx.bg, std::mem::take(&mut ctx.vertices)));

                match gpu.render(bg, &vertices) {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost) => {
                        gpu.reconfigure();
                        window.request_redraw();
                    }
                    Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Timeout) => {}
                    Err(e) => log::error!("render error: {}", e),
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                with_ctx(|ctx| {
                    ctx.mouse_x = position.x as f32 / gpu.scale_factor as f32;
                    ctx.mouse_y = position.y as f32 / gpu.scale_factor as f32;
                });
            }

            WindowEvent::MouseInput { state, .. } => {
                with_ctx(|ctx| {
                    ctx.mouse_pressed = state == ElementState::Pressed;
                });
            }

            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let pressed = key_event.state == ElementState::Pressed;
                with_ctx(|ctx| {
                    ctx.key_pressed = pressed;
                    if pressed {
                        if let winit::keyboard::Key::Character(ref c) = key_event.logical_key {
                            if let Some(ch) = c.chars().next() {
                                ctx.key_code = ch;
                            }
                        }
                    }
                });
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Open the window and start the draw loop.
///
/// `draw_fn` is called once per frame. Configure the window with [`size`],
/// [`title`], and [`smooth`] *before* calling `run`.
///
/// This function blocks until the window is closed.
pub fn run(draw_fn: fn()) {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Warn);

    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        SetProcessDpiAwarenessContext(-2);
    }

    let (width, height, win_title, msaa) =
        with_ctx(|ctx| (ctx.width, ctx.height, ctx.title.clone(), ctx.msaa_samples));

    let window_attrs = Window::default_attributes()
        .with_title(win_title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64));

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = Runner2D { draw_fn, window_attrs, msaa, window: None, gpu: None };
    event_loop.run_app(&mut app).expect("event loop error");
}
