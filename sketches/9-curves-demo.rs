//! Bézier path → [`Force::BezierPath`] with editable density **timing** on the curve.
//!
//! - **Blue handles:** cubic Bézier control points (spatial shape).
//! - **Coral markers:** slide along the curve; they set `t1` and `t2` on [`DensityProfile`], shifting
//!   how the four knot multipliers `y0`…`y3` are distributed from curve start to end.
//! - **Space** toggles overlay.
//!
//! Event-driven redraw; light preview while dragging, full stipple after release.

use std::cell::RefCell;

use oripop_canvas::prelude::*;
use oripop_canvas::{generate_dots, Bezier, DensityProfile, Dot, Force, Params, Point};

const W: f32 = 880.0;
const H: f32 = 880.0;

const DOTS_IDLE: u32 = 18_000;
const DOTS_DRAG: u32 = 1_600;

const HIT_SPATIAL: f32 = 36.0;
const HIT_DENSITY: f32 = 30.0;
/// Minimum gap between t=0, t1, t2, t=1 when dragging density knots.
const T_MARGIN: f32 = 0.04;

thread_local! {
    static EDITOR: RefCell<EditorState> = RefCell::new(EditorState::default());
}

struct EditorState {
    spatial: Bezier,
    density: DensityProfile,
    show_overlay: bool,
    drag: Drag,
    prev_space_held: bool,
    path_falloff: f32,

    cached_dots: Vec<Dot>,
    cache_valid: bool,
    cache_spatial: Bezier,
    cache_density: DensityProfile,
    cache_falloff: f32,
    cache_dot_count: u32,
    cache_was_preview: bool,
}

#[derive(Clone, Copy, Default)]
enum Drag {
    #[default]
    None,
    Spatial(u8),
    /// 0 → `t1`, 1 → `t2`
    DensityT(u8),
}

impl Default for EditorState {
    fn default() -> Self {
        let spatial = Bezier::new(
            Point::new(0.06, 0.88),
            Point::new(0.38, 0.62),
            Point::new(0.62, 0.28),
            Point::new(0.94, 0.06),
        );
        Self {
            spatial,
            density: DensityProfile::default(),
            show_overlay: true,
            drag: Drag::None,
            prev_space_held: false,
            path_falloff: 110.0,
            cached_dots: Vec::new(),
            cache_valid: false,
            cache_spatial: spatial,
            cache_density: DensityProfile::default(),
            cache_falloff: 110.0,
            cache_dot_count: 0,
            cache_was_preview: false,
        }
    }
}

fn main() {
    eprintln!(
        "curves-demo — blue: Bezier handles; coral: slide density timing on curve; Space: overlay."
    );
    size(W as u32, H as u32);
    title("curve-controlled density");
    smooth(1);
    redraw_continuous(false);
    run(draw);
}

fn spatial_knot(b: &Bezier, i: u8) -> Point {
    match i {
        0 => b.p0,
        1 => b.p1,
        2 => b.p2,
        _ => b.p3,
    }
}

fn set_spatial_knot(b: &mut Bezier, i: u8, x: f32, y: f32) {
    let p = Point::new(x, y);
    match i {
        0 => b.p0 = p,
        1 => b.p1 = p,
        2 => b.p2 = p,
        _ => b.p3 = p,
    }
}

fn dist_sq(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy
}

fn density_knot_screen(b: &Bezier, density: &DensityProfile, which: u8) -> (f32, f32) {
    let t = if which == 0 { density.t1 } else { density.t2 };
    let t = t.clamp(T_MARGIN, 1.0 - T_MARGIN);
    let p = b.eval(t);
    (p.x * W, p.y * H)
}

fn pick_handle(mx: f32, my: f32, s: &EditorState) -> Option<Drag> {
    if !s.show_overlay {
        return None;
    }
    let mut best_d = f32::MAX;
    let mut best: Option<Drag> = None;

    for i in 0u8..4 {
        let p = spatial_knot(&s.spatial, i);
        let sx = p.x * W;
        let sy = p.y * H;
        let d = dist_sq(mx, my, sx, sy);
        if d <= HIT_SPATIAL * HIT_SPATIAL && d < best_d {
            best_d = d;
            best = Some(Drag::Spatial(i));
        }
    }

    for i in 0u8..2 {
        let (sx, sy) = density_knot_screen(&s.spatial, &s.density, i);
        let d = dist_sq(mx, my, sx, sy);
        if d <= HIT_DENSITY * HIT_DENSITY && d < best_d {
            best_d = d;
            best = Some(Drag::DensityT(i));
        }
    }

    best
}

fn drag_active(d: Drag) -> bool {
    !matches!(d, Drag::None)
}

fn build_params(s: &EditorState, dot_count: u32, preview: bool) -> Params {
    let mut params = Params::default();
    params.canvas.width = W;
    params.canvas.height = H;
    params.distribution.dot_count = dot_count;
    params.distribution.density_pow = if preview { 1.2 } else { 1.35 };
    params.field.singularity.strength = 0.0;
    if preview {
        params.field.warp_amount = 0.0;
        params.field.warp_frequency = 0.0;
    } else {
        params.field.warp_amount = 0.018;
        params.field.warp_frequency = 5.5;
    }
    params.field.forces = vec![Force::BezierPath {
        curve: s.spatial,
        profile: s.density,
        falloff: s.path_falloff,
        strength: 1.0,
    }];
    params
}

fn refresh_stipple(s: &mut EditorState) {
    let dragging = drag_active(s.drag);
    let preview = dragging;
    let want_count = if preview { DOTS_DRAG } else { DOTS_IDLE };

    let field_changed = !s.cache_valid
        || s.spatial != s.cache_spatial
        || s.density != s.cache_density
        || (s.path_falloff - s.cache_falloff).abs() > 1e-4
        || want_count != s.cache_dot_count
        || preview != s.cache_was_preview;

    if !field_changed {
        return;
    }

    let p = build_params(s, want_count, preview);
    s.cached_dots = generate_dots(&p, 0.0);
    s.cache_spatial = s.spatial;
    s.cache_density = s.density;
    s.cache_falloff = s.path_falloff;
    s.cache_dot_count = want_count;
    s.cache_was_preview = preview;
    s.cache_valid = true;
}

fn apply_density_drag(s: &mut EditorState, mx: f32, my: f32) {
    let Drag::DensityT(which) = s.drag else {
        return;
    };
    let p = Point::new((mx / W).clamp(0.0, 1.0), (my / H).clamp(0.0, 1.0));
    let mut t_new = s.spatial.closest_param(&p, 96).clamp(T_MARGIN, 1.0 - T_MARGIN);

    let t1 = s.density.t1;
    let t2 = s.density.t2;
    if which == 0 {
        let hi = (t2 - T_MARGIN).max(T_MARGIN + 1e-4);
        t_new = t_new.clamp(T_MARGIN, hi);
        s.density.t1 = t_new;
    } else {
        let lo = (t1 + T_MARGIN).min(1.0 - T_MARGIN - 1e-4);
        t_new = t_new.clamp(lo, 1.0 - T_MARGIN);
        s.density.t2 = t_new;
    }
}

fn draw_spatial_bezier(b: &Bezier) {
    stroke(120, 200, 160);
    stroke_weight(2.0);
    let steps = 48;
    let mut px = b.eval(0.0);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let q = b.eval(t);
        line(px.x * W, px.y * H, q.x * W, q.y * H);
        px = q;
    }
}

fn draw_control_polyline(b: &Bezier) {
    stroke(70, 90, 120);
    stroke_weight(1.0);
    line(b.p0.x * W, b.p0.y * H, b.p1.x * W, b.p1.y * H);
    line(b.p2.x * W, b.p2.y * H, b.p3.x * W, b.p3.y * H);
}

fn draw_spatial_handles(b: &Bezier, drag: Drag) {
    let r = 13.0f32;
    for i in 0u8..4 {
        let p = spatial_knot(b, i);
        let sx = p.x * W;
        let sy = p.y * H;
        let hi = matches!(drag, Drag::Spatial(j) if j == i);
        if hi {
            fill(255, 220, 140);
            stroke(255, 255, 255);
        } else {
            fill(90, 140, 220);
            stroke(40, 60, 90);
        }
        stroke_weight(2.0);
        ellipse(sx - r, sy - r, r * 2.0, r * 2.0);
    }
}

/// Triangular markers on the curve for density timing knots (t₁, t₂).
fn draw_density_handles(b: &Bezier, density: &DensityProfile, drag: Drag) {
    let half = 11.0f32;
    for i in 0u8..2 {
        let (cx, cy) = density_knot_screen(b, density, i);
        let hi = matches!(drag, Drag::DensityT(j) if j == i);
        if hi {
            fill(255, 150, 120);
            stroke(255, 255, 255);
        } else {
            fill(220, 100, 85);
            stroke(80, 35, 30);
        }
        stroke_weight(2.0);
        triangle(
            cx,
            cy - half,
            cx - half,
            cy + half * 0.85,
            cx + half,
            cy + half * 0.85,
        );
    }
}

fn draw() {
    EDITOR.with(|cell| {
        let mut s = cell.borrow_mut();

        let space_held = key_pressed() && key() == ' ';
        if space_held && !s.prev_space_held {
            s.show_overlay = !s.show_overlay;
        }
        s.prev_space_held = space_held;

        let mx = mouse_x();
        let my = mouse_y();

        if mouse_pressed() {
            if matches!(s.drag, Drag::None) {
                if let Some(d) = pick_handle(mx, my, &s) {
                    s.drag = d;
                }
            }
        } else {
            s.drag = Drag::None;
        }

        match s.drag {
            Drag::Spatial(i) => {
                let nx = (mx / W).clamp(0.0, 1.0);
                let ny = (my / H).clamp(0.0, 1.0);
                set_spatial_knot(&mut s.spatial, i, nx, ny);
            }
            Drag::DensityT(_) => apply_density_drag(&mut s, mx, my),
            Drag::None => {}
        }

        refresh_stipple(&mut s);

        background(10, 10, 14);

        no_stroke();
        for dot in &s.cached_dots {
            let w = dot.w;
            let v = (35.0 + w * 210.0).min(255.0) as u8;
            fill(v, v, v + 6);
            let sz = dot.r * W * 2.0;
            rect(dot.x - sz * 0.5, dot.y - sz * 0.5, sz, sz);
        }

        if s.show_overlay {
            draw_control_polyline(&s.spatial);
            draw_spatial_bezier(&s.spatial);
            draw_density_handles(&s.spatial, &s.density, s.drag);
            draw_spatial_handles(&s.spatial, s.drag);
        }
    });
}
