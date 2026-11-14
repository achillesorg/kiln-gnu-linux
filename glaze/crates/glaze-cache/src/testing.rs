//! Small filesystem helpers shared by the `glaze-cache` unit tests.
//!
//! Every path is confined to `<temp>/glaze-cache-tests-*`; the real user cache
//! directory (`$XDG_CACHE_HOME/glaze`) is never touched by a test.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a unique, empty temporary directory for a test.
pub fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "glaze-cache-tests-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Write `contents` to `dir/rel`, creating parent directories. Returns the path.
pub fn write(dir: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, contents).unwrap();
    path
}

/// Best-effort recursive cleanup; tests must not leave temp trees behind.
pub fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}
