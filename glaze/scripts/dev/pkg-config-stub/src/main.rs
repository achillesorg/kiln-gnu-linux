//! A minimal `pkg-config` stand-in.
//!
//! Purpose: let `cargo check` / `cargo clippy` **type-check** the GTK4 apps
//! (`apps/glaze-shell`, `apps/glaze-settings`) on a host where GTK4 is not
//! installed. Since `check` and `clippy` do not link, the `gtk4-sys`/`gio-sys`
//! etc. build scripts only need their `pkg-config` probes to answer; the flags
//! they return are never used.
//!
//! This is a verification aid, not a runtime component. On Fedora you build the
//! apps normally against real GTK4.

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `pkg-config --version` -> our own version.
    if args.iter().any(|a| a == "--version") && !args.iter().any(|a| a == "--modversion") {
        println!("1.9.5");
        exit(0);
    }
    // `pkg-config --modversion <pkg>` -> high enough to satisfy any `>= x.y`.
    if args.iter().any(|a| a == "--modversion") {
        println!("999.0.0");
        exit(0);
    }
    // `pkg-config --variable=<x> <pkg>` -> any plausible value.
    if args.iter().any(|a| a.starts_with("--variable=")) {
        println!("/usr");
        exit(0);
    }
    // --exists / --atleast-version / --print-errors / --cflags / --libs:
    // succeed quietly.
    exit(0);
}
