//! Deterministic cache signature over the XDG application directories.
//!
//! A [`Signature`] captures exactly enough of the filesystem to decide whether
//! an on-disk registry cache is still trustworthy: for every
//! `<data_dir>/applications` directory it records the directory's own
//! modification time plus a stable hash of the sorted `*.desktop` inventory
//! (relative path, mtime and size of each file). No file *contents* are read, so
//! computing a signature stays cheap even on systems with many entries.
//!
//! Everything degrades to a stable value: a directory that does not exist is
//! recorded as "absent" and remains valid until it appears, and a
//! pre-epoch/unavailable timestamp is guarded rather than panicking.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// FNV-1a 64-bit. Chosen over `std::collections::hash_map::DefaultHasher`
/// because it is deterministic across runs, platforms and Rust versions — a
/// cache signature must not change merely because the toolchain did.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(FNV_OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    /// Length-prefixed write so concatenated fields cannot collide
    /// (`ab` + `c` must not hash the same as `a` + `bc`).
    fn write_field(&mut self, bytes: &[u8]) {
        self.write(&(bytes.len() as u64).to_le_bytes());
        self.write(bytes);
    }

    fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// What was observed for one `<data_dir>/applications` directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirSignature {
    /// The `<data_dir>/applications` directory that was inspected.
    pub applications_dir: PathBuf,
    /// Directory mtime in nanoseconds since the UNIX epoch. `None` means the
    /// directory is absent (a stable "not there" state) or its timestamp could
    /// not be read.
    pub mtime_nanos: Option<i64>,
    /// Stable hash of the sorted desktop-file inventory.
    pub files_hash: u64,
    /// Number of `.desktop` files seen (kept for diagnostics and tests).
    pub file_count: usize,
}

/// A signature over the whole ordered set of application directories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// One entry per applications directory, sorted by path so the result does
    /// not depend on the order the caller handed the directories over.
    pub dirs: Vec<DirSignature>,
}

impl Signature {
    /// True when `dirs` still describes the same state this signature was
    /// computed from. An unchanged tree — including a directory that stays
    /// absent — remains valid.
    pub fn is_valid_for(&self, dirs: &[PathBuf]) -> bool {
        *self == signature(dirs)
    }
}

/// Compute a [`Signature`] for the given XDG data directories.
pub fn signature(dirs: &[PathBuf]) -> Signature {
    let mut out: Vec<DirSignature> = dirs.iter().map(|d| dir_signature(d)).collect();
    out.sort_by(|a, b| a.applications_dir.cmp(&b.applications_dir));
    // Collapse a directory listed more than once so ordering quirks in the
    // caller's list cannot invalidate an otherwise identical cache.
    out.dedup();
    Signature { dirs: out }
}

fn dir_signature(data_dir: &Path) -> DirSignature {
    let apps = data_dir.join("applications");
    let dir_mtime = std::fs::metadata(&apps).ok().and_then(|m| mtime_nanos(&m));

    let mut hasher = Fnv::new();
    let mut files = walk_desktop_files(&apps);
    files.sort();
    let count = files.len();
    hasher.write_u64(count as u64);

    for path in &files {
        let rel = path.strip_prefix(&apps).unwrap_or(path);
        // Normalise separators so a signature is stable across platforms.
        let rel = rel.to_string_lossy().replace('\\', "/");
        hasher.write_field(rel.as_bytes());
        match std::fs::metadata(path) {
            Ok(meta) => {
                write_time(&mut hasher, mtime_nanos(&meta));
                hasher.write_u64(meta.len());
            }
            Err(_) => {
                // Unreadable file: encode a distinct, deterministic state.
                write_time(&mut hasher, None);
                hasher.write_u64(u64::MAX);
            }
        }
    }

    DirSignature {
        applications_dir: apps,
        mtime_nanos: dir_mtime,
        files_hash: hasher.finish(),
        file_count: count,
    }
}

/// Encode an optional timestamp unambiguously.
fn write_time(hasher: &mut Fnv, t: Option<i64>) {
    match t {
        Some(n) => {
            hasher.write_u64(1);
            hasher.write_u64(n as u64);
        }
        None => hasher.write_u64(0),
    }
}

/// Modification time as signed nanoseconds since the UNIX epoch.
///
/// Pre-epoch times (or a platform that cannot report them) are guarded so the
/// signature never panics and stays deterministic.
fn mtime_nanos(meta: &std::fs::Metadata) -> Option<i64> {
    let modified: SystemTime = meta.modified().ok()?;
    match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => Some(d.as_nanos().min(i64::MAX as u128) as i64),
        Err(before) => Some(-(before.duration().as_nanos().min(i64::MAX as u128) as i64)),
    }
}

/// Recursively collect `*.desktop` files under `dir` (sorted).
fn walk_desktop_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                stack.push(path);
            } else if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{cleanup, temp_dir, write};
    use std::time::Duration;

    #[test]
    fn unchanged_directories_stay_valid() {
        let root = temp_dir("sig-stable");
        write(&root, "applications/a.desktop", "[Desktop Entry]\nName=A\n");
        write(
            &root,
            "applications/sub/b.desktop",
            "[Desktop Entry]\nName=B\n",
        );
        let dirs = vec![root.clone()];

        let sig = signature(&dirs);
        assert!(sig.is_valid_for(&dirs));
        // Recomputing with no filesystem change must reproduce the same value.
        assert_eq!(sig, signature(&dirs));
        assert_eq!(sig.dirs[0].file_count, 2);

        cleanup(&root);
    }

    #[test]
    fn adding_a_desktop_file_invalidates() {
        let root = temp_dir("sig-add");
        write(&root, "applications/a.desktop", "[Desktop Entry]\nName=A\n");
        let dirs = vec![root.clone()];
        let sig = signature(&dirs);

        write(
            &root,
            "applications/new.desktop",
            "[Desktop Entry]\nName=N\n",
        );
        assert!(!sig.is_valid_for(&dirs));

        cleanup(&root);
    }

    #[test]
    fn removing_a_desktop_file_invalidates() {
        let root = temp_dir("sig-remove");
        let a = write(&root, "applications/a.desktop", "[Desktop Entry]\nName=A\n");
        write(&root, "applications/b.desktop", "[Desktop Entry]\nName=B\n");
        let dirs = vec![root.clone()];
        let sig = signature(&dirs);

        std::fs::remove_file(&a).unwrap();
        assert!(!sig.is_valid_for(&dirs));

        cleanup(&root);
    }

    #[test]
    fn touching_a_desktop_file_invalidates() {
        let root = temp_dir("sig-touch");
        // Same byte length before and after, so only the mtime can change.
        let file = write(
            &root,
            "applications/a.desktop",
            "[Desktop Entry]\nName=AAAA\n",
        );
        let dirs = vec![root.clone()];
        let sig = signature(&dirs);

        // A short pause guarantees an observable mtime advance even on
        // filesystems with coarse timestamp resolution.
        std::thread::sleep(Duration::from_millis(25));
        std::fs::write(&file, "[Desktop Entry]\nName=BBBB\n").unwrap();

        assert!(!sig.is_valid_for(&dirs));

        cleanup(&root);
    }

    #[test]
    fn missing_directory_is_stable_until_it_appears() {
        let root = temp_dir("sig-missing");
        let absent = root.join("definitely-not-there");
        let dirs = vec![absent.clone()];

        let sig = signature(&dirs);
        assert!(sig.dirs[0].mtime_nanos.is_none());
        // Still absent => still valid (must not spuriously invalidate).
        assert!(sig.is_valid_for(&dirs));

        // Creating the directory is a real change, so it now invalidates.
        std::fs::create_dir_all(absent.join("applications")).unwrap();
        assert!(!sig.is_valid_for(&dirs));

        cleanup(&root);
    }

    #[test]
    fn order_of_directories_does_not_matter() {
        let a = temp_dir("sig-order-a");
        let b = temp_dir("sig-order-b");
        write(&a, "applications/a.desktop", "[Desktop Entry]\nName=A\n");
        write(&b, "applications/b.desktop", "[Desktop Entry]\nName=B\n");

        assert_eq!(
            signature(&[a.clone(), b.clone()]),
            signature(&[b.clone(), a.clone()])
        );

        cleanup(&a);
        cleanup(&b);
    }
}
