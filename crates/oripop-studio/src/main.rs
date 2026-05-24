//! Ori Pop Studio — texture library CLI (Phase 1).

mod bake;
mod engine;
mod play;

use std::env;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use oripop_project::TextureLibrary;

use crate::bake::BakeOptions;
use crate::engine::find_engine_root;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("oripop-studio error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return Ok(());
    };

    match cmd.as_str() {
        "library" => cmd_library(&mut args),
        "play" => cmd_play(&mut args),
        "bake" => cmd_bake(&mut args),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        _ => {
            eprintln!("unknown command: {cmd}\n");
            print_usage();
            Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown command"))
        }
    }
}

fn cmd_library(args: &mut impl Iterator<Item = String>) -> io::Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    match sub.as_str() {
        "list" => {
            let rest: Vec<_> = args.collect();
            let path = flag_path(&rest, "--library").unwrap_or_else(default_library_path);
            let library = TextureLibrary::load(&path)?;
            println!("{} ({})", library.manifest.title, library.manifest.engine_version);
            if library.designs().is_empty() {
                println!("  (no designs)");
            } else {
                for d in library.designs() {
                    println!("  {}  {}", d.id, d.path);
                }
            }
            Ok(())
        }
        _ => {
            eprintln!("usage: oripop-studio library list [--library PATH]");
            Ok(())
        }
    }
}

fn cmd_play(args: &mut impl Iterator<Item = String>) -> io::Result<()> {
    let rest: Vec<_> = args.collect();
    let (library, design) = parse_library_and_design(&rest)?;
    let engine = engine_root()?;
    play::play(&library, &design, &engine)
}

fn cmd_bake(args: &mut impl Iterator<Item = String>) -> io::Result<()> {
    let rest: Vec<_> = args.collect();
    let (library, design) = parse_library_and_design(&rest)?;
    let (png, manifest) = bake::bake(&library, &design, BakeOptions::default())?;
    println!("baked {}", png.display());
    println!("manifest {}", manifest.display());
    Ok(())
}

fn parse_library_and_design(rest: &[String]) -> io::Result<(TextureLibrary, String)> {
    let path = flag_path(rest, "--library").unwrap_or_else(default_library_path);
    let library = TextureLibrary::load(&path)?;
    let design = flag(rest, "--design").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "--design is required")
    })?;
    Ok((library, design))
}

fn flag(rest: &[String], name: &str) -> Option<String> {
    rest.iter()
        .position(|a| a == name)
        .and_then(|i| rest.get(i + 1))
        .cloned()
}

fn flag_path(rest: &[String], name: &str) -> Option<PathBuf> {
    flag(rest, name).map(PathBuf::from)
}

fn default_library_path() -> PathBuf {
    if let Ok(root) = engine_root() {
        return root.join("examples/texture-library");
    }
    PathBuf::from("examples/texture-library")
}

fn engine_root() -> io::Result<PathBuf> {
    find_engine_root(&env::current_dir()?).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not find ori-pop engine root (workspace Cargo.toml)",
        )
    })
}

fn print_usage() {
    eprintln!(
        "oripop-studio {} — Phase 1 texture library CLI\n\
         \n\
         COMMANDS:\n\
           library list [--library PATH]     List designs in a texture library\n\
           play --design ID [--library PATH] Generate build, compile, and run design\n\
           bake --design ID [--library PATH] Headless stipple bake → PNG + manifest\n\
         \n\
         Default library: examples/texture-library (relative to engine root)\n",
        env!("CARGO_PKG_VERSION")
    );
}
