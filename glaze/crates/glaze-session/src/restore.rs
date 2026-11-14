//! Session restore: which applications were running when the session last ended.
//!
//! The settings app exposes `session.restore_apps`, but the value is only useful
//! if something actually records and replays the running set. This module is the
//! portable half of that: it persists the last running desktop ids and computes a
//! **restore plan** that contains only applications that are still installed.
//!
//! Nothing is fabricated. An id in the saved file that no longer resolves to a
//! registry entry is dropped from the plan rather than launched blindly, and an
//! empty/corrupt file yields an empty plan instead of an error at login.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use glaze_common::paths;
use glaze_service::registry::AppRegistry;

/// Schema version of the saved file.
pub const CURRENT_VERSION: u32 = 1;

/// Most applications restored by default. A session that had thirty windows
/// should not fire thirty processes before the desktop is even up
/// (`GLAZE.md` §5 P1 — restore, but stay responsive).
pub const DEFAULT_MAX_RESTORE: usize = 8;

/// Errors from reading or writing the restore file.
#[derive(Debug)]
pub enum RestoreError {
    /// The file could not be read or written.
    Io(std::io::Error),
    /// The file is not valid JSON for this schema.
    Parse(serde_json::Error),
    /// The value could not be serialised.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::Io(e) => write!(f, "session file I/O error: {e}"),
            RestoreError::Parse(e) => write!(f, "session file parse error: {e}"),
            RestoreError::Serialize(e) => write!(f, "session file serialise error: {e}"),
        }
    }
}

impl std::error::Error for RestoreError {}

impl From<std::io::Error> for RestoreError {
    fn from(e: std::io::Error) -> Self {
        RestoreError::Io(e)
    }
}

/// The application ids the last session was running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningApps {
    /// Schema version, so an older file can be migrated instead of discarded.
    pub version: u32,
    /// Desktop ids in launch order (deduplicated).
    pub app_ids: Vec<String>,
}

impl Default for RunningApps {
    fn default() -> Self {
        RunningApps {
            version: CURRENT_VERSION,
            app_ids: Vec::new(),
        }
    }
}

impl RunningApps {
    /// Build from a list of desktop ids, dropping blanks and duplicates while
    /// preserving the order they were seen in.
    pub fn new<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut app_ids: Vec<String> = Vec::new();
        for id in ids {
            let id = id.as_ref().trim();
            if id.is_empty() || app_ids.iter().any(|existing| existing == id) {
                continue;
            }
            app_ids.push(id.to_string());
        }
        RunningApps {
            version: CURRENT_VERSION,
            app_ids,
        }
    }

    /// `$XDG_STATE_HOME/glaze/session.json`.
    pub fn default_path() -> PathBuf {
        paths::state_home().join("glaze").join("session.json")
    }

    /// Parse from JSON and migrate to the current schema.
    pub fn from_json(s: &str) -> Result<Self, RestoreError> {
        let mut running: RunningApps = serde_json::from_str(s).map_err(RestoreError::Parse)?;
        running.migrate();
        Ok(running)
    }

    /// Serialise to JSON.
    pub fn to_json(&self) -> Result<String, RestoreError> {
        serde_json::to_string_pretty(self).map_err(RestoreError::Serialize)
    }

    /// Load from `path`. A missing or unreadable file yields an empty set: a
    /// broken session file must never block login.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text).unwrap_or_else(|e| {
                eprintln!(
                    "glaze-session: ignoring {} ({e}); no applications will be restored",
                    path.display()
                );
                RunningApps::default()
            }),
            Err(_) => RunningApps::default(),
        }
    }

    /// Save to `path`, creating parent directories.
    pub fn save_to(&self, path: &Path) -> Result<(), RestoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    /// Bring an older file up to the current schema.
    pub fn migrate(&mut self) {
        // v0 -> v1: nothing removed; blank/duplicate ids are still filtered by
        // the caller's `restore_plan`.
        self.version = CURRENT_VERSION;
    }

    /// True when nothing would be restored.
    pub fn is_empty(&self) -> bool {
        self.app_ids.is_empty()
    }

    /// How many ids are stored.
    pub fn len(&self) -> usize {
        self.app_ids.len()
    }
}

/// The desktop ids to actually restore.
///
/// Rules, all of them safety-driven:
/// - an id that is no longer installed is **dropped**, not launched;
/// - duplicates collapse to their first occurrence;
/// - at most `max` are returned (0 means [`DEFAULT_MAX_RESTORE`]);
/// - relative order is preserved so the taskbar order matches the last session.
pub fn restore_plan(stored: &RunningApps, available: &AppRegistry, max: usize) -> Vec<String> {
    let limit = if max == 0 { DEFAULT_MAX_RESTORE } else { max };
    let mut plan: Vec<String> = Vec::with_capacity(limit.min(stored.len()));
    for id in &stored.app_ids {
        if plan.len() >= limit {
            break;
        }
        if plan.iter().any(|existing| existing == id) {
            continue;
        }
        if available.get(id).is_some() {
            plan.push(id.clone());
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    use glaze_common::model::AppEntry;

    fn app(id: &str) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: id.into(),
            comment: None,
            generic_name: None,
            exec: format!("/usr/bin/{id} %u"),
            try_exec: None,
            icon: None,
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    fn registry() -> AppRegistry {
        AppRegistry::from_entries(vec![
            app("org.mozilla.firefox"),
            app("org.gnome.Terminal"),
            app("glaze-settings"),
        ])
    }

    fn tempdir(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("glaze-restore-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn new_dedupes_and_preserves_order() {
        let running = RunningApps::new(["b", "a", "b", "  ", "c", "a"]);
        assert_eq!(running.app_ids, vec!["b", "a", "c"]);
        assert_eq!(running.version, CURRENT_VERSION);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempdir("roundtrip");
        let path = dir.join("nested").join("session.json");
        let running = RunningApps::new(["org.mozilla.firefox", "glaze-settings"]);
        running.save_to(&path).unwrap();
        let back = RunningApps::load_from(&path);
        assert_eq!(back, running);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_or_corrupt_file_yields_an_empty_set() {
        let dir = tempdir("missing");
        let missing = dir.join("nope.json");
        assert!(RunningApps::load_from(&missing).is_empty());

        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, "{not json").unwrap();
        assert!(
            RunningApps::load_from(&corrupt).is_empty(),
            "a broken session file must not block login"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_versions_migrate_forward() {
        let text = r#"{"version": 0, "app_ids": ["org.mozilla.firefox"]}"#;
        let running = RunningApps::from_json(text).unwrap();
        assert_eq!(running.version, CURRENT_VERSION);
        assert_eq!(running.app_ids, vec!["org.mozilla.firefox"]);
    }

    #[test]
    fn the_plan_drops_applications_that_are_no_longer_installed() {
        let stored = RunningApps::new([
            "org.mozilla.firefox",
            "org.gnome.Terminal",
            "org.example.Uninstalled",
        ]);
        let plan = restore_plan(&stored, &registry(), DEFAULT_MAX_RESTORE);
        assert_eq!(plan, vec!["org.mozilla.firefox", "org.gnome.Terminal"]);
    }

    #[test]
    fn the_plan_respects_the_cap_and_order() {
        let stored =
            RunningApps::new(["glaze-settings", "org.gnome.Terminal", "org.mozilla.firefox"]);
        let plan = restore_plan(&stored, &registry(), 2);
        assert_eq!(plan, vec!["glaze-settings", "org.gnome.Terminal"]);

        // Zero means "use the default", not "restore nothing".
        let all = restore_plan(&stored, &registry(), 0);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn an_empty_store_plans_nothing() {
        assert!(restore_plan(&RunningApps::default(), &registry(), 0).is_empty());
    }
}
