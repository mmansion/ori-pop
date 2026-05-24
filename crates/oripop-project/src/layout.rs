use serde::{Deserialize, Serialize};

use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FabricationLayout {
    pub format_version: u32,
    pub panels: Vec<PanelRect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelRect {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub rotation_deg: f32,
}

impl FabricationLayout {
    pub fn new(panels: Vec<PanelRect>) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            panels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabrication_layout_round_trip() {
        let layout = FabricationLayout::new(vec![PanelRect {
            id: "sleeve-left".to_string(),
            x: 120.0,
            y: 80.0,
            width: 2048.0,
            height: 3072.0,
            rotation_deg: 0.0,
        }]);
        let json = serde_json::to_string_pretty(&layout).unwrap();
        let back: FabricationLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, back);
    }
}
