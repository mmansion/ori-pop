use serde::{Deserialize, Serialize};

use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub format_version: u32,
    pub engine_version: String,
    pub title: String,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_design: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atlas: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub designs: Vec<DesignEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library_refs: Vec<LibraryRefEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignEntry {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryRefEntry {
    pub design_id: String,
    pub uri: String,
}

impl ProjectManifest {
    pub fn new(title: impl Into<String>, created: impl Into<String>) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            title: title.into(),
            created: created.into(),
            default_design: None,
            assembly: None,
            atlas: None,
            designs: Vec::new(),
            library_refs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_manifest_round_trip() {
        let mut p = ProjectManifest::new("Spring garment", "2026-05-24T12:00:00Z");
        p.assembly = Some("assembly/garment.glb".to_string());
        p.atlas = Some("atlas/atlas.oripop".to_string());
        p.designs.push(DesignEntry {
            id: "field-span-v1".to_string(),
            path: "designs/field-span-v1".to_string(),
        });
        let json = serde_json::to_string_pretty(&p).unwrap();
        let back: ProjectManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
