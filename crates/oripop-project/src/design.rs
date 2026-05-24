use serde::{Deserialize, Serialize};

use crate::canvas::CanvasKind;
use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignManifest {
    pub format_version: u32,
    pub id: String,
    pub title: String,
    pub engine_version: String,
    pub canvas: CanvasKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub params: String,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_ref: Option<LibraryRef>,
}

impl DesignManifest {
    pub fn new(id: impl Into<String>, title: impl Into<String>, canvas: CanvasKind) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            id: id.into(),
            title: title.into(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            canvas,
            tags: Vec::new(),
            params: "params.json".to_string(),
            entry: "main.rs".to_string(),
            library_ref: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryRef {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params_override: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasKind;

    #[test]
    fn design_manifest_round_trip() {
        let m = DesignManifest::new(
            "coral-field-v2",
            "Coral field study",
            CanvasKind::Provisional {
                width: 1024,
                height: 1024,
            },
        );
        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: DesignManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
