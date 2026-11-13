use std::{fs, path::PathBuf, process::ExitCode};

use clap::Parser;

use crate::app::App;

mod app;
mod bridge;
mod canvas;
mod js;
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
    let cli = Cli::parse();

    let js = match fs::read_to_string(&cli.script) {
        Ok(js) => js,
        Err(error) => {
            eprintln!("widjet: failed to read {}: {error}", cli.script.display());
            return ExitCode::FAILURE;
        }
    };

    App::new().run(&js);

    ExitCode::SUCCESS
}
