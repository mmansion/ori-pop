//! Offscreen canvases (`create_graphics`) — Processing's `PGraphics`.
//!
//! A [`Graphics`] records drawing commands into its own surface, independent
//! of the main canvas. The windowed host renders it to a GPU texture once per
//! frame and samples it wherever [`crate::draw::image`] /
//! [`crate::draw::image_sized`] placed it.
//!
//! ```no_run
//! use oripop_canvas::prelude::*;
//!
//! fn draw_with(g: &mut Graphics) {
//!     g.background(20, 20, 30);          // clears previous content
//!     g.stroke(255, 200, 100);
//!     g.line(0.0, 0.0, 100.0, 100.0);
//!     image(g, 40.0, 40.0);              // place on the main canvas
//! }
//! ```
//!
//! Unlike the main canvas, [`Graphics::background`] *clears* recorded
//! geometry: skip it to let content accumulate across frames, call it to
//! redraw from scratch.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::draw::{ArcMode, GraphicsFrame, Recorder, ShapeMode, StrokeCap, StrokeJoin};

/// Ids start at 1; 0 is the solid-color sentinel in draw runs.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// An offscreen drawing surface with its own state and transform stack.
pub struct Graphics {
    id: u64,
    width: u32,
    height: u32,
    rec: Recorder,
}

/// Create an offscreen canvas of the given size in logical pixels.
pub fn create_graphics(width: u32, height: u32) -> Graphics {
    Graphics {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        width: width.max(1),
        height: height.max(1),
        rec: Recorder::new(),
    }
}

impl Graphics {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Copy this canvas's recorded content for the current frame.
    pub(crate) fn snapshot(&self) -> GraphicsFrame {
        GraphicsFrame {
            id: self.id,
            width: self.width,
            height: self.height,
            bg: self.rec.bg,
            vertices: self.rec.vertices.clone(),
        }
    }

    /// Clear the canvas to an opaque RGB color, discarding previously
    /// recorded geometry.
    pub fn background(&mut self, r: u8, g: u8, b: u8) {
        self.background_a(r, g, b, 255);
    }

    /// Clear the canvas to an RGBA color, discarding previously recorded
    /// geometry.
    pub fn background_a(&mut self, r: u8, g: u8, b: u8, a: u8) {
        self.rec.clear();
        self.rec.set_background_a(r, g, b, a);
    }

    pub fn stroke(&mut self, r: u8, g: u8, b: u8) {
        self.rec.set_stroke_a(r, g, b, 255);
    }

    pub fn stroke_a(&mut self, r: u8, g: u8, b: u8, a: u8) {
        self.rec.set_stroke_a(r, g, b, a);
    }

    pub fn no_stroke(&mut self) {
        self.rec.set_no_stroke();
    }

    pub fn fill(&mut self, r: u8, g: u8, b: u8) {
        self.rec.set_fill_a(r, g, b, 255);
    }

    pub fn fill_a(&mut self, r: u8, g: u8, b: u8, a: u8) {
        self.rec.set_fill_a(r, g, b, a);
    }

    pub fn no_fill(&mut self) {
        self.rec.set_no_fill();
    }

    pub fn stroke_weight(&mut self, w: f32) {
        self.rec.set_stroke_weight(w);
    }

    pub fn push(&mut self) {
        self.rec.push();
    }

    pub fn pop(&mut self) {
        self.rec.pop();
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.rec.translate(dx, dy);
    }

    pub fn rotate(&mut self, angle: f32) {
        self.rec.rotate(angle);
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.rec.scale(sx, sy);
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.rec.line(x1, y1, x2, y2);
    }

    pub fn point(&mut self, x: f32, y: f32) {
        self.rec.point(x, y);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rec.rect(x, y, w, h);
    }

    pub fn ellipse(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rec.ellipse(x, y, w, h);
    }

    pub fn circle(&mut self, x: f32, y: f32, d: f32) {
        self.rec.circle(x, y, d);
    }

    pub fn square(&mut self, x: f32, y: f32, s: f32) {
        self.rec.square(x, y, s);
    }

    pub fn quad(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) {
        self.rec.quad(x1, y1, x2, y2, x3, y3, x4, y4);
    }

    pub fn triangle(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        self.rec.triangle(x1, y1, x2, y2, x3, y3);
    }

    pub fn arc(&mut self, x: f32, y: f32, w: f32, h: f32, start: f32, stop: f32) {
        self.rec.arc(x, y, w, h, start, stop, ArcMode::Open);
    }

    pub fn arc_with_mode(&mut self, x: f32, y: f32, w: f32, h: f32, start: f32, stop: f32, mode: ArcMode) {
        self.rec.arc(x, y, w, h, start, stop, mode);
    }

    pub fn bezier(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) {
        self.rec.bezier(x1, y1, x2, y2, x3, y3, x4, y4);
    }

    pub fn curve(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) {
        self.rec.curve(x1, y1, x2, y2, x3, y3, x4, y4);
    }

    pub fn begin_shape(&mut self) {
        self.rec.begin_shape();
    }

    pub fn vertex(&mut self, x: f32, y: f32) {
        self.rec.vertex(x, y);
    }

    pub fn bezier_vertex(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        self.rec.bezier_vertex(cx1, cy1, cx2, cy2, x, y);
    }

    pub fn quadratic_vertex(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.rec.quadratic_vertex(cx, cy, x, y);
    }

    pub fn curve_vertex(&mut self, x: f32, y: f32) {
        self.rec.curve_vertex(x, y);
    }

    pub fn begin_contour(&mut self) {
        self.rec.begin_contour();
    }

    pub fn end_contour(&mut self) {
        self.rec.end_contour();
    }

    pub fn end_shape(&mut self) {
        self.rec.end_shape(false);
    }

    pub fn end_shape_close(&mut self) {
        self.rec.end_shape(true);
    }

    pub fn rect_mode(&mut self, mode: ShapeMode) {
        self.rec.set_rect_mode(mode);
    }

    pub fn ellipse_mode(&mut self, mode: ShapeMode) {
        self.rec.set_ellipse_mode(mode);
    }

    pub fn stroke_cap(&mut self, cap: StrokeCap) {
        self.rec.set_stroke_cap(cap);
    }

    pub fn stroke_join(&mut self, join: StrokeJoin) {
        self.rec.set_stroke_join(join);
    }
}
