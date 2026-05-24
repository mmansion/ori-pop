use serde::{Deserialize, Serialize};

use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasManifest {
    pub format_version: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub dpi: u32,
    pub physical_width_mm: f32,
    pub physical_height_mm: f32,
    pub fabrication_layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_layout: Option<String>,
    pub cut_lines: String,
    pub panels_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelManifest {
    pub format_version: u32,
    pub id: String,
    pub title: String,
    pub physical_width_mm: f32,
    pub physical_height_mm: f32,
    pub mesh: PanelLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_source: Option<PanelImportSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelLink {
    pub material_slot: String,
    pub uv_island_index: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelImportSource {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutLinesManifest {
    pub format_version: u32,
    pub paths: Vec<CutLinePath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutLinePath {
    pub id: String,
    pub closed: bool,
    /// `[x, y]` points in atlas pixel space.
    pub points: Vec<[f32; 2]>,
}

impl AtlasManifest {
    pub fn new(
        width_px: u32,
        height_px: u32,
        dpi: u32,
        physical_width_mm: f32,
        physical_height_mm: f32,
    ) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            width_px,
            height_px,
            dpi,
            physical_width_mm,
            physical_height_mm,
            fabrication_layout: "fabrication_layout.json".to_string(),
            authoring_layout: None,
            cut_lines: "cut_lines.json".to_string(),
            panels_dir: "panels/".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_lines_round_trip() {
        let c = CutLinesManifest {
            format_version: FORMAT_VERSION,
            paths: vec![CutLinePath {
                id: "seam-armhole-left".to_string(),
                closed: false,
                points: vec![[120.0, 80.0], [2168.0, 80.0]],
            }],
        };
        let json = serde_json::to_string_pretty(&c).unwrap();
        let back: CutLinesManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
