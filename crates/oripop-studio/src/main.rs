//! Ori Pop Studio — texture library shell (GUI + CLI).

mod app;
mod bake;
mod cli;
mod editor;
mod engine;
mod gpu;
mod paths;
mod play;
mod preview;
mod window;

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        return cli::run_cli();
    }

    match app::run_gui() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("oripop-studio error: {e}");
            ExitCode::FAILURE
        }
    }
}
