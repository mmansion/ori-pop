//! Player scene presets — sketch ortho camera and canvas plane setup.
//!
//! Used by [`crate::run_sketch`] and [`crate::run3d`] when [`PlayerMode::Sketch`] is active.

use glam::{Mat4, Vec3};

use crate::camera::Projection;
use crate::mesh::MeshKind;
use crate::scene::{ObjectTexture, Scene3D};

/// How the host frames canvas-authored content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlayerMode {
    /// Orthographic view of a single canvas plane (Processing-style sketches).
    #[default]
    Sketch,
    /// Free 3D scene; canvas draws still raster to the plane when present.
    Scene,
}

/// Logical canvas size in pixels: `(width, height, pixel_density)`.
pub fn canvas_pixel_size() -> (u32, u32, u32) {
    let (w, h, _, _, density) = oripop_canvas::draw::settings();
    (w, h, density)
}

/// Configure an orthographic camera that maps canvas pixels 1:1 to the window.
pub fn setup_sketch_camera(scene: &mut Scene3D, canvas_w: f32, canvas_h: f32) {
    scene.camera.projection = Projection::Orthographic;
    scene.camera.ortho_half_height = (canvas_h * 0.5).max(1.0);
    scene.camera.eye = Vec3::new(0.0, 0.0, 1.0);
    scene.camera.target = Vec3::ZERO;
    scene.camera.up = Vec3::Y;
    scene.width = canvas_w;
    scene.height = canvas_h;
}

/// Insert the canvas sampling plane if the scene does not already contain one.
///
/// The unit [`MeshKind::Plane`] is scaled to `canvas_w` × `canvas_h` world units
/// (matching canvas pixel coordinates) in the XY plane at Z = 0.
pub fn ensure_canvas_plane(scene: &mut Scene3D, canvas_w: f32, canvas_h: f32) {
    let has_canvas_plane = scene
        .objects
        .iter()
        .any(|o| o.visible && o.texture == ObjectTexture::Canvas);
    if has_canvas_plane {
        return;
    }
    let transform = Mat4::from_scale(Vec3::new(canvas_w, canvas_h, 1.0));
    scene.add_with_texture(MeshKind::Plane, transform, ObjectTexture::Canvas);
}

/// Apply sketch-mode scene defaults before the user draw callback.
pub fn prepare_sketch_scene(scene: &mut Scene3D) {
    scene.player_mode = PlayerMode::Sketch;
    scene.orbit_enabled = false;
    scene.clear();
}

/// Apply sketch framing after the user draw callback.
pub fn finalize_sketch_scene(scene: &mut Scene3D) {
    let (w, h, density) = canvas_pixel_size();
    let cw = w as f32;
    let ch = h as f32;
    let _ = density;
    setup_sketch_camera(scene, cw, ch);
    ensure_canvas_plane(scene, cw, ch);
}
