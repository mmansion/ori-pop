//! CPU-side mesh — the canonical geometric representation.
//!
//! [`CpuMesh`] is the handoff format between the geometry kernel and every
//! downstream consumer: the GPU uploader in `oripop-3d`, the fabrication
//! exporter in `oripop-fab`, and glTF serialization.
//!
//! It is distinct from `oripop-3d`'s `GpuMesh`, which is a cached wgpu
//! buffer upload and carries no semantic information beyond raw bytes.

use glam::Vec3;
use crate::surface::Surface;

// ── BoundingBox ───────────────────────────────────────────────────────────────

/// Axis-aligned bounding box in Z-up world space.
#[derive(Clone, Debug, Default)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl BoundingBox {
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    pub fn expand(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn centre(&self) -> Vec3 { (self.min + self.max) * 0.5 }
    pub fn size(&self)   -> Vec3 { self.max - self.min }
    pub fn diagonal(&self) -> f32 { self.size().length() }
}

// ── CpuMesh ───────────────────────────────────────────────────────────────────

/// CPU-side tessellated mesh.
///
/// All arrays are parallel: `positions[i]`, `normals[i]`, `uvs[i]` describe
/// the same vertex. `indices` is a flat list of triangles (3 indices per tri).
///
/// Z-up right-handed convention throughout.
#[derive(Clone, Debug, Default)]
pub struct CpuMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals:   Vec<[f32; 3]>,
    pub uvs:       Vec<[f32; 2]>,
    pub indices:   Vec<u32>,
}

impl CpuMesh {
    pub fn new() -> Self { Self::default() }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize { self.positions.len() }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize { self.indices.len() / 3 }

    /// Axis-aligned bounding box.
    pub fn bounding_box(&self) -> BoundingBox {
        let mut bb = BoundingBox::empty();
        for p in &self.positions {
            bb.expand(Vec3::from(*p));
        }
        bb
    }

    // ── Tessellation ──────────────────────────────────────────────────────────

    /// Tessellate a parametric surface into a mesh.
    ///
    /// `u_steps` × `v_steps` quads are generated, each split into two
    /// triangles.  For closed surfaces (sphere longitude, torus) the seam
    /// is handled correctly — the last column of vertices re-uses the first.
    ///
    /// # Parameters
    /// - `u_steps` — columns of quads (longitude for sphere/cylinder).
    /// - `v_steps` — rows of quads (latitude / height).
    ///
    /// Reasonable defaults: `(64, 48)` for smooth organic shapes,
    /// `(1, 1)` for a flat plane, `(128, 2)` for a thin cylinder.
    pub fn from_surface(surface: &dyn Surface, u_steps: u32, v_steps: u32) -> Self {
        let cols = u_steps + 1;
        let rows = v_steps + 1;

        let mut positions = Vec::with_capacity((cols * rows) as usize);
        let mut normals   = Vec::with_capacity((cols * rows) as usize);
        let mut uvs       = Vec::with_capacity((cols * rows) as usize);
        let mut indices   = Vec::with_capacity((u_steps * v_steps * 6) as usize);

        for row in 0..rows {
            for col in 0..cols {
                let u = col as f32 / u_steps as f32;
                let v = row as f32 / v_steps as f32;

                let p = surface.point(u, v);
                let n = surface.normal(u, v);

                positions.push(p.to_array());
                normals.push(n.to_array());
                uvs.push([u, v]);
            }
        }

        // Determine winding order from the first interior quad.
        //
        // Different parametric surfaces have different ∂u × ∂v orientations:
        //   Plane:  ∂u × ∂v = +Z (outward) → [tl, tr, br] is correct.
        //   Sphere: ∂u × ∂v = inward        → [tl, bl, br] is correct.
        //
        // Sample one face away from any pole and compare the face normal to
        // the stored vertex normal to pick the outward-facing winding.
        let sc = u_steps / 4;
        let sr = v_steps / 4;
        let pa = Vec3::from(positions[(sr       * cols + sc    ) as usize]);
        let pb = Vec3::from(positions[(sr       * cols + sc + 1) as usize]); // tr
        let pc = Vec3::from(positions[((sr + 1) * cols + sc    ) as usize]); // bl
        let face_du_dv = (pb - pa).cross(pc - pa); // direction of ∂u × ∂v
        let vertex_n   = Vec3::from(normals[(sr * cols + sc) as usize]);
        // If ∂u × ∂v aligns with the outward normal, use [tl, tr, br];
        // otherwise the surface has the opposite orientation and we use [tl, bl, br].
        let natural = face_du_dv.dot(vertex_n) > 0.0;

        for row in 0..v_steps {
            for col in 0..u_steps {
                let tl = row       * cols + col;
                let tr = row       * cols + col + 1;
                let bl = (row + 1) * cols + col;
                let br = (row + 1) * cols + col + 1;

                if natural {
                    indices.extend_from_slice(&[tl, tr, br]);
                    indices.extend_from_slice(&[tl, br, bl]);
                } else {
                    indices.extend_from_slice(&[tl, bl, br]);
                    indices.extend_from_slice(&[tl, br, tr]);
                }
            }
        }

        Self { positions, normals, uvs, indices }
    }

    // ── Merge ─────────────────────────────────────────────────────────────────

    /// Append another mesh, offsetting its indices to avoid collisions.
    pub fn merge(&mut self, other: &CpuMesh) {
        let offset = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);
        self.indices.extend(other.indices.iter().map(|i| i + offset));
    }

    // ── Normals ───────────────────────────────────────────────────────────────

    /// Recompute per-vertex normals by averaging face normals.
    /// Useful after deforming positions or when surface normals aren't available.
    pub fn recompute_normals(&mut self) {
        let n = self.positions.len();
        let mut accum = vec![Vec3::ZERO; n];

        for tri in self.indices.chunks_exact(3) {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let pa = Vec3::from(self.positions[a]);
            let pb = Vec3::from(self.positions[b]);
            let pc = Vec3::from(self.positions[c]);
            let face_normal = (pb - pa).cross(pc - pa);
            accum[a] += face_normal;
            accum[b] += face_normal;
            accum[c] += face_normal;
        }

        self.normals = accum.iter()
            .map(|n| n.normalize_or_zero().to_array())
            .collect();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{Plane, UvSphere, Cylinder};

    #[test]
    fn plane_tessellation_vertex_count() {
        let s = Plane::new(2.0, 2.0);
        let m = CpuMesh::from_surface(&s, 4, 4);
        assert_eq!(m.vertex_count(), 25);     // (4+1)*(4+1)
        assert_eq!(m.triangle_count(), 32);   // 4*4*2
    }

    #[test]
    fn sphere_tessellation_smoke() {
        let s = UvSphere::new(1.0);
        let m = CpuMesh::from_surface(&s, 32, 24);
        assert_eq!(m.vertex_count(), 33 * 25);
        assert!(m.triangle_count() > 0);
        // All positions should be approximately unit distance from origin
        for p in &m.positions {
            let len = Vec3::from(*p).length();
            assert!((len - 1.0).abs() < 1e-4, "vertex not on unit sphere: {len}");
        }
    }

    #[test]
    fn cylinder_bounding_box() {
        let s  = Cylinder::new(1.0, 3.0);
        let m  = CpuMesh::from_surface(&s, 32, 8);
        let bb = m.bounding_box();
        // Height should span [0, 3]
        assert!((bb.min.z - 0.0).abs() < 0.01);
        assert!((bb.max.z - 3.0).abs() < 0.01);
    }

    #[test]
    fn recompute_normals_plane() {
        let s = Plane::new(1.0, 1.0);
        let mut m = CpuMesh::from_surface(&s, 2, 2);
        m.recompute_normals();
        for n in &m.normals {
            let nv = Vec3::from(*n);
            // Plane normals should point +Z
            assert!(nv.dot(Vec3::Z) > 0.9, "plane normal not +Z: {nv}");
        }
    }

    #[test]
    fn merge_two_planes() {
        let s = Plane::new(1.0, 1.0);
        let m = CpuMesh::from_surface(&s, 2, 2);
        let vc = m.vertex_count();
        let tc = m.triangle_count();
        let mut merged = m.clone();
        merged.merge(&CpuMesh::from_surface(&s, 2, 2));
        assert_eq!(merged.vertex_count(), vc * 2);
        assert_eq!(merged.triangle_count(), tc * 2);
    }
}
