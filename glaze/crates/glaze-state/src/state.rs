//! Persistent launcher state: recently used applications, launch counts and a
//! versioned on-disk schema.
//!
//! The state file is *not* configuration. It lives under
//! `$XDG_STATE_HOME/glaze/state.toml` (see [`glaze_common::paths::state_file`])
//! and holds data the desktop derives from real usage: which applications were
//! launched, when, and how often. Losing it only loses history, never user
//! preferences, so every load path degrades to a default rather than failing
//! hard.
//!
//! All time handling is an explicit `now_unix` parameter: the tested functions
//! never read the system clock, so ordering is fully deterministic under test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use glaze_common::paths::state_file;

/// Schema version written by this build. Bump when the layout changes and add a
/// matching step in [`LauncherState::migrate`].
pub const CURRENT_VERSION: u32 = 1;

/// One recently-launched application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    /// Stable desktop-file id (see [`glaze_common::model::AppEntry::id`]).
    pub app_id: String,
    /// Unix timestamp (seconds) of the most recent launch.
    pub last_used_unix: u64,
    /// Number of times this application has been launched.
    pub count: u32,
}

/// Everything the launcher remembers between sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherState {
    /// On-disk schema version; see [`LauncherState::migrate`].
    pub version: u32,
    /// Recently used applications (unordered; use [`LauncherState::recent_ids`]).
    pub recent: Vec<RecentEntry>,
    /// Lifetime launch counter per desktop id.
    pub launch_counts: BTreeMap<String, u32>,
}

impl Default for LauncherState {
    fn default() -> Self {
        LauncherState {
            version: CURRENT_VERSION,
            recent: Vec::new(),
            launch_counts: BTreeMap::new(),
        }
    }
}

/// Errors that can occur while reading or writing the state file.
#[derive(Debug)]
pub enum StateError {
    /// Filesystem failure (read, write, create directory).
    Io(std::io::Error),
    /// The file exists but could not be parsed as the expected format.
    Parse(String),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::Io(e) => write!(f, "launcher state I/O error: {e}"),
            StateError::Parse(m) => write!(f, "launcher state parse error: {m}"),
        }
    }
}

impl std::error::Error for StateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StateError::Io(e) => Some(e),
            StateError::Parse(_) => None,
        }
    }
}

impl From<std::io::Error> for StateError {
    fn from(e: std::io::Error) -> Self {
        StateError::Io(e)
    }
}

impl LauncherState {
    /// Load from the canonical location, falling back to [`Default`] when the
    /// file is missing or unreadable. Convenience for the shell; tests should
    /// use [`LauncherState::load_from`] with an explicit path.
    pub fn load() -> LauncherState {
        Self::load_from(&state_file()).unwrap_or_default()
    }

    /// Load from an explicit path. A missing file is **not** an error: it
    /// returns the default state so first run is transparent. A present but
    /// corrupt file is also tolerated (returns default) so a bad write can never
    /// brick the launcher.
    pub fn load_from(path: &Path) -> Result<LauncherState, StateError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LauncherState::default())
            }
            Err(e) => return Err(StateError::Io(e)),
        };
        Self::from_toml_str(&text)
    }

    /// Parse a state document, applying schema migration. Exposed so the shell
    /// and tests can round-trip without touching the filesystem.
    pub fn from_toml_str(text: &str) -> Result<LauncherState, StateError> {
        let mut state: LauncherState =
            toml::from_str(text).map_err(|e| StateError::Parse(e.to_string()))?;
        state.migrate();
        Ok(state)
    }

    /// Serialise to TOML.
    pub fn to_toml(&self) -> String {
        // Serialising a plain data structure of scalars/Vec/BTreeMap cannot fail.
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Persist to the canonical location, creating parent directories.
    pub fn save(&self) -> Result<(), StateError> {
        self.save_to(&state_file())
    }

    /// Persist to an explicit path, creating parent directories.
    pub fn save_to(&self, path: &Path) -> Result<(), StateError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml())?;
        Ok(())
    }

    /// The state file path this build uses, for diagnostics/UI.
    pub fn path() -> PathBuf {
        state_file()
    }

    /// Record a launch of `app_id` at `now_unix`: bump its total count, refresh
    /// its recent entry (or create one), and keep `launch_counts` in sync.
    pub fn record_launch(&mut self, app_id: &str, now_unix: u64) {
        *self.launch_counts.entry(app_id.to_string()).or_insert(0) += 1;
        match self.recent.iter_mut().find(|r| r.app_id == app_id) {
            Some(entry) => {
                entry.last_used_unix = now_unix;
                entry.count = entry.count.saturating_add(1);
            }
            None => self.recent.push(RecentEntry {
                app_id: app_id.to_string(),
                last_used_unix: now_unix,
                count: 1,
            }),
        }
    }

    /// Recently-used ids, most-recent first, capped at `limit`. Ties on
    /// `last_used_unix` break by higher count, then alphabetically so the order
    /// is always deterministic.
    pub fn recent_ids(&self, limit: usize) -> Vec<String> {
        let mut entries: Vec<&RecentEntry> = self.recent.iter().collect();
        entries.sort_by(|a, b| {
            b.last_used_unix
                .cmp(&a.last_used_unix)
                .then(b.count.cmp(&a.count))
                .then(a.app_id.cmp(&b.app_id))
        });
        entries
            .into_iter()
            .take(limit)
            .map(|e| e.app_id.clone())
            .collect()
    }

    /// Ids ordered by lifetime launch count, highest first, capped at `limit`.
    /// Ties break by recency, then alphabetically.
    pub fn most_used(&self, limit: usize) -> Vec<String> {
        let mut counts: Vec<(&String, &u32)> = self.launch_counts.iter().collect();
        let last_used = |id: &str| {
            self.recent
                .iter()
                .find(|r| r.app_id == id)
                .map(|r| r.last_used_unix)
                .unwrap_or(0)
        };
        counts.sort_by(|a, b| {
            b.1.cmp(a.1)
                .then(last_used(b.0).cmp(&last_used(a.0)))
                .then(a.0.cmp(b.0))
        });
        counts
            .into_iter()
            .take(limit)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Forget everything about `app_id` (recent entry and lifetime count).
    /// Returns `true` if anything was removed.
    pub fn forget(&mut self, app_id: &str) -> bool {
        let before = self.recent.len();
        self.recent.retain(|r| r.app_id != app_id);
        let removed_count = self.launch_counts.remove(app_id).is_some();
        self.recent.len() != before || removed_count
    }

    /// Cap the recent list at `max_entries`, keeping the most recently used.
    /// Lifetime counts are intentionally kept: pruning history should not erase
    /// how often an app has been used.
    pub fn prune(&mut self, max_entries: usize) {
        if self.recent.len() <= max_entries {
            return;
        }
        let keep: Vec<String> = self.recent_ids(max_entries);
        self.recent.retain(|r| keep.contains(&r.app_id));
    }

    /// Bring an older on-disk document up to [`CURRENT_VERSION`]. Unknown or
    /// future versions are left untouched so a newer shell can still read them.
    pub fn migrate(&mut self) {
        // v0 = files written before the `version` field existed; the rest of the
        // layout is identical, so upgrading is just stamping the version.
        if self.version == 0 {
            self.version = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Unique, self-cleaning temp directory (no extra dev-dependency needed).
    fn temp_path(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "glaze-state-test-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        std::fs::create_dir_all(&p).unwrap();
        p.join("state.toml")
    }

    fn sample() -> LauncherState {
        let mut s = LauncherState::default();
        s.record_launch("firefox", 1_000);
        s.record_launch("firefox", 2_000);
        s.record_launch("kate", 3_000);
        s
    }

    #[test]
    fn missing_file_loads_default() {
        let path = temp_path("missing");
        let state = LauncherState::load_from(&path).unwrap();
        assert_eq!(state, LauncherState::default());
        assert_eq!(state.version, CURRENT_VERSION);
        assert!(state.recent.is_empty());
    }

    #[test]
    fn persistence_round_trip() {
        let path = temp_path("roundtrip");
        let state = sample();
        state.save_to(&path).unwrap();
        assert!(path.exists(), "save_to must create the file");

        let loaded = LauncherState::load_from(&path).unwrap();
        assert_eq!(loaded, state);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn toml_contains_version() {
        let text = sample().to_toml();
        assert!(text.contains("version = 1"), "got:\n{text}");
    }

    #[test]
    fn record_launch_updates_count_and_timestamp() {
        let mut s = LauncherState::default();
        s.record_launch("app", 100);
        s.record_launch("app", 500);
        let entry = s.recent.iter().find(|r| r.app_id == "app").unwrap();
        assert_eq!(entry.count, 2);
        assert_eq!(entry.last_used_unix, 500);
        assert_eq!(s.launch_counts["app"], 2);
        // Exactly one recent entry, not duplicated.
        assert_eq!(s.recent.len(), 1);
    }

    #[test]
    fn record_launch_reorders_recent() {
        let mut s = LauncherState::default();
        s.record_launch("a", 10);
        s.record_launch("b", 20);
        assert_eq!(s.recent_ids(10), vec!["b", "a"]);
        // Re-launching `a` makes it the most recent again.
        s.record_launch("a", 30);
        assert_eq!(s.recent_ids(10), vec!["a", "b"]);
    }

    #[test]
    fn recent_ids_respects_limit_and_is_deterministic() {
        // Two entries with identical timestamps and counts break ties by id.
        let mut s = LauncherState::default();
        s.record_launch("zzz", 42);
        s.record_launch("aaa", 42);
        assert_eq!(s.recent_ids(10), vec!["aaa", "zzz"]);
        assert_eq!(s.recent_ids(1), vec!["aaa"]);
    }

    #[test]
    fn most_used_orders_by_count() {
        let mut s = LauncherState::default();
        s.record_launch("rare", 10);
        for t in 0..5 {
            s.record_launch("common", 100 + t);
        }
        assert_eq!(s.most_used(10), vec!["common", "rare"]);
        assert_eq!(s.most_used(1), vec!["common"]);
    }

    #[test]
    fn forget_removes_everything() {
        let mut s = sample();
        assert!(s.forget("firefox"));
        assert!(!s.recent.iter().any(|r| r.app_id == "firefox"));
        assert!(!s.launch_counts.contains_key("firefox"));
        // Forgetting an unknown id is a no-op.
        assert!(!s.forget("ghost"));
    }

    #[test]
    fn prune_caps_size_keeping_recent() {
        let mut s = LauncherState::default();
        for (i, id) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            s.record_launch(id, 100 + i as u64);
        }
        s.prune(3);
        assert_eq!(s.recent.len(), 3);
        // Newest three kept: e, d, c.
        assert_eq!(s.recent_ids(10), vec!["e", "d", "c"]);
        // Lifetime counts survive pruning.
        assert_eq!(s.launch_counts.len(), 5);
        // Pruning below current size does nothing extra.
        s.prune(10);
        assert_eq!(s.recent.len(), 3);
    }

    #[test]
    fn migrate_stamps_legacy_version() {
        // A v0 document: no `version` key at all.
        let legacy = r#"
[[recent]]
app_id = "firefox"
last_used_unix = 1234
count = 3

[launch_counts]
firefox = 3
"#;
        let state = LauncherState::from_toml_str(legacy).unwrap();
        assert_eq!(state.version, CURRENT_VERSION);
        assert_eq!(state.recent.len(), 1);
        assert_eq!(state.launch_counts["firefox"], 3);
    }

    #[test]
    fn canonical_path_is_under_glaze_state() {
        let p = LauncherState::path();
        assert_eq!(p, state_file());
        assert!(
            p.ends_with("glaze/state.toml") || p.ends_with("glaze\\state.toml"),
            "unexpected state path: {p:?}"
        );
    }

    #[test]
    fn corrupt_file_is_tolerated_by_load() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "this is not = valid [ toml").unwrap();
        // Explicit API surfaces the parse error...
        assert!(matches!(
            LauncherState::load_from(&path),
            Err(StateError::Parse(_))
        ));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
