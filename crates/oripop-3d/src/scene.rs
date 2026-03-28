//! Scene graph: objects, materials, and texture-generation parameters.

use glam::{Mat4, Vec3};
use crate::{camera::Camera, mesh::MeshKind};

// ── Texture generation parameters ────────────────────────────────────────────

/// Parameters that control the GPU compute shader that generates the texture
/// applied to all 3D objects in the scene.
///
/// Tweak these every frame to make the texture evolve over time.
pub struct TextureGenParams {
    /// Number of FBM octaves. Range 1–8; higher = more fine detail.
    /// Default: 6.
    pub octaves: u32,
    /// Domain-warp intensity. Range 0–4; higher = more swirling distortion.
    /// Default: 2.0.
    pub warp_strength: f32,
    /// Base spatial frequency of the noise. Range 0.1–10.
    /// Default: 3.0.
    pub frequency: f32,
    /// Seed offset that shifts the pattern — useful for differentiating
    /// objects or presets.  Default: 0.0.
    pub seed: f32,
}

impl Default for TextureGenParams {
    fn default() -> Self {
        Self {
            octaves:       6,
            warp_strength: 2.0,
            frequency:     3.0,
            seed:          0.0,
        }
    }
}

// ── Object ────────────────────────────────────────────────────────────────────

/// A single instance of a built-in mesh primitive placed in the world.
pub struct Object3D {
    pub mesh:      MeshKind,
    /// World-space transform applied to this object.
    pub transform: Mat4,
}

// ── Scene ─────────────────────────────────────────────────────────────────────

/// The 3D scene state that is passed into the user's draw function every frame.
///
/// # Usage
/// ```ignore
/// fn draw(scene: &mut Scene3D) {
///     background(8, 5, 18);                           // clear colour
///     scene.camera.eye = Vec3::new(0.0, 2.0, 5.0);
///     scene.gen.warp_strength = 2.5;
///
///     scene.clear();
///     scene.add(MeshKind::Sphere, Mat4::from_rotation_y(scene.time));
///
///     // 2D overlay using oripop-core drawing API
///     stroke(200, 200, 255);
///     line(10.0, 10.0, 100.0, 10.0);
/// }
/// ```
pub struct Scene3D {
    /// Perspective camera controlling the view.
    pub camera: Camera,
    /// Directional light direction (world space, need not be normalised).
    /// Default: `Vec3::new(1.0, 2.0, 1.0)`.
    pub light_dir: Vec3,
    /// Parameters for the generative texture compute shader.
    pub gen: TextureGenParams,
    /// Elapsed seconds since `run3d` was called.
    pub time: f32,

    pub(crate) objects: Vec<Object3D>,
    /// Logical pixel width of the window (updated on resize).
    pub width:  f32,
    /// Logical pixel height of the window (updated on resize).
    pub height: f32,
}

impl Scene3D {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            camera:    Camera::default(),
            light_dir: Vec3::new(1.0, 2.0, 1.0),
            gen:       TextureGenParams::default(),
            time:      0.0,
            objects:   Vec::new(),
            width,
            height,
        }
    }

    /// Remove all objects from the scene (call at the top of each draw frame).
    pub fn clear(&mut self) {
        self.objects.clear();
    }

    /// Add a primitive instance to the scene with the given world transform.
    ///
    /// ```ignore
    /// scene.add(MeshKind::Sphere, Mat4::from_rotation_y(scene.time * 0.4));
    /// scene.add(MeshKind::Plane,  Mat4::from_translation(Vec3::new(0.0, -1.2, 0.0)));
    /// ```
    pub fn add(&mut self, mesh: MeshKind, transform: Mat4) {
        self.objects.push(Object3D { mesh, transform });
    }

    /// Aspect ratio of the window (width / height).
    pub fn aspect(&self) -> f32 {
        if self.height > 0.0 { self.width / self.height } else { 1.0 }
    }
}
