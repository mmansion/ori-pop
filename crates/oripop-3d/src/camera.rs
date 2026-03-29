//! Perspective camera — Z-up right-handed, matching the CAD / robotics /
//! fabrication convention (XY is the ground plane, Z points up).

use glam::{Mat4, Vec3, Vec4};

/// Remaps clip-space depth from OpenGL's [−1, 1] to wgpu's [0, 1].
///
/// Applied once per `view_proj` call: `z_wgpu = z_gl × 0.5 + w × 0.5`.
/// Defined as a constant — the matrix never changes.
const WGPU_DEPTH_REMAP: Mat4 = Mat4::from_cols(
    Vec4::new(1.0, 0.0, 0.0, 0.0),
    Vec4::new(0.0, 1.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 0.5, 0.0),
    Vec4::new(0.0, 0.0, 0.5, 1.0),
);

/// Perspective camera.
///
/// Coordinate convention: **Z-up right-handed** (ISO 80000-2, ROS, STEP).
/// X is right, Y is forward/depth, Z is up.  This aligns with CAD tools,
/// robotics frameworks, 3D printing slicers, and fabrication machines.
///
/// The view-projection matrix is corrected to wgpu's NDC depth range [0, 1].
pub struct Camera {
    /// World-space eye position. Default: `(4, -4, 3)` — front-right-above.
    pub eye: Vec3,
    /// World-space look-at target. Default: origin.
    pub target: Vec3,
    /// Up vector. Default: `Vec3::Z` (Z is up).
    pub up: Vec3,
    /// Vertical field of view in **radians**. Default: 45°.
    pub fov_y: f32,
    /// Near clip plane. Default: 0.1.
    pub near: f32,
    /// Far clip plane. Default: 500.0.
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye:    Vec3::new(4.0, -4.0, 3.0),
            target: Vec3::ZERO,
            up:     Vec3::Z,
            fov_y:  std::f32::consts::FRAC_PI_4,
            near:   0.1,
            far:    500.0,
        }
    }
}

impl Camera {
    /// Combined view-projection matrix, ready to upload as a GPU uniform.
    ///
    /// wgpu uses NDC depth range [0, 1].  glam's `perspective_rh` produces
    /// OpenGL-style [−1, 1], so a depth-remapping matrix is applied:
    /// `z_wgpu = z_gl × 0.5 + w × 0.5`.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far);
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        WGPU_DEPTH_REMAP * proj * view
    }
}
