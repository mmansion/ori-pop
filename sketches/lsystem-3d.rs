//! Generative L-system rendered as a 3D wireframe, spinning in space.
//!
//! An L-system (axiom: `F`, rule: `F → F[+F][-F][^F][&F]`, 3 iterations)
//! produces a 3D branching tree via turtle graphics.  Branches are projected
//! to screen space and drawn as 2D lines — a true wireframe with no filled
//! polygons or textures.  The structure grows progressively from its root.
//!
//! Coordinate system: Z-up right-handed (Z = growth axis).
//!
//! Run with:
//!   cargo run --bin lsystem-3d

use oripop_3d::prelude::*;
use std::sync::OnceLock;

fn main() {
    size(960, 640);
    title("L-System 3D — generative wireframe");
    smooth(4);
    run3d(draw);
}

// ── L-system ─────────────────────────────────────────────────────────────────

/// Cached segments — built once on first frame, reused every frame after.
static SEGMENTS: OnceLock<Vec<[Vec3; 2]>> = OnceLock::new();

fn build_lsystem() -> Vec<[Vec3; 2]> {
    // String rewriting: F → F[+F][-F][^F][&F]
    // 3 iterations → 5³ = 125 draw segments.
    let mut string = String::from("F");
    for _ in 0..3 {
        string = string
            .chars()
            .flat_map(|ch| match ch {
                'F' => "F[+F][-F][^F][&F]".chars().collect::<Vec<_>>(),
                c   => vec![c],
            })
            .collect();
    }

    // 3D turtle state: position + orthonormal local frame (heading, left, up).
    #[derive(Clone)]
    struct State { pos: Vec3, h: Vec3, l: Vec3, u: Vec3 }

    let angle = 25.7_f32.to_radians();
    let step  = 0.13_f32; // tuned so the tree fits inside a unit cube

    let mut stack: Vec<State> = Vec::new();
    let mut st = State {
        pos: Vec3::new(0.0, 0.0, -0.45), // start near the bottom of the cube
        h:   Vec3::Z,                     // grow upward
        l:   Vec3::X,                     // local left
        u:   Vec3::Y,                     // local up
    };
    let mut segs: Vec<[Vec3; 2]> = Vec::new();

    for ch in string.chars() {
        match ch {
            'F' => {
                let next = st.pos + st.h * step;
                segs.push([st.pos, next]);
                st.pos = next;
            }
            // Yaw left / right (rotate around local up)
            '+' => { let q = Quat::from_axis_angle(st.u,  angle); st.h = q*st.h; st.l = q*st.l; }
            '-' => { let q = Quat::from_axis_angle(st.u, -angle); st.h = q*st.h; st.l = q*st.l; }
            // Pitch up / down (rotate around local left)
            '^' => { let q = Quat::from_axis_angle(st.l,  angle); st.h = q*st.h; st.u = q*st.u; }
            '&' => { let q = Quat::from_axis_angle(st.l, -angle); st.h = q*st.h; st.u = q*st.u; }
            // Roll left / right (rotate around heading)
            '\\' => { let q = Quat::from_axis_angle(st.h,  angle); st.l = q*st.l; st.u = q*st.u; }
            '/'  => { let q = Quat::from_axis_angle(st.h, -angle); st.l = q*st.l; st.u = q*st.u; }
            '[' => stack.push(st.clone()),
            ']' => { if let Some(s) = stack.pop() { st = s; } }
            _   => {}
        }
    }

    segs
}

// ── 3D → 2D projection ───────────────────────────────────────────────────────

/// Project a world-space point through the combined MVP matrix to pixel coords.
/// Returns `None` if the point is behind the camera or outside the view frustum.
fn project(p: Vec3, mvp: Mat4, w: f32, h: f32) -> Option<(f32, f32)> {
    let clip = mvp * p.extend(1.0);
    if clip.w < 0.001 { return None; } // behind near plane
    let nx = clip.x / clip.w;
    let ny = clip.y / clip.w;
    // Broad frustum cull — skip if clearly off-screen
    if nx.abs() > 2.0 || ny.abs() > 2.0 { return None; }
    Some((
        (nx + 1.0) * 0.5 * w,
        (1.0 - ny) * 0.5 * h,
    ))
}

/// Draw a projected line segment; skips if either endpoint is behind camera.
fn seg(a: Vec3, b: Vec3, mvp: Mat4, w: f32, h: f32) {
    if let (Some(p0), Some(p1)) = (project(a, mvp, w, h), project(b, mvp, w, h)) {
        line(p0.0, p0.1, p1.0, p1.1);
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

// ── Draw loop ─────────────────────────────────────────────────────────────────

fn draw(scene: &mut Scene3D) {
    let t = scene.time;
    let w = scene.width;
    let h = scene.height;

    background(5, 3, 12);

    // ── Camera — slow orbit, high angle ─────────────────────────────────────
    let cam_r = 3.0_f32;
    scene.camera.eye    = Vec3::new(cam_r * (t * 0.18).sin(), cam_r * (t * 0.18).cos(), 1.8);
    scene.camera.target = Vec3::new(0.0, 0.0, 0.05);
    scene.camera.fov_y  = std::f32::consts::FRAC_PI_4;

    // No 3D meshes — the entire visual is drawn via the 2D overlay.
    scene.clear();

    // Combined MVP: camera view-projection × spinning model matrix
    let spin = Mat4::from_rotation_z(t * 0.32);
    let mvp  = scene.camera.view_proj(scene.aspect()) * spin;

    // ── L-system branches ────────────────────────────────────────────────────
    let segments = SEGMENTS.get_or_init(build_lsystem);
    let total    = segments.len();

    // Progressive growth: ~28 new segments per second
    let visible = ((t * 28.0) as usize).min(total);

    no_fill();
    for (i, seg_pts) in segments[..visible].iter().enumerate() {
        let frac = i as f32 / total as f32;

        // Warm amber at root → cool blue-white at tips
        let r = lerp(240.0, 80.0,  frac) as u8;
        let g = lerp(180.0, 160.0, frac) as u8;
        let b = lerp(80.0,  255.0, frac) as u8;
        let a = lerp(230.0, 140.0, frac) as u8;

        // Thick trunk → hairline tips
        stroke_weight(lerp(2.0, 0.6, frac));
        stroke_a(r, g, b, a);

        seg(seg_pts[0], seg_pts[1], mvp, w, h);
    }

    // ── Bounding cube wireframe ───────────────────────────────────────────────
    let s = 0.5_f32;
    let corners: &[(Vec3, Vec3)] = &[
        // Bottom face
        (Vec3::new(-s,-s,-s), Vec3::new( s,-s,-s)),
        (Vec3::new( s,-s,-s), Vec3::new( s, s,-s)),
        (Vec3::new( s, s,-s), Vec3::new(-s, s,-s)),
        (Vec3::new(-s, s,-s), Vec3::new(-s,-s,-s)),
        // Top face
        (Vec3::new(-s,-s, s), Vec3::new( s,-s, s)),
        (Vec3::new( s,-s, s), Vec3::new( s, s, s)),
        (Vec3::new( s, s, s), Vec3::new(-s, s, s)),
        (Vec3::new(-s, s, s), Vec3::new(-s,-s, s)),
        // Vertical pillars
        (Vec3::new(-s,-s,-s), Vec3::new(-s,-s, s)),
        (Vec3::new( s,-s,-s), Vec3::new( s,-s, s)),
        (Vec3::new( s, s,-s), Vec3::new( s, s, s)),
        (Vec3::new(-s, s,-s), Vec3::new(-s, s, s)),
    ];

    stroke_weight(0.8);
    stroke_a(70, 60, 120, 110);
    for (a, b) in corners {
        seg(*a, *b, mvp, w, h);
    }

    // ── Growth progress bar ───────────────────────────────────────────────────
    let bar_w = (w - 48.0) * (visible as f32 / total as f32);
    stroke_weight(1.0);
    stroke_a(80, 60, 140, 80);
    line(24.0, h - 20.0, 24.0 + bar_w, h - 20.0);
}
