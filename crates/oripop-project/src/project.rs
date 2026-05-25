//! Project manifest (`project.oripop`) and on-disk project loader.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::texture::TextureManifest;
use crate::FORMAT_VERSION;

/// Top-level project manifest: one per project folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub format_version: u32,
    pub engine_version: String,
    pub title:          String,
    pub created:        String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_texture: Option<String>,
}

impl ProjectManifest {
    pub fn new(title: impl Into<String>, created: impl Into<String>) -> Self {
        Self {
            format_version:  FORMAT_VERSION,
            engine_version:  env!("CARGO_PKG_VERSION").to_string(),
            title:           title.into(),
            created:         created.into(),
            default_texture: None,
        }
    }
}

/// One texture discovered inside `<project>/textures/`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureEntry {
    pub id:   String,
    pub path: PathBuf,
}

/// A loaded project on disk.
#[derive(Debug, Clone)]
pub struct Project {
    pub root:     PathBuf,
    pub manifest: ProjectManifest,
    pub textures: Vec<TextureEntry>,
}

impl Project {
    /// Load `project.oripop` from `root` and scan `<root>/textures/` for
    /// folders that contain a `texture.oripop`.
    pub fn load(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join("project.oripop");
        let text = fs::read_to_string(&manifest_path)?;
        let manifest: ProjectManifest = serde_json::from_str(&text).map_err(json_err)?;
        if manifest.format_version != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported project.oripop format_version {} (expected {FORMAT_VERSION})",
                    manifest.format_version
                ),
            ));
        }

        let textures_dir = root.join("textures");
        let mut textures = Vec::new();
        if textures_dir.is_dir() {
            for entry in fs::read_dir(&textures_dir)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let manifest_path = entry.path().join("texture.oripop");
                if !manifest_path.is_file() {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().to_string();
                textures.push(TextureEntry {
                    id,
                    path: entry.path(),
                });
            }
        }
        textures.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(Self {
            root,
            manifest,
            textures,
        })
    }

    /// Resolve a texture id to its folder and manifest.
    pub fn texture(&self, id: &str) -> io::Result<(PathBuf, TextureManifest)> {
        let entry = self
            .textures
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| missing_texture(id))?;
        let manifest_path = entry.path.join("texture.oripop");
        let text = fs::read_to_string(&manifest_path)?;
        let manifest: TextureManifest = serde_json::from_str(&text).map_err(json_err)?;
        if manifest.id != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "texture.oripop id {:?} does not match folder {:?}",
                    manifest.id, id
                ),
            ));
        }
        Ok((entry.path.clone(), manifest))
    }
}

fn missing_texture(id: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("texture not found in project: {id}"),
    )
}

fn json_err(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasKind;

    #[test]
    fn project_manifest_round_trip() {
        let p = ProjectManifest::new("Example", "2026-05-25T00:00:00Z");
        let json = serde_json::to_string_pretty(&p).unwrap();
        let back: ProjectManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn load_example_project() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../projects/example-project");
        if !root.join("project.oripop").is_file() {
            // example may not exist when running outside the workspace
            return;
        }
        let project = Project::load(&root).expect("example project");
        assert!(!project.textures.is_empty());
        let (dir, manifest) = project
            .texture("coral-stipple")
            .expect("coral-stipple");
        assert!(dir.join("Cargo.toml").is_file());
        assert!(matches!(
            manifest.canvas,
            CanvasKind::Provisional { width: 1024, height: 1024 }
        ));
    }
}
