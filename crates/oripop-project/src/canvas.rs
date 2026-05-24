use serde::{Deserialize, Serialize};

/// Which 2D domain a design renders into.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanvasKind {
    /// Fixed pixel buffer with no garment panels.
    Provisional {
        width: u32,
        height: u32,
    },
    /// Built-in mesh UV parameterization for exploration previz.
    PrimitiveUv {
        mesh: PrimitiveMesh,
    },
    /// Production atlas with one or more panel regions.
    Atlas {
        atlas_ref: String,
        panels: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveMesh {
    Plane,
    Sphere,
    Cube,
    Cylinder,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisional_canvas_round_trip() {
        let c = CanvasKind::Provisional {
            width: 1024,
            height: 1024,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: CanvasKind = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
