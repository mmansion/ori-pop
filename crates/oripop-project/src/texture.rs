//! Per-texture manifest (`texture.oripop`).

use serde::{Deserialize, Serialize};

use crate::canvas::CanvasKind;
use crate::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureManifest {
    pub format_version: u32,
    pub id:             String,
    pub title:          String,
    pub engine_version: String,
    pub canvas:         CanvasKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags:           Vec<String>,
    pub params:         String,
}

impl TextureManifest {
    pub fn new(id: impl Into<String>, title: impl Into<String>, canvas: CanvasKind) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            id:             id.into(),
            title:          title.into(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            canvas,
            tags:           Vec::new(),
            params:         "params.json".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasKind;

    #[test]
    fn texture_manifest_round_trip() {
        let m = TextureManifest::new(
            "coral-stipple",
            "Coral stipple field",
            CanvasKind::Provisional {
                width:  1024,
                height: 1024,
            },
        );
        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: TextureManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
