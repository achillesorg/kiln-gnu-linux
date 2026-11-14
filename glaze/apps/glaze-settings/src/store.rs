//! The configuration store behind every control.
//!
//! Two TOML documents are edited **in place** with `toml_edit`, so comments,
//! key order and keys this app does not know survive a save:
//!
//! - `config.toml` (`$XDG_CONFIG_HOME/glaze/config.toml`), the shared Glaze
//!   configuration that `glaze-shell` watches and reloads live;
//! - `settings.toml` (same directory), a sidecar for settings that the current
//!   `glaze-common` schema has no key for yet (see `docs/settings-needs.md`).
//!   Nothing outside this app reads it today; the UI says so on those rows.
//!
//! Safety rules implemented here:
//! - every change to `config.toml` is validated with
//!   `glaze_common::config_check::parse_checked` before it is written; a change
//!   that would produce an error at its own key is refused (the file is not
//!   touched) and the message is returned for an inline error;
//! - the original file is copied to a timestamped backup before the first
//!   write of a session and before every destructive operation (reset,
//!   import, undo of a reset);
//! - writes are atomic (temporary file + rename in the same directory);
//! - every change pushes an undo snapshot (both documents);
//! - a file changed on disk by someone else (the shell saving pins) is
//!   re-read before the next change is applied, so their edit is kept;
//! - a `config.toml` that does not parse puts the store in read-only mode:
//!   nothing is written until the user fixes or resets it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use glaze_common::config::GlazeConfig;
use glaze_common::config_check::{self, Loaded};
use toml_edit::{DocumentMut, Item};

/// Which document a key lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Doc {
    /// `config.toml` (shared, live-reloaded by the shell).
    Main,
    /// `settings.toml` (settings-only sidecar for keys without a schema yet).
    Extra,
}

/// One edit in a batch.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Set(Doc, String, toml::Value),
    Remove(Doc, String),
}

#[derive(Debug, Clone)]
struct Snapshot {
    main: String,
    extra: String,
    label: String,
}

/// Number of timestamped backups kept per file.
pub const BACKUPS_KEPT: usize = 10;
/// Depth of the undo stack.
pub const UNDO_DEPTH: usize = 64;

pub struct Store {
    main_path: PathBuf,
    extra_path: PathBuf,
    backup_dir: PathBuf,
    main: DocumentMut,
    extra: DocumentMut,
    main_stamp: Option<SystemTime>,
    extra_stamp: Option<SystemTime>,
    undo: Vec<Snapshot>,
    backed_up: bool,
    /// Set when `config.toml` exists but does not parse; writes are refused.
    read_only: Option<String>,
    last_backup: Option<PathBuf>,
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn read_doc(path: &Path) -> Result<DocumentMut, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => s.parse::<DocumentMut>().map_err(|e| {
            let line = e
                .span()
                .map(|sp| s[..sp.start.min(s.len())].matches('\n').count() + 1);
            match line {
                Some(l) => format!("line {l}: {}", e.message()),
                None => e.message().to_string(),
            }
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Convert a `toml::Value` into a `toml_edit` item (tables stay tables).
pub fn to_item(v: &toml::Value) -> Item {
    match v {
        toml::Value::Table(t) => {
            let mut tbl = toml_edit::Table::new();
            for (k, v) in t {
                tbl.insert(k, to_item(v));
            }
            Item::Table(tbl)
        }
        other => Item::Value(to_edit_value(other)),
    }
}

fn to_edit_value(v: &toml::Value) -> toml_edit::Value {
    match v {
        toml::Value::String(s) => s.as_str().into(),
        toml::Value::Integer(i) => (*i).into(),
        toml::Value::Float(f) => (*f).into(),
        toml::Value::Boolean(b) => (*b).into(),
        toml::Value::Datetime(d) => d.to_string().into(),
        toml::Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for x in a {
                arr.push(to_edit_value(x));
            }
            toml_edit::Value::Array(arr)
        }
        toml::Value::Table(t) => {
            let mut it = toml_edit::InlineTable::new();
            for (k, v) in t {
                it.insert(k, to_edit_value(v));
            }
            toml_edit::Value::InlineTable(it)
        }
    }
}

fn split(path: &str) -> Vec<&str> {
    path.split('.').filter(|s| !s.is_empty()).collect()
}

fn set_path(doc: &mut DocumentMut, path: &str, value: &toml::Value) {
    let segs = split(path);
    let Some((last, parents)) = segs.split_last() else {
        return;
    };
    let mut cur: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for seg in parents {
        let needs_new = !matches!(cur.get(seg), Some(i) if i.is_table_like());
        if needs_new {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            cur.insert(seg, Item::Table(t));
        }
        cur = cur
            .get_mut(seg)
            .and_then(Item::as_table_like_mut)
            .expect("just inserted a table");
    }
    let new = to_item(value);
    match (cur.get_mut(last), new) {
        // Assign in place so the key's comments survive; keep the value's
        // own decoration (spacing, trailing comment) too.
        (Some(Item::Value(old)), Item::Value(mut v)) => {
            let decor = old.decor().clone();
            *v.decor_mut() = decor;
            *old = v;
        }
        (Some(slot), new) => *slot = new,
        (None, new) => {
            cur.insert(last, new);
        }
    }
}

fn remove_path(doc: &mut DocumentMut, path: &str) -> bool {
    fn rec(t: &mut dyn toml_edit::TableLike, segs: &[&str]) -> bool {
        match segs {
            [] => false,
            [last] => t.remove(last).is_some(),
            [first, rest @ ..] => {
                let Some(child) = t.get_mut(first).and_then(Item::as_table_like_mut) else {
                    return false;
                };
                let removed = rec(child, rest);
                if removed && child.is_empty() {
                    t.remove(first);
                }
                removed
            }
        }
    }
    rec(doc.as_table_mut(), &split(path))
}

/// Read a value at a dotted path from a parsed table.
pub fn lookup<'a>(table: &'a toml::Table, path: &str) -> Option<&'a toml::Value> {
    let segs = split(path);
    let (last, parents) = segs.split_last()?;
    let mut cur = table;
    for seg in parents {
        cur = cur.get(*seg)?.as_table()?;
    }
    cur.get(*last)
}

fn now_tag() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // UTC calendar date from the epoch (civil-from-days, Howard Hinnant).
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// Write `contents` to `path` atomically (temporary file + rename).
pub fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".into());
    let tmp = path.with_file_name(format!(".{name}.glaze-settings.tmp"));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

impl Store {
    /// Open the real files under the user's XDG directories.
    pub fn open_default() -> Store {
        let main = glaze_common::paths::config_file();
        let extra = glaze_common::paths::glaze_config_dir().join("settings.toml");
        let backups = glaze_common::paths::state_home().join("glaze/settings-backups");
        Store::open(main, extra, backups)
    }

    pub fn open(main_path: PathBuf, extra_path: PathBuf, backup_dir: PathBuf) -> Store {
        let (main, read_only) = match read_doc(&main_path) {
            Ok(d) => (d, None),
            Err(e) => (DocumentMut::new(), Some(e)),
        };
        let extra = read_doc(&extra_path).unwrap_or_default();
        Store {
            main_stamp: stamp(&main_path),
            extra_stamp: stamp(&extra_path),
            main_path,
            extra_path,
            backup_dir,
            main,
            extra,
            undo: Vec::new(),
            backed_up: false,
            read_only,
            last_backup: None,
        }
    }

    pub fn main_path(&self) -> &Path {
        &self.main_path
    }

    pub fn extra_path(&self) -> &Path {
        &self.extra_path
    }

    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    pub fn last_backup(&self) -> Option<&Path> {
        self.last_backup.as_deref()
    }

    /// `Some(reason)` when `config.toml` does not parse (writes refused).
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// The text of a document as it would be written.
    pub fn text(&self, doc: Doc) -> String {
        match doc {
            Doc::Main => self.main.to_string(),
            Doc::Extra => self.extra.to_string(),
        }
    }

    /// The typed, validated configuration (with per-key recovery).
    pub fn loaded(&self) -> Loaded {
        config_check::parse_checked(&self.main.to_string())
    }

    pub fn config(&self) -> GlazeConfig {
        self.loaded().config
    }

    /// A value as written in a document (not defaulted).
    pub fn get(&self, doc: Doc, path: &str) -> Option<toml::Value> {
        let table: toml::Table = self.text(doc).parse().ok()?;
        lookup(&table, path).cloned()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Label of the change that `undo` would revert.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    /// Re-read files that changed on disk since we last read or wrote them.
    pub fn refresh_if_changed(&mut self) -> bool {
        let mut changed = false;
        if stamp(&self.main_path) != self.main_stamp {
            match read_doc(&self.main_path) {
                Ok(d) => {
                    self.main = d;
                    self.read_only = None;
                }
                Err(e) => self.read_only = Some(e),
            }
            self.main_stamp = stamp(&self.main_path);
            changed = true;
        }
        if stamp(&self.extra_path) != self.extra_stamp {
            if let Ok(d) = read_doc(&self.extra_path) {
                self.extra = d;
            }
            self.extra_stamp = stamp(&self.extra_path);
            changed = true;
        }
        changed
    }

    /// Copy the current on-disk files into the backup directory. Returns the
    /// backup of `config.toml` (if it existed).
    pub fn backup_now(&mut self, reason: &str) -> std::io::Result<Option<PathBuf>> {
        std::fs::create_dir_all(&self.backup_dir)?;
        let tag = format!("{}-{}", now_tag(), sanitize(reason));
        let mut out = None;
        for (src, stem) in [(&self.main_path, "config"), (&self.extra_path, "settings")] {
            if src.is_file() {
                let mut dst = self.backup_dir.join(format!("{stem}-{tag}.toml"));
                let mut n = 1;
                while dst.exists() {
                    dst = self.backup_dir.join(format!("{stem}-{tag}-{n}.toml"));
                    n += 1;
                }
                std::fs::copy(src, &dst)?;
                if stem == "config" {
                    out = Some(dst.clone());
                }
                prune(&self.backup_dir, stem, BACKUPS_KEPT);
            }
        }
        if out.is_some() {
            self.last_backup = out.clone();
        }
        self.backed_up = true;
        Ok(out)
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            main: self.main.to_string(),
            extra: self.extra.to_string(),
            label: label.to_string(),
        }
    }

    fn write(&mut self, docs: &[Doc]) -> Result<(), String> {
        if !self.backed_up {
            self.backup_now("session")
                .map_err(|e| format!("could not back up the configuration: {e}"))?;
        }
        for d in docs {
            let (path, text) = match d {
                Doc::Main => (&self.main_path, self.main.to_string()),
                Doc::Extra => (&self.extra_path, self.extra.to_string()),
            };
            atomic_write(path, &text)
                .map_err(|e| format!("could not save {}: {e}", path.display()))?;
        }
        self.main_stamp = stamp(&self.main_path);
        self.extra_stamp = stamp(&self.extra_path);
        Ok(())
    }

    /// Apply one change. See [`Store::apply`].
    pub fn set(
        &mut self,
        doc: Doc,
        path: &str,
        value: toml::Value,
        label: &str,
    ) -> Result<(), String> {
        self.apply(
            vec![Change::Set(doc, path.to_string(), value)],
            label,
            false,
        )
    }

    /// Remove one key (back to its default).
    pub fn remove(&mut self, doc: Doc, path: &str, label: &str) -> Result<(), String> {
        self.apply(vec![Change::Remove(doc, path.to_string())], label, false)
    }

    /// Apply a batch of changes as one undoable step. `destructive` forces a
    /// fresh backup first. Returns an error (and changes nothing) when the
    /// store is read-only, when a change introduces a validation error at the
    /// key it touched, or when the write fails.
    pub fn apply(
        &mut self,
        changes: Vec<Change>,
        label: &str,
        destructive: bool,
    ) -> Result<(), String> {
        self.refresh_if_changed();
        let touches_main = changes
            .iter()
            .any(|c| matches!(c, Change::Set(Doc::Main, ..) | Change::Remove(Doc::Main, _)));
        if let Some(reason) = &self.read_only {
            if touches_main {
                return Err(format!(
                    "config.toml does not parse ({reason}); fix or reset it first"
                ));
            }
        }
        let before = self.snapshot(label);
        let before_errors: Vec<String> = self
            .loaded()
            .issues
            .iter()
            .filter(|i| i.is_error())
            .map(|i| i.path.clone())
            .collect();
        let mut touched = Vec::new();
        for c in &changes {
            match c {
                Change::Set(Doc::Main, p, v) => {
                    set_path(&mut self.main, p, v);
                    touched.push(p.clone());
                }
                Change::Set(Doc::Extra, p, v) => set_path(&mut self.extra, p, v),
                Change::Remove(Doc::Main, p) => {
                    remove_path(&mut self.main, p);
                }
                Change::Remove(Doc::Extra, p) => {
                    remove_path(&mut self.extra, p);
                }
            }
        }
        if touches_main {
            let loaded = self.loaded();
            let new_err = loaded.issues.iter().find(|i| {
                i.is_error()
                    && !before_errors.contains(&i.path)
                    && (i.path.is_empty()
                        || touched.iter().any(|t| {
                            i.path == *t
                                || i.path.starts_with(&format!("{t}."))
                                || t.starts_with(&format!("{}.", i.path))
                        }))
            });
            if let Some(issue) = new_err {
                let msg = issue.message.clone();
                self.restore(&before);
                return Err(msg);
            }
        }
        if destructive {
            if let Err(e) = self.backup_now(label) {
                self.restore(&before);
                return Err(format!("could not back up before \"{label}\": {e}"));
            }
        }
        let mut docs = vec![];
        if touches_main {
            docs.push(Doc::Main);
        }
        if changes.iter().any(|c| {
            matches!(
                c,
                Change::Set(Doc::Extra, ..) | Change::Remove(Doc::Extra, _)
            )
        }) {
            docs.push(Doc::Extra);
        }
        if before.main == self.main.to_string() && before.extra == self.extra.to_string() {
            return Ok(());
        }
        if let Err(e) = self.write(&docs) {
            self.restore(&before);
            return Err(e);
        }
        self.undo.push(before);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        Ok(())
    }

    fn restore(&mut self, s: &Snapshot) {
        self.main = s.main.parse().unwrap_or_default();
        self.extra = s.extra.parse().unwrap_or_default();
    }

    /// Revert the last change. Returns its label.
    pub fn undo(&mut self) -> Result<Option<String>, String> {
        let Some(s) = self.undo.pop() else {
            return Ok(None);
        };
        self.refresh_if_changed();
        let current = self.snapshot("");
        self.restore(&s);
        if let Err(e) = self.write(&[Doc::Main, Doc::Extra]) {
            self.restore(&current);
            self.undo.push(s);
            return Err(e);
        }
        self.read_only = None;
        Ok(Some(s.label))
    }

    /// Replace `config.toml` with defaults after a backup (used when the file
    /// does not parse). Keeps the sidecar.
    pub fn reset_broken_main(&mut self) -> Result<Option<PathBuf>, String> {
        let b = self.backup_now("broken").map_err(|e| e.to_string())?;
        let before = self.snapshot("Reset broken config.toml");
        self.main = DocumentMut::new();
        self.read_only = None;
        self.write(&[Doc::Main])?;
        self.undo.push(before);
        Ok(b)
    }
}

fn sanitize(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "change".into()
    } else {
        out.chars().take(32).collect()
    }
}

fn prune(dir: &Path, stem: &str, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{stem}-")) && n.ends_with(".toml"))
        })
        .collect();
    files.sort();
    while files.len() > keep {
        let _ = std::fs::remove_file(files.remove(0));
    }
}

/// Serialise a serde value (enum, list of enums) to a TOML value.
pub fn value_of<T: serde::Serialize>(v: &T) -> toml::Value {
    toml::Value::try_from(v).unwrap_or(toml::Value::String(String::new()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "glaze-settings-test-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn store(dir: &Path) -> Store {
        Store::open(
            dir.join("config.toml"),
            dir.join("settings.toml"),
            dir.join("backups"),
        )
    }

    #[test]
    fn set_preserves_comments_and_unknown_keys() {
        let d = tmp_dir("comments");
        std::fs::write(
            d.join("config.toml"),
            "# my precious comment\n[panel]\n# edge\nposition = \"top\"\nfuture_key = 3\n",
        )
        .unwrap();
        let mut s = store(&d);
        s.set(Doc::Main, "panel.position", "left".into(), "Taskbar edge")
            .unwrap();
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("# my precious comment"));
        assert!(text.contains("# edge"));
        assert!(text.contains("future_key = 3"));
        assert!(text.contains("position = \"left\""));
        assert_eq!(
            s.config().panel.position,
            glaze_common::config::PanelPosition::Left
        );
    }

    #[test]
    fn invalid_values_are_refused_and_not_written() {
        let d = tmp_dir("invalid");
        let mut s = store(&d);
        let err = s
            .set(Doc::Main, "panel.height", toml::Value::Integer(500), "h")
            .unwrap_err();
        assert!(err.contains("24..=96"), "{err}");
        assert!(!d.join("config.toml").exists());
        let err = s
            .set(Doc::Main, "appearance.accent", "#zzz".into(), "a")
            .unwrap_err();
        assert!(err.contains("invalid colour"), "{err}");
        let err = s
            .set(Doc::Main, "panel.position", "middle".into(), "p")
            .unwrap_err();
        assert!(!err.is_empty());
        assert!(!s.can_undo());
    }

    #[test]
    fn undo_restores_previous_text() {
        let d = tmp_dir("undo");
        let mut s = store(&d);
        s.set(Doc::Main, "panel.clock_24h", false.into(), "Clock")
            .unwrap();
        s.set(Doc::Extra, "windows.gaps", toml::Value::Integer(12), "Gaps")
            .unwrap();
        assert_eq!(s.undo_label(), Some("Gaps"));
        assert_eq!(s.undo().unwrap().as_deref(), Some("Gaps"));
        assert!(s.get(Doc::Extra, "windows.gaps").is_none());
        assert_eq!(s.undo().unwrap().as_deref(), Some("Clock"));
        assert!(s.config().panel.clock_24h);
        assert_eq!(s.undo().unwrap(), None);
    }

    #[test]
    fn first_write_and_destructive_changes_make_backups() {
        let d = tmp_dir("backup");
        std::fs::write(d.join("config.toml"), "[panel]\nposition = \"top\"\n").unwrap();
        let mut s = store(&d);
        s.set(Doc::Main, "panel.autohide", "always".into(), "Autohide")
            .unwrap();
        let n1 = std::fs::read_dir(d.join("backups")).unwrap().count();
        assert_eq!(n1, 1);
        // Second ordinary write: no new backup.
        s.set(Doc::Main, "panel.autohide", "smart".into(), "Autohide")
            .unwrap();
        assert_eq!(std::fs::read_dir(d.join("backups")).unwrap().count(), 1);
        s.apply(
            vec![Change::Remove(Doc::Main, "panel".into())],
            "Reset taskbar",
            true,
        )
        .unwrap();
        assert_eq!(std::fs::read_dir(d.join("backups")).unwrap().count(), 2);
        let first = std::fs::read_dir(d.join("backups"))
            .unwrap()
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect::<Vec<_>>();
        assert!(first
            .iter()
            .any(|t| t.contains("position = \"top\"") && !t.contains("autohide")));
        assert!(s.get(Doc::Main, "panel").is_none());
    }

    #[test]
    fn broken_config_is_read_only_until_reset() {
        let d = tmp_dir("broken");
        std::fs::write(d.join("config.toml"), "[panel\nposition = 1\n").unwrap();
        let mut s = store(&d);
        assert!(s.read_only().unwrap().contains("line 1"));
        assert!(s
            .set(Doc::Main, "panel.position", "top".into(), "x")
            .is_err());
        // The sidecar still works.
        s.set(Doc::Extra, "windows.gaps", toml::Value::Integer(4), "g")
            .unwrap();
        let b = s.reset_broken_main().unwrap().unwrap();
        assert!(std::fs::read_to_string(b).unwrap().contains("[panel\n"));
        assert!(s.read_only().is_none());
        s.set(Doc::Main, "panel.position", "top".into(), "x")
            .unwrap();
    }

    #[test]
    fn external_edits_are_picked_up_before_the_next_change() {
        let d = tmp_dir("external");
        let mut s = store(&d);
        s.set(Doc::Main, "panel.clock_24h", false.into(), "c")
            .unwrap();
        // The shell saves pins behind our back (mtime granularity guard).
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            d.join("config.toml"),
            "[panel]\nclock_24h = false\n[launcher]\npinned = [\"foot\"]\n",
        )
        .unwrap();
        // Force a different mtime even on coarse filesystems.
        s.main_stamp = None;
        s.set(Doc::Main, "panel.show_seconds", true.into(), "s")
            .unwrap();
        let c = s.config();
        assert_eq!(c.launcher.pinned, vec!["foot".to_string()]);
        assert!(c.panel.show_seconds);
    }

    #[test]
    fn nested_paths_and_removal_tidy_empty_tables() {
        let d = tmp_dir("nested");
        let mut s = store(&d);
        s.set(
            Doc::Main,
            "panel.monitors.HDMI-A-1.position",
            "top".into(),
            "m",
        )
        .unwrap();
        assert_eq!(
            s.config().panel.for_monitor("HDMI-A-1").position,
            glaze_common::config::PanelPosition::Top
        );
        s.remove(Doc::Main, "panel.monitors.HDMI-A-1.position", "m")
            .unwrap();
        assert!(s.get(Doc::Main, "panel").is_none(), "{}", s.text(Doc::Main));
    }

    #[test]
    fn enum_lists_serialise_kebab_case() {
        use glaze_common::config::PanelSection;
        let v = value_of(&vec![PanelSection::Start, PanelSection::LayoutToggle]);
        assert_eq!(v.to_string(), "[\"start\", \"layout-toggle\"]");
    }

    #[test]
    fn backups_are_pruned() {
        let d = tmp_dir("prune");
        std::fs::write(d.join("config.toml"), "").unwrap();
        let mut s = store(&d);
        for i in 0..(BACKUPS_KEPT + 3) {
            s.backup_now(&format!("r{i}")).unwrap();
        }
        let n = std::fs::read_dir(d.join("backups"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("config-"))
            .count();
        assert_eq!(n, BACKUPS_KEPT);
    }
}
