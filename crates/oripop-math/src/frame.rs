//! Coordinate frames in Z-up right-handed world space.
//!
//! A [`Frame`] is an orthonormal basis attached to a point in space.
//! It represents robot end-effector poses, print-bed orientation,
//! workpiece datums, joint frames, and surface tangent frames.
//!
//! Convention: X = right, Y = forward, Z = up.

use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

/// A coordinate frame — an origin with an orthonormal Z-up basis.
///
/// Serializable and portable. An agent can read and write frame values
/// to position robot targets, define work coordinates, or set camera poses.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Frame {
    pub origin: Vec3,
    /// X axis — "right" in local frame.
    pub x_axis: Vec3,
    /// Y axis — "forward" in local frame.
    pub y_axis: Vec3,
    /// Z axis — "up" in local frame.
    pub z_axis: Vec3,
}

impl Default for Frame {
    fn default() -> Self { Self::identity() }
}

impl Frame {
    /// The world frame: origin at zero, axes aligned with world X/Y/Z.
    pub fn identity() -> Self {
        Self {
            origin: Vec3::ZERO,
            x_axis: Vec3::X,
            y_axis: Vec3::Y,
            z_axis: Vec3::Z,
        }
    }

    /// Construct a frame from an origin and a Z-up direction.
    /// X and Y are chosen to be consistent with Z-up convention.
    pub fn from_origin_and_z(origin: Vec3, z_up: Vec3) -> Self {
        let z = z_up.normalize_or(Vec3::Z);
        // Choose X perpendicular to Z, preferring world-X when possible.
        let world_x = if z.dot(Vec3::X).abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let x = world_x.cross(z).normalize_or(Vec3::X);
        let y = z.cross(x).normalize_or(Vec3::Y);
        Self { origin, x_axis: x, y_axis: y, z_axis: z }
    }

    /// Frame from a surface point, normal (→ Z), and optional tangent hint (→ X).
    pub fn from_surface_point(origin: Vec3, normal: Vec3, tangent_hint: Vec3) -> Self {
        let z = normal.normalize_or(Vec3::Z);
        let rejected = tangent_hint.reject_from(z);
        let x = if rejected.length() > 1e-6 {
            rejected.normalize()
        } else {
            let alt = if z.dot(Vec3::X).abs() < 0.9 { Vec3::X } else { Vec3::Y };
            alt.reject_from(z).normalize_or(Vec3::X)
        };
        let y = z.cross(x).normalize_or(Vec3::Y);
        Self { origin, x_axis: x, y_axis: y, z_axis: z }
    }

    /// Column-major 4×4 matrix (right-multiply convention, compatible with glam/wgpu).
    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_cols(
            Vec4::new(self.x_axis.x, self.x_axis.y, self.x_axis.z, 0.0),
            Vec4::new(self.y_axis.x, self.y_axis.y, self.y_axis.z, 0.0),
            Vec4::new(self.z_axis.x, self.z_axis.y, self.z_axis.z, 0.0),
            Vec4::new(self.origin.x, self.origin.y, self.origin.z, 1.0),
        )
    }

    /// Reconstruct a frame from a column-major 4×4 matrix.
    pub fn from_mat4(m: Mat4) -> Self {
        let cols = m.to_cols_array_2d();
        Self {
            x_axis: Vec3::new(cols[0][0], cols[0][1], cols[0][2]),
            y_axis: Vec3::new(cols[1][0], cols[1][1], cols[1][2]),
            z_axis: Vec3::new(cols[2][0], cols[2][1], cols[2][2]),
            origin: Vec3::new(cols[3][0], cols[3][1], cols[3][2]),
        }
    }

    /// Transform a point from local frame coordinates to world coordinates.
    pub fn transform_point(&self, local: Vec3) -> Vec3 {
        self.origin
            + self.x_axis * local.x
            + self.y_axis * local.y
            + self.z_axis * local.z
    }

    /// Transform a direction from local frame to world (no translation).
    pub fn transform_dir(&self, local: Vec3) -> Vec3 {
        self.x_axis * local.x + self.y_axis * local.y + self.z_axis * local.z
    }

    /// Inverse: transform a world point to local frame coordinates.
    pub fn inverse_transform_point(&self, world: Vec3) -> Vec3 {
        let d = world - self.origin;
        Vec3::new(d.dot(self.x_axis), d.dot(self.y_axis), d.dot(self.z_axis))
    }

    /// Inverse: transform a world direction to local frame coordinates.
    pub fn inverse_transform_dir(&self, world: Vec3) -> Vec3 {
        Vec3::new(world.dot(self.x_axis), world.dot(self.y_axis), world.dot(self.z_axis))
    }

    /// Compose: apply `other` frame relative to `self`.
    pub fn compose(&self, other: &Frame) -> Frame {
        Frame {
            origin: self.transform_point(other.origin),
            x_axis: self.transform_dir(other.x_axis),
            y_axis: self.transform_dir(other.y_axis),
            z_axis: self.transform_dir(other.z_axis),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips_mat4() {
        let f  = Frame::identity();
        let m  = f.to_mat4();
        let f2 = Frame::from_mat4(m);
        assert!((f.x_axis - f2.x_axis).length() < 1e-5);
        assert!((f.y_axis - f2.y_axis).length() < 1e-5);
        assert!((f.z_axis - f2.z_axis).length() < 1e-5);
    }

    #[test]
    fn transform_point_round_trips() {
        let f = Frame::from_origin_and_z(Vec3::new(1.0, 2.0, 3.0), Vec3::Z);
        let local = Vec3::new(1.0, 0.5, 0.0);
        let world = f.transform_point(local);
        let back  = f.inverse_transform_point(world);
        assert!((back - local).length() < 1e-5);
    }

    #[test]
    fn surface_frame_z_is_normal() {
        let normal = Vec3::new(0.0, 0.0, 1.0);
        let f = Frame::from_surface_point(Vec3::ZERO, normal, Vec3::X);
        assert!((f.z_axis - normal).length() < 1e-5);
        assert!(f.x_axis.dot(f.z_axis).abs() < 1e-5, "x must be perpendicular to z");
    }

    #[test]
    fn frame_serializes_to_json() {
        let f    = Frame::identity();
        let json = serde_json::to_string(&f).unwrap();
        let back: Frame = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }
}
