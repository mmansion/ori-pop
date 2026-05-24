//! Locate the ori-pop engine (workspace) root.

use std::path::{Path, PathBuf};

/// Walk upward from `start` looking for the workspace `Cargo.toml`.
pub fn find_engine_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let cargo = dir.join("Cargo.toml");
        if cargo.is_file() {
            if let Ok(text) = std::fs::read_to_string(&cargo) {
                if text.contains("[workspace]") && text.contains("oripop-studio") {
                    return Some(dir);
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn finds_repo_root_from_crate() {
        let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = find_engine_root(&start).expect("engine root");
        assert!(root.join("crates/oripop-runtime").is_dir());
    }
}
