//! 3D mesh geometry: vertices, index buffers, and primitive generators.

use bytemuck::{Pod, Zeroable};
use std::f32::consts::PI;

// ── Vertex ───────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal:   [f32; 3],
    pub uv:       [f32; 2],
}

impl Vertex3D {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
        ],
    };
}

// ── Mesh ─────────────────────────────────────────────────────────────────────

pub struct Mesh {
    pub vertices: Vec<Vertex3D>,
    pub indices:  Vec<u32>,
}

// ── Primitive kind ───────────────────────────────────────────────────────────

/// A built-in mesh primitive that the renderer pre-uploads to the GPU.
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

// ── Primitive generators ─────────────────────────────────────────────────────

/// Flat XZ plane, centred at the origin, Y-up.
pub fn plane(size: f32) -> Mesh {
    let h = size * 0.5;
    let vertices = vec![
        Vertex3D { position: [-h, 0.0, -h], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
        Vertex3D { position: [ h, 0.0, -h], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
        Vertex3D { position: [ h, 0.0,  h], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
        Vertex3D { position: [-h, 0.0,  h], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    Mesh { vertices, indices }
}

/// Axis-aligned cube, centred at the origin.
pub fn cube(size: f32) -> Mesh {
    let h = size * 0.5;
    let mut vertices: Vec<Vertex3D> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // (corner positions for 4 verts, outward normal)
    let faces: &[([[f32; 3]; 4], [f32; 3])] = &[
        ([[-h,  h, -h], [ h,  h, -h], [ h,  h,  h], [-h,  h,  h]], [0.0,  1.0,  0.0]), // +Y
        ([[-h, -h,  h], [ h, -h,  h], [ h, -h, -h], [-h, -h, -h]], [0.0, -1.0,  0.0]), // -Y
        ([[-h, -h,  h], [ h, -h,  h], [ h,  h,  h], [-h,  h,  h]], [0.0,  0.0,  1.0]), // +Z
        ([[ h, -h, -h], [-h, -h, -h], [-h,  h, -h], [ h,  h, -h]], [0.0,  0.0, -1.0]), // -Z
        ([[ h, -h,  h], [ h, -h, -h], [ h,  h, -h], [ h,  h,  h]], [1.0,  0.0,  0.0]), // +X
        ([[-h, -h, -h], [-h, -h,  h], [-h,  h,  h], [-h,  h, -h]], [-1.0, 0.0,  0.0]), // -X
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

/// UV sphere, Y-up, centred at the origin.
///
/// `sectors` = horizontal divisions, `stacks` = vertical divisions.
/// Reasonable defaults: `uv_sphere(1.0, 48, 32)`.
pub fn uv_sphere(radius: f32, sectors: u32, stacks: u32) -> Mesh {
    let mut vertices: Vec<Vertex3D> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let sector_step = 2.0 * PI / sectors as f32;
    let stack_step  = PI / stacks as f32;

    for i in 0..=stacks {
        // stack angle from +Y (PI/2) to -Y (-PI/2)
        let phi = PI / 2.0 - i as f32 * stack_step;
        let xz  = radius * phi.cos();
        let y   = radius * phi.sin();

        for j in 0..=sectors {
            let theta = j as f32 * sector_step;
            let x = xz * theta.cos();
            let z = xz * theta.sin();

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
