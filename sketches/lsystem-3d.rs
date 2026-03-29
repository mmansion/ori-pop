//! Growing 3D lattice — axis-aligned grid that expands outward from the centre
//! of a wireframe cube, shell by shell, filling it with nodes and edges.
//!
//! Nodes sit at integer multiples of `STEP` on all three axes.  Each node
//! connects to its 6 axis-aligned neighbours (+X, −X, +Y, −Y, +Z, −Z).
//! The growth front is an octahedral shell keyed by Manhattan distance from
//! the origin; concentric shells are revealed progressively.
//!
//! Run with:
//!   cargo run --bin lsystem-3d

use oripop_3d::prelude::*;

// ── Lattice parameters ────────────────────────────────────────────────────────

/// Grid spacing.  HALF × STEP fits just inside the ±0.5 cube.
const STEP: f32 = 0.15;
/// Half-extent in grid cells.  3 × 0.15 = 0.45, inside the ±0.5 cube.
const HALF: i32 = 3;
/// Manhattan shells revealed per second.
const GROW_RATE: f32 = 1.5;

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    size(960, 640);
    title("3D Lattice — axis-aligned grid growing into a cube");
    smooth(4);
    run3d(draw);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t.clamp(0.0, 1.0) }

fn project(p: Vec3, mvp: Mat4, w: f32, h: f32) -> Option<(f32, f32)> {
    let clip = mvp * p.extend(1.0);
    if clip.w < 0.001 { return None; }
    let nx = clip.x / clip.w;
    let ny = clip.y / clip.w;
    if nx.abs() > 2.5 || ny.abs() > 2.5 { return None; }
    Some(((nx + 1.0) * 0.5 * w, (1.0 - ny) * 0.5 * h))
}

// ── Draw ──────────────────────────────────────────────────────────────────────

fn draw(scene: &mut Scene3D) {
    let t = scene.time;
    let w = scene.width;
    let h = scene.height;

    background(2, 1, 6);

    // Enable right-click orbit + scroll zoom.
    // The runner manages scene.camera.eye — don't set it here.
    scene.orbit_enabled = true;
    scene.camera.fov_y  = std::f32::consts::FRAC_PI_4;
    scene.clear();

    // Scene is static — right-click drag to orbit, scroll to zoom.
    let mvp = scene.camera.view_proj(scene.aspect());

    let max_shell     = (HALF * 3) as f32;
    let current_shell = (t * GROW_RATE).min(max_shell + 1.0);

    // ── Edges ─────────────────────────────────────────────────────────────────
    // For each node, emit the three edges toward its +X, +Y, +Z neighbours.
    // This covers every edge in the lattice exactly once.
    no_fill();

    for iz in -HALF..=HALF {
        for iy in -HALF..=HALF {
            for ix in -HALF..=HALF {
                let a    = Vec3::new(ix as f32 * STEP, iy as f32 * STEP, iz as f32 * STEP);
                let da   = (ix.abs() + iy.abs() + iz.abs()) as f32;

                for (dx, dy, dz) in [(1i32,0i32,0i32), (0,1,0), (0,0,1)] {
                    let (bx, by, bz) = (ix + dx, iy + dy, iz + dz);
                    if bx > HALF || by > HALF || bz > HALF { continue; }

                    let b  = Vec3::new(bx as f32 * STEP, by as f32 * STEP, bz as f32 * STEP);
                    let db = (bx.abs() + by.abs() + bz.abs()) as f32;

                    // Shell = the furthest endpoint.
                    // Smooth fade-in as the growth front crosses this edge.
                    let shell  = da.max(db);
                    let fade   = (current_shell - shell + 0.8).clamp(0.0, 1.0);
                    if fade <= 0.0 { continue; }

                    // Brightness falls off from centre → edge.
                    let centre = 1.0 - shell / max_shell;
                    let bright = lerp(30.0, 255.0, centre * fade);
                    let alpha  = lerp(20.0, 200.0, centre * fade);

                    // Slightly thicker lines near centre.
                    stroke_weight(lerp(0.4, 1.6, centre));

                    // Cool blue-white palette: near white at centre, blue at edge.
                    stroke_a(
                        (bright * 0.75) as u8,
                        (bright * 0.88) as u8,
                        bright          as u8,
                        alpha           as u8,
                    );

                    if let (Some(p0), Some(p1)) = (project(a, mvp, w, h), project(b, mvp, w, h)) {
                        line(p0.0, p0.1, p1.0, p1.1);
                    }
                }
            }
        }
    }

    // ── Nodes ─────────────────────────────────────────────────────────────────
    // Draw a small dot at each visible lattice node.
    no_stroke();

    for iz in -HALF..=HALF {
        for iy in -HALF..=HALF {
            for ix in -HALF..=HALF {
                let p    = Vec3::new(ix as f32 * STEP, iy as f32 * STEP, iz as f32 * STEP);
                let dist = (ix.abs() + iy.abs() + iz.abs()) as f32;

                let fade   = (current_shell - dist + 0.8).clamp(0.0, 1.0);
                if fade <= 0.0 { continue; }

                let centre = 1.0 - dist / max_shell;
                let bright = lerp(60.0, 255.0, centre * fade) as u8;
                let alpha  = lerp(40.0, 230.0, centre * fade) as u8;
                let radius = lerp(0.8, 3.0, centre);

                fill_a(bright, bright, (bright as f32 * 1.05).min(255.0) as u8, alpha);

                if let Some((sx, sy)) = project(p, mvp, w, h) {
                    ellipse(sx - radius, sy - radius, radius * 2.0, radius * 2.0);
                }
            }
        }
    }

    // ── Bounding cube ─────────────────────────────────────────────────────────
    let s = 0.5_f32;
    no_fill();
    stroke_weight(0.5);
    stroke_a(45, 40, 80, 70);

    let cube_edges: &[(Vec3, Vec3)] = &[
        (Vec3::new(-s,-s,-s), Vec3::new( s,-s,-s)),
        (Vec3::new( s,-s,-s), Vec3::new( s, s,-s)),
        (Vec3::new( s, s,-s), Vec3::new(-s, s,-s)),
        (Vec3::new(-s, s,-s), Vec3::new(-s,-s,-s)),
        (Vec3::new(-s,-s, s), Vec3::new( s,-s, s)),
        (Vec3::new( s,-s, s), Vec3::new( s, s, s)),
        (Vec3::new( s, s, s), Vec3::new(-s, s, s)),
        (Vec3::new(-s, s, s), Vec3::new(-s,-s, s)),
        (Vec3::new(-s,-s,-s), Vec3::new(-s,-s, s)),
        (Vec3::new( s,-s,-s), Vec3::new( s,-s, s)),
        (Vec3::new( s, s,-s), Vec3::new( s, s, s)),
        (Vec3::new(-s, s,-s), Vec3::new(-s, s, s)),
    ];
    for (a, b) in cube_edges {
        if let (Some(p0), Some(p1)) = (project(*a, mvp, w, h), project(*b, mvp, w, h)) {
            line(p0.0, p0.1, p1.0, p1.1);
        }
    }
}
