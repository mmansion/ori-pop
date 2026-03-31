//! Bézier path stipples in **3D**: dots are rasterized into a CPU buffer, uploaded as a texture, and
//! sampled on a [`MeshKind::Plane`] in the real wgpu path ([`ObjectTexture::StippleCanvas`]).
//!
//! - **Blue handles / coral timing markers:** drawn in a 2D overlay, projected from the stipple plane so
//!   they stay glued to the texture when you orbit (toggle with **H**). **Space** toggles the egui inspector.
//! - Default framing is orthographic from +Z; with **Orbit camera** off, that view is restored each frame.
//!
//! Run: `cargo run -p sketches --bin curves-3d-demo`

use std::cell::RefCell;

use oripop_3d::prelude::*;
use oripop_core::{generate_dots, Bezier, DensityProfile, Dot, Force, Params, Point};

/// Bézier `(x,y)` in \([0,1]^2\) (top-left origin) ↔ unit plane local \([-0.5,0.5]^2\) at \(Z=0\),
/// matching stipple UV upload (`v = 1 - y`).
#[inline]
fn bezier_to_plane_local(b: &Point) -> Vec3 {
    Vec3::new(b.x - 0.5, 0.5 - b.y, 0.0)
}

/// Project a plane-local point through `mvp` into logical pixel coords (same convention as
/// [`oripop_core`] / `shader.wgsl`).
fn project_plane_local_to_screen(mvp: Mat4, w: f32, h: f32, local: Vec3) -> Option<Vec2> {
    let clip = mvp * local.extend(1.0);
    if clip.w.abs() < 1e-8 {
        return None;
    }
    if clip.w <= 0.0 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let px = (ndc_x + 1.0) * 0.5 * w;
    let py = (1.0 - ndc_y) * 0.5 * h;
    Some(Vec2::new(px, py))
}

fn project_point_to_screen(mvp: Mat4, w: f32, h: f32, p: &Point) -> Option<Vec2> {
    project_plane_local_to_screen(mvp, w, h, bezier_to_plane_local(p))
}

/// Ray through pixel `(mx, my)` → intersection with the XY plane of `plane_model` (Z normal).
fn screen_to_bezier_on_plane(
    view_proj: Mat4,
    plane_model: Mat4,
    mx: f32,
    my: f32,
    w: f32,
    h: f32,
) -> Option<Point> {
    let inv_vp = view_proj.inverse();
    let nx = (mx / w) * 2.0 - 1.0;
    let ny = 1.0 - (my / h) * 2.0;
    let near_h = inv_vp * Vec4::new(nx, ny, 0.0, 1.0);
    let far_h = inv_vp * Vec4::new(nx, ny, 1.0, 1.0);
    if near_h.w.abs() < 1e-8 || far_h.w.abs() < 1e-8 {
        return None;
    }
    let near = near_h.truncate() / near_h.w;
    let far = far_h.truncate() / far_h.w;
    let dir = (far - near).normalize();
    let n_w = plane_model.transform_vector3(Vec3::Z);
    if n_w.length_squared() < 1e-12 {
        return None;
    }
    let n_w = n_w.normalize();
    let denom = dir.dot(n_w);
    if denom.abs() < 1e-6 {
        return None;
    }
    let p0 = plane_model.transform_point3(Vec3::ZERO);
    let t = (p0 - near).dot(n_w) / denom;
    let hit = near + dir * t;
    let plane_inv = plane_model.inverse();
    let local = plane_inv.transform_point3(hit);
    if local.z.abs() > 0.02 {
        return None;
    }
    let bx = (local.x + 0.5).clamp(0.0, 1.0);
    let by = (0.5 - local.y).clamp(0.0, 1.0);
    Some(Point::new(bx, by))
}

const CANVAS: f32 = STIPPLE_CANVAS_SIZE as f32;
const DOTS_IDLE: u32 = 18_000;
const DOTS_DRAG: u32 = 1_600;
const HIT_SPATIAL: f32 = 36.0;
const HIT_DENSITY: f32 = 30.0;
const T_MARGIN: f32 = 0.04;

thread_local! {
    static EDITOR: RefCell<EditorState> = RefCell::new(EditorState::default());
}

struct EditorState {
    spatial: Bezier,
    density: DensityProfile,
    show_overlay: bool,
    drag: Drag,
    prev_h_held: bool,
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
            prev_h_held: false,
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
        "curves-3d-demo — stipple texture on a plane; H: handles; Space: inspector; ortho top view."
    );
    size(960, 720);
    title("curves → 3D stipple plane");
    smooth(4);
    redraw_continuous(false);
    run3d(draw);
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

fn density_knot_screen_proj(
    b: &Bezier,
    density: &DensityProfile,
    which: u8,
    mvp: Mat4,
    w: f32,
    h: f32,
) -> Option<(f32, f32)> {
    let t = if which == 0 { density.t1 } else { density.t2 };
    let t = t.clamp(T_MARGIN, 1.0 - T_MARGIN);
    let p = b.eval(t);
    project_point_to_screen(mvp, w, h, &p).map(|v| (v.x, v.y))
}

fn pick_handle(mx: f32, my: f32, mvp: Mat4, w: f32, h: f32, s: &EditorState) -> Option<Drag> {
    if !s.show_overlay {
        return None;
    }
    let mut best_d = f32::MAX;
    let mut best: Option<Drag> = None;

    for i in 0u8..4 {
        let p = spatial_knot(&s.spatial, i);
        let Some(v) = project_point_to_screen(mvp, w, h, &p) else {
            continue;
        };
        let d = dist_sq(mx, my, v.x, v.y);
        if d <= HIT_SPATIAL * HIT_SPATIAL && d < best_d {
            best_d = d;
            best = Some(Drag::Spatial(i));
        }
    }

    for i in 0u8..2 {
        let Some((sx, sy)) = density_knot_screen_proj(&s.spatial, &s.density, i, mvp, w, h) else {
            continue;
        };
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
    params.canvas.width = CANVAS;
    params.canvas.height = CANVAS;
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

fn apply_density_drag(
    s: &mut EditorState,
    mx: f32,
    my: f32,
    view_proj: Mat4,
    plane_model: Mat4,
    w: f32,
    h: f32,
) {
    let Drag::DensityT(which) = s.drag else {
        return;
    };
    let Some(p) = screen_to_bezier_on_plane(view_proj, plane_model, mx, my, w, h) else {
        return;
    };
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

fn blit_clear(buf: &mut [u8], bg: [u8; 4]) {
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&bg);
    }
}

/// Map logical Y (top-left origin, y grows down — same as dots from [`generate_dots`]) to a texture
/// row for GPU upload. The parametric plane uses `uv.v = 0` at **world −Y** (bottom of the ortho
/// view) and `v = 1` at **world +Y** (top); WebGPU’s first image row is `v = 0`, so we flip here or
/// the stipple field appears mirrored vertically on the mesh.
fn logical_row_to_texel_row(logical_y: i32, dim: i32) -> i32 {
    (dim - 1 - logical_y).clamp(0, dim - 1)
}

fn blit_rect(buf: &mut [u8], dim: u32, x0: i32, y0: i32, x1: i32, y1: i32, rgba: [u8; 4]) {
    let d = dim as i32;
    let x0 = x0.max(0).min(d);
    let y0 = y0.max(0).min(d);
    let x1 = x1.max(0).min(d);
    let y1 = y1.max(0).min(d);
    for y in y0..y1 {
        let ty = logical_row_to_texel_row(y, d);
        let row = (ty * d) as usize * 4;
        for x in x0..x1 {
            let i = row + x as usize * 4;
            buf[i..i + 4].copy_from_slice(&rgba);
        }
    }
}

fn raster_stipple(buf: &mut [u8], dots: &[Dot]) {
    blit_clear(buf, [10, 10, 14, 255]);
    let dim = STIPPLE_CANVAS_SIZE;
    for dot in dots {
        let sz = dot.r * CANVAS * 2.0;
        let half = (sz * 0.5).max(0.5);
        let cx = dot.x as i32;
        let cy = dot.y as i32;
        let r = half.ceil() as i32;
        let v = (35.0 + dot.w * 210.0).min(255.0) as u8;
        let rgba = [v, v, v.saturating_add(6), 255];
        blit_rect(buf, dim, cx - r, cy - r, cx + r, cy + r, rgba);
    }
}

fn draw_spatial_bezier(b: &Bezier, mvp: Mat4, w: f32, h: f32) {
    stroke(120, 200, 160);
    stroke_weight(2.0);
    let steps = 48;
    let mut prev_s = project_point_to_screen(mvp, w, h, &b.eval(0.0));
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let q_s = project_point_to_screen(mvp, w, h, &b.eval(t));
        if let (Some(a), Some(b)) = (prev_s, q_s) {
            line(a.x, a.y, b.x, b.y);
        }
        prev_s = q_s;
    }
}

fn draw_control_polyline(b: &Bezier, mvp: Mat4, w: f32, h: f32) {
    stroke(70, 90, 120);
    stroke_weight(1.0);
    let pts = [b.p0, b.p1, b.p2, b.p3];
    for wnd in pts.windows(2) {
        if let (Some(a), Some(b)) = (
            project_point_to_screen(mvp, w, h, &wnd[0]),
            project_point_to_screen(mvp, w, h, &wnd[1]),
        ) {
            line(a.x, a.y, b.x, b.y);
        }
    }
}

fn draw_spatial_handles(b: &Bezier, drag: Drag, mvp: Mat4, w: f32, h: f32) {
    let r = 13.0f32;
    for i in 0u8..4 {
        let p = spatial_knot(b, i);
        let Some(v) = project_point_to_screen(mvp, w, h, &p) else {
            continue;
        };
        let sx = v.x;
        let sy = v.y;
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

fn draw_density_handles(b: &Bezier, density: &DensityProfile, drag: Drag, mvp: Mat4, w: f32, h: f32) {
    let half = 11.0f32;
    for i in 0u8..2 {
        let Some((cx, cy)) = density_knot_screen_proj(b, density, i, mvp, w, h) else {
            continue;
        };
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

fn draw(scene: &mut Scene3D) {
    let w = scene.width.max(1.0);
    let h = scene.height.max(1.0);

    // Orbit / auto-spin are toggled in the egui inspector — do not reset them here (draw runs
    // before the inspector each frame, so assigning false would undo every click).
    //
    // When orbit is off, re-apply the default top-down ortho each frame. When orbit is on, keep the
    // camera the runner left from the previous frame so overlay projection matches the 3D pass.
    if !scene.orbit_enabled {
        scene.camera.projection = Projection::Orthographic;
        scene.camera.eye = Vec3::new(0.0, 0.0, 6.0);
        scene.camera.target = Vec3::ZERO;
        scene.camera.up = Vec3::Y;
        scene.camera.ortho_half_height = 0.55;
    }

    scene.light_dir = Vec3::new(0.3, -0.4, 1.0);

    background(10, 10, 14);

    let aspect = scene.aspect().max(1e-6);
    let hh = scene.camera.ortho_half_height;
    let sx = 2.0 * hh * aspect;
    let sy = 2.0 * hh;
    let plane_model = Mat4::from_scale(Vec3::new(sx, sy, 1.0));
    let view_proj = scene.camera.view_proj(aspect);
    let mvp = view_proj * plane_model;

    EDITOR.with(|cell| {
        let mut s = cell.borrow_mut();

        let h_held = key_pressed() && (key() == 'h' || key() == 'H');
        if h_held && !s.prev_h_held {
            s.show_overlay = !s.show_overlay;
        }
        s.prev_h_held = h_held;

        let mx = mouse_x();
        let my = mouse_y();

        if mouse_pressed() {
            if matches!(s.drag, Drag::None) {
                if let Some(d) = pick_handle(mx, my, mvp, w, h, &s) {
                    s.drag = d;
                }
            }
        } else {
            s.drag = Drag::None;
        }

        match s.drag {
            Drag::Spatial(i) => {
                if let Some(p) = screen_to_bezier_on_plane(view_proj, plane_model, mx, my, w, h) {
                    set_spatial_knot(&mut s.spatial, i, p.x, p.y);
                }
            }
            Drag::DensityT(_) => apply_density_drag(&mut s, mx, my, view_proj, plane_model, w, h),
            Drag::None => {}
        }

        refresh_stipple(&mut s);
        raster_stipple(&mut scene.stipple_canvas, &s.cached_dots);

        if s.show_overlay {
            draw_control_polyline(&s.spatial, mvp, w, h);
            draw_spatial_bezier(&s.spatial, mvp, w, h);
            draw_density_handles(&s.spatial, &s.density, s.drag, mvp, w, h);
            draw_spatial_handles(&s.spatial, s.drag, mvp, w, h);
        }
    });

    scene.clear();
    scene.add_named_with_texture(
        "Stipple plane",
        MeshKind::Plane,
        plane_model,
        ObjectTexture::StippleCanvas,
    );
}
