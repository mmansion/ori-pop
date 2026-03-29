//! Parametric surfaces — the geometric foundation of the design tree.
//!
//! A [`Surface`] maps `(u, v) ∈ [0,1]²` to a point in Z-up right-handed
//! world space, along with its normal and curvature at that point.
//!
//! Surface-aware UV generation (Level 2 in the roadmap) passes curvature and
//! arc-length data from the surface directly into the compute shader, so the
//! generative pattern responds to the geometry it inhabits.

use glam::Vec3;
use std::f32::consts::{PI, TAU};

// ── Principal curvatures ─────────────────────────────────────────────────────

/// The two principal curvatures at a surface point, and their directions.
///
/// - `k1` ≥ `k2` — the maximum and minimum normal curvatures.
/// - `d1`, `d2` — unit tangent directions of `k1` and `k2` (orthogonal).
/// - Gaussian curvature `K = k1 * k2`.
/// - Mean curvature `H = (k1 + k2) / 2`.
///
/// A surface is developable where `K ≈ 0` (at least one curvature is zero).
#[derive(Clone, Debug)]
pub struct PrincipalCurvatures {
    pub k1: f32,
    pub k2: f32,
    pub d1: Vec3,
    pub d2: Vec3,
}

impl PrincipalCurvatures {
    pub fn mean(&self) -> f32 {
        (self.k1 + self.k2) * 0.5
    }
    pub fn gaussian(&self) -> f32 {
        self.k1 * self.k2
    }
    /// True if the Gaussian curvature is negligible — surface can be unrolled flat.
    pub fn is_developable(&self) -> bool {
        self.gaussian().abs() < 1e-5
    }
    /// Zero curvature everywhere — a flat plane.
    pub fn flat(tangent_u: Vec3, tangent_v: Vec3) -> Self {
        Self { k1: 0.0, k2: 0.0, d1: tangent_u, d2: tangent_v }
    }
}

// ── Surface trait ─────────────────────────────────────────────────────────────

/// A smooth parametric surface `S(u,v): [0,1]² → ℝ³` in Z-up world space.
///
/// All implementations must be deterministic and produce identical results
/// for the same `(u, v)` input — making the surface a pure mathematical object.
pub trait Surface: Send + Sync {
    /// World-space position at parameter `(u, v)`.
    fn point(&self, u: f32, v: f32) -> Vec3;

    /// Outward unit normal at `(u, v)`.
    fn normal(&self, u: f32, v: f32) -> Vec3;

    /// Principal curvatures at `(u, v)`.
    fn curvature(&self, u: f32, v: f32) -> PrincipalCurvatures;

    /// True if `u = 0` and `u = 1` join seamlessly (e.g. sphere longitude).
    fn is_closed_u(&self) -> bool { false }

    /// True if `v = 0` and `v = 1` join seamlessly (e.g. torus latitude).
    fn is_closed_v(&self) -> bool { false }

    /// Approximate arc-length scale at `(u, v)` along the u direction.
    /// Used to remap flat UV to arc-length-normalized UV for surface-aware generation.
    fn arc_length_u(&self, u: f32, v: f32) -> f32 {
        let h = 1e-4_f32;
        let p0 = self.point((u - h).max(0.0), v);
        let p1 = self.point((u + h).min(1.0), v);
        (p1 - p0).length() / (2.0 * h)
    }

    /// Approximate arc-length scale at `(u, v)` along the v direction.
    fn arc_length_v(&self, u: f32, v: f32) -> f32 {
        let h = 1e-4_f32;
        let p0 = self.point(u, (v - h).max(0.0));
        let p1 = self.point(u, (v + h).min(1.0));
        (p1 - p0).length() / (2.0 * h)
    }

    /// Sample whether this surface can be unrolled flat without distortion.
    /// Checks a grid of points — O(n²).
    ///
    /// The default uses numerical curvature, which is available on any
    /// concrete type via the free function [`is_developable_surface`].
    fn is_developable(&self) -> bool where Self: Sized {
        is_developable_surface(self)
    }
}

// ── Numerical helpers ─────────────────────────────────────────────────────────

/// Compute a surface normal by central-difference finite differences on `point()`.
pub fn numerical_normal(s: &dyn Surface, u: f32, v: f32) -> Vec3 {
    let h = 1e-4_f32;
    let du = s.point((u + h).min(1.0), v) - s.point((u - h).max(0.0), v);
    let dv = s.point(u, (v + h).min(1.0)) - s.point(u, (v - h).max(0.0));
    du.cross(dv).normalize_or_zero()
}

/// Compute principal curvatures numerically via the shape operator (Weingarten map).
/// Uses central differences on `point()` to build the first and second
/// fundamental forms, then extracts eigenvalues.
pub fn numerical_curvature(s: &dyn Surface, u: f32, v: f32) -> PrincipalCurvatures {
    let h = 5e-4_f32;
    let p   = s.point(u, v);
    let pu  = s.point((u + h).min(1.0), v);
    let puu = s.point((u + 2.0*h).min(1.0), v);
    let pum = s.point((u - h).max(0.0), v);
    let pv  = s.point(u, (v + h).min(1.0));
    let pvv = s.point(u, (v + 2.0*h).min(1.0));
    let pvm = s.point(u, (v - h).max(0.0));
    let puv = s.point((u + h).min(1.0), (v + h).min(1.0));

    let fu  = (pu  - pum) / (2.0 * h);
    let fv  = (pv  - pvm) / (2.0 * h);
    let fuu = (puu - 2.0*p + pum) / (h * h);
    let fvv = (pvv - 2.0*p + pvm) / (h * h);
    let fuv = (puv - pu - pv + p)  / (h * h);

    let n = fu.cross(fv);
    let n_len = n.length();
    if n_len < 1e-10 {
        return PrincipalCurvatures::flat(Vec3::X, Vec3::Y);
    }
    let n = n / n_len;

    // First fundamental form coefficients
    let ee = fu.dot(fu);
    let ff = fu.dot(fv);
    let gg = fv.dot(fv);
    let denom = ee * gg - ff * ff;
    if denom.abs() < 1e-12 {
        return PrincipalCurvatures::flat(fu.normalize_or(Vec3::X), fv.normalize_or(Vec3::Y));
    }

    // Second fundamental form coefficients
    let ll = fuu.dot(n);
    let mm = fuv.dot(n);
    let nn = fvv.dot(n);

    // Mean and Gaussian curvatures
    let h_mean = (ee * nn - 2.0 * ff * mm + gg * ll) / (2.0 * denom);
    let k_gauss = (ll * nn - mm * mm) / denom;

    let discriminant = (h_mean * h_mean - k_gauss).max(0.0).sqrt();
    let k1 = h_mean + discriminant;
    let k2 = h_mean - discriminant;

    // Principal directions (approximate: align d1 with fu for now)
    let d1 = fu.normalize_or(Vec3::X);
    let d2 = n.cross(d1).normalize_or(Vec3::Y);

    PrincipalCurvatures { k1, k2, d1, d2 }
}

// ── Standalone helpers ────────────────────────────────────────────────────────

/// Check whether a surface is developable by sampling a coarse grid.
/// Works on any `&dyn Surface`.
pub fn is_developable_surface(s: &dyn Surface) -> bool {
    let n = 8u32;
    for i in 0..n {
        for j in 0..n {
            let u = i as f32 / (n - 1) as f32;
            let v = j as f32 / (n - 1) as f32;
            if !s.curvature(u, v).is_developable() {
                return false;
            }
        }
    }
    true
}

// ── UvSphere ──────────────────────────────────────────────────────────────────

/// A sphere of given radius centred at the origin.
///
/// Z-up: poles at (0,0,±radius), longitude runs in the XY plane.
/// `u` = longitude in [0,1] → angle in [0, 2π].
/// `v` = latitude  in [0,1] → angle from +Z pole (0) to −Z pole (1).
///
/// Closed in `u` (seam at u=0/1). Open in `v` (poles are degenerate).
pub struct UvSphere {
    pub radius: f32,
}

impl UvSphere {
    pub fn new(radius: f32) -> Self { Self { radius } }
}

impl Surface for UvSphere {
    fn point(&self, u: f32, v: f32) -> Vec3 {
        let theta = u * TAU;            // longitude [0, 2π]
        let phi   = v * PI;             // colatitude from +Z [0, π]
        let r     = self.radius;
        Vec3::new(
            r * phi.sin() * theta.cos(),
            r * phi.sin() * theta.sin(),
            r * phi.cos(),
        )
    }

    fn normal(&self, u: f32, v: f32) -> Vec3 {
        // For a sphere the normal is simply the normalised position.
        self.point(u, v).normalize_or_zero()
    }

    fn curvature(&self, _u: f32, _v: f32) -> PrincipalCurvatures {
        // Sphere: k1 = k2 = 1/r everywhere. Directions are arbitrary tangents.
        let k = 1.0 / self.radius;
        PrincipalCurvatures { k1: k, k2: k, d1: Vec3::X, d2: Vec3::Y }
    }

    fn is_closed_u(&self) -> bool { true }
}

// ── Plane ─────────────────────────────────────────────────────────────────────

/// A flat rectangular XY plane at Z=0 in Z-up world space.
///
/// UV space maps directly to world XY: `u` → X, `v` → Y.
/// This is the simplest surface — UV space *is* world space (scaled).
///
/// A flat plane is fully developable by definition (`K = 0` everywhere).
pub struct Plane {
    /// Width along X.
    pub width:  f32,
    /// Height along Y.
    pub height: f32,
}

impl Plane {
    pub fn new(width: f32, height: f32) -> Self { Self { width, height } }
    pub fn square(size: f32) -> Self { Self::new(size, size) }
}

impl Surface for Plane {
    fn point(&self, u: f32, v: f32) -> Vec3 {
        Vec3::new(
            (u - 0.5) * self.width,
            (v - 0.5) * self.height,
            0.0,
        )
    }
    fn normal(&self, _u: f32, _v: f32) -> Vec3 { Vec3::Z }
    fn curvature(&self, _u: f32, _v: f32) -> PrincipalCurvatures {
        PrincipalCurvatures::flat(Vec3::X, Vec3::Y)
    }
}

// ── Cylinder ──────────────────────────────────────────────────────────────────

/// An open cylinder: radius `r`, height `h`, axis along Z.
///
/// `u` = longitude [0, 2π], closed.
/// `v` = height from Z=0 to Z=height, open.
///
/// Developable: `K = 0` everywhere (it unrolls to a flat rectangle).
pub struct Cylinder {
    pub radius: f32,
    pub height: f32,
}

impl Cylinder {
    pub fn new(radius: f32, height: f32) -> Self { Self { radius, height } }
}

impl Surface for Cylinder {
    fn point(&self, u: f32, v: f32) -> Vec3 {
        let theta = u * TAU;
        Vec3::new(
            self.radius * theta.cos(),
            self.radius * theta.sin(),
            v * self.height,
        )
    }
    fn normal(&self, u: f32, _v: f32) -> Vec3 {
        let theta = u * TAU;
        Vec3::new(theta.cos(), theta.sin(), 0.0)
    }
    fn curvature(&self, _u: f32, _v: f32) -> PrincipalCurvatures {
        let k = 1.0 / self.radius;
        PrincipalCurvatures { k1: k, k2: 0.0, d1: Vec3::Y, d2: Vec3::Z }
    }
    fn is_closed_u(&self) -> bool { true }
}

// ── Torus ─────────────────────────────────────────────────────────────────────

/// A torus centred at the origin, axis along Z.
///
/// `major_radius` = distance from Z-axis to tube centre.
/// `minor_radius` = tube radius.
/// `u` = toroidal angle (around Z), closed.
/// `v` = poloidal angle (around tube), closed.
pub struct Torus {
    pub major_radius: f32,
    pub minor_radius: f32,
}

impl Torus {
    pub fn new(major_radius: f32, minor_radius: f32) -> Self {
        Self { major_radius, minor_radius }
    }
}

impl Surface for Torus {
    fn point(&self, u: f32, v: f32) -> Vec3 {
        let phi   = u * TAU;  // toroidal
        let theta = v * TAU;  // poloidal
        let r     = self.major_radius + self.minor_radius * theta.cos();
        Vec3::new(
            r * phi.cos(),
            r * phi.sin(),
            self.minor_radius * theta.sin(),
        )
    }

    fn normal(&self, u: f32, v: f32) -> Vec3 {
        // Outward normal on the tube surface.
        let phi   = u * TAU;
        let theta = v * TAU;
        Vec3::new(
            theta.cos() * phi.cos(),
            theta.cos() * phi.sin(),
            theta.sin(),
        )
    }

    fn curvature(&self, _u: f32, v: f32) -> PrincipalCurvatures {
        let theta = v * TAU;
        let r  = self.minor_radius;
        let rr = self.major_radius;
        let k1 = 1.0 / r;
        let k2 = theta.cos() / (rr + r * theta.cos());
        PrincipalCurvatures { k1, k2, d1: Vec3::Z, d2: Vec3::Y }
    }

    fn is_closed_u(&self) -> bool { true }
    fn is_closed_v(&self) -> bool { true }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_poles() {
        let s = UvSphere::new(1.0);
        let north = s.point(0.0, 0.0);
        let south = s.point(0.0, 1.0);
        assert!((north - Vec3::Z).length() < 1e-5,   "north pole should be +Z");
        assert!((south - (-Vec3::Z)).length() < 1e-5, "south pole should be -Z");
    }

    #[test]
    fn sphere_normal_is_outward() {
        let s = UvSphere::new(2.0);
        let p = s.point(0.25, 0.5);
        let n = s.normal(0.25, 0.5);
        assert!(p.dot(n) > 0.0, "normal should point outward");
    }

    #[test]
    fn sphere_constant_curvature() {
        let r = 1.5;
        let s = UvSphere::new(r);
        let c = s.curvature(0.3, 0.4);
        assert!((c.k1 - 1.0/r).abs() < 1e-5);
        assert!((c.k2 - 1.0/r).abs() < 1e-5);
        assert!(!c.is_developable());
    }

    #[test]
    fn plane_is_flat() {
        let s = Plane::new(10.0, 10.0);
        assert!((s.normal(0.5, 0.5) - Vec3::Z).length() < 1e-6);
        assert!(s.curvature(0.5, 0.5).is_developable());
        assert!(s.is_developable());
    }

    #[test]
    fn cylinder_is_developable() {
        let s = Cylinder::new(1.0, 5.0);
        assert!(s.is_developable());
    }

    #[test]
    fn torus_not_developable() {
        let s = Torus::new(3.0, 1.0);
        assert!(!s.is_developable());
    }

    #[test]
    fn plane_uv_maps_to_xy() {
        let s = Plane::new(2.0, 4.0);
        let corner = s.point(0.0, 0.0);
        assert!((corner - Vec3::new(-1.0, -2.0, 0.0)).length() < 1e-5);
        let centre = s.point(0.5, 0.5);
        assert!(centre.length() < 1e-5);
    }
}
