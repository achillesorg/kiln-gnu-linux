//! Persisted user desktop shortcuts.
//!
//! The desktop surface is user-configurable: a shortcut can pin an installed
//! application, run a literal command, sit at a fixed grid position and carry a
//! custom label/icon. That intent is stored as TOML under
//! `$XDG_CONFIG_HOME/glaze/desktop.toml` (`GLAZE.md` §5 P1 "minimal launchable
//! desktop shortcuts", §11 "persist via versioned configuration").
//!
//! All I/O goes through explicit paths so tests are deterministic and never
//! touch the developer's real config. Loading is forgiving by design: a missing,
//! unreadable or corrupt file yields [`ShortcutFile::default`] rather than
//! failing the whole desktop.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use glaze_common::paths;
use serde::{Deserialize, Serialize};

/// Current on-disk schema version for `desktop.toml`.
pub const DESKTOP_SHORTCUTS_VERSION: u32 = 1;

/// A shortcut's pinned grid cell. Row-major coordinates; the layout engine maps
/// these onto the column-major desktop pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IconPos {
    /// Zero-based column.
    pub column: u32,
    /// Zero-based row within the column.
    pub row: u32,
}

/// One persisted desktop shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcut {
    /// Stable icon key (see [`DesktopIcon::key`](crate::icons::DesktopIcon::key)).
    /// This is the identity used for selection and de-duplication.
    pub key: String,
    /// Custom display label; empty means "use the application/program name".
    pub label: String,
    /// Custom icon name or path; empty means "use the application icon".
    pub icon: Option<String>,
    /// Installed application this shortcut launches, by desktop-file id.
    pub app_id: Option<String>,
    /// Literal command argv for a user-created shortcut (never a shell string).
    pub cmd: Vec<String>,
    /// Pinned position, or `None` to let the layout engine place it.
    pub position: Option<IconPos>,
}

impl Shortcut {
    /// True when activating this shortcut can do something: it either names an
    /// application or carries a non-empty command.
    pub fn is_launchable(&self) -> bool {
        self.app_id.as_deref().is_some_and(|s| !s.trim().is_empty())
            || self.cmd.first().is_some_and(|p| !p.is_empty())
    }
}

/// The whole `desktop.toml` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutFile {
    /// Schema version; see [`ShortcutFile::migrate`].
    pub version: u32,
    /// The shortcuts, in display order.
    pub shortcuts: Vec<Shortcut>,
}

impl Default for ShortcutFile {
    fn default() -> Self {
        ShortcutFile {
            version: DESKTOP_SHORTCUTS_VERSION,
            shortcuts: Vec::new(),
        }
    }
}

/// Errors from saving shortcuts. Loading never errors (see [`ShortcutFile::load_from`]).
#[derive(Debug)]
pub enum ShortcutError {
    /// Filesystem failure.
    Io(std::io::Error),
    /// TOML serialisation failure.
    Serialize(toml::ser::Error),
}

impl std::fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShortcutError::Io(e) => write!(f, "desktop shortcut I/O error: {e}"),
            ShortcutError::Serialize(e) => write!(f, "desktop shortcut serialise error: {e}"),
        }
    }
}

impl std::error::Error for ShortcutError {}

impl From<std::io::Error> for ShortcutError {
    fn from(e: std::io::Error) -> Self {
        ShortcutError::Io(e)
    }
}

impl From<toml::ser::Error> for ShortcutError {
    fn from(e: toml::ser::Error) -> Self {
        ShortcutError::Serialize(e)
    }
}

impl ShortcutFile {
    /// The default location: `$XDG_CONFIG_HOME/glaze/desktop.toml`.
    pub fn default_path() -> PathBuf {
        paths::glaze_config_dir().join("desktop.toml")
    }

    /// Load from `path`.
    ///
    /// A missing, unreadable or corrupt file returns [`ShortcutFile::default`]
    /// so a broken config can never prevent the desktop from starting. Any
    /// successfully parsed document is passed through [`ShortcutFile::migrate`].
    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match toml::from_str::<ShortcutFile>(&text) {
            Ok(mut file) => {
                file.migrate();
                file
            }
            Err(_) => Self::default(),
        }
    }

    /// Save to `path`, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<(), ShortcutError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, self.to_toml_string()?)?;
        Ok(())
    }

    /// Serialise to a TOML string.
    pub fn to_toml_string(&self) -> Result<String, ShortcutError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Upgrade an older document in place and normalise it.
    ///
    /// - Any version (including a future/newer one) is clamped to
    ///   [`DESKTOP_SHORTCUTS_VERSION`], because unknown fields are ignored
    ///   rather than discarded.
    /// - Shortcuts with an empty key are dropped, and duplicate keys are
    ///   de-duplicated first-wins, so keys stay a stable identity.
    pub fn migrate(&mut self) {
        // v0 -> v1: initial schema. Placeholder for real structural changes.
        self.version = DESKTOP_SHORTCUTS_VERSION;
        let mut seen = BTreeSet::new();
        self.shortcuts
            .retain(|s| !s.key.trim().is_empty() && seen.insert(s.key.clone()));
    }

    /// Look up a shortcut by key.
    pub fn get(&self, key: &str) -> Option<&Shortcut> {
        self.shortcuts.iter().find(|s| s.key == key)
    }

    /// Insert `shortcut`, replacing any existing entry with the same key.
    pub fn upsert(&mut self, shortcut: Shortcut) {
        match self.shortcuts.iter_mut().find(|s| s.key == shortcut.key) {
            Some(existing) => *existing = shortcut,
            None => self.shortcuts.push(shortcut),
        }
    }

    /// Remove the shortcut with `key`; returns whether anything was removed.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.shortcuts.len();
        self.shortcuts.retain(|s| s.key != key);
        self.shortcuts.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("glaze-desktop-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample() -> ShortcutFile {
        ShortcutFile {
            version: DESKTOP_SHORTCUTS_VERSION,
            shortcuts: vec![
                Shortcut {
                    key: "app:org.mozilla.firefox".into(),
                    label: "Browser".into(),
                    icon: Some("firefox".into()),
                    app_id: Some("org.mozilla.firefox".into()),
                    cmd: vec![],
                    position: Some(IconPos { column: 0, row: 0 }),
                },
                Shortcut {
                    key: "cmd:\u{1f}htop".into(),
                    label: String::new(),
                    icon: None,
                    app_id: None,
                    cmd: vec!["htop".into()],
                    position: None,
                },
            ],
        }
    }

    #[test]
    fn default_path_is_under_glaze_config() {
        let p = ShortcutFile::default_path();
        assert!(p.ends_with("desktop.toml"));
        assert!(
            p.to_string_lossy().contains("glaze"),
            "unexpected path: {}",
            p.display()
        );
    }

    #[test]
    fn round_trips_through_a_file() {
        let dir = temp_dir("round");
        let path = dir.join("nested").join("desktop.toml");
        let file = sample();
        file.save_to(&path).unwrap();
        assert!(path.is_file(), "save must create parent dirs and the file");
        let back = ShortcutFile::load_from(&path);
        assert_eq!(back, file);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_yields_default() {
        let dir = temp_dir("missing");
        let file = ShortcutFile::load_from(&dir.join("does-not-exist.toml"));
        assert_eq!(file, ShortcutFile::default());
        assert!(file.shortcuts.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_yields_default() {
        let dir = temp_dir("corrupt");
        let path = dir.join("desktop.toml");
        fs::write(&path, "this is = not [ valid toml").unwrap();
        assert_eq!(ShortcutFile::load_from(&path), ShortcutFile::default());

        // Structurally valid TOML but the wrong shape for a shortcut.
        fs::write(&path, "[shortcuts]\nkey = 5\n").unwrap();
        assert_eq!(ShortcutFile::load_from(&path), ShortcutFile::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_bumps_version_and_keeps_data() {
        let dir = temp_dir("migrate");
        let path = dir.join("desktop.toml");
        // An old file without a version field, plus one with version 0.
        fs::write(
            &path,
            "[[shortcuts]]\nkey = \"app:editor\"\napp_id = \"editor\"\n",
        )
        .unwrap();
        let file = ShortcutFile::load_from(&path);
        assert_eq!(file.version, DESKTOP_SHORTCUTS_VERSION);
        assert_eq!(file.shortcuts.len(), 1);
        assert_eq!(file.shortcuts[0].key, "app:editor");

        let mut old = ShortcutFile {
            version: 0,
            shortcuts: vec![Shortcut {
                key: "cmd:\u{1f}ls".into(),
                cmd: vec!["ls".into()],
                ..Shortcut::default()
            }],
        };
        old.migrate();
        assert_eq!(old.version, DESKTOP_SHORTCUTS_VERSION);
        assert_eq!(old.shortcuts.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_drops_empty_keys_and_dedupes_first_wins() {
        let mut file = ShortcutFile {
            version: 0,
            shortcuts: vec![
                Shortcut {
                    key: "app:a".into(),
                    label: "first".into(),
                    ..Shortcut::default()
                },
                Shortcut {
                    key: "app:a".into(),
                    label: "second".into(),
                    ..Shortcut::default()
                },
                Shortcut {
                    key: "   ".into(),
                    ..Shortcut::default()
                },
            ],
        };
        file.migrate();
        assert_eq!(file.shortcuts.len(), 1);
        assert_eq!(file.shortcuts[0].label, "first");
    }

    #[test]
    fn migration_reports_current_version_for_future_files() {
        let mut file = ShortcutFile {
            version: 99,
            shortcuts: vec![],
        };
        file.migrate();
        assert_eq!(file.version, DESKTOP_SHORTCUTS_VERSION);
    }

    #[test]
    fn upsert_and_remove_behave() {
        let mut file = ShortcutFile::default();
        file.upsert(Shortcut {
            key: "app:a".into(),
            label: "A".into(),
            ..Shortcut::default()
        });
        assert_eq!(file.shortcuts.len(), 1);
        file.upsert(Shortcut {
            key: "app:a".into(),
            label: "A2".into(),
            ..Shortcut::default()
        });
        assert_eq!(file.shortcuts.len(), 1);
        assert_eq!(file.get("app:a").unwrap().label, "A2");
        assert!(file.remove("app:a"));
        assert!(!file.remove("app:a"));
        assert!(file.shortcuts.is_empty());
    }

    #[test]
    fn is_launchable_requires_a_target() {
        assert!(!Shortcut::default().is_launchable());
        assert!(Shortcut {
            app_id: Some("editor".into()),
            ..Shortcut::default()
        }
        .is_launchable());
        assert!(Shortcut {
            cmd: vec!["htop".into()],
            ..Shortcut::default()
        }
        .is_launchable());
        assert!(!Shortcut {
            cmd: vec![String::new()],
            ..Shortcut::default()
        }
        .is_launchable());
    }
}
