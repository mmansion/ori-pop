//! Perspective camera with glam-backed view-projection matrix.

use glam::{Mat4, Vec3, Vec4};

/// Perspective camera, Y-up right-handed, matching wgpu's NDC (depth 0..1).
pub struct Camera {
    /// World-space eye position.
    pub eye: Vec3,
    /// World-space look-at target.
    pub target: Vec3,
    /// Up vector (default `Vec3::Y`).
    pub up: Vec3,
    /// Vertical field of view in **radians**. Default: 45°.
    pub fov_y: f32,
    /// Near clip plane distance. Default: 0.1.
    pub near: f32,
    /// Far clip plane distance. Default: 200.0.
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye:    Vec3::new(0.0, 2.0, 5.0),
            target: Vec3::ZERO,
            up:     Vec3::Y,
            fov_y:  std::f32::consts::FRAC_PI_4,
            near:   0.1,
            far:    200.0,
        }
    }
}

impl Camera {
    /// Combined view-projection matrix ready to be uploaded as a uniform.
    ///
    /// wgpu/WebGPU uses NDC depth range 0..1.  glam 0.27's `perspective_rh`
    /// produces OpenGL-style -1..1 depth, so we apply a depth-correction
    /// matrix that remaps z: `z_wgpu = z_gl * 0.5 + w * 0.5`.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj_gl = Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far);
        let view    = Mat4::look_at_rh(self.eye, self.target, self.up);

        // Map clip-space depth from [-1, 1] → [0, 1]  (column-major)
        let depth_fix = Mat4::from_cols(
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.5, 0.0),
            Vec4::new(0.0, 0.0, 0.5, 1.0),
        );
        depth_fix * proj_gl * view
    }
}
