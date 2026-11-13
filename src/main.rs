use std::{fs, path::PathBuf, process::ExitCode};

use clap::Parser;
use log::error;

use crate::{script_thread::ScriptThread, wayland::WaylandPlatform};

mod canvas;
mod js;
mod platform;
mod script_thread;
mod task;
mod wayland;

/// A lightweight widget runtime for Wayland.
#[derive(Debug, Parser)]
#[command(name = "widjet", version, about)]
struct Cli {
    /// Path to the JavaScript widget script.
    script: PathBuf,
}

fn main() -> ExitCode {
    env_logger::init();

    let cli = Cli::parse();

    let js = match fs::read_to_string(&cli.script) {
        Ok(js) => js,
        Err(error) => {
            error!("widjet: failed to read {}: {error}", cli.script.display());
            return ExitCode::FAILURE;
        }
    };

    let platform = match WaylandPlatform::new() {
        Ok(platform) => platform,
        Err(error) => {
            error!("widjet: {error}");
            return ExitCode::FAILURE;
        }
    };

    match ScriptThread::new(platform).run(&js) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!("widjet: {error}");
            ExitCode::FAILURE
        }
    }
}
