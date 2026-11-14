//! `glaze-settings`: the Glaze settings application (Kiln GNU/Linux).
//!
//! A Windows 2000 Control-Panel structure in the flat cut-paper Glaze style:
//! a sidebar of categories with a search box that finds settings on every
//! page, paper cards of rows bound to the live configuration, and header
//! actions for undo, reset, theme import and theme export.
//!
//! Module map:
//! - portable core (unit-tested on any host): [`store`], [`catalog`],
//!   [`search`], [`validate`], [`keys`], [`themeio`], [`sysinfo`],
//!   [`desktop_apps`], [`assets`], [`cli`];
//! - GTK user interface (Linux only): `ui`.

// The GTK user interface is Linux-only, so on other hosts some core helpers
// it uses are unused.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

mod assets;
mod catalog;
mod cli;
mod colourscheme;
mod desktop_apps;
mod keys;
mod search;
mod store;
mod style;
mod swaywin;
mod sysinfo;
mod themeio;
mod validate;

#[cfg(target_os = "linux")]
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    let opts = match cli::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(message) => {
            eprintln!("glaze-settings: {message}\n\n{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };
    match opts.command {
        cli::Command::Help => {
            print!("{}", cli::USAGE);
            ExitCode::SUCCESS
        }
        cli::Command::Version => {
            println!(
                "glaze-settings {} (Glaze {})",
                env!("CARGO_PKG_VERSION"),
                glaze_common::VERSION
            );
            ExitCode::SUCCESS
        }
        cli::Command::Run => run_gui(opts),
        _ => cli::run_headless(&opts),
    }
}

#[cfg(target_os = "linux")]
fn run_gui(opts: cli::Options) -> ExitCode {
    ui::run(opts)
}

#[cfg(not(target_os = "linux"))]
fn run_gui(_opts: cli::Options) -> ExitCode {
    eprintln!(
        "glaze-settings: the window needs GTK4 on Linux; headless commands work here (see --help)"
    );
    ExitCode::FAILURE
}
