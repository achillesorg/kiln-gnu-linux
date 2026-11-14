//! `glaze-cache` — an on-disk cache for the discovered application registry so
//! the shell and launcher start instantly, with correct invalidation.
//!
//! The cache is only ever trusted when the [`Signature`] captured at discovery
//! time still matches the current state of the XDG application directories. If
//! the cache is missing, corrupt, or written by an unknown format version, the
//! caller transparently falls back to a real discovery — this crate never
//! panics and never invents registry entries.
//!
//! ```no_run
//! use std::path::PathBuf;
//! use glaze_cache::{load_or_discover, CacheOutcome};
//! use glaze_service::registry::DiscoverOptions;
//!
//! let dirs: Vec<PathBuf> = vec![PathBuf::from("/usr/share")];
//! let cache = glaze_cache::default_path();
//! let (registry, outcome) = load_or_discover(&dirs, DiscoverOptions::default(), &cache);
//! match outcome {
//!     CacheOutcome::Hit => println!("loaded {} apps from cache", registry.len()),
//!     CacheOutcome::Miss => println!("discovered {} apps", registry.len()),
//! }
//! ```

pub mod signature;
pub mod store;

#[cfg(test)]
mod testing;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use glaze_common::paths;
use glaze_service::registry::{AppRegistry, DiscoverOptions};

pub use signature::{DirSignature, Signature};
pub use store::{CacheError, CachedRegistry, CACHE_VERSION};

/// Whether a call was served from the cache or required a fresh discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOutcome {
    /// The cache was current and was used.
    Hit,
    /// The cache was missing/stale/corrupt; the registry was re-discovered and
    /// the cache rewritten.
    Miss,
}

/// Glaze's default cache location: `$XDG_CACHE_HOME/glaze/apps-cache.json`.
pub fn default_path() -> PathBuf {
    paths::cache_home().join("glaze").join("apps-cache.json")
}

/// Return the application registry, using `cache_path` when it is still valid.
///
/// On a cache hit the cached application list is returned and `cache_path` is
/// left untouched. On a miss the registry is discovered from `dirs` with `opts`,
/// the result is written back to `cache_path` (best effort — a write failure is
/// never fatal), and the freshly discovered registry is returned.
///
/// A cache is considered a miss when it is absent, cannot be parsed, is written
/// by an unknown format version, or its signature no longer matches `dirs`.
pub fn load_or_discover(
    dirs: &[PathBuf],
    opts: DiscoverOptions,
    cache_path: &Path,
) -> (AppRegistry, CacheOutcome) {
    if let Ok(mut cached) = CachedRegistry::load_from(cache_path) {
        if cached.migrate() && cached.signature.is_valid_for(dirs) {
            return (AppRegistry::from_entries(cached.apps), CacheOutcome::Hit);
        }
    }

    let registry = AppRegistry::discover_in(dirs, opts);
    let cache = CachedRegistry::new(signature::signature(dirs), registry.apps().to_vec());
    // Caching is an optimisation; never let it break discovery.
    let _ = cache.save_to(cache_path);
    (registry, CacheOutcome::Miss)
}

/// Delete the cache file. Missing files are not an error, so this is idempotent.
pub fn invalidate(cache_path: &Path) -> io::Result<()> {
    match fs::remove_file(cache_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::signature;
    use crate::testing::{cleanup, temp_dir, write};
    use glaze_common::model::AppEntry;
    use std::time::Duration;

    fn opts() -> DiscoverOptions {
        DiscoverOptions {
            include_no_display: false,
            check_try_exec: false,
            current_desktops: vec!["Glaze".to_string()],
            locale: "en".to_string(),
        }
    }

    fn fixture(tag: &str) -> PathBuf {
        let root = temp_dir(tag);
        write(
            &root,
            "applications/browser.desktop",
            "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser %u\n",
        );
        write(
            &root,
            "applications/text/editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\n",
        );
        root
    }

    fn sample_app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            comment: None,
            generic_name: None,
            exec: id.to_string(),
            try_exec: None,
            icon: None,
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/apps/{id}.desktop"),
        }
    }

    #[test]
    fn cold_miss_writes_cache_and_matches_discovery() {
        let root = fixture("cache-cold");
        let dirs = vec![root.clone()];
        let cache_path = root.join("cache/apps-cache.json");

        let (registry, outcome) = load_or_discover(&dirs, opts(), &cache_path);

        assert_eq!(outcome, CacheOutcome::Miss);
        assert!(cache_path.is_file(), "cache file should have been written");
        assert_eq!(
            registry.apps(),
            AppRegistry::discover_in(&dirs, opts()).apps()
        );

        cleanup(&root);
    }

    #[test]
    fn warm_hit_returns_identical_apps() {
        let root = fixture("cache-warm");
        let dirs = vec![root.clone()];
        let cache_path = root.join("cache/apps-cache.json");

        let (first, first_outcome) = load_or_discover(&dirs, opts(), &cache_path);
        assert_eq!(first_outcome, CacheOutcome::Miss);

        let (second, second_outcome) = load_or_discover(&dirs, opts(), &cache_path);

        assert_eq!(second_outcome, CacheOutcome::Hit);
        assert_eq!(
            first.apps(),
            second.apps(),
            "warm hit must match the cold run"
        );
        assert_eq!(
            second.apps(),
            AppRegistry::discover_in(&dirs, opts()).apps()
        );

        cleanup(&root);
    }

    #[test]
    fn changing_mtime_triggers_a_miss() {
        let root = fixture("cache-mtime");
        let dirs = vec![root.clone()];
        let cache_path = root.join("cache/apps-cache.json");

        let (_, first_outcome) = load_or_discover(&dirs, opts(), &cache_path);
        assert_eq!(first_outcome, CacheOutcome::Miss);
        let (_, warm) = load_or_discover(&dirs, opts(), &cache_path);
        assert_eq!(warm, CacheOutcome::Hit);

        // Rewrite a fixture with different content (same file, new mtime/size).
        std::thread::sleep(Duration::from_millis(25));
        write(
            &root,
            "applications/browser.desktop",
            "[Desktop Entry]\nType=Application\nName=Browser\nExec=surf %u\n",
        );

        let (registry, stale_outcome) = load_or_discover(&dirs, opts(), &cache_path);
        assert_eq!(stale_outcome, CacheOutcome::Miss);
        assert_eq!(
            registry.apps(),
            AppRegistry::discover_in(&dirs, opts()).apps()
        );

        cleanup(&root);
    }

    #[test]
    fn corrupt_cache_falls_back_to_discovery() {
        let root = fixture("cache-corrupt");
        let dirs = vec![root.clone()];
        let cache_path = root.join("cache/apps-cache.json");
        write(&root, "cache/apps-cache.json", "}{ not a cache");

        let (registry, outcome) = load_or_discover(&dirs, opts(), &cache_path);

        assert_eq!(outcome, CacheOutcome::Miss);
        assert_eq!(
            registry.apps(),
            AppRegistry::discover_in(&dirs, opts()).apps()
        );
        // The corrupt document must have been replaced with a valid one.
        assert!(CachedRegistry::load_from(&cache_path).is_ok());

        cleanup(&root);
    }

    #[test]
    fn unknown_version_falls_back_to_discovery() {
        let root = fixture("cache-version");
        let dirs = vec![root.clone()];
        let cache_path = root.join("cache/apps-cache.json");

        // A well-formed document from a newer, unknown format.
        let doc = CachedRegistry {
            version: CACHE_VERSION + 1,
            signature: signature(&dirs),
            apps: vec![sample_app("ghost", "Ghost")],
        };
        doc.save_to(&cache_path).unwrap();

        let (registry, outcome) = load_or_discover(&dirs, opts(), &cache_path);

        assert_eq!(outcome, CacheOutcome::Miss);
        assert!(
            registry.get("ghost").is_none(),
            "apps from an unknown format must not be served"
        );
        assert_eq!(
            registry.apps(),
            AppRegistry::discover_in(&dirs, opts()).apps()
        );

        cleanup(&root);
    }

    #[test]
    fn invalidate_removes_the_cache() {
        let root = fixture("cache-invalidate");
        let dirs = vec![root.clone()];
        let cache_path = root.join("cache/apps-cache.json");

        let (_, _) = load_or_discover(&dirs, opts(), &cache_path);
        assert!(cache_path.is_file());

        invalidate(&cache_path).unwrap();
        assert!(!cache_path.exists());

        // Idempotent: invalidating again is not an error.
        invalidate(&cache_path).unwrap();

        // And the next call is a miss that rebuilds the cache.
        let (_, outcome) = load_or_discover(&dirs, opts(), &cache_path);
        assert_eq!(outcome, CacheOutcome::Miss);
        assert!(cache_path.is_file());

        cleanup(&root);
    }

    #[test]
    fn default_path_lives_under_glaze_cache() {
        let p = default_path();
        assert!(p.ends_with("glaze/apps-cache.json") || p.ends_with("glaze\\apps-cache.json"));
    }
}
