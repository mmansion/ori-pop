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
//! use oripop_canvas::prelude::*;
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

use lyon::math::{point as lpt, vector as lvec, Angle, Box2D};
use lyon::path::{Path, Winding};
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, LineCap,
    LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor,
    VertexBuffers,
};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowId},
};

// ── Vertex (format v2) ──────────────────────────────────
//
// 36 bytes: position, RGBA color, UV, texture slot. Slot 0.0 = solid color;
// slot 1.0 = multiply by the bound 2D texture (images / glyph atlas /
// offscreen canvases). See `shader.wgsl`.

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
    tex: f32,
}

/// Size in bytes of one 2D vertex as produced by [`take_2d_vertices`].
pub const VERTEX_2D_STRIDE: usize = std::mem::size_of::<Vertex>();

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
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32,
            },
        ],
    };
}

/// Builds solid-color vertices for the lyon tessellators.
struct SolidCtor {
    color: [f32; 4],
}

impl FillVertexConstructor<Vertex> for SolidCtor {
    fn new_vertex(&mut self, v: FillVertex) -> Vertex {
        Vertex {
            position: v.position().to_array(),
            color: self.color,
            uv: [0.0, 0.0],
            tex: 0.0,
        }
    }
}

impl StrokeVertexConstructor<Vertex> for SolidCtor {
    fn new_vertex(&mut self, v: StrokeVertex) -> Vertex {
        Vertex {
            position: v.position().to_array(),
            color: self.color,
            uv: [0.0, 0.0],
            tex: 0.0,
        }
    }
}

/// Expand an indexed tessellation result into the flat (non-indexed)
/// triangle-list vertex stream the 2D pipeline consumes.
fn append_indexed(out: &mut Vec<Vertex>, buf: &VertexBuffers<Vertex, u32>) {
    out.reserve(buf.indices.len());
    for &i in &buf.indices {
        out.push(buf.vertices[i as usize]);
    }
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
    /// When false, the event loop does not spin at max FPS; redraws are requested from input and resize only.
    continuous_redraw: bool,
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
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
}

impl Context {
    fn new() -> Self {
        Self {
            width: 400,
            height: 400,
            title: String::from("ori-pop"),
            msaa_samples: 4,
            continuous_redraw: true,
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
            fill_tess: FillTessellator::new(),
            stroke_tess: StrokeTessellator::new(),
        }
    }

    fn reset_frame(&mut self) {
        self.vertices.clear();
        self.frame_count += 1;
        self.matrix = IDENTITY;
        self.matrix_stack.clear();
    }

    /// Apply the current transform to a locally-built path.
    ///
    /// Paths are transformed *before* stroking, so stroke weight stays in
    /// canvas pixels regardless of `scale()` (matching previous behavior).
    fn apply_transform(&self, path: Path) -> Path {
        if self.matrix == IDENTITY {
            return path;
        }
        let m = &self.matrix;
        // euclid Transform2D: x' = m11*x + m21*y + m31, y' = m12*x + m22*y + m32
        let t = lyon::math::Transform::new(m[0], m[3], m[1], m[4], m[2], m[5]);
        path.transformed(&t)
    }

    fn fill_path(&mut self, path: &Path, color: [f32; 4]) {
        let mut buf: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let result = self.fill_tess.tessellate_path(
            path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut buf, SolidCtor { color }),
        );
        if result.is_ok() {
            append_indexed(&mut self.vertices, &buf);
        }
    }

    fn stroke_path(&mut self, path: &Path, color: [f32; 4]) {
        let weight = self.state.stroke_weight;
        if weight <= 0.0 {
            return;
        }
        // Processing defaults: round caps, miter joins. Exposed as
        // stroke_cap()/stroke_join() state in a later tier.
        let options = StrokeOptions::default()
            .with_line_width(weight)
            .with_start_cap(LineCap::Round)
            .with_end_cap(LineCap::Round)
            .with_line_join(LineJoin::Miter);
        let mut buf: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let result = self.stroke_tess.tessellate_path(
            path,
            &options,
            &mut BuffersBuilder::new(&mut buf, SolidCtor { color }),
        );
        if result.is_ok() {
            append_indexed(&mut self.vertices, &buf);
        }
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

/// When `true` (default), the window redraws every frame at display rate (good for animation).
/// When `false`, redraws run only after input, resize, or an explicit need — much lower CPU
/// for interactive editors. Call before [`run`].
pub fn redraw_continuous(enabled: bool) {
    with_ctx(|ctx| ctx.continuous_redraw = enabled);
}

/// Current value set by [`redraw_continuous`] (default `true`). Used by `run3d` to avoid idle redraws.
pub fn continuous_redraw_enabled() -> bool {
    with_ctx(|ctx| ctx.continuous_redraw)
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
        let dx = x2 - x1;
        let dy = y2 - y1;
        if dx * dx + dy * dy < 1e-8 {
            return;
        }
        let mut b = Path::builder();
        b.begin(lpt(x1, y1));
        b.line_to(lpt(x2, y2));
        b.end(false);
        let path = ctx.apply_transform(b.build());
        let color = ctx.state.stroke_color;
        ctx.stroke_path(&path, color);
    });
}

/// Draw a single point at (`x`, `y`) as a filled dot whose diameter equals
/// the current stroke weight.
pub fn point(x: f32, y: f32) {
    with_ctx(|ctx| {
        if !ctx.state.has_stroke {
            return;
        }
        let half = ctx.state.stroke_weight * 0.5;
        if half <= 0.0 {
            return;
        }
        let mut b = Path::builder();
        b.add_circle(lpt(x, y), half, Winding::Positive);
        let path = ctx.apply_transform(b.build());
        let color = ctx.state.stroke_color;
        ctx.fill_path(&path, color);
    });
}

/// Draw a rectangle with its top-left corner at (`x`, `y`) and the given
/// width and height. Respects current fill and stroke settings.
pub fn rect(x: f32, y: f32, w: f32, h: f32) {
    with_ctx(|ctx| {
        let mut b = Path::builder();
        b.add_rectangle(
            &Box2D::new(lpt(x, y), lpt(x + w, y + h)),
            Winding::Positive,
        );
        let path = ctx.apply_transform(b.build());
        if ctx.state.has_fill {
            let color = ctx.state.fill_color;
            ctx.fill_path(&path, color);
        }
        if ctx.state.has_stroke {
            let color = ctx.state.stroke_color;
            ctx.stroke_path(&path, color);
        }
    });
}

/// Draw an ellipse bounded by the rectangle at (`x`, `y`) with the given
/// width and height. Respects current fill and stroke settings.
pub fn ellipse(x: f32, y: f32, w: f32, h: f32) {
    with_ctx(|ctx| {
        let rx = w * 0.5;
        let ry = h * 0.5;
        let mut b = Path::builder();
        b.add_ellipse(
            lpt(x + rx, y + ry),
            lvec(rx, ry),
            Angle::radians(0.0),
            Winding::Positive,
        );
        let path = ctx.apply_transform(b.build());
        if ctx.state.has_fill {
            let color = ctx.state.fill_color;
            ctx.fill_path(&path, color);
        }
        if ctx.state.has_stroke {
            let color = ctx.state.stroke_color;
            ctx.stroke_path(&path, color);
        }
    });
}

/// Draw a triangle with vertices at (`x1`,`y1`), (`x2`,`y2`), (`x3`,`y3`).
/// Respects current fill and stroke settings.
pub fn triangle(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
    with_ctx(|ctx| {
        let mut b = Path::builder();
        b.begin(lpt(x1, y1));
        b.line_to(lpt(x2, y2));
        b.line_to(lpt(x3, y3));
        b.close();
        let path = ctx.apply_transform(b.build());
        if ctx.state.has_fill {
            let color = ctx.state.fill_color;
            ctx.fill_path(&path, color);
        }
        if ctx.state.has_stroke {
            let color = ctx.state.stroke_color;
            ctx.stroke_path(&path, color);
        }
    });
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
    /// Persistent vertex buffer — grown on demand, never shrunk.
    vertex_buf:     Option<wgpu::Buffer>,
    vertex_buf_cap: u64,
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

    fn render(&mut self, bg: wgpu::Color, vertices: &[Vertex]) -> Result<(), wgpu::SurfaceError> {
        let output      = self.surface.get_current_texture()?;
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (target_view, resolve_target) = match &self.msaa_view {
            Some(msaa) => (msaa, Some(&surface_view)),
            None       => (&surface_view, None),
        };

        // Upload vertices into the persistent buffer, growing it only when needed.
        let bytes = bytemuck::cast_slice::<Vertex, u8>(vertices);
        let has_verts = !bytes.is_empty();
        if has_verts {
            let needed = bytes.len() as u64;
            if self.vertex_buf_cap < needed {
                let cap = needed.next_power_of_two().max(4096);
                self.vertex_buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label:              Some("vertex buffer"),
                    size:               cap,
                    usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                self.vertex_buf_cap = cap;
            }
            self.queue.write_buffer(self.vertex_buf.as_ref().unwrap(), 0, bytes);
        }

        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("oripop render") },
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oripop pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           target_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(bg),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:        None,
                occlusion_query_set:     None,
            });

            if has_verts {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buf.as_ref().unwrap().slice(..));
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
        entries: &bind_group_layout_entries_2d(),
    });

    let (white_view, sampler) = create_white_texture_2d(&device, &queue);
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oripop bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&white_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
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
        vertex_buf:     None,
        vertex_buf_cap: 0,
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
// thread-local drawing state as oripop-canvas's 2D API.

/// The WGSL source of the 2D drawing shader.
/// oripop-3d embeds this to create a compatible 2D overlay pipeline.
pub const SHADER_2D_WGSL: &str = include_str!("shader.wgsl");

/// Returns the vertex buffer layout for the 2D drawing pipeline.
/// oripop-3d uses this to wire up a matching vertex buffer in its renderer.
pub fn vertex_2d_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    Vertex::LAYOUT
}

/// Bind group layout entries for the 2D pipeline (`shader.wgsl`):
/// binding 0 = uniforms (vertex), binding 1 = 2D texture (fragment),
/// binding 2 = sampler (fragment). Hosts embedding the 2D pipeline
/// (oripop-3d overlay, studio preview/bake) must use this exact layout.
pub fn bind_group_layout_entries_2d() -> [wgpu::BindGroupLayoutEntry; 3] {
    [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

/// Create the default 1x1 white texture + linear sampler bound at
/// bindings 1 and 2 when no image/atlas is in use. Solid-color vertices
/// (texture slot 0.0) bypass the sample in the shader, so this is purely
/// a placeholder to satisfy the layout.
pub fn create_white_texture_2d(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("oripop 2d white"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8; 4],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("oripop 2d sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (view, sampler)
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
/// Each vertex is [`VERTEX_2D_STRIDE`] (36) bytes: `[f32; 2]` position at
/// offset 0, `[f32; 4]` RGBA colour at offset 8, `[f32; 2]` UV at offset 24,
/// `f32` texture slot at offset 32.
pub fn take_2d_vertices() -> (wgpu::Color, Vec<u8>) {
    with_ctx(|ctx| {
        let bg    = ctx.bg;
        let bytes = bytemuck::cast_slice(&ctx.vertices).to_vec();
        ctx.vertices.clear();
        (bg, bytes)
    })
}

/// Update mouse position only, preserving the current pressed state.
/// Call from `CursorMoved` in the event loop.
pub fn set_mouse_pos(x: f32, y: f32) {
    with_ctx(|ctx| { ctx.mouse_x = x; ctx.mouse_y = y; });
}

/// Update mouse button state only, preserving the current position.
/// Call from `MouseInput` in the event loop.
pub fn set_mouse_pressed(pressed: bool) {
    with_ctx(|ctx| { ctx.mouse_pressed = pressed; });
}

/// Update all mouse state at once. Use the focused helpers above when only
/// one component changes.
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
        if let Some(win) = self.window.as_ref() {
            win.request_redraw();
        }
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

            WindowEvent::CursorEntered { .. } => {
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                with_ctx(|ctx| {
                    ctx.mouse_x = position.x as f32 / gpu.scale_factor as f32;
                    ctx.mouse_y = position.y as f32 / gpu.scale_factor as f32;
                });
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            WindowEvent::MouseInput { state, .. } => {
                with_ctx(|ctx| {
                    ctx.mouse_pressed = state == ElementState::Pressed;
                });
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let pressed = key_event.state == ElementState::Pressed;
                with_ctx(|ctx| {
                    ctx.key_pressed = pressed;
                    if pressed {
                        match &key_event.logical_key {
                            winit::keyboard::Key::Character(c) => {
                                if let Some(ch) = c.chars().next() {
                                    ctx.key_code = ch;
                                }
                            }
                            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) => {
                                ctx.key_code = ' ';
                            }
                            _ => {}
                        }
                    }
                });
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = &self.window {
            if with_ctx(|ctx| ctx.continuous_redraw) {
                window.request_redraw();
            }
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
