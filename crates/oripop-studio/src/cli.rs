//! CLI subcommands (project list, bake).

use std::env;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use oripop_project::Project;

use crate::bake::BakeOptions;
use crate::gpu::PreviewGpu;
use crate::paths::default_project_path;
use crate::preview::load_cartridge;

pub fn run_cli() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return ExitCode::SUCCESS;
    };

    let result = match cmd.as_str() {
        "project" => cmd_project(&mut args),
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
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("oripop-studio error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_project(args: &mut impl Iterator<Item = String>) -> io::Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    match sub.as_str() {
        "list" => {
            let rest: Vec<_> = args.collect();
            let path = flag_path(&rest, "--project").unwrap_or_else(default_project_path);
            let project = Project::load(&path)?;
            println!(
                "{} ({})",
                project.manifest.title, project.manifest.engine_version
            );
            if project.textures.is_empty() {
                println!("  (no textures)");
            } else {
                for t in &project.textures {
                    println!("  {}  {}", t.id, t.path.display());
                }
            }
            Ok(())
        }
        _ => {
            eprintln!("usage: oripop-studio project list [--project PATH]");
            Ok(())
        }
    }
}

fn cmd_bake(args: &mut impl Iterator<Item = String>) -> io::Result<()> {
    let rest: Vec<_> = args.collect();
    let (project, texture_id) = parse_project_and_texture(&rest)?;
    let (cartridge, width, height) = load_cartridge(&project, &texture_id)?;
    let mut gpu = PreviewGpu::new_headless()?;
    let (png, manifest) = crate::bake::bake(
        &project,
        &texture_id,
        &cartridge,
        width,
        height,
        &mut gpu,
        BakeOptions::default(),
    )?;
    println!("baked {}", png.display());
    println!("manifest {}", manifest.display());
    Ok(())
}

fn parse_project_and_texture(rest: &[String]) -> io::Result<(Project, String)> {
    let path = flag_path(rest, "--project").unwrap_or_else(default_project_path);
    let project = Project::load(&path)?;
    let texture = flag(rest, "--texture").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "--texture is required")
    })?;
    Ok((project, texture))
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

pub fn print_usage() {
    eprintln!(
        "oripop-studio {} — texture project shell\n\
         \n\
         Launch with no arguments to open the studio window.\n\
         \n\
         COMMANDS:\n\
           project list [--project PATH]                List textures in a project\n\
           bake --texture ID [--project PATH]           Headless GPU bake → PNG + manifest\n\
         \n\
         Default project: projects/example-project (relative to engine root)\n",
        env!("CARGO_PKG_VERSION")
    );
}
