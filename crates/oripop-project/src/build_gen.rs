//! Generate `.oripop/build/` Cargo workspace for Play.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::TextureLibrary;

/// Output paths after a successful build generation.
#[derive(Debug, Clone)]
pub struct GeneratedBuild {
    pub build_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub bin_name: String,
}

/// Write `.oripop/build/Cargo.toml` and copy `main.rs` for one library design.
pub fn generate_library_build(
    library: &TextureLibrary,
    design_id: &str,
    engine_root: &Path,
) -> io::Result<GeneratedBuild> {
    let (design_dir, design) = library.design(design_id)?;
    let main_src = design_dir.join(&design.entry);
    if !main_src.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("design entry not found: {}", main_src.display()),
        ));
    }

    let build_dir = library.root.join(".oripop/build");
    let src_dir = build_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let bin_name = sanitize_bin_name(&design.id);
    let dest_rs = src_dir.join(format!("{bin_name}.rs"));
    fs::copy(&main_src, &dest_rs)?;

    let runtime_path = relative_path(&build_dir, &engine_root.join("crates/oripop-runtime"))?;
    let canvas_path = relative_path(&build_dir, &engine_root.join("crates/oripop-canvas"))?;
    let manifest_path = build_dir.join("Cargo.toml");
    let manifest = format!(
        r#"[package]
name = "oripop-library-build"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[[bin]]
name = "{bin_name}"
path = "src/{bin_name}.rs"

[dependencies]
oripop-runtime = {{ path = "{runtime_path}" }}
oripop-canvas = {{ path = "{canvas_path}" }}
serde_json = "1"
"#
    );
    fs::write(&manifest_path, manifest)?;

    Ok(GeneratedBuild {
        build_dir,
        manifest_path,
        bin_name,
    })
}

fn sanitize_bin_name(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "design".to_string()
    } else {
        out
    }
}

/// Best-effort relative path for Cargo.toml `{ path = "..." }`.
fn relative_path(from: &Path, to: &Path) -> io::Result<String> {
    relative_path_simple(from, to)
}

fn relative_path_simple(from: &Path, to: &Path) -> io::Result<String> {
    let from = fs::canonicalize(from)?;
    let to = fs::canonicalize(to)?;
    let mut rel = PathBuf::new();
    let mut cur = from.as_path();
    while !to.starts_with(cur) {
        match cur.parent() {
            Some(p) => {
                rel.push("..");
                cur = p;
            }
            None => {
                return Ok(to.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    if let Ok(rest) = to.strip_prefix(cur) {
        rel.push(rest);
    }
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn generate_build_for_example() {
        let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let lib_root = engine.join("examples/texture-library");
        let library = TextureLibrary::load(&lib_root).unwrap();
        let gen = generate_library_build(&library, "coral-stipple", &engine).unwrap();
        assert!(gen.manifest_path.is_file());
        assert!(gen.build_dir.join("src/coral-stipple.rs").is_file());
    }
}
