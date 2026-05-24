//! Default paths for library and engine root.

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

pub fn default_library_path() -> PathBuf {
    if let Ok(root) = engine_root() {
        return root.join("examples/texture-library");
    }
    PathBuf::from("examples/texture-library")
}
