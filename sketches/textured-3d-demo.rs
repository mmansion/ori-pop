//! Generative-texture 3D demo.
//!
//! A UV sphere and a ground plane are rendered with a GPU-generated texture
//! (domain-warped FBM noise, animated in real time).  A lightweight 2D
//! overlay — drawn with the normal oripop-core API — is composited on top.
//!
//! Run with:
//!   cargo run --bin textured-3d-demo

use oripop_3d::prelude::*;

fn main() {
    size(960, 640);
    title("oripop-3d — generative textures");
    smooth(4);
    run3d(draw);
}

fn draw(scene: &mut Scene3D) {
    let t = scene.time;

    // ── Background clear colour ─────────────────────────────────────────────
    background(6, 4, 14);

    // ── Texture generation ──────────────────────────────────────────────────
    // These params are written into the compute shader uniform every frame.
    scene.gen.frequency     = 3.0;
    scene.gen.octaves       = 6;
    scene.gen.warp_strength = 2.0 + 0.6 * (t * 0.25).sin();

    // ── Camera — slow orbit around the scene ────────────────────────────────
    let orbit_r = 5.0_f32;
    let orbit_y = 1.8_f32 + 0.4 * (t * 0.18).sin();
    scene.camera.eye    = Vec3::new(orbit_r * (t * 0.15).sin(), orbit_y, orbit_r * (t * 0.15).cos());
    scene.camera.target = Vec3::new(0.0, 0.2, 0.0);
    scene.camera.fov_y  = std::f32::consts::FRAC_PI_4;

    // ── Light ───────────────────────────────────────────────────────────────
    scene.light_dir = Vec3::new(
        (t * 0.3).sin(),
        2.0,
        (t * 0.3).cos(),
    );

    // ── Scene objects ───────────────────────────────────────────────────────
    scene.clear();

    // Sphere — slowly rotating
    scene.add(
        MeshKind::Sphere,
        Mat4::from_rotation_y(t * 0.35),
    );

    // Ground plane — scaled up, offset downward, slightly different seed
    scene.gen.seed = 0.5;
    scene.add(
        MeshKind::Plane,
        Mat4::from_scale(Vec3::splat(6.0))
            * Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)),
    );
    scene.gen.seed = 0.0; // reset for next frame

    // ── 2D overlay ──────────────────────────────────────────────────────────
    let w = scene.width;
    let h = scene.height;

    // Thin horizontal rule at the bottom
    no_fill();
    stroke_weight(1.0);
    stroke_a(160, 140, 220, 120);
    line(24.0, h - 28.0, w - 24.0, h - 28.0);

    // Tick marks
    stroke_a(120, 100, 180, 80);
    let ticks = 32_u32;
    for i in 0..=ticks {
        let x  = 24.0 + (w - 48.0) * i as f32 / ticks as f32;
        let th = if i % 8 == 0 { 8.0 } else { 4.0 };
        line(x, h - 28.0 - th, x, h - 28.0);
    }

    // Corner brackets
    let bx = 24.0_f32;
    let by = 24.0_f32;
    let bl = 18.0_f32;
    stroke_weight(1.5);
    stroke_a(180, 160, 240, 160);

    // top-left
    line(bx,      by,      bx + bl, by);
    line(bx,      by,      bx,      by + bl);
    // top-right
    line(w - bx,       by,      w - bx - bl, by);
    line(w - bx,       by,      w - bx,      by + bl);
    // bottom-left
    line(bx,      h - by, bx + bl, h - by);
    line(bx,      h - by, bx,      h - by - bl);
    // bottom-right
    line(w - bx,       h - by, w - bx - bl, h - by);
    line(w - bx,       h - by, w - bx,      h - by - bl);
}
