//! DesignTree — the complete mathematical description of a design.
//!
//! A `DesignTree` is a directed acyclic graph of [`Node`]s connected by
//! [`Edge`]s.  It is the portable, serializable, deterministic representation
//! of everything that defines a designed object: geometry, generative fields,
//! materials, and fabrication intent.
//!
//! # Serialization
//!
//! Two formats are supported:
//! - **RON** (Rusty Object Notation) — human-readable, Rust-native, used for
//!   saving and version-controlling designs.
//! - **JSON** — for interoperability with agents, web tools, and glTF `extras`.
//!
//! Both round-trip losslessly.

use serde::{Deserialize, Serialize};
use crate::node::{Edge, Node, NodeId, Port, PortType};
use crate::value::Param;

// ── Metadata ──────────────────────────────────────────────────────────────────

/// Descriptive metadata attached to a design.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Metadata {
    pub title:       Option<String>,
    pub author:      Option<String>,
    pub description: Option<String>,
    /// ISO-8601 creation timestamp — set once, never updated.
    pub created:     Option<String>,
    pub tags:        Vec<String>,
}

// ── DesignTree ────────────────────────────────────────────────────────────────

/// The complete mathematical description of a design.
///
/// Given the same `DesignTree`, any evaluator — renderer, fabrication bridge,
/// AI agent — produces the same result.  The tree is the design.
///
/// # Invariants
///
/// - Node IDs are unique within a tree.
/// - Edges connect ports of matching [`PortType`].
/// - The graph is acyclic (data flows in one direction only).
///
/// These invariants are not enforced by this crate (which is data-only) but
/// are expected by all evaluators.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DesignTree {
    /// Monotonically increasing schema version. Currently `1`.
    pub version:  u32,
    pub nodes:    Vec<Node>,
    pub edges:    Vec<Edge>,
    pub metadata: Metadata,

    #[serde(skip)]
    next_id: u64,
}

impl Default for DesignTree {
    fn default() -> Self {
        Self::new()
    }
}

impl DesignTree {
    pub fn new() -> Self {
        Self {
            version:  1,
            nodes:    Vec::new(),
            edges:    Vec::new(),
            metadata: Metadata::default(),
            next_id:  1,
        }
    }

    // ── Node management ───────────────────────────────────────────────────────

    /// Allocate a fresh [`NodeId`] and add the node to the tree.
    /// Returns the assigned id.
    pub fn add(&mut self, mut node: Node) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        node.id = id;
        self.nodes.push(node);
        id
    }

    /// Find a node by id (immutable).
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find a node by id (mutable).
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Find a node by label (immutable). Returns the first match.
    pub fn node_by_label(&self, label: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.label == label)
    }

    // ── Edge management ───────────────────────────────────────────────────────

    /// Connect an output port to an input port.
    ///
    /// No type checking is done here — the evaluator is responsible for
    /// validating that port types match.
    pub fn connect(
        &mut self,
        from_node: NodeId, from_port: impl Into<String>,
        to_node:   NodeId, to_port:   impl Into<String>,
    ) {
        self.edges.push(Edge {
            from_node,
            from_port: from_port.into(),
            to_node,
            to_port:   to_port.into(),
        });
    }

    /// All edges feeding into a given node.
    pub fn inputs_of(&self, id: NodeId) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.to_node == id)
    }

    /// All edges leaving a given node.
    pub fn outputs_of(&self, id: NodeId) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.from_node == id)
    }

    // ── Serialization ─────────────────────────────────────────────────────────

    /// Serialize to human-readable RON.  Use this for saving to disk and
    /// version control.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        let pretty = ron::ser::PrettyConfig::new().depth_limit(6);
        ron::ser::to_string_pretty(self, pretty)
    }

    /// Deserialize from RON.
    pub fn from_ron(s: &str) -> Result<Self, ron::error::SpannedError> {
        let mut tree: Self = ron::from_str(s)?;
        // Restore next_id from the highest existing id.
        tree.next_id = tree.nodes.iter().map(|n| n.id.0 + 1).max().unwrap_or(1);
        Ok(tree)
    }

    /// Serialize to JSON.  Use this for glTF `extras`, agent payloads, and
    /// web interoperability.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        let mut tree: Self = serde_json::from_str(s)?;
        tree.next_id = tree.nodes.iter().map(|n| n.id.0 + 1).max().unwrap_or(1);
        Ok(tree)
    }
}

// ── Builder helpers ───────────────────────────────────────────────────────────

/// Convenience: build a `UvSphere` node with standard ports and parameters.
pub fn uv_sphere_node(label: impl Into<String>, radius: f32) -> Node {
    use crate::value::Domain;
    Node::new(NodeId(0), label, "UvSphere")
        .with_param(
            Param::new("radius", radius)
                .with_domain(Domain::float(0.001, 1_000.0))
                .with_doc("Sphere radius in world units (Z-up).")
        )
        .with_param(
            Param::new("sectors", 48_u32)
                .with_domain(Domain::uint(3, 256))
                .with_doc("Longitudinal divisions.")
        )
        .with_param(
            Param::new("stacks", 32_u32)
                .with_domain(Domain::uint(2, 256))
                .with_doc("Latitudinal divisions.")
        )
        .with_output(Port::new("surface", PortType::Surface)
            .with_doc("Parametric surface — feeds UvField and Mesh generators."))
        .with_output(Port::new("mesh", PortType::Mesh)
            .with_doc("Tessellated mesh at the configured resolution."))
}

/// Convenience: build a flat `Plane` node.
pub fn plane_node(label: impl Into<String>, width: f32, height: f32) -> Node {
    use crate::value::Domain;
    Node::new(NodeId(0), label, "Plane")
        .with_param(
            Param::new("width", width)
                .with_domain(Domain::positive())
                .with_doc("Plane width along X (Z-up).")
        )
        .with_param(
            Param::new("height", height)
                .with_domain(Domain::positive())
                .with_doc("Plane height along Y (Z-up).")
        )
        .with_output(Port::new("surface", PortType::Surface))
        .with_output(Port::new("mesh", PortType::Mesh))
}

/// Convenience: build a `DomainWarpFbm` UV-field node.
pub fn domain_warp_fbm_node(label: impl Into<String>) -> Node {
    use crate::value::Domain;
    Node::new(NodeId(0), label, "DomainWarpFbm")
        .with_param(
            Param::new("octaves", 6_u32)
                .with_domain(Domain::uint(1, 8))
                .with_doc("FBM octave count — higher = finer detail.")
        )
        .with_param(
            Param::new("warp_strength", 2.0_f32)
                .with_domain(Domain::float(0.0, 4.0))
                .with_doc("Domain warp intensity — higher = more swirling.")
        )
        .with_param(
            Param::new("frequency", 3.0_f32)
                .with_domain(Domain::float(0.1, 10.0))
                .with_doc("Base spatial frequency of the noise.")
        )
        .with_param(
            Param::new("seed", 0.0_f32)
                .with_doc("Pattern offset — shifts the noise without changing character.")
        )
        .with_input(Port::new("surface", PortType::Surface)
            .with_doc("Optional surface for curvature-aware generation (Level 2)."))
        .with_output(Port::new("field", PortType::UvField)
            .with_doc("UV-space scalar field — drives rendering, toolpath, and export."))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> DesignTree {
        let mut tree = DesignTree::new();
        tree.metadata.title  = Some("Test sphere".into());
        tree.metadata.author = Some("ori-pop".into());

        let sphere_id = tree.add(uv_sphere_node("Sphere", 1.0));
        let field_id  = tree.add(domain_warp_fbm_node("FBM Texture"));
        tree.connect(sphere_id, "surface", field_id, "surface");
        tree
    }

    #[test]
    fn tree_ron_round_trip() {
        let tree = sample_tree();
        let ron  = tree.to_ron().expect("RON serialize");
        let back = DesignTree::from_ron(&ron).expect("RON deserialize");
        assert_eq!(back.nodes.len(), tree.nodes.len());
        assert_eq!(back.edges.len(), tree.edges.len());
        assert_eq!(back.metadata.title, tree.metadata.title);
    }

    #[test]
    fn tree_json_round_trip() {
        let tree = sample_tree();
        let json = tree.to_json().expect("JSON serialize");
        let back = DesignTree::from_json(&json).expect("JSON deserialize");
        assert_eq!(back.nodes.len(), 2);
        assert_eq!(back.edges.len(), 1);
    }

    #[test]
    fn next_id_restored_after_deserialize() {
        let tree  = sample_tree();
        let json  = tree.to_json().unwrap();
        let mut back = DesignTree::from_json(&json).unwrap();
        // Adding a new node should not collide with existing ids.
        let new_id = back.add(plane_node("Ground", 6.0, 6.0));
        assert!(back.nodes.iter().filter(|n| n.id == new_id).count() == 1);
    }

    #[test]
    fn inputs_outputs_of() {
        let tree = sample_tree();
        let sphere_id = tree.nodes[0].id;
        let field_id  = tree.nodes[1].id;
        assert_eq!(tree.outputs_of(sphere_id).count(), 1);
        assert_eq!(tree.inputs_of(field_id).count(),   1);
        assert_eq!(tree.inputs_of(sphere_id).count(),  0);
    }
}
