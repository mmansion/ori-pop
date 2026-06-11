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
pub(crate) struct Vertex {
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

// ── Frame data (draw runs) ──────────────────────────────
//
// A frame is no longer one flat vertex stream: it is a stream plus a list of
// contiguous *runs*, each bound to one texture. Run `tex == 0` is solid
// geometry (white placeholder texture); other values reference an offscreen
// [`crate::graphics::Graphics`] canvas rendered earlier in the frame.

/// A contiguous range of vertices drawn with a single texture binding.
#[derive(Copy, Clone, Debug)]
pub(crate) struct DrawRun {
    /// 0 = solid color; otherwise a `Graphics` id.
    pub tex: u64,
    pub start: u32,
    pub count: u32,
}

/// Recorded content of one offscreen `Graphics` canvas for this frame.
pub(crate) struct GraphicsFrame {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub bg: wgpu::Color,
    pub vertices: Vec<Vertex>,
}

/// Everything the windowed host needs to render one frame.
pub(crate) struct Frame2D {
    pub bg: wgpu::Color,
    /// Whether `background()` was called: clear the persistent canvas.
    pub clear: bool,
    /// Canvas supersampling factor.
    pub density: u32,
    pub vertices: Vec<Vertex>,
    pub runs: Vec<DrawRun>,
    pub graphics: Vec<GraphicsFrame>,
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

// ── Modes & attributes ──────────────────────────────────

/// How `rect`/`ellipse` (and `square`/`circle`/`arc`) interpret their
/// coordinate arguments. Mirrors Processing's `rectMode()`/`ellipseMode()`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ShapeMode {
    /// (x, y) is the top-left corner; (w, h) are dimensions.
    Corner,
    /// (x1, y1) and (x2, y2) are opposite corners.
    Corners,
    /// (x, y) is the center; (w, h) are dimensions.
    Center,
    /// (x, y) is the center; (rx, ry) are radii.
    Radius,
}

/// Arc rendering mode (Processing's `OPEN` / `CHORD` / `PIE`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ArcMode {
    /// Processing default: stroke is the open arc, fill is a pie wedge.
    Open,
    /// Both stroke and fill closed by the chord.
    Chord,
    /// Both stroke and fill closed through the center (wedge).
    Pie,
}

/// Stroke endpoint style (Processing's `strokeCap()`).
/// Note Processing naming: `Square` is a flat cap at the endpoint,
/// `Project` extends past it by half the stroke weight.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StrokeCap {
    Round,
    Square,
    Project,
}

/// Stroke corner style (Processing's `strokeJoin()`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StrokeJoin {
    Miter,
    Bevel,
    Round,
}

/// How `fill`/`stroke`/`background` channel arguments are interpreted
/// (Processing's `colorMode()`). All channels stay 0–255.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ColorMode {
    /// Channels are red, green, blue (default).
    Rgb,
    /// Channels are hue, saturation, brightness.
    Hsb,
}

/// A resolved RGBA color (Processing's `color()` value).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn to_f32(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
}

/// Interpolate between two colors by `t` in [0, 1] (per RGBA channel).
pub fn lerp_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color {
        r: l(from.r, to.r),
        g: l(from.g, to.g),
        b: l(from.b, to.b),
        a: l(from.a, to.a),
    }
}

/// HSB (0–255 per channel, Processing default ranges) → RGB.
fn hsb_to_rgb(h: u8, s: u8, b: u8) -> (u8, u8, u8) {
    let h = h as f32 / 255.0;
    let s = s as f32 / 255.0;
    let v = b as f32 / 255.0;
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

// ── Draw State ──────────────────────────────────────────

#[derive(Copy, Clone)]
struct DrawState {
    stroke_color: [f32; 4],
    stroke_weight: f32,
    has_stroke: bool,
    fill_color: [f32; 4],
    has_fill: bool,
    rect_mode: ShapeMode,
    ellipse_mode: ShapeMode,
    cap: StrokeCap,
    join: StrokeJoin,
    color_mode: ColorMode,
}

impl Default for DrawState {
    fn default() -> Self {
        Self {
            stroke_color: [1.0, 1.0, 1.0, 1.0],
            stroke_weight: 1.0,
            has_stroke: true,
            fill_color: [1.0, 1.0, 1.0, 1.0],
            has_fill: true,
            // Processing defaults.
            rect_mode: ShapeMode::Corner,
            ellipse_mode: ShapeMode::Center,
            cap: StrokeCap::Round,
            join: StrokeJoin::Miter,
            color_mode: ColorMode::Rgb,
        }
    }
}

/// Resolve mode-dependent shape arguments to (x, y, w, h) with (x, y) the
/// top-left corner.
fn resolve_box(mode: ShapeMode, a: f32, b: f32, c: f32, d: f32) -> (f32, f32, f32, f32) {
    match mode {
        ShapeMode::Corner => (a, b, c, d),
        ShapeMode::Corners => (a, b, c - a, d - b),
        ShapeMode::Center => (a - c * 0.5, b - d * 0.5, c, d),
        ShapeMode::Radius => (a - c, b - d, c * 2.0, d * 2.0),
    }
}

// ── Custom shapes (begin_shape / vertex / end_shape) ────

/// One recorded vertex of a custom shape.
#[derive(Copy, Clone)]
enum SVtx {
    /// Straight segment to (x, y).
    V([f32; 2]),
    /// Cubic bezier via two controls to an anchor.
    B([f32; 2], [f32; 2], [f32; 2]),
    /// Quadratic bezier via one control to an anchor.
    Q([f32; 2], [f32; 2]),
    /// Catmull-Rom curve vertex (first/last act as control points).
    C([f32; 2]),
}

struct ShapeRec {
    outer: Vec<SVtx>,
    contours: Vec<Vec<SVtx>>,
    in_contour: bool,
}

impl ShapeRec {
    fn new() -> Self {
        Self { outer: Vec::new(), contours: Vec::new(), in_contour: false }
    }

    fn current(&mut self) -> &mut Vec<SVtx> {
        if self.in_contour {
            self.contours.last_mut().expect("begin_contour pushes a ring")
        } else {
            &mut self.outer
        }
    }
}

/// Append one ring of shape vertices to a path builder, expanding
/// Catmull-Rom runs into cubic beziers. `close` ends the ring closed.
fn emit_ring(b: &mut lyon::path::path::Builder, ring: &[SVtx], close: bool) {
    if ring.is_empty() {
        return;
    }

    let mut started = false;
    let start_or_line = |b: &mut lyon::path::path::Builder, p: [f32; 2], started: &mut bool| {
        if *started {
            b.line_to(lpt(p[0], p[1]));
        } else {
            b.begin(lpt(p[0], p[1]));
            *started = true;
        }
    };

    let mut i = 0;
    while i < ring.len() {
        match ring[i] {
            SVtx::V(p) => {
                start_or_line(b, p, &mut started);
                i += 1;
            }
            SVtx::B(c1, c2, to) => {
                if !started {
                    b.begin(lpt(to[0], to[1]));
                    started = true;
                } else {
                    b.cubic_bezier_to(lpt(c1[0], c1[1]), lpt(c2[0], c2[1]), lpt(to[0], to[1]));
                }
                i += 1;
            }
            SVtx::Q(c, to) => {
                if !started {
                    b.begin(lpt(to[0], to[1]));
                    started = true;
                } else {
                    b.quadratic_bezier_to(lpt(c[0], c[1]), lpt(to[0], to[1]));
                }
                i += 1;
            }
            SVtx::C(_) => {
                // Collect the run of consecutive curve vertices.
                let run_start = i;
                while i < ring.len() && matches!(ring[i], SVtx::C(_)) {
                    i += 1;
                }
                let pts: Vec<[f32; 2]> = ring[run_start..i]
                    .iter()
                    .map(|v| match v {
                        SVtx::C(p) => *p,
                        _ => unreachable!(),
                    })
                    .collect();
                if pts.len() < 4 {
                    // Processing draws nothing until four curve vertices
                    // exist; degrade to straight segments.
                    for p in &pts {
                        start_or_line(b, *p, &mut started);
                    }
                } else {
                    start_or_line(b, pts[1], &mut started);
                    for w in pts.windows(4) {
                        let (p0, p1, p2, p3) = (w[0], w[1], w[2], w[3]);
                        let c1 = [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0];
                        let c2 = [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0];
                        b.cubic_bezier_to(
                            lpt(c1[0], c1[1]),
                            lpt(c2[0], c2[1]),
                            lpt(p2[0], p2[1]),
                        );
                    }
                }
            }
        }
    }

    if started {
        b.end(close);
    }
}

// ── Recorder ────────────────────────────────────────────
//
// The drawing surface: draw state, transform stack, tessellators, and the
// recorded vertex stream + draw runs. The main canvas owns one (inside the
// thread-local `Context`); each offscreen `Graphics` owns its own.

pub(crate) struct Recorder {
    state: DrawState,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) runs: Vec<DrawRun>,
    pub(crate) bg: wgpu::Color,
    /// True when an opaque `background()` was called this frame: the canvas
    /// clears. When false, previous frame content persists (Processing
    /// semantics). Translucent backgrounds blend a wash quad instead.
    pub(crate) bg_set: bool,
    /// Canvas dimensions in logical pixels (for translucent washes).
    pub(crate) surface_size: (f32, f32),
    matrix: [f32; 6],
    matrix_stack: Vec<[f32; 6]>,
    style_stack: Vec<DrawState>,
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
    shape: Option<ShapeRec>,
}

impl Recorder {
    pub(crate) fn new() -> Self {
        Self {
            state: DrawState::default(),
            vertices: Vec::new(),
            runs: Vec::new(),
            bg: wgpu::Color::BLACK,
            bg_set: false,
            surface_size: (400.0, 400.0),
            matrix: IDENTITY,
            matrix_stack: Vec::new(),
            style_stack: Vec::new(),
            fill_tess: FillTessellator::new(),
            stroke_tess: StrokeTessellator::new(),
            shape: None,
        }
    }

    /// Discard recorded geometry (used by `Graphics::background` and the
    /// per-frame reset). Draw state (colors, weight) persists.
    pub(crate) fn clear(&mut self) {
        self.vertices.clear();
        self.runs.clear();
    }

    /// Per-frame reset: clear geometry and the transform/style stacks.
    pub(crate) fn reset(&mut self) {
        self.clear();
        self.bg_set = false;
        self.matrix = IDENTITY;
        self.matrix_stack.clear();
        self.style_stack.clear();
    }

    // ── state setters ──

    /// Resolve three channel arguments under the current color mode.
    pub(crate) fn make_color(&self, c1: u8, c2: u8, c3: u8, a: u8) -> Color {
        match self.state.color_mode {
            ColorMode::Rgb => Color::rgba(c1, c2, c3, a),
            ColorMode::Hsb => {
                let (r, g, b) = hsb_to_rgb(c1, c2, c3);
                Color::rgba(r, g, b, a)
            }
        }
    }

    pub(crate) fn set_color_mode(&mut self, mode: ColorMode) {
        self.state.color_mode = mode;
    }

    pub(crate) fn set_background_color(&mut self, c: Color) {
        if c.a == 255 {
            // Opaque: hard clear at the start of this frame's pass.
            self.bg = wgpu::Color {
                r: c.r as f64 / 255.0,
                g: c.g as f64 / 255.0,
                b: c.b as f64 / 255.0,
                a: 1.0,
            };
            self.bg_set = true;
        } else {
            // Translucent (p5-style `background(c, alpha)`): blend a
            // fullscreen wash over the persistent canvas — the idiomatic
            // way to fade trails. Ignores the current transform.
            let color = c.to_f32();
            let (w, h) = self.surface_size;
            let v = |x: f32, y: f32| Vertex {
                position: [x, y],
                color,
                uv: [0.0, 0.0],
                tex: 0.0,
            };
            self.vertices.extend_from_slice(&[
                v(0.0, 0.0),
                v(w, 0.0),
                v(w, h),
                v(0.0, 0.0),
                v(w, h),
                v(0.0, h),
            ]);
            self.note_run(0, 6);
        }
    }

    pub(crate) fn set_background_a(&mut self, c1: u8, c2: u8, c3: u8, a: u8) {
        let c = self.make_color(c1, c2, c3, a);
        self.set_background_color(c);
    }

    pub(crate) fn set_stroke_color(&mut self, c: Color) {
        self.state.stroke_color = c.to_f32();
        self.state.has_stroke = true;
    }

    pub(crate) fn set_stroke_a(&mut self, c1: u8, c2: u8, c3: u8, a: u8) {
        let c = self.make_color(c1, c2, c3, a);
        self.set_stroke_color(c);
    }

    pub(crate) fn set_no_stroke(&mut self) {
        self.state.has_stroke = false;
    }

    pub(crate) fn set_fill_color(&mut self, c: Color) {
        self.state.fill_color = c.to_f32();
        self.state.has_fill = true;
    }

    pub(crate) fn set_fill_a(&mut self, c1: u8, c2: u8, c3: u8, a: u8) {
        let c = self.make_color(c1, c2, c3, a);
        self.set_fill_color(c);
    }

    pub(crate) fn set_no_fill(&mut self) {
        self.state.has_fill = false;
    }

    pub(crate) fn set_stroke_weight(&mut self, w: f32) {
        self.state.stroke_weight = w;
    }

    pub(crate) fn set_rect_mode(&mut self, mode: ShapeMode) {
        self.state.rect_mode = mode;
    }

    pub(crate) fn set_ellipse_mode(&mut self, mode: ShapeMode) {
        self.state.ellipse_mode = mode;
    }

    pub(crate) fn set_stroke_cap(&mut self, cap: StrokeCap) {
        self.state.cap = cap;
    }

    pub(crate) fn set_stroke_join(&mut self, join: StrokeJoin) {
        self.state.join = join;
    }

    // ── transforms ──

    pub(crate) fn push(&mut self) {
        self.matrix_stack.push(self.matrix);
    }

    pub(crate) fn pop(&mut self) {
        if let Some(m) = self.matrix_stack.pop() {
            self.matrix = m;
        }
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        let t = [1.0, 0.0, dx, 0.0, 1.0, dy];
        self.matrix = mat_mult(&self.matrix, &t);
    }

    pub(crate) fn rotate(&mut self, angle: f32) {
        let c = angle.cos();
        let s = angle.sin();
        let r = [c, -s, 0.0, s, c, 0.0];
        self.matrix = mat_mult(&self.matrix, &r);
    }

    pub(crate) fn scale(&mut self, sx: f32, sy: f32) {
        let s = [sx, 0.0, 0.0, 0.0, sy, 0.0];
        self.matrix = mat_mult(&self.matrix, &s);
    }

    pub(crate) fn shear_x(&mut self, angle: f32) {
        let m = [1.0, angle.tan(), 0.0, 0.0, 1.0, 0.0];
        self.matrix = mat_mult(&self.matrix, &m);
    }

    pub(crate) fn shear_y(&mut self, angle: f32) {
        let m = [1.0, 0.0, 0.0, angle.tan(), 1.0, 0.0];
        self.matrix = mat_mult(&self.matrix, &m);
    }

    pub(crate) fn reset_matrix(&mut self) {
        self.matrix = IDENTITY;
    }

    pub(crate) fn push_style(&mut self) {
        self.style_stack.push(self.state);
    }

    pub(crate) fn pop_style(&mut self) {
        if let Some(s) = self.style_stack.pop() {
            self.state = s;
        }
    }

    // ── geometry helpers ──

    fn xform(&self, x: f32, y: f32) -> [f32; 2] {
        let m = &self.matrix;
        [m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]]
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

    /// Extend (or start) the run covering `added` vertices just appended.
    fn note_run(&mut self, tex: u64, added: usize) {
        if added == 0 {
            return;
        }
        let added = added as u32;
        match self.runs.last_mut() {
            Some(run) if run.tex == tex => run.count += added,
            _ => {
                let start = self.vertices.len() as u32 - added;
                self.runs.push(DrawRun { tex, start, count: added });
            }
        }
    }

    fn fill_path(&mut self, path: &Path, color: [f32; 4]) {
        let mut buf: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let result = self.fill_tess.tessellate_path(
            path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut buf, SolidCtor { color }),
        );
        if result.is_ok() {
            let before = self.vertices.len();
            append_indexed(&mut self.vertices, &buf);
            let added = self.vertices.len() - before;
            self.note_run(0, added);
        }
    }

    fn stroke_path(&mut self, path: &Path, color: [f32; 4]) {
        let weight = self.state.stroke_weight;
        if weight <= 0.0 {
            return;
        }
        // Processing naming: SQUARE is a flat (butt) cap, PROJECT extends.
        let cap = match self.state.cap {
            StrokeCap::Round => LineCap::Round,
            StrokeCap::Square => LineCap::Butt,
            StrokeCap::Project => LineCap::Square,
        };
        let join = match self.state.join {
            StrokeJoin::Miter => LineJoin::Miter,
            StrokeJoin::Bevel => LineJoin::Bevel,
            StrokeJoin::Round => LineJoin::Round,
        };
        let options = StrokeOptions::default()
            .with_line_width(weight)
            .with_start_cap(cap)
            .with_end_cap(cap)
            .with_line_join(join);
        let mut buf: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let result = self.stroke_tess.tessellate_path(
            path,
            &options,
            &mut BuffersBuilder::new(&mut buf, SolidCtor { color }),
        );
        if result.is_ok() {
            let before = self.vertices.len();
            append_indexed(&mut self.vertices, &buf);
            let added = self.vertices.len() - before;
            self.note_run(0, added);
        }
    }

    // ── primitives ──

    pub(crate) fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        if !self.state.has_stroke {
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
        let path = self.apply_transform(b.build());
        let color = self.state.stroke_color;
        self.stroke_path(&path, color);
    }

    pub(crate) fn point(&mut self, x: f32, y: f32) {
        if !self.state.has_stroke {
            return;
        }
        let half = self.state.stroke_weight * 0.5;
        if half <= 0.0 {
            return;
        }
        let mut b = Path::builder();
        b.add_circle(lpt(x, y), half, Winding::Positive);
        let path = self.apply_transform(b.build());
        let color = self.state.stroke_color;
        self.fill_path(&path, color);
    }

    /// Fill + stroke a finished path with the current state.
    fn paint_path(&mut self, path: Path) {
        let path = self.apply_transform(path);
        if self.state.has_fill {
            let color = self.state.fill_color;
            self.fill_path(&path, color);
        }
        if self.state.has_stroke {
            let color = self.state.stroke_color;
            self.stroke_path(&path, color);
        }
    }

    pub(crate) fn rect(&mut self, a: f32, b_: f32, c: f32, d: f32) {
        let (x, y, w, h) = resolve_box(self.state.rect_mode, a, b_, c, d);
        let mut b = Path::builder();
        b.add_rectangle(&Box2D::new(lpt(x, y), lpt(x + w, y + h)), Winding::Positive);
        self.paint_path(b.build());
    }

    pub(crate) fn square(&mut self, a: f32, b: f32, s: f32) {
        self.rect(a, b, s, s);
    }

    pub(crate) fn ellipse(&mut self, a: f32, b_: f32, c: f32, d: f32) {
        let (x, y, w, h) = resolve_box(self.state.ellipse_mode, a, b_, c, d);
        let rx = w * 0.5;
        let ry = h * 0.5;
        let mut b = Path::builder();
        b.add_ellipse(
            lpt(x + rx, y + ry),
            lvec(rx, ry),
            Angle::radians(0.0),
            Winding::Positive,
        );
        self.paint_path(b.build());
    }

    pub(crate) fn circle(&mut self, a: f32, b: f32, d: f32) {
        self.ellipse(a, b, d, d);
    }

    pub(crate) fn quad(
        &mut self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        x3: f32, y3: f32,
        x4: f32, y4: f32,
    ) {
        let mut b = Path::builder();
        b.begin(lpt(x1, y1));
        b.line_to(lpt(x2, y2));
        b.line_to(lpt(x3, y3));
        b.line_to(lpt(x4, y4));
        b.close();
        self.paint_path(b.build());
    }

    /// Arc of the ellipse described by (a, b, c, d) under `ellipse_mode`,
    /// from `start` to `stop` radians (0 = +x axis, increasing clockwise on
    /// screen because y grows down — Processing semantics).
    pub(crate) fn arc(&mut self, a: f32, b_: f32, c: f32, d: f32, start: f32, stop: f32, mode: ArcMode) {
        if stop <= start {
            return;
        }
        let (x, y, w, h) = resolve_box(self.state.ellipse_mode, a, b_, c, d);
        let rx = w * 0.5;
        let ry = h * 0.5;
        let center = lpt(x + rx, y + ry);
        let sweep = (stop - start).min(std::f32::consts::TAU);
        let arc = lyon::geom::Arc {
            center,
            radii: lvec(rx, ry),
            start_angle: Angle::radians(start),
            sweep_angle: Angle::radians(sweep),
            x_rotation: Angle::radians(0.0),
        };
        let from = arc.from();

        // Build one ring with the requested closure shape.
        let build = |kind: ArcMode| -> Path {
            let mut b = Path::builder();
            match kind {
                ArcMode::Pie => {
                    b.begin(center);
                    b.line_to(from);
                }
                _ => {
                    b.begin(from);
                }
            }
            arc.for_each_quadratic_bezier(&mut |q| {
                b.quadratic_bezier_to(q.ctrl, q.to);
            });
            match kind {
                ArcMode::Open => b.end(false),
                ArcMode::Chord | ArcMode::Pie => b.close(),
            }
            b.build()
        };

        // Processing default (`Open`): pie-shaped fill, open-arc stroke.
        if self.state.has_fill {
            let fill_kind = match mode {
                ArcMode::Chord => ArcMode::Chord,
                _ => ArcMode::Pie,
            };
            let path = self.apply_transform(build(fill_kind));
            let color = self.state.fill_color;
            self.fill_path(&path, color);
        }
        if self.state.has_stroke {
            let path = self.apply_transform(build(mode));
            let color = self.state.stroke_color;
            self.stroke_path(&path, color);
        }
    }

    /// Stroke a cubic bezier from (x1, y1) to (x4, y4) with control points
    /// (x2, y2) and (x3, y3).
    pub(crate) fn bezier(
        &mut self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        x3: f32, y3: f32,
        x4: f32, y4: f32,
    ) {
        if !self.state.has_stroke {
            return;
        }
        let mut b = Path::builder();
        b.begin(lpt(x1, y1));
        b.cubic_bezier_to(lpt(x2, y2), lpt(x3, y3), lpt(x4, y4));
        b.end(false);
        let path = self.apply_transform(b.build());
        let color = self.state.stroke_color;
        self.stroke_path(&path, color);
    }

    /// Stroke a Catmull-Rom segment between (x2, y2) and (x3, y3);
    /// (x1, y1) and (x4, y4) shape the curve as control points.
    pub(crate) fn curve(
        &mut self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        x3: f32, y3: f32,
        x4: f32, y4: f32,
    ) {
        if !self.state.has_stroke {
            return;
        }
        let c1 = lpt(x2 + (x3 - x1) / 6.0, y2 + (y3 - y1) / 6.0);
        let c2 = lpt(x3 - (x4 - x2) / 6.0, y3 - (y4 - y2) / 6.0);
        let mut b = Path::builder();
        b.begin(lpt(x2, y2));
        b.cubic_bezier_to(c1, c2, lpt(x3, y3));
        b.end(false);
        let path = self.apply_transform(b.build());
        let color = self.state.stroke_color;
        self.stroke_path(&path, color);
    }

    // ── custom shapes ──

    pub(crate) fn begin_shape(&mut self) {
        self.shape = Some(ShapeRec::new());
    }

    pub(crate) fn vertex(&mut self, x: f32, y: f32) {
        if let Some(s) = self.shape.as_mut() {
            s.current().push(SVtx::V([x, y]));
        }
    }

    pub(crate) fn bezier_vertex(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        if let Some(s) = self.shape.as_mut() {
            s.current().push(SVtx::B([cx1, cy1], [cx2, cy2], [x, y]));
        }
    }

    pub(crate) fn quadratic_vertex(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if let Some(s) = self.shape.as_mut() {
            s.current().push(SVtx::Q([cx, cy], [x, y]));
        }
    }

    pub(crate) fn curve_vertex(&mut self, x: f32, y: f32) {
        if let Some(s) = self.shape.as_mut() {
            s.current().push(SVtx::C([x, y]));
        }
    }

    pub(crate) fn begin_contour(&mut self) {
        if let Some(s) = self.shape.as_mut() {
            s.contours.push(Vec::new());
            s.in_contour = true;
        }
    }

    pub(crate) fn end_contour(&mut self) {
        if let Some(s) = self.shape.as_mut() {
            s.in_contour = false;
        }
    }

    pub(crate) fn end_shape(&mut self, close: bool) {
        let Some(shape) = self.shape.take() else { return };
        let mut b = Path::builder();
        // The outer ring closes only when requested; contours always close.
        emit_ring(&mut b, &shape.outer, close);
        for ring in &shape.contours {
            emit_ring(&mut b, ring, true);
        }
        // Even-odd fill (lyon default) makes contours read as holes.
        self.paint_path(b.build());
    }

    pub(crate) fn triangle(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        let mut b = Path::builder();
        b.begin(lpt(x1, y1));
        b.line_to(lpt(x2, y2));
        b.line_to(lpt(x3, y3));
        b.close();
        let path = self.apply_transform(b.build());
        if self.state.has_fill {
            let color = self.state.fill_color;
            self.fill_path(&path, color);
        }
        if self.state.has_stroke {
            let color = self.state.stroke_color;
            self.stroke_path(&path, color);
        }
    }

    /// Append a textured quad referencing an offscreen canvas (`tex` is the
    /// `Graphics` id). UVs cover the full texture, color is white.
    pub(crate) fn image_quad(&mut self, tex: u64, x: f32, y: f32, w: f32, h: f32) {
        let color = [1.0, 1.0, 1.0, 1.0];
        let v = |px: f32, py: f32, u: f32, vv: f32| Vertex {
            position: self.xform(px, py),
            color,
            uv: [u, vv],
            tex: 1.0,
        };
        let tl = v(x, y, 0.0, 0.0);
        let tr = v(x + w, y, 1.0, 0.0);
        let br = v(x + w, y + h, 1.0, 1.0);
        let bl = v(x, y + h, 0.0, 1.0);
        self.vertices.extend_from_slice(&[tl, tr, br, tl, br, bl]);
        self.note_run(tex, 6);
    }
}

// ── Context ─────────────────────────────────────────────

/// Which mouse button is currently held (Processing's `mouseButton`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Center,
}

/// Registered sketch event handlers (Processing's mousePressed()/keyPressed()
/// functions, as explicit registrations).
#[derive(Default, Copy, Clone)]
struct Handlers {
    mouse_pressed: Option<fn()>,
    mouse_released: Option<fn()>,
    mouse_moved: Option<fn()>,
    mouse_dragged: Option<fn()>,
    mouse_wheel: Option<fn(f32)>,
    key_pressed: Option<fn()>,
    key_released: Option<fn()>,
}

struct Context {
    width: u32,
    height: u32,
    title: String,
    msaa_samples: u32,
    /// When false, the event loop does not spin at max FPS; redraws are requested from input and resize only.
    continuous_redraw: bool,
    rec: Recorder,
    graphics_frames: Vec<GraphicsFrame>,
    frame_count: u64,
    mouse_x: f32,
    mouse_y: f32,
    mouse_pressed: bool,
    mouse_button: Option<MouseButton>,
    /// Mouse position sampled at the start of this frame / previous frame.
    frame_mouse: (f32, f32),
    pmouse: (f32, f32),
    /// Wheel movement: accumulating since last frame / readable this frame.
    wheel_accum: f32,
    wheel_frame: f32,
    key_pressed: bool,
    key_code: char,
    start_time: std::time::Instant,
    target_fps: Option<f32>,
    handlers: Handlers,
    /// Canvas supersampling factor for high-resolution output.
    density: u32,
    /// Snapshot request: resolved path, taken after the next render.
    pending_save: Option<std::path::PathBuf>,
}

impl Context {
    fn new() -> Self {
        Self {
            width: 400,
            height: 400,
            title: String::from("ori-pop"),
            msaa_samples: 4,
            continuous_redraw: true,
            rec: Recorder::new(),
            graphics_frames: Vec::new(),
            frame_count: 0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_pressed: false,
            mouse_button: None,
            frame_mouse: (0.0, 0.0),
            pmouse: (0.0, 0.0),
            wheel_accum: 0.0,
            wheel_frame: 0.0,
            key_pressed: false,
            key_code: '\0',
            start_time: std::time::Instant::now(),
            target_fps: None,
            handlers: Handlers::default(),
            density: 1,
            pending_save: None,
        }
    }

    fn reset_frame(&mut self) {
        self.rec.reset();
        self.graphics_frames.clear();
        self.frame_count += 1;
        self.pmouse = self.frame_mouse;
        self.frame_mouse = (self.mouse_x, self.mouse_y);
        self.wheel_frame = self.wheel_accum;
        self.wheel_accum = 0.0;
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
        ctx.rec.surface_size = (width as f32, height as f32);
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

/// Background with alpha. With `a == 255` this is a hard clear, identical
/// to [`background`]. With `a < 255` it blends a translucent fullscreen
/// wash over the persistent canvas instead of clearing — the idiomatic
/// trail-fade (`background_a(0, 0, 0, 10)`), as in p5.js.
pub fn background_a(r: u8, g: u8, b: u8, a: u8) {
    with_ctx(|ctx| ctx.rec.set_background_a(r, g, b, a));
}

/// Set the stroke (outline) color to an opaque RGB value and enable stroke.
pub fn stroke(r: u8, g: u8, b: u8) {
    stroke_a(r, g, b, 255);
}

/// Set the stroke color with alpha transparency and enable stroke.
///
/// Alpha is 0 (fully transparent) to 255 (fully opaque).
pub fn stroke_a(r: u8, g: u8, b: u8, a: u8) {
    with_ctx(|ctx| ctx.rec.set_stroke_a(r, g, b, a));
}

/// Disable stroke for subsequent shapes.
pub fn no_stroke() {
    with_ctx(|ctx| ctx.rec.set_no_stroke());
}

/// Set the fill color to an opaque RGB value and enable fill.
pub fn fill(r: u8, g: u8, b: u8) {
    fill_a(r, g, b, 255);
}

/// Set the fill color with alpha transparency and enable fill.
///
/// Alpha is 0 (fully transparent) to 255 (fully opaque).
pub fn fill_a(r: u8, g: u8, b: u8, a: u8) {
    with_ctx(|ctx| ctx.rec.set_fill_a(r, g, b, a));
}

/// Disable fill for subsequent shapes (outlines only).
pub fn no_fill() {
    with_ctx(|ctx| ctx.rec.set_no_fill());
}

/// Set the stroke thickness in logical pixels. Default is 1.0.
pub fn stroke_weight(w: f32) {
    with_ctx(|ctx| ctx.rec.set_stroke_weight(w));
}

/// Set how color channel arguments are interpreted: [`ColorMode::Rgb`]
/// (default) or [`ColorMode::Hsb`]. All channels stay 0–255.
pub fn color_mode(mode: ColorMode) {
    with_ctx(|ctx| ctx.rec.set_color_mode(mode));
}

/// Build an opaque [`Color`] from three channels under the current
/// [`color_mode`].
pub fn color(c1: u8, c2: u8, c3: u8) -> Color {
    with_ctx(|ctx| ctx.rec.make_color(c1, c2, c3, 255))
}

/// Build a [`Color`] with alpha under the current [`color_mode`].
pub fn color_a(c1: u8, c2: u8, c3: u8, a: u8) -> Color {
    with_ctx(|ctx| ctx.rec.make_color(c1, c2, c3, a))
}

/// Set the fill from a resolved [`Color`].
pub fn fill_color(c: Color) {
    with_ctx(|ctx| ctx.rec.set_fill_color(c));
}

/// Set the stroke from a resolved [`Color`].
pub fn stroke_color(c: Color) {
    with_ctx(|ctx| ctx.rec.set_stroke_color(c));
}

/// Clear the canvas to a resolved [`Color`].
pub fn background_color(c: Color) {
    with_ctx(|ctx| ctx.rec.set_background_color(c));
}

/// Set an opaque grayscale fill (`fill_gray(g)` = `fill(g, g, g)` in RGB).
pub fn fill_gray(g: u8) {
    with_ctx(|ctx| ctx.rec.set_fill_color(Color::rgb(g, g, g)));
}

/// Set an opaque grayscale stroke.
pub fn stroke_gray(g: u8) {
    with_ctx(|ctx| ctx.rec.set_stroke_color(Color::rgb(g, g, g)));
}

/// Clear the canvas to an opaque gray.
pub fn background_gray(g: u8) {
    with_ctx(|ctx| ctx.rec.set_background_color(Color::rgb(g, g, g)));
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

/// Horizontal mouse position at the previous frame.
pub fn pmouse_x() -> f32 {
    with_ctx(|ctx| ctx.pmouse.0)
}

/// Vertical mouse position at the previous frame.
pub fn pmouse_y() -> f32 {
    with_ctx(|ctx| ctx.pmouse.1)
}

/// The mouse button currently (or most recently) held, if any.
pub fn mouse_button() -> Option<MouseButton> {
    with_ctx(|ctx| ctx.mouse_button)
}

/// Scroll wheel movement during the previous frame (positive = up/away,
/// in lines).
pub fn mouse_wheel() -> f32 {
    with_ctx(|ctx| ctx.wheel_frame)
}

/// Milliseconds elapsed since the sketch started.
pub fn millis() -> u64 {
    with_ctx(|ctx| ctx.start_time.elapsed().as_millis() as u64)
}

/// Cap the draw loop at `fps` frames per second. By default the loop runs
/// at display rate (vsync).
pub fn frame_rate(fps: f32) {
    with_ctx(|ctx| ctx.target_fps = if fps > 0.0 { Some(fps) } else { None });
}

// ── Event handlers ──────────────────────────────────────
//
// The classic creative-coding event functions. Rust cannot discover magic
// global functions like Processing's `mousePressed()`, so handlers are
// registered explicitly (call these in `main()` before `run`).

/// Register a handler called when a mouse button is pressed.
pub fn on_mouse_pressed(f: fn()) {
    with_ctx(|ctx| ctx.handlers.mouse_pressed = Some(f));
}

/// Register a handler called when a mouse button is released.
pub fn on_mouse_released(f: fn()) {
    with_ctx(|ctx| ctx.handlers.mouse_released = Some(f));
}

/// Register a handler called when the mouse moves with no button held.
pub fn on_mouse_moved(f: fn()) {
    with_ctx(|ctx| ctx.handlers.mouse_moved = Some(f));
}

/// Register a handler called when the mouse moves with a button held.
pub fn on_mouse_dragged(f: fn()) {
    with_ctx(|ctx| ctx.handlers.mouse_dragged = Some(f));
}

/// Register a handler called on scroll; receives the wheel delta in lines.
pub fn on_mouse_wheel(f: fn(f32)) {
    with_ctx(|ctx| ctx.handlers.mouse_wheel = Some(f));
}

/// Register a handler called when a key is pressed ([`key`] holds the char).
pub fn on_key_pressed(f: fn()) {
    with_ctx(|ctx| ctx.handlers.key_pressed = Some(f));
}

/// Register a handler called when a key is released.
pub fn on_key_released(f: fn()) {
    with_ctx(|ctx| ctx.handlers.key_released = Some(f));
}

// ── Snapshots (baking) ──────────────────────────────────

/// Canvas supersampling factor (1–4, p5's `pixelDensity`). The persistent
/// canvas renders at `density` x the window's physical resolution, so
/// [`save_frame`] exports at high resolution while the window shows a
/// downsampled view. Call before [`run`]; high values cost GPU memory.
pub fn pixel_density(density: u32) {
    with_ctx(|ctx| ctx.density = density.clamp(1, 4));
}

/// Save a snapshot of the canvas (everything currently visible, including
/// accumulated/persistent content) as a PNG after this frame finishes.
///
/// Runs of `#` in the file name are replaced with the zero-padded frame
/// number, Processing-style: `save_frame("out/frame-####.png")`.
/// Resolution = window physical pixels x [`pixel_density`].
pub fn save_frame(path: &str) {
    with_ctx(|ctx| {
        let resolved = expand_frame_pattern(path, ctx.frame_count);
        ctx.pending_save = Some(std::path::PathBuf::from(resolved));
    });
}

/// Replace each run of '#' with the zero-padded frame number.
fn expand_frame_pattern(pattern: &str, frame: u64) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#' {
            let mut width = 1;
            while chars.peek() == Some(&'#') {
                chars.next();
                width += 1;
            }
            out.push_str(&format!("{frame:0width$}"));
        } else {
            out.push(c);
        }
    }
    out
}

/// Push current transform onto the stack. Pair with pop().
pub fn push() {
    with_ctx(|ctx| ctx.rec.push());
}

/// Pop transform from the stack. Pair with push().
pub fn pop() {
    with_ctx(|ctx| ctx.rec.pop());
}

/// Translate by (dx, dy) in current units.
pub fn translate(dx: f32, dy: f32) {
    with_ctx(|ctx| ctx.rec.translate(dx, dy));
}

/// Rotate by angle in radians (counter-clockwise).
pub fn rotate(angle: f32) {
    with_ctx(|ctx| ctx.rec.rotate(angle));
}

/// Scale by (sx, sy). Use scale(s) for uniform scale.
pub fn scale(sx: f32, sy: f32) {
    with_ctx(|ctx| ctx.rec.scale(sx, sy));
}

/// Shear along the x axis by `angle` radians.
pub fn shear_x(angle: f32) {
    with_ctx(|ctx| ctx.rec.shear_x(angle));
}

/// Shear along the y axis by `angle` radians.
pub fn shear_y(angle: f32) {
    with_ctx(|ctx| ctx.rec.shear_y(angle));
}

/// Replace the current transform with the identity.
pub fn reset_matrix() {
    with_ctx(|ctx| ctx.rec.reset_matrix());
}

/// Save the current drawing style (colors, weight, modes, caps). Pair with
/// [`pop_style`]. Independent of the transform stack ([`push`]/[`pop`]).
pub fn push_style() {
    with_ctx(|ctx| ctx.rec.push_style());
}

/// Restore the most recently pushed drawing style.
pub fn pop_style() {
    with_ctx(|ctx| ctx.rec.pop_style());
}

/// Draw a line from (`x1`, `y1`) to (`x2`, `y2`) using the current stroke
/// color and weight. Does nothing if stroke is disabled.
pub fn line(x1: f32, y1: f32, x2: f32, y2: f32) {
    with_ctx(|ctx| ctx.rec.line(x1, y1, x2, y2));
}

/// Draw a single point at (`x`, `y`) as a filled dot whose diameter equals
/// the current stroke weight.
pub fn point(x: f32, y: f32) {
    with_ctx(|ctx| ctx.rec.point(x, y));
}

/// Draw a rectangle with its top-left corner at (`x`, `y`) and the given
/// width and height. Respects current fill and stroke settings.
pub fn rect(x: f32, y: f32, w: f32, h: f32) {
    with_ctx(|ctx| ctx.rec.rect(x, y, w, h));
}

/// Draw an ellipse centered at (`x`, `y`) with the given width and height
/// (Processing default `ellipse_mode(Center)`; change with [`ellipse_mode`]).
/// Respects current fill and stroke settings.
pub fn ellipse(x: f32, y: f32, w: f32, h: f32) {
    with_ctx(|ctx| ctx.rec.ellipse(x, y, w, h));
}

/// Draw a circle centered at (`x`, `y`) with diameter `d` (under the
/// current [`ellipse_mode`]).
pub fn circle(x: f32, y: f32, d: f32) {
    with_ctx(|ctx| ctx.rec.circle(x, y, d));
}

/// Draw a square with its top-left corner at (`x`, `y`) and side `s`
/// (under the current [`rect_mode`]).
pub fn square(x: f32, y: f32, s: f32) {
    with_ctx(|ctx| ctx.rec.square(x, y, s));
}

/// Draw a quadrilateral through four points in order.
pub fn quad(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) {
    with_ctx(|ctx| ctx.rec.quad(x1, y1, x2, y2, x3, y3, x4, y4));
}

/// Draw a triangle with vertices at (`x1`,`y1`), (`x2`,`y2`), (`x3`,`y3`).
/// Respects current fill and stroke settings.
pub fn triangle(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
    with_ctx(|ctx| ctx.rec.triangle(x1, y1, x2, y2, x3, y3));
}

/// Draw an arc of the ellipse described by (`x`, `y`, `w`, `h`) under the
/// current [`ellipse_mode`], from `start` to `stop` radians (0 = +x axis,
/// increasing clockwise on screen). Processing default rendering: open
/// stroke, pie-shaped fill. See [`arc_with_mode`] for chord/pie closure.
pub fn arc(x: f32, y: f32, w: f32, h: f32, start: f32, stop: f32) {
    with_ctx(|ctx| ctx.rec.arc(x, y, w, h, start, stop, ArcMode::Open));
}

/// [`arc`] with an explicit [`ArcMode`] (`Open`, `Chord`, or `Pie`).
pub fn arc_with_mode(x: f32, y: f32, w: f32, h: f32, start: f32, stop: f32, mode: ArcMode) {
    with_ctx(|ctx| ctx.rec.arc(x, y, w, h, start, stop, mode));
}

/// Stroke a cubic bezier from (`x1`, `y1`) to (`x4`, `y4`) with control
/// points (`x2`, `y2`) and (`x3`, `y3`).
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) {
    with_ctx(|ctx| ctx.rec.bezier(x1, y1, x2, y2, x3, y3, x4, y4));
}

/// Stroke a Catmull-Rom curve between (`x2`, `y2`) and (`x3`, `y3`);
/// the first and last points shape the curve as control points.
pub fn curve(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) {
    with_ctx(|ctx| ctx.rec.curve(x1, y1, x2, y2, x3, y3, x4, y4));
}

// ── Custom shapes ──

/// Start recording a custom shape. Add vertices with [`vertex`],
/// [`bezier_vertex`], [`quadratic_vertex`], [`curve_vertex`]; cut holes with
/// [`begin_contour`]/[`end_contour`]; finish with [`end_shape`] or
/// [`end_shape_close`].
pub fn begin_shape() {
    with_ctx(|ctx| ctx.rec.begin_shape());
}

/// Add a straight-segment vertex to the current shape.
pub fn vertex(x: f32, y: f32) {
    with_ctx(|ctx| ctx.rec.vertex(x, y));
}

/// Add a cubic-bezier segment to the current shape: two control points,
/// then the anchor.
pub fn bezier_vertex(cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
    with_ctx(|ctx| ctx.rec.bezier_vertex(cx1, cy1, cx2, cy2, x, y));
}

/// Add a quadratic-bezier segment to the current shape: one control point,
/// then the anchor.
pub fn quadratic_vertex(cx: f32, cy: f32, x: f32, y: f32) {
    with_ctx(|ctx| ctx.rec.quadratic_vertex(cx, cy, x, y));
}

/// Add a Catmull-Rom curve vertex. The first and last curve vertices of a
/// run act as control points and are not drawn through.
pub fn curve_vertex(x: f32, y: f32) {
    with_ctx(|ctx| ctx.rec.curve_vertex(x, y));
}

/// Start a hole (inner contour) in the current shape.
pub fn begin_contour() {
    with_ctx(|ctx| ctx.rec.begin_contour());
}

/// Finish the current hole.
pub fn end_contour() {
    with_ctx(|ctx| ctx.rec.end_contour());
}

/// Finish the current shape, leaving the outline open.
pub fn end_shape() {
    with_ctx(|ctx| ctx.rec.end_shape(false));
}

/// Finish the current shape, closing the outline (Processing's
/// `endShape(CLOSE)`).
pub fn end_shape_close() {
    with_ctx(|ctx| ctx.rec.end_shape(true));
}

// ── Modes & attributes ──

/// Set how [`rect`]/[`square`] interpret their arguments. Default
/// [`ShapeMode::Corner`].
pub fn rect_mode(mode: ShapeMode) {
    with_ctx(|ctx| ctx.rec.set_rect_mode(mode));
}

/// Set how [`ellipse`]/[`circle`]/[`arc`] interpret their arguments.
/// Default [`ShapeMode::Center`].
pub fn ellipse_mode(mode: ShapeMode) {
    with_ctx(|ctx| ctx.rec.set_ellipse_mode(mode));
}

/// Set the stroke endpoint style. Default [`StrokeCap::Round`].
pub fn stroke_cap(cap: StrokeCap) {
    with_ctx(|ctx| ctx.rec.set_stroke_cap(cap));
}

/// Set the stroke corner style. Default [`StrokeJoin::Miter`].
pub fn stroke_join(join: StrokeJoin) {
    with_ctx(|ctx| ctx.rec.set_stroke_join(join));
}

// ── Curve evaluation (pure math, per coordinate) ────────

/// Evaluate a cubic bezier coordinate at `t` in [0, 1].
/// `a` and `d` are anchors; `b` and `c` are control points.
pub fn bezier_point(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
}

/// Tangent (derivative) of a cubic bezier coordinate at `t` in [0, 1].
pub fn bezier_tangent(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * (b - a) + 6.0 * u * t * (c - b) + 3.0 * t * t * (d - c)
}

/// Evaluate a Catmull-Rom coordinate at `t` in [0, 1] between `b` and `c`;
/// `a` and `d` are the neighboring control points.
pub fn curve_point(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * b)
        + (-a + c) * t
        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
        + (-a + 3.0 * b - 3.0 * c + d) * t3)
}

/// Tangent (derivative) of a Catmull-Rom coordinate at `t` in [0, 1].
pub fn curve_tangent(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let t2 = t * t;
    0.5 * ((-a + c)
        + 2.0 * (2.0 * a - 5.0 * b + 4.0 * c - d) * t
        + 3.0 * (-a + 3.0 * b - 3.0 * c + d) * t2)
}

/// Draw an offscreen [`Graphics`](crate::graphics::Graphics) canvas at
/// (`x`, `y`) at its native size.
///
/// Only supported in the windowed `run()` host for now; under `run3d` and
/// in studio cartridges the placement quad renders white (texture payloads
/// arrive with the next cartridge ABI revision).
pub fn image(g: &crate::graphics::Graphics, x: f32, y: f32) {
    image_sized(g, x, y, g.width() as f32, g.height() as f32);
}

/// Draw an offscreen [`Graphics`](crate::graphics::Graphics) canvas into the
/// rectangle at (`x`, `y`) with size (`w`, `h`).
pub fn image_sized(g: &crate::graphics::Graphics, x: f32, y: f32, w: f32, h: f32) {
    with_ctx(|ctx| {
        if !ctx.graphics_frames.iter().any(|f| f.id == g.id()) {
            ctx.graphics_frames.push(g.snapshot());
        }
        ctx.rec.image_quad(g.id(), x, y, w, h);
    });
}

// ── GPU ─────────────────────────────────────────────────

const MIN_SURFACE_PIXELS: u32 = 2;

/// Format and sample count for offscreen `Graphics` targets.
const GFX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const GFX_MSAA: u32 = 4;

/// The persistent canvas uses a float format so repeated low-alpha washes
/// (`background_a(.., 10)` trail fades) decay smoothly instead of stalling
/// on 8-bit quantization and leaving permanent ghosts.
const CANVAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// GPU resources for one offscreen `Graphics` canvas.
struct GfxTarget {
    width: u32,
    height: u32,
    color_view: wgpu::TextureView,
    msaa_view: wgpu::TextureView,
    /// Uniforms for rendering *into* the target (resolution = canvas size).
    render_bind: wgpu::BindGroup,
    /// Bind group for sampling the target in the main pass.
    sample_bind: wgpu::BindGroup,
    vbuf: Option<wgpu::Buffer>,
    vbuf_cap: u64,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    white_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    msaa_view: Option<wgpu::TextureView>,
    msaa_samples: u32,
    surface_format: wgpu::TextureFormat,
    scale_factor: f64,
    /// Persistent vertex buffer — grown on demand, never shrunk.
    vertex_buf:     Option<wgpu::Buffer>,
    vertex_buf_cap: u64,
    /// Pipeline targeting offscreen `Graphics` textures.
    gfx_pipeline: wgpu::RenderPipeline,
    /// Pipeline targeting the persistent float canvas.
    canvas_pipeline: wgpu::RenderPipeline,
    gfx_targets: std::collections::HashMap<u64, GfxTarget>,
    /// Persistent accumulation canvas: cleared only when `background()` is
    /// called, otherwise content carries across frames (Processing
    /// semantics). Blitted to the swapchain every frame.
    canvas_color_view: Option<wgpu::TextureView>,
    canvas_msaa_view: Option<wgpu::TextureView>,
    canvas_bind: Option<wgpu::BindGroup>,
    canvas_size: (u32, u32),
    canvas_density: u32,
    /// Actual canvas texture extent (size x density).
    canvas_tex_size: (u32, u32),
    canvas_init: bool,
    blit_vbuf: Option<wgpu::Buffer>,
    /// Pipeline for snapshotting the canvas into an Rgba8 texture.
    save_pipeline: wgpu::RenderPipeline,
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

    /// (Re)create the persistent canvas textures when the window size or
    /// density changes. Content is lost on resize (the next frame redraws it).
    fn ensure_canvas_targets(&mut self, density: u32) {
        let size = (self.config.width, self.config.height);
        if self.canvas_size == size
            && self.canvas_density == density
            && self.canvas_color_view.is_some()
        {
            return;
        }
        let tex_size = (size.0.max(1) * density, size.1.max(1) * density);
        let extent = wgpu::Extent3d {
            width: tex_size.0,
            height: tex_size.1,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oripop canvas color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CANVAS_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oripop canvas msaa"),
            size: extent,
            mip_level_count: 1,
            sample_count: GFX_MSAA,
            dimension: wgpu::TextureDimension::D2,
            format: CANVAS_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oripop canvas blit bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.canvas_color_view = Some(color_view);
        self.canvas_msaa_view = Some(msaa_view);
        self.canvas_bind = Some(bind);
        self.canvas_size = size;
        self.canvas_density = density;
        self.canvas_tex_size = tex_size;
        self.canvas_init = false;
    }

    /// Snapshot the persistent canvas into a PNG at full canvas-texture
    /// resolution (window physical pixels x pixel_density).
    fn save_canvas_png(&mut self, path: &std::path::Path) -> Result<(), String> {
        if self.canvas_color_view.is_none() {
            return Err("canvas not initialized".into());
        }
        let (w, h) = self.canvas_tex_size;

        // Convert the float canvas to Rgba8 via a save pass (the hardware
        // handles sRGB encoding), then read it back.
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oripop save target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("oripop save") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oripop save pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.save_pipeline);
            pass.set_bind_group(0, self.canvas_bind.as_ref().unwrap(), &[]);
            pass.set_vertex_buffer(0, self.blit_vbuf.as_ref().unwrap().slice(..));
            pass.draw(0..6, 0..1);
        }

        let bytes_per_row_unpadded = w * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes_per_row = bytes_per_row_unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oripop save readback"),
            size: (bytes_per_row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;

        let view = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((bytes_per_row_unpadded * h) as usize);
        for row in 0..h {
            let start = (row * bytes_per_row) as usize;
            pixels.extend_from_slice(&view[start..start + bytes_per_row_unpadded as usize]);
        }
        drop(view);
        readback.unmap();

        // The canvas is an opaque backbuffer; export fully opaque.
        for px in pixels.chunks_exact_mut(4) {
            px[3] = 255;
        }

        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
        }
        image::save_buffer(path, &pixels, w, h, image::ColorType::Rgba8)
            .map_err(|e| e.to_string())
    }

    /// Upload the fullscreen quad that blits the canvas to the window.
    fn write_blit_quad(&mut self) {
        let lw = (self.config.width as f64 / self.scale_factor) as f32;
        let lh = (self.config.height as f64 / self.scale_factor) as f32;
        let v = |x: f32, y: f32, u: f32, vv: f32| Vertex {
            position: [x, y],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [u, vv],
            tex: 1.0,
        };
        let quad = [
            v(0.0, 0.0, 0.0, 0.0),
            v(lw, 0.0, 1.0, 0.0),
            v(lw, lh, 1.0, 1.0),
            v(0.0, 0.0, 0.0, 0.0),
            v(lw, lh, 1.0, 1.0),
            v(0.0, lh, 0.0, 1.0),
        ];
        if self.blit_vbuf.is_none() {
            self.blit_vbuf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oripop blit quad"),
                size: (std::mem::size_of::<Vertex>() * 6) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        self.queue
            .write_buffer(self.blit_vbuf.as_ref().unwrap(), 0, bytemuck::cast_slice(&quad));
    }

    /// (Re)create the GPU target for one offscreen `Graphics` canvas and
    /// upload this frame's vertices.
    fn prepare_gfx_target(&mut self, gf: &GraphicsFrame) {
        let needs_new = self
            .gfx_targets
            .get(&gf.id)
            .map(|t| t.width != gf.width || t.height != gf.height)
            .unwrap_or(true);

        if needs_new {
            let extent = wgpu::Extent3d {
                width: gf.width.max(1),
                height: gf.height.max(1),
                depth_or_array_layers: 1,
            };
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("oripop graphics color"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: GFX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("oripop graphics msaa"),
                size: extent,
                mip_level_count: 1,
                sample_count: GFX_MSAA,
                dimension: wgpu::TextureDimension::D2,
                format: GFX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());

            let uniform = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oripop graphics uniforms"),
                contents: bytemuck::cast_slice(&[Uniforms {
                    resolution: [gf.width as f32, gf.height as f32],
                    _pad: [0.0; 2],
                }]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let render_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oripop graphics render bind"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.white_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            // Sampling bind group reuses the main uniform buffer (its
            // contents track window resolution; the binding stays valid).
            let sample_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oripop graphics sample bind"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&color_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
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

        let target = self.gfx_targets.get_mut(&gf.id).expect("gfx target");
        let bytes = bytemuck::cast_slice::<Vertex, u8>(&gf.vertices);
        if !bytes.is_empty() {
            let needed = bytes.len() as u64;
            if target.vbuf_cap < needed {
                let cap = needed.next_power_of_two().max(4096);
                target.vbuf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("oripop graphics vertices"),
                    size: cap,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                target.vbuf_cap = cap;
            }
            self.queue.write_buffer(target.vbuf.as_ref().unwrap(), 0, bytes);
        }
    }

    fn render(&mut self, frame: &Frame2D) -> Result<(), wgpu::SurfaceError> {
        let output      = self.surface.get_current_texture()?;
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Mutable phase: prepare all GPU resources ──
        for gf in &frame.graphics {
            self.prepare_gfx_target(gf);
        }
        self.ensure_canvas_targets(frame.density);
        self.write_blit_quad();

        // Clear the canvas when background() was called, and always on a
        // fresh canvas texture (its content is undefined).
        let canvas_load = if frame.clear || !self.canvas_init {
            wgpu::LoadOp::Clear(frame.bg)
        } else {
            wgpu::LoadOp::Load
        };
        self.canvas_init = true;

        // Upload main vertices into the persistent buffer.
        let bytes = bytemuck::cast_slice::<Vertex, u8>(&frame.vertices);
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

        // ── Encode phase (immutable borrows only) ──
        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("oripop render") },
        );

        // Offscreen graphics passes (before the canvas pass samples them).
        for gf in &frame.graphics {
            let Some(target) = self.gfx_targets.get(&gf.id) else { continue };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oripop graphics pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa_view,
                    resolve_target: Some(&target.color_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(gf.bg),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let Some(vbuf) = target.vbuf.as_ref() {
                if !gf.vertices.is_empty() {
                    pass.set_pipeline(&self.gfx_pipeline);
                    pass.set_bind_group(0, &target.render_bind, &[]);
                    pass.set_vertex_buffer(0, vbuf.slice(..));
                    pass.draw(0..gf.vertices.len() as u32, 0..1);
                }
            }
        }

        // Canvas pass: replay draw runs onto the persistent canvas.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oripop canvas pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           self.canvas_msaa_view.as_ref().unwrap(),
                    resolve_target: Some(self.canvas_color_view.as_ref().unwrap()),
                    ops: wgpu::Operations {
                        load:  canvas_load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:        None,
                occlusion_query_set:     None,
            });

            if has_verts {
                pass.set_pipeline(&self.canvas_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buf.as_ref().unwrap().slice(..));
                for run in &frame.runs {
                    let bind = if run.tex == 0 {
                        &self.uniform_bind_group
                    } else {
                        match self.gfx_targets.get(&run.tex) {
                            Some(t) => &t.sample_bind,
                            None => continue,
                        }
                    };
                    pass.set_bind_group(0, bind, &[]);
                    pass.draw(run.start..run.start + run.count, 0..1);
                }
            }
        }

        // Blit pass: present the canvas to the window surface.
        {
            let (target_view, resolve_target) = match &self.msaa_view {
                Some(msaa) => (msaa, Some(&surface_view)),
                None       => (&surface_view, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oripop blit pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           target_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes:        None,
                occlusion_query_set:     None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, self.canvas_bind.as_ref().unwrap(), &[]);
            pass.set_vertex_buffer(0, self.blit_vbuf.as_ref().unwrap().slice(..));
            pass.draw(0..6, 0..1);
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

    let make_pipeline = |label: &str,
                         target: wgpu::TextureFormat,
                         samples: u32,
                         blend: wgpu::BlendState| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
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
                    format: target,
                    blend: Some(blend),
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
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    };

    // The surface pipeline only blits the opaque canvas: blending must be
    // OFF so accumulated sub-1.0 canvas alpha can't bleed the clear color
    // through (that reads as a dark halo around translucent content).
    let pipeline = make_pipeline("oripop blit pipeline", format, msaa_samples, wgpu::BlendState::REPLACE);
    let gfx_pipeline =
        make_pipeline("oripop graphics pipeline", GFX_FORMAT, GFX_MSAA, wgpu::BlendState::ALPHA_BLENDING);
    let canvas_pipeline =
        make_pipeline("oripop canvas pipeline", CANVAS_FORMAT, GFX_MSAA, wgpu::BlendState::ALPHA_BLENDING);
    let save_pipeline = make_pipeline(
        "oripop save pipeline",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        wgpu::BlendState::REPLACE,
    );

    let msaa_view = create_msaa_texture(&device, format, phys_w, phys_h, msaa_samples);

    let scale_factor = phys_w as f64 / logical_w.max(1) as f64;

    Gpu {
        surface,
        device,
        queue,
        config,
        pipeline,
        layout: bind_group_layout,
        white_view,
        sampler,
        uniform_buffer,
        uniform_bind_group,
        msaa_view,
        msaa_samples,
        surface_format: format,
        scale_factor,
        vertex_buf:     None,
        vertex_buf_cap: 0,
        gfx_pipeline,
        canvas_pipeline,
        gfx_targets: std::collections::HashMap::new(),
        canvas_color_view: None,
        canvas_msaa_view: None,
        canvas_bind: None,
        canvas_size: (0, 0),
        canvas_density: 0,
        canvas_tex_size: (0, 0),
        canvas_init: false,
        blit_vbuf: None,
        save_pipeline,
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
        let bg    = ctx.rec.bg;
        let bytes = bytemuck::cast_slice(&ctx.rec.vertices).to_vec();
        ctx.rec.vertices.clear();
        ctx.rec.runs.clear();
        ctx.graphics_frames.clear();
        (bg, bytes)
    })
}

/// Drain the full frame (vertices + draw runs + offscreen graphics) for the
/// windowed host. Internal: external hosts use [`take_2d_vertices`].
pub(crate) fn take_frame_2d() -> Frame2D {
    with_ctx(|ctx| Frame2D {
        bg: ctx.rec.bg,
        clear: ctx.rec.bg_set,
        density: ctx.density,
        vertices: std::mem::take(&mut ctx.rec.vertices),
        runs: std::mem::take(&mut ctx.rec.runs),
        graphics: std::mem::take(&mut ctx.graphics_frames),
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
    last_frame:   Option<std::time::Instant>,
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
                // Optional frame limiter (frame_rate()). Default is vsync.
                if let Some(fps) = with_ctx(|ctx| ctx.target_fps) {
                    let target = std::time::Duration::from_secs_f32(1.0 / fps);
                    if let Some(last) = self.last_frame {
                        let elapsed = last.elapsed();
                        if elapsed < target {
                            std::thread::sleep(target - elapsed);
                        }
                    }
                }
                self.last_frame = Some(std::time::Instant::now());

                with_ctx(|ctx| ctx.reset_frame());
                (self.draw_fn)();
                let frame = take_frame_2d();

                match gpu.render(&frame) {
                    Ok(()) => {
                        // Snapshot request (save_frame) after a good frame.
                        if let Some(path) = with_ctx(|ctx| ctx.pending_save.take()) {
                            match gpu.save_canvas_png(&path) {
                                Ok(()) => eprintln!("saved {}", path.display()),
                                Err(e) => log::error!("save_frame failed: {e}"),
                            }
                        }
                    }
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
                let handler = with_ctx(|ctx| {
                    ctx.mouse_x = position.x as f32 / gpu.scale_factor as f32;
                    ctx.mouse_y = position.y as f32 / gpu.scale_factor as f32;
                    if ctx.mouse_pressed {
                        ctx.handlers.mouse_dragged
                    } else {
                        ctx.handlers.mouse_moved
                    }
                });
                if let Some(f) = handler {
                    f();
                }
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                let handler = with_ctx(|ctx| {
                    ctx.mouse_pressed = pressed;
                    if pressed {
                        ctx.mouse_button = match button {
                            winit::event::MouseButton::Left => Some(MouseButton::Left),
                            winit::event::MouseButton::Right => Some(MouseButton::Right),
                            winit::event::MouseButton::Middle => Some(MouseButton::Center),
                            _ => None,
                        };
                        ctx.handlers.mouse_pressed
                    } else {
                        ctx.handlers.mouse_released
                    }
                });
                if let Some(f) = handler {
                    f();
                }
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
                };
                let handler = with_ctx(|ctx| {
                    ctx.wheel_accum += lines;
                    ctx.handlers.mouse_wheel
                });
                if let Some(f) = handler {
                    f(lines);
                }
                if !with_ctx(|ctx| ctx.continuous_redraw) {
                    window.request_redraw();
                }
            }

            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let pressed = key_event.state == ElementState::Pressed;
                let handler = with_ctx(|ctx| {
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
                        ctx.handlers.key_pressed
                    } else {
                        ctx.handlers.key_released
                    }
                });
                if let Some(f) = handler {
                    f();
                }
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

    let mut app = Runner2D { draw_fn, window_attrs, msaa, window: None, gpu: None, last_frame: None };
    event_loop.run_app(&mut app).expect("event loop error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::create_graphics;

    #[test]
    fn solid_drawing_coalesces_into_one_run() {
        begin_frame();
        stroke(255, 255, 255);
        line(0.0, 0.0, 10.0, 0.0);
        line(0.0, 5.0, 10.0, 5.0);
        fill(100, 100, 100);
        rect(2.0, 2.0, 4.0, 4.0);
        let frame = take_frame_2d();
        assert!(!frame.vertices.is_empty());
        assert_eq!(frame.runs.len(), 1, "solid geometry must share one run");
        assert_eq!(frame.runs[0].tex, 0);
        assert_eq!(frame.runs[0].count as usize, frame.vertices.len());
        assert!(frame.graphics.is_empty());
    }

    #[test]
    fn image_emits_textured_run_and_graphics_snapshot() {
        begin_frame();
        let mut g = create_graphics(64, 32);
        g.background(10, 10, 10);
        g.fill(200, 100, 50);
        g.rect(8.0, 8.0, 16.0, 16.0);

        stroke(255, 255, 255);
        line(0.0, 0.0, 10.0, 0.0);
        image(&g, 4.0, 4.0);
        line(0.0, 5.0, 10.0, 5.0);

        let frame = take_frame_2d();
        // solid run, textured quad run, solid run again
        assert_eq!(frame.runs.len(), 3);
        assert_eq!(frame.runs[0].tex, 0);
        assert_ne!(frame.runs[1].tex, 0);
        assert_eq!(frame.runs[1].count, 6);
        assert_eq!(frame.runs[2].tex, 0);
        let total: u32 = frame.runs.iter().map(|r| r.count).sum();
        assert_eq!(total as usize, frame.vertices.len());

        assert_eq!(frame.graphics.len(), 1);
        assert_eq!(frame.graphics[0].id, frame.runs[1].tex);
        assert_eq!(frame.graphics[0].width, 64);
        assert_eq!(frame.graphics[0].height, 32);
        assert!(!frame.graphics[0].vertices.is_empty());
    }

    #[test]
    fn new_primitives_emit_geometry() {
        begin_frame();
        no_stroke();
        fill(255, 255, 255);
        quad(0.0, 0.0, 10.0, 0.0, 12.0, 8.0, 2.0, 8.0);
        let after_quad = take_frame_2d().vertices.len();
        assert!(after_quad > 0);

        begin_frame();
        stroke(255, 255, 255);
        no_fill();
        arc(50.0, 50.0, 40.0, 40.0, 0.0, std::f32::consts::PI);
        bezier(0.0, 0.0, 10.0, 20.0, 30.0, 20.0, 40.0, 0.0);
        curve(0.0, 0.0, 10.0, 10.0, 30.0, 10.0, 40.0, 0.0);
        assert!(!take_frame_2d().vertices.is_empty());
    }

    #[test]
    fn shape_with_contour_fills_hole() {
        begin_frame();
        no_stroke();
        fill(255, 255, 255);
        // 100x100 square with a 50x50 hole: even-odd fill leaves the ring.
        begin_shape();
        vertex(0.0, 0.0);
        vertex(100.0, 0.0);
        vertex(100.0, 100.0);
        vertex(0.0, 100.0);
        begin_contour();
        vertex(25.0, 25.0);
        vertex(75.0, 25.0);
        vertex(75.0, 75.0);
        vertex(25.0, 75.0);
        end_contour();
        end_shape_close();
        let frame = take_frame_2d();
        assert!(!frame.vertices.is_empty());
        // The hole's center must not be covered by any triangle.
        let inside = |px: f32, py: f32| {
            frame.vertices.chunks(3).any(|t| {
                let s = |a: &Vertex, b: &Vertex| {
                    (px - b.position[0]) * (a.position[1] - b.position[1])
                        - (a.position[0] - b.position[0]) * (py - b.position[1])
                };
                let (d1, d2, d3) = (s(&t[0], &t[1]), s(&t[1], &t[2]), s(&t[2], &t[0]));
                let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
                let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
                !(has_neg && has_pos)
            })
        };
        assert!(inside(10.0, 10.0), "ring area must be filled");
        assert!(!inside(50.0, 50.0), "hole must stay empty");
    }

    #[test]
    fn curve_math_matches_endpoints() {
        // Catmull-Rom passes through its inner anchors.
        assert!((curve_point(0.0, 1.0, 2.0, 3.0, 0.0) - 1.0).abs() < 1e-5);
        assert!((curve_point(0.0, 1.0, 2.0, 3.0, 1.0) - 2.0).abs() < 1e-5);
        // Bezier hits its anchors.
        assert!((bezier_point(1.0, 5.0, 9.0, 3.0, 0.0) - 1.0).abs() < 1e-5);
        assert!((bezier_point(1.0, 5.0, 9.0, 3.0, 1.0) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn shape_modes_resolve_boxes() {
        assert_eq!(resolve_box(ShapeMode::Corner, 10.0, 20.0, 30.0, 40.0), (10.0, 20.0, 30.0, 40.0));
        assert_eq!(resolve_box(ShapeMode::Corners, 10.0, 20.0, 40.0, 60.0), (10.0, 20.0, 30.0, 40.0));
        assert_eq!(resolve_box(ShapeMode::Center, 25.0, 40.0, 30.0, 40.0), (10.0, 20.0, 30.0, 40.0));
        assert_eq!(resolve_box(ShapeMode::Radius, 25.0, 40.0, 15.0, 20.0), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn frame_pattern_expands_hashes() {
        assert_eq!(expand_frame_pattern("out/frame-####.png", 7), "out/frame-0007.png");
        assert_eq!(expand_frame_pattern("a#b", 12), "a12b");
        assert_eq!(expand_frame_pattern("plain.png", 3), "plain.png");
    }

    #[test]
    fn translucent_background_is_a_wash_not_a_clear() {
        begin_frame();
        background_a(10, 10, 10, 24); // translucent: wash quad, no clear
        let frame = take_frame_2d();
        assert!(!frame.clear, "translucent background must not hard-clear");
        assert_eq!(frame.vertices.len(), 6, "wash is a fullscreen quad");

        begin_frame();
        background(10, 10, 10); // opaque: clear, no geometry
        let frame = take_frame_2d();
        assert!(frame.clear);
        assert!(frame.vertices.is_empty());
    }

    #[test]
    fn hsb_color_mode_resolves_known_hues() {
        // Pure red: hue 0, full saturation/brightness.
        assert_eq!(hsb_to_rgb(0, 255, 255), (255, 0, 0));
        // Green is one third around the wheel.
        assert_eq!(hsb_to_rgb(85, 255, 255), (0, 255, 0));
        // Zero saturation is gray at the brightness level.
        assert_eq!(hsb_to_rgb(123, 0, 128), (128, 128, 128));
    }

    #[test]
    fn lerp_color_interpolates_channels() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgba(255, 100, 0, 55);
        let mid = lerp_color(a, b, 0.5);
        assert_eq!((mid.r, mid.g, mid.b), (128, 50, 0));
        assert_eq!(lerp_color(a, b, 0.0), Color::rgba(0, 0, 0, 255));
        assert_eq!(lerp_color(a, b, 1.0), b);
    }

    #[test]
    fn push_pop_style_round_trips() {
        let mut rec = Recorder::new();
        rec.set_fill_a(10, 20, 30, 255);
        rec.push_style();
        rec.set_fill_a(200, 200, 200, 255);
        rec.set_stroke_weight(9.0);
        rec.pop_style();
        assert_eq!(rec.state.fill_color, Color::rgb(10, 20, 30).to_f32());
        assert_eq!(rec.state.stroke_weight, 1.0);
    }

    #[test]
    fn graphics_background_clears_recorded_geometry() {
        let mut g = create_graphics(32, 32);
        g.fill(255, 0, 0);
        g.rect(0.0, 0.0, 8.0, 8.0);
        let before = g.snapshot().vertices.len();
        assert!(before > 0);
        g.background(0, 0, 0);
        assert_eq!(g.snapshot().vertices.len(), 0);
    }
}
