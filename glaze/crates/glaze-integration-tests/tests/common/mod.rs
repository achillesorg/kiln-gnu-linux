//! Shared helpers for the Glaze cross-crate integration tests.
//!
//! Lives in `tests/common/mod.rs` (a module, not a test target) so the test
//! binaries that `include!` it share one fixture writer. Only `std` and the
//! crate's dev-dependencies are used — no extra crates are pulled in.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use glaze_service::DiscoverOptions;

/// Create a unique temp root for a test, clearing any stale leftovers first.
///
/// The tag must be unique per test (the tests inside one binary run in parallel
/// threads that share a process id); callers combine it with the pid so files
/// from a previous run are never reused.
pub fn temp_root(tag: &str) -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!("glaze-it-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temp root");
    root
}

/// Write a `.desktop` fixture at `<root>/applications/<rel>`, returning the path.
pub fn write_desktop(root: &Path, rel: &str, body: &str) -> PathBuf {
    let path = root.join("applications").join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture dir");
    }
    fs::write(&path, body).expect("write fixture");
    path
}

/// Best-effort removal of a temp root (never fails the test on cleanup).
pub fn cleanup(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

/// Discovery options matching a deterministic test environment (no `TryExec`
/// probing of the host, `Glaze` as the only current desktop).
pub fn options(locale: &str) -> DiscoverOptions {
    DiscoverOptions {
        include_no_display: false,
        check_try_exec: false,
        current_desktops: vec!["Glaze".to_string()],
        locale: locale.to_string(),
    }
}

/// As [`options`] but also keeping `NoDisplay=true` entries.
pub fn options_with_no_display(locale: &str) -> DiscoverOptions {
    DiscoverOptions {
        include_no_display: true,
        ..options(locale)
    }
}
