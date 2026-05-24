//! Compile and run a library design binary.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use oripop_project::{generate_library_build, TextureLibrary};

pub fn play(library: &TextureLibrary, design_id: &str, engine_root: &Path) -> io::Result<()> {
    let (design_dir, _) = library.design(design_id)?;
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

    let bin_path = gen
        .build_dir
        .join("target/debug")
        .join(exe_name(&gen.bin_name));

    let run_status = Command::new(&bin_path)
        .env("ORIPOP_DESIGN_DIR", &design_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if !run_status.success() {
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
