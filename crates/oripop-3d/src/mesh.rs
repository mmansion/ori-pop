//! 3D mesh geometry: vertex layout, index buffers, and primitive generators.
//!
//! All primitives use the **Z-up right-handed** convention:
//! X = right, Y = forward/depth, Z = up.
//! The XY plane is the ground plane (fabrication bed).

use bytemuck::{Pod, Zeroable};
use std::f32::consts::PI;

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
            MeshKind::Sphere => uv_sphere(1.0, 48, 32),
            MeshKind::Cube   => cube(1.0),
            MeshKind::Plane  => plane(1.0),
        }
    }
}

// ── Primitive generators — all Z-up right-handed ─────────────────────────────

/// Flat XY plane centred at the origin, lying in the Z = 0 ground plane.
///
/// Normal points in the **+Z** direction (up).
/// UV: (0,0) at (−size/2, −size/2), (1,1) at (+size/2, +size/2).
pub fn plane(size: f32) -> Mesh {
    let h = size * 0.5;
    // CCW winding when viewed from +Z (top).
    let vertices = vec![
        Vertex3D { position: [-h, -h, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
        Vertex3D { position: [ h, -h, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
        Vertex3D { position: [ h,  h, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
        Vertex3D { position: [-h,  h, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    Mesh { vertices, indices }
}

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

/// UV sphere centred at the origin, **Z-up** (poles at ±Z).
///
/// `sectors` = horizontal (longitude) divisions.
/// `stacks`  = vertical (latitude) divisions.
/// Reasonable defaults: `uv_sphere(1.0, 48, 32)`.
///
/// UV: u ∈ [0,1] wraps longitude, v ∈ [0,1] goes from +Z pole to −Z pole.
pub fn uv_sphere(radius: f32, sectors: u32, stacks: u32) -> Mesh {
    let mut vertices: Vec<Vertex3D> = Vec::new();
    let mut indices:  Vec<u32>      = Vec::new();

    let sector_step = 2.0 * PI / sectors as f32;
    let stack_step  = PI / stacks as f32;

    for i in 0..=stacks {
        // phi goes from +π/2 (north pole, +Z) to −π/2 (south pole, −Z).
        let phi = PI / 2.0 - i as f32 * stack_step;
        let xy  = radius * phi.cos(); // radius of the ring in the XY plane
        let z   = radius * phi.sin(); // height along Z

        for j in 0..=sectors {
            let theta = j as f32 * sector_step;
            let x     = xy * theta.cos();
            let y     = xy * theta.sin();

            let nx = x / radius;
            let ny = y / radius;
            let nz = z / radius;

            let u = j as f32 / sectors as f32;
            let v = i as f32 / stacks as f32;

            vertices.push(Vertex3D {
                position: [x, y, z],
                normal:   [nx, ny, nz],
                uv:       [u, v],
            });
        }
    }

    for i in 0..stacks {
        let mut k1 = i * (sectors + 1);
        let mut k2 = k1 + sectors + 1;
        for _j in 0..sectors {
            if i != 0 {
                indices.extend_from_slice(&[k1, k2, k1 + 1]);
            }
            if i != stacks - 1 {
                indices.extend_from_slice(&[k1 + 1, k2, k2 + 1]);
            }
            k1 += 1;
            k2 += 1;
        }
    }

    Mesh { vertices, indices }
}
