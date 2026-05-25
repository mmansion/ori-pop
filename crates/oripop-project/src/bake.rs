use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakeManifest {
    pub format_version: u32,
    pub texture_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub created: String,
    pub image: String,
    pub width_px: u32,
    pub height_px: u32,
    pub layout: BakeLayout,
    pub canvas_kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<BakeLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params_snapshot: Option<Value>,
    pub reproducible: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BakeLayout {
    Fabrication,
    Authoring,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakeLock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl BakeManifest {
    pub fn new(
        texture_id: impl Into<String>,
        created: impl Into<String>,
        image: impl Into<String>,
        width_px: u32,
        height_px: u32,
        reproducible: bool,
    ) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            texture_id: texture_id.into(),
            project_id: None,
            created: created.into(),
            image: image.into(),
            width_px,
            height_px,
            layout: BakeLayout::Fabrication,
            canvas_kind: "provisional".to_string(),
            panels: Vec::new(),
            lock: None,
            params_snapshot: None,
            reproducible,
            notes: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_manifest_round_trip() {
        let mut b = BakeManifest::new(
            "coral-field-v2",
            "2026-05-24T14:12:00Z",
            "2026-05-24T14-12-00.png",
            1024,
            1024,
            true,
        );
        b.lock = Some(BakeLock {
            frame: Some(847),
            time: Some(14.12),
            seed: Some(985_734_123),
        });
        let json = serde_json::to_string_pretty(&b).unwrap();
        let back: BakeManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }
}
