//! Default paths for project and engine root.

use std::env;
use std::io;
use std::path::PathBuf;

use crate::engine::find_engine_root;

pub fn engine_root() -> io::Result<PathBuf> {
    find_engine_root(&env::current_dir()?).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not find ori-pop engine root (workspace Cargo.toml)",
        )
    })
}

pub fn default_project_path() -> PathBuf {
    if let Ok(root) = engine_root() {
        return root.join("projects/example-project");
    }
    PathBuf::from("projects/example-project")
}
