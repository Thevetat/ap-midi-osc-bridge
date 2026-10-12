mod adapters;
mod ap;
mod midi;
mod profile;
mod runtime;

use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Parser)]
#[command(version, about = "MIDI to AUTO/PILOT OSC bridge")]
struct Options {
    /// List MIDI ports without opening them.
    #[arg(long)]
    list: bool,
    /// Read-only MIDI log for an input name or #index; sends no MIDI or OSC.
    #[arg(long, value_name = "INPUT", conflicts_with = "list")]
    monitor: Option<String>,
    /// Log MIDI and OSC traffic.
    #[arg(long)]
    verbose: bool,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    osc: Option<SocketAddr>,
    feedback: Option<SocketAddr>,
}

fn app_directory() -> Result<PathBuf> {
    let executable = std::env::current_exe().context("Locate the executable")?;
    let directory = executable.parent().context("Executable has no directory")?;
    // `cargo run` in a debug build can use the source profiles directly. Packaged
    // release builds always use their own directory, never an arbitrary cwd.
    if cfg!(debug_assertions) && !directory.join("profiles").is_dir() {
        let cwd = std::env::current_dir()?;
        if cwd.join("Cargo.toml").is_file() && cwd.join("profiles").is_dir() {
            return Ok(cwd);
        }
    }
    Ok(directory.to_path_buf())
}

fn run() -> Result<()> {
    let options = Options::parse();
    if options.list {
        return midi::list();
    }
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::Relaxed))?;
    if let Some(input) = options.monitor {
        return midi::monitor(&input, running);
    }

    let directory = app_directory()?;
    let settings_path = directory.join("settings.json");
    let settings: Settings = match std::fs::read_to_string(&settings_path) {
        Ok(json) => serde_json::from_str(&json)
            .with_context(|| format!("Read settings from {}", settings_path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(error) => return Err(error).context("Read settings.json"),
    };
    let destination = settings
        .osc
        .unwrap_or_else(|| ([127, 0, 0, 1], 9090).into());
    let feedback = settings
        .feedback
        .unwrap_or_else(|| ([127, 0, 0, 1], 9096).into());
    println!(
        "ap-midi-osc-bridge {} — Ctrl+C to stop",
        env!("CARGO_PKG_VERSION")
    );
    runtime::run(
        &directory.join("profiles"),
        destination,
        feedback,
        options.verbose,
        running,
    )
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            // Keep a directly launched Windows console open long enough to read an
            // error, without pausing commands run from an existing shell or pipe.
            #[cfg(windows)]
            {
                use std::io::IsTerminal;
                #[link(name = "kernel32")]
                extern "system" {
                    fn GetConsoleProcessList(processes: *mut u32, count: u32) -> u32;
                }
                let mut processes = [0u32; 2];
                let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) };
                if count == 1 && std::io::stdin().is_terminal() {
                    eprintln!("Press Enter to close.");
                    let _ = std::io::stdin().read_line(&mut String::new());
                }
            }
            std::process::ExitCode::FAILURE
        }
    }
}
