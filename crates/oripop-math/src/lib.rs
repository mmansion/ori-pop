//! # oripop-math
//!
//! The GPU-free geometry kernel and design-tree foundation of ori-pop.
//!
//! Every other crate in the workspace depends on this one for shared types.
//! Because it carries no GPU weight (`wgpu` and `winit` are not dependencies),
//! it compiles and tests on any machine including headless CI.
//!
//! ## Core abstractions
//!
//! ### The Design Tree
//! [`tree::DesignTree`] is the complete mathematical description of a design —
//! a directed acyclic graph of typed, named, serializable [`node::Node`]s.
//! It is the portable, deterministic, agent-readable representation of
//! everything that defines a designed object.
//!
//! ### Surfaces
//! [`surface::Surface`] is a parametric surface `S(u,v): [0,1]² → ℝ³` in
//! Z-up right-handed world space. Concrete implementations: [`surface::UvSphere`],
//! [`surface::Plane`], [`surface::Cylinder`], [`surface::Torus`].
//!
//! ### Meshes
//! [`mesh::CpuMesh`] is the CPU-side tessellated mesh — the handoff format
//! between the geometry kernel, the GPU uploader, and fabrication exporters.
//!
//! ### Frames
//! [`frame::Frame`] is a Z-up coordinate frame — origin plus orthonormal basis.
//! Represents robot poses, print-bed orientation, and surface tangent frames.
//!
//! ## Coordinate convention
//!
//! **Z-up right-handed** throughout: X = right, Y = forward, Z = up.
//! XY is the ground / build plane. Gravity is `(0, 0, -9.81)`.

pub mod frame;
pub mod mesh;
pub mod node;
pub mod surface;
pub mod tree;
pub mod value;

// Flat re-exports for convenience
pub use frame::Frame;
pub use mesh::{BoundingBox, CpuMesh};
pub use node::{Edge, Node, NodeId, Port, PortType};
pub use surface::{Cylinder, Plane, PrincipalCurvatures, Surface, Torus, UvSphere};
pub use tree::{DesignTree, Metadata};
pub use value::{Domain, Param, Value};
