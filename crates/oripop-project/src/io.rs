//! Read and write studio manifests on disk.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::design::DesignManifest;
use crate::library::{LibraryEntry, LibraryManifest};
use crate::FORMAT_VERSION;

/// A loaded texture library root directory.
#[derive(Debug, Clone)]
pub struct TextureLibrary {
    pub root: PathBuf,
    pub manifest: LibraryManifest,
}

impl TextureLibrary {
    /// Load `library.oripop` from `root`.
    pub fn load(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let path = root.join("library.oripop");
        let text = fs::read_to_string(&path)?;
        let manifest: LibraryManifest = serde_json::from_str(&text).map_err(json_err)?;
        if manifest.format_version != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported library.oripop format_version {} (expected {FORMAT_VERSION})",
                    manifest.format_version
                ),
            ));
        }
        Ok(Self { root, manifest })
    }

    /// Resolve a design id to its folder and manifest.
    pub fn design(&self, id: &str) -> io::Result<(PathBuf, DesignManifest)> {
        let entry = self
            .manifest
            .designs
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| missing_design(id))?;
        let dir = self.root.join(&entry.path);
        let manifest_path = dir.join("design.oripop");
        let text = fs::read_to_string(&manifest_path)?;
        let manifest: DesignManifest = serde_json::from_str(&text).map_err(json_err)?;
        if manifest.id != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "design.oripop id {:?} does not match requested {:?}",
                    manifest.id, id
                ),
            ));
        }
        Ok((dir, manifest))
    }

    /// List design entries.
    pub fn designs(&self) -> &[LibraryEntry] {
        &self.manifest.designs
    }
}

fn missing_design(id: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("design not found in library: {id}"),
    )
}

fn json_err(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasKind;
    use crate::design::DesignManifest;

    #[test]
    fn load_example_library() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/texture-library");
        let lib = TextureLibrary::load(&root).expect("example library");
        assert!(!lib.designs().is_empty());
        let (dir, design) = lib.design("coral-stipple").expect("coral-stipple");
        assert!(dir.join("main.rs").is_file());
        assert!(matches!(
            design.canvas,
            CanvasKind::Provisional { width: 1024, height: 1024 }
        ));
        let _ = DesignManifest::new("x", "y", design.canvas); // silence unused import check
    }
}
