use serde::{Deserialize, Serialize};

use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryManifest {
    pub format_version: u32,
    pub engine_version: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub designs: Vec<LibraryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<LibraryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub path: String,
}

impl LibraryManifest {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            title: title.into(),
            designs: Vec::new(),
            presets: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_manifest_round_trip() {
        let mut lib = LibraryManifest::new("Test library");
        lib.designs.push(LibraryEntry {
            id: "coral-field-v2".to_string(),
            path: "designs/coral-field-v2".to_string(),
        });
        let json = serde_json::to_string_pretty(&lib).unwrap();
        let back: LibraryManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(lib, back);
    }
}
