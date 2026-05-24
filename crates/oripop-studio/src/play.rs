//! Compile and run a library design binary.

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use oripop_project::{generate_library_build, GeneratedBuild, TextureLibrary};

/// Write `.oripop/build/` and compile the design binary.
pub fn compile_design(
    library: &TextureLibrary,
    design_id: &str,
    engine_root: &Path,
) -> io::Result<GeneratedBuild> {
    let gen = generate_library_build(library, design_id, engine_root)?;
    let build_status = Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(&gen.manifest_path)
        .arg("--bin")
        .arg(&gen.bin_name)
        .status()?;
    if !build_status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "cargo build failed",
        ));
    }
    Ok(gen)
}

/// Build if needed, then spawn the preview process (non-blocking).
pub fn spawn_play(
    library: &TextureLibrary,
    design_id: &str,
    engine_root: &Path,
) -> io::Result<Child> {
    let (design_dir, _) = library.design(design_id)?;
    let gen = compile_design(library, design_id, engine_root)?;
    let bin_path = gen
        .build_dir
        .join("target/debug")
        .join(exe_name(&gen.bin_name));

    Command::new(&bin_path)
        .env("ORIPOP_DESIGN_DIR", &design_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// Build, run, and wait until the preview window closes.
pub fn play(library: &TextureLibrary, design_id: &str, engine_root: &Path) -> io::Result<()> {
    let mut child = spawn_play(library, design_id, engine_root)?;
    let status = child.wait()?;
    if !status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "design exited with error"));
    }
    Ok(())
}

fn exe_name(bin: &str) -> String {
    if cfg!(windows) {
        format!("{bin}.exe")
    } else {
        bin.to_string()
    }
}
