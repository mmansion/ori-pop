//! Scene graph: objects, materials, texture-generation parameters, and inspector state.
//!
//! All spatial values use the **Z-up right-handed** convention.

use glam::{Mat4, Vec3};
use crate::{camera::Camera, mesh::MeshKind};

// ── Object identity ───────────────────────────────────────────────────────────

/// Unique handle for a scene object.  Returned by [`Scene3D::add`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u32);

// ── Texture generation parameters ────────────────────────────────────────────

/// Parameters that control the GPU compute shader generating the texture
/// applied to all 3D objects in the scene.  Modify each frame to animate.
#[derive(Clone)]
pub struct TextureGenParams {
    /// FBM octave count. Range 1–8; higher = finer detail. Default: 6.
    pub octaves: u32,
    /// Domain-warp intensity. Range 0–4; higher = more swirling. Default: 2.0.
    pub warp_strength: f32,
    /// Base spatial frequency of the noise. Range 0.1–10. Default: 3.0.
    pub frequency: f32,
    /// Seed offset — shifts the pattern to differentiate objects. Default: 0.0.
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

// ── Scene object ──────────────────────────────────────────────────────────────

/// A single mesh instance placed in the world.
pub struct Object3D {
    /// Unique handle for this object within the scene.
    pub id:        ObjectId,
    /// Optional human-readable label shown in the inspector.
    pub label:     Option<String>,
    /// Mesh primitive to render.
    pub mesh:      MeshKind,
    /// World-space transform (Z-up right-handed).
    pub transform: Mat4,
    /// Whether this object is visible. Default: true.
    pub visible:   bool,
}

// ── Scene ─────────────────────────────────────────────────────────────────────

/// The 3D scene state handed to the user's draw callback every frame.
///
/// Everything lives in **Z-up right-handed** world space:
/// X = right, Y = depth/forward, Z = up, XY = ground plane.
///
/// # Minimal example
/// ```ignore
/// fn draw(scene: &mut Scene3D) {
///     background(6, 4, 14);
///
///     scene.camera.eye = Vec3::new(4.0, -4.0, 3.0);
///     scene.gen.warp_strength = 2.5;
///
///     scene.clear();
///     scene.add(MeshKind::Sphere, Mat4::IDENTITY);
///
///     // 2D overlay via oripop-core API
///     stroke(200, 200, 255);
///     line(10.0, 10.0, 400.0, 10.0);
/// }
/// ```
pub struct Scene3D {
    /// Perspective camera. Z-up by default.
    pub camera: Camera,

    /// Directional light direction in world space (need not be normalised).
    ///
    /// Default: `Vec3::new(1.0, -1.0, 2.0)` — from front-right-above in Z-up.
    pub light_dir: Vec3,

    /// Parameters for the generative texture compute shader.
    pub gen: TextureGenParams,

    /// Elapsed seconds since `run3d` was called.
    pub time: f32,

    /// Logical pixel width of the window (updated on resize).
    pub width: f32,

    /// Logical pixel height of the window (updated on resize).
    pub height: f32,

    /// Show the egui inspector panel. Toggle with the **Tab** key. Default: `false`.
    pub show_inspector: bool,

    /// When `true`, right-click drag orbits the camera and the scroll wheel
    /// zooms.  Set this in your draw callback and do **not** overwrite
    /// `scene.camera.eye` — the runner manages it.
    pub orbit_enabled: bool,

    pub(crate) objects: Vec<Object3D>,
    next_id:            u32,
}

impl Scene3D {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            camera:         Camera::default(),
            light_dir:      Vec3::new(1.0, -1.0, 2.0),
            gen:            TextureGenParams::default(),
            time:           0.0,
            width,
            height,
            show_inspector: false,
            orbit_enabled:  false,
            objects:        Vec::new(),
            next_id:        0,
        }
    }

    /// Remove all objects from the scene.
    ///
    /// Call at the top of each draw frame before re-adding objects.
    pub fn clear(&mut self) {
        self.objects.clear();
    }

    /// Add a mesh primitive with the given world transform.
    ///
    /// Returns an [`ObjectId`] that can be used to identify the object
    /// in the inspector or for future reference.
    ///
    /// ```ignore
    /// scene.clear();
    /// let sphere_id = scene.add(MeshKind::Sphere, Mat4::IDENTITY);
    /// let plane_id  = scene.add(
    ///     MeshKind::Plane,
    ///     Mat4::from_translation(Vec3::new(0.0, 0.0, -1.2))
    ///         * Mat4::from_scale(Vec3::splat(6.0)),
    /// );
    /// ```
    pub fn add(&mut self, mesh: MeshKind, transform: Mat4) -> ObjectId {
        self.add_labeled(mesh, transform, None)
    }

    /// Add a named mesh primitive.  The label appears in the inspector panel.
    pub fn add_named(
        &mut self,
        label: impl Into<String>,
        mesh:  MeshKind,
        transform: Mat4,
    ) -> ObjectId {
        self.add_labeled(mesh, transform, Some(label.into()))
    }

    fn add_labeled(
        &mut self,
        mesh:      MeshKind,
        transform: Mat4,
        label:     Option<String>,
    ) -> ObjectId {
        let id = ObjectId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.objects.push(Object3D { id, label, mesh, transform, visible: true });
        id
    }

    /// Aspect ratio of the window (width / height).
    pub fn aspect(&self) -> f32 {
        if self.height > 0.0 { self.width / self.height } else { 1.0 }
    }
}
