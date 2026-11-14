//! The on-disk cache document: a versioned, JSON-serialised snapshot of the
//! discovered registry plus the signature that proves it is current.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use glaze_common::model::AppEntry;
use serde::{Deserialize, Serialize};

use crate::signature::Signature;

/// On-disk cache format version. Bump this whenever the JSON layout changes so
/// [`CachedRegistry::migrate`] can upgrade or reject older documents.
pub const CACHE_VERSION: u32 = 1;

/// A discovered registry together with the state it was discovered from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedRegistry {
    /// Format version of this document.
    pub version: u32,
    /// Signature of the application directories at discovery time.
    pub signature: Signature,
    /// The discovered applications.
    pub apps: Vec<AppEntry>,
}

/// Why reading a cache failed. Never fatal to callers: a bad cache simply means
/// "re-discover".
#[derive(Debug)]
pub enum CacheError {
    /// The file could not be read (missing, permission, I/O).
    Io(io::Error),
    /// The file exists but is not a valid cache document.
    Corrupt(serde_json::Error),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::Io(e) => write!(f, "cache I/O error: {e}"),
            CacheError::Corrupt(e) => write!(f, "cache is corrupt: {e}"),
        }
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CacheError::Io(e) => Some(e),
            CacheError::Corrupt(e) => Some(e),
        }
    }
}

impl From<io::Error> for CacheError {
    fn from(e: io::Error) -> Self {
        CacheError::Io(e)
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(e: serde_json::Error) -> Self {
        CacheError::Corrupt(e)
    }
}

impl CachedRegistry {
    /// Build a current-version cache document.
    pub fn new(signature: Signature, apps: Vec<AppEntry>) -> Self {
        CachedRegistry {
            version: CACHE_VERSION,
            signature,
            apps,
        }
    }

    /// Bring a loaded cache up to the current [`CACHE_VERSION`] in place.
    ///
    /// Returns `true` when the document is usable afterwards and `false` when
    /// the stored version is unknown, so the caller must re-discover. Older
    /// formats are upgraded here as they are introduced: rewrite the stored
    /// fields, set `self.version = CACHE_VERSION`, and return `true`.
    pub fn migrate(&mut self) -> bool {
        if self.version == CACHE_VERSION {
            return true;
        }
        // Older formats are upgraded here as they are introduced, e.g. by
        // rewriting fields and setting `self.version = CACHE_VERSION`.
        false
    }

    /// Write the cache to an explicit path, creating parent directories.
    ///
    /// The write goes through a temporary sibling and a rename so a crash can
    /// never leave a half-written file behind.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let data = serde_json::to_vec_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let tmp = tmp_sibling(path);
        fs::write(&tmp, data)?;
        match fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(first_err) => {
                // Some platforms refuse to atomically replace; retry once after
                // unlinking the destination before giving up.
                let _ = fs::remove_file(path);
                if fs::rename(&tmp, path).is_ok() {
                    Ok(())
                } else {
                    let _ = fs::remove_file(&tmp);
                    Err(first_err)
                }
            }
        }
    }

    /// Alias for [`CachedRegistry::save_to`].
    pub fn save(&self, path: &Path) -> io::Result<()> {
        self.save_to(path)
    }

    /// Read a cache from an explicit path.
    pub fn load_from(path: &Path) -> Result<Self, CacheError> {
        let bytes = fs::read(path)?;
        let cache: CachedRegistry = serde_json::from_slice(&bytes)?;
        Ok(cache)
    }

    /// Alias for [`CachedRegistry::load_from`].
    pub fn load(path: &Path) -> Result<Self, CacheError> {
        Self::load_from(path)
    }
}

/// A unique temporary path next to `path` (same directory => atomic rename).
fn tmp_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".tmp{}", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::signature;
    use crate::testing::{cleanup, temp_dir, write};

    fn sample_app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            comment: Some("A test application".to_string()),
            generic_name: Some("Test".to_string()),
            exec: format!("{id} %u"),
            try_exec: None,
            icon: Some("test-icon".to_string()),
            categories: vec!["Utility".to_string()],
            keywords: vec!["test".to_string(), "sample".to_string()],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/apps/{id}.desktop"),
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let root = temp_dir("store-roundtrip");
        let path = root.join("nested/apps-cache.json");
        let cache = CachedRegistry::new(
            signature(&[]),
            vec![sample_app("alpha", "Alpha"), sample_app("beta", "Beta")],
        );

        cache.save_to(&path).unwrap();
        assert!(path.is_file());

        let loaded = CachedRegistry::load_from(&path).unwrap();
        assert_eq!(loaded, cache);
        assert_eq!(loaded.version, CACHE_VERSION);

        cleanup(&root);
    }

    #[test]
    fn save_and_load_aliases_match() {
        let root = temp_dir("store-alias");
        let path = root.join("apps-cache.json");
        let cache = CachedRegistry::new(signature(&[]), vec![sample_app("x", "X")]);

        cache.save(&path).unwrap();
        assert_eq!(CachedRegistry::load(&path).unwrap(), cache);

        cleanup(&root);
    }

    #[test]
    fn save_overwrites_an_existing_file() {
        let root = temp_dir("store-overwrite");
        let path = root.join("apps-cache.json");

        let first = CachedRegistry::new(signature(&[]), vec![sample_app("x", "X")]);
        first.save_to(&path).unwrap();

        let second = CachedRegistry::new(signature(&[]), vec![sample_app("y", "Y")]);
        second.save_to(&path).unwrap();

        assert_eq!(CachedRegistry::load_from(&path).unwrap(), second);

        cleanup(&root);
    }

    #[test]
    fn corrupt_json_is_reported_not_panicked() {
        let root = temp_dir("store-corrupt");
        let path = write(&root, "apps-cache.json", "{ this is not json");
        assert!(matches!(
            CachedRegistry::load_from(&path),
            Err(CacheError::Corrupt(_))
        ));
        cleanup(&root);
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let root = temp_dir("store-missing");
        let path = root.join("does-not-exist.json");
        assert!(matches!(
            CachedRegistry::load_from(&path),
            Err(CacheError::Io(_))
        ));
        cleanup(&root);
    }

    #[test]
    fn migrate_accepts_current_and_rejects_unknown_versions() {
        let mut fresh = CachedRegistry::new(signature(&[]), vec![]);
        assert!(fresh.migrate());

        fresh.version = CACHE_VERSION + 99;
        assert!(!fresh.migrate());
    }
}
