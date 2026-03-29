//! 3D mesh geometry: vertex layout, index buffers, and primitive generators.
//!
//! All primitives use the **Z-up right-handed** convention:
//! X = right, Y = forward/depth, Z = up.
//! The XY plane is the ground plane (fabrication bed).
//!
//! [`MeshKind::Sphere`] and [`MeshKind::Plane`] are generated through
//! `oripop-math`'s parametric surface layer — the same mathematical objects
//! that feed the design tree, curvature analysis, and fabrication export.
//! [`MeshKind::Cube`] remains hand-built until `oripop-geo` provides SDF/CSG.

use bytemuck::{Pod, Zeroable};

// ── Vertex ───────────────────────────────────────────────────────────────────

/// A single 3D vertex with position, surface normal, and UV texture coordinates.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal:   [f32; 3],
    pub uv:       [f32; 2],
}

impl Vertex3D {
    /// wgpu vertex buffer layout matching the attribute locations in `shader3d.wgsl`.
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode:    wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset:           0,
                shader_location:  0,
                format:           wgpu::VertexFormat::Float32x3, // position
            },
            wgpu::VertexAttribute {
                offset:           12,
                shader_location:  1,
                format:           wgpu::VertexFormat::Float32x3, // normal
            },
            wgpu::VertexAttribute {
                offset:           24,
                shader_location:  2,
                format:           wgpu::VertexFormat::Float32x2, // uv
            },
        ],
    };
}

// ── Mesh ─────────────────────────────────────────────────────────────────────

/// CPU-side mesh: vertex + index data ready to upload to GPU.
pub struct Mesh {
    pub vertices: Vec<Vertex3D>,
    pub indices:  Vec<u32>,
}

impl Mesh {
    /// Convert an `oripop-math` [`CpuMesh`] into a GPU-ready [`Mesh`].
    ///
    /// This is the bridge between the parametric math layer and the renderer.
    /// The interleaved `Vertex3D` layout required by the wgpu pipeline is built
    /// here from the separate position / normal / UV arrays of `CpuMesh`.
    pub fn from_math(cpu: oripop_math::CpuMesh) -> Self {
        let vertices = cpu.positions
            .into_iter()
            .zip(cpu.normals)
            .zip(cpu.uvs)
            .map(|((position, normal), uv)| Vertex3D { position, normal, uv })
            .collect();
        Self { vertices, indices: cpu.indices }
    }
}

// ── Primitive kind ───────────────────────────────────────────────────────────

/// Built-in mesh primitive that the renderer pre-uploads to the GPU at startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshKind {
    Sphere,
    Cube,
    Plane,
}

impl MeshKind {
    pub(crate) fn build(self) -> Mesh {
        match self {
            // Sphere and Plane are generated through oripop-math's parametric
            // surface layer — the same objects used for design-tree evaluation,
            // curvature analysis, and fabrication export.
            MeshKind::Sphere => {
                let cpu = oripop_math::CpuMesh::from_surface(
                    &oripop_math::UvSphere::new(1.0),
                    64,  // sectors (longitude divisions)
                    48,  // stacks  (latitude divisions)
                );
                Mesh::from_math(cpu)
            }
            MeshKind::Plane => {
                let cpu = oripop_math::CpuMesh::from_surface(
                    &oripop_math::Plane::square(1.0),
                    1, 1,
                );
                Mesh::from_math(cpu)
            }
            // Cube remains hand-built — a cube is not a smooth parametric surface.
            // A proper CSG / SDF cube will replace this when oripop-geo lands.
            MeshKind::Cube => cube(1.0),
        }
    }
}

// ── Primitive generators — all Z-up right-handed ─────────────────────────────

/// Axis-aligned cube centred at the origin, Z-up.
///
/// +Z = top, −Z = bottom, −Y = front (toward default camera), +Y = back,
/// +X = right, −X = left.
pub fn cube(size: f32) -> Mesh {
    let h = size * 0.5;
    let mut vertices: Vec<Vertex3D> = Vec::new();
    let mut indices:  Vec<u32>      = Vec::new();

    // Each entry: ([4 corner positions], outward normal)
    // Winding is CCW when viewed from outside (right-hand rule → thumb = normal).
    let faces: &[([[f32; 3]; 4], [f32; 3])] = &[
        // Top (+Z)
        ([[-h,-h, h],[ h,-h, h],[ h, h, h],[-h, h, h]], [ 0.0,  0.0,  1.0]),
        // Bottom (−Z)
        ([[-h, h,-h],[ h, h,-h],[ h,-h,-h],[-h,-h,-h]], [ 0.0,  0.0, -1.0]),
        // Front (−Y, faces the default camera at Y < 0)
        ([[-h,-h,-h],[ h,-h,-h],[ h,-h, h],[-h,-h, h]], [ 0.0, -1.0,  0.0]),
        // Back (+Y)
        ([[ h, h,-h],[-h, h,-h],[-h, h, h],[ h, h, h]], [ 0.0,  1.0,  0.0]),
        // Right (+X)
        ([[ h, h,-h],[ h, h, h],[ h,-h, h],[ h,-h,-h]], [ 1.0,  0.0,  0.0]),
        // Left (−X)
        ([[-h,-h,-h],[-h,-h, h],[-h, h, h],[-h, h,-h]], [-1.0,  0.0,  0.0]),
    ];
    let uvs: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    for (positions, normal) in faces {
        let base = vertices.len() as u32;
        for (i, pos) in positions.iter().enumerate() {
            vertices.push(Vertex3D { position: *pos, normal: *normal, uv: uvs[i] });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    Mesh { vertices, indices }
}
