//! Application registry: discovers and validates real `.desktop` entries.
//!
//! The registry is the *only* source of applications in Glaze. It walks the XDG
//! data directories in precedence order, parses every `.desktop` file, applies
//! the standard visibility rules (`Hidden`, `NoDisplay`, `OnlyShowIn`,
//! `NotShowIn`, `TryExec`) and de-duplicates by desktop-file id so a user entry
//! shadows a system entry of the same name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glaze_common::model::AppEntry;
use glaze_common::paths;

use crate::desktop::{tokenize_exec, DesktopFile, ExecContext};

/// Options controlling discovery.
#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    /// Include entries marked `NoDisplay=true` (usually not wanted in menus).
    pub include_no_display: bool,
    /// Verify `TryExec` binaries exist before accepting an entry.
    pub check_try_exec: bool,
    /// Desktop names used for `OnlyShowIn`/`NotShowIn` matching.
    pub current_desktops: Vec<String>,
    /// Locale used for localised `Name`/`Comment` keys.
    pub locale: String,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        let current_desktops = std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.split(':').map(|p| p.trim().to_string()).collect())
            .unwrap_or_else(|| vec!["Glaze".to_string()]);
        let locale = std::env::var("LANG")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "en".to_string());
        DiscoverOptions {
            include_no_display: false,
            check_try_exec: true,
            current_desktops,
            locale,
        }
    }
}

/// The discovered application registry.
#[derive(Debug, Clone, Default)]
pub struct AppRegistry {
    apps: Vec<AppEntry>,
}

impl AppRegistry {
    /// Discover using the process's real XDG environment.
    pub fn discover(options: DiscoverOptions) -> Self {
        Self::discover_in(&paths::data_dirs(), options)
    }

    /// Discover from an explicit set of data directories (test-friendly).
    pub fn discover_in(dirs: &[PathBuf], options: DiscoverOptions) -> Self {
        // id -> entry; first (highest precedence) definition wins.
        let mut map: BTreeMap<String, AppEntry> = BTreeMap::new();
        for dir in dirs {
            let apps_dir = dir.join("applications");
            if !apps_dir.is_dir() {
                continue;
            }
            for path in walk_desktop_files(&apps_dir) {
                let Some(id) = desktop_id(&apps_dir, &path) else {
                    continue;
                };
                if map.contains_key(&id) {
                    continue; // shadowed by a higher-precedence entry
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let entry =
                    DesktopFile::parse(&text).to_entry(&path.to_string_lossy(), &options.locale);
                if let Some(app) = to_app_entry(&id, entry, &options) {
                    map.insert(id, app);
                }
            }
        }
        let mut apps: Vec<AppEntry> = map.into_values().collect();
        apps.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        AppRegistry { apps }
    }

    /// Build a registry from already-parsed entries (used by tests and IPC).
    pub fn from_entries(mut apps: Vec<AppEntry>) -> Self {
        apps.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        AppRegistry { apps }
    }

    /// All visible applications.
    pub fn apps(&self) -> &[AppEntry] {
        &self.apps
    }

    /// Number of registered applications.
    pub fn len(&self) -> usize {
        self.apps.len()
    }

    /// True when no applications were discovered.
    pub fn is_empty(&self) -> bool {
        self.apps.is_empty()
    }

    /// Look up an application by its desktop id.
    pub fn get(&self, id: &str) -> Option<&AppEntry> {
        self.apps.iter().find(|a| a.id == id)
    }
}

/// Convert a parsed desktop entry into an [`AppEntry`], or `None` if it should
/// not be shown or is invalid.
fn to_app_entry(
    id: &str,
    entry: crate::desktop::DesktopEntry,
    options: &DiscoverOptions,
) -> Option<AppEntry> {
    if entry.type_ != "Application" {
        return None;
    }
    if entry.hidden {
        return None;
    }
    if entry.no_display && !options.include_no_display {
        return None;
    }
    if entry.name.trim().is_empty() {
        return None;
    }
    let exec = entry.exec?.trim().to_string();
    // An entry whose Exec cannot be tokenised into at least one argument is
    // broken; refuse it rather than showing an app that cannot launch.
    let empty_ctx = ExecContext::default();
    if tokenize_exec(&exec, &empty_ctx).is_err() {
        return None;
    }

    if !entry.only_show_in.is_empty() {
        let shown = entry.only_show_in.iter().any(|d| {
            options
                .current_desktops
                .iter()
                .any(|c| c.eq_ignore_ascii_case(d))
        });
        if !shown {
            return None;
        }
    }
    if !entry.not_show_in.is_empty() {
        let hidden = entry.not_show_in.iter().any(|d| {
            options
                .current_desktops
                .iter()
                .any(|c| c.eq_ignore_ascii_case(d))
        });
        if hidden {
            return None;
        }
    }

    if options.check_try_exec {
        if let Some(try_exec) = entry
            .try_exec
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !program_exists(try_exec) {
                return None;
            }
        }
    }

    Some(AppEntry {
        id: id.to_string(),
        name: entry.name,
        comment: entry.comment,
        generic_name: entry.generic_name,
        exec,
        try_exec: entry.try_exec,
        icon: entry.icon,
        categories: entry.categories,
        keywords: entry.keywords,
        terminal: entry.terminal,
        no_display: entry.no_display,
        working_dir: entry.path,
        source_path: entry.source_path,
    })
}

/// Compute the desktop-file id: the path relative to `applications/`, without
/// the `.desktop` suffix, with `/` replaced by `-`.
fn desktop_id(applications_dir: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(applications_dir).ok()?;
    let mut id = rel.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = id.strip_suffix(".desktop") {
        id = stripped.to_string();
    }
    Some(id.replace('/', "-"))
}

/// Recursively collect `*.desktop` files under `dir`.
fn walk_desktop_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
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

/// True when `program` is an executable that exists (absolute path or on PATH).
pub fn program_exists(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn opts() -> DiscoverOptions {
        DiscoverOptions {
            include_no_display: false,
            check_try_exec: false,
            current_desktops: vec!["Glaze".into()],
            locale: "en".into(),
        }
    }

    fn tempdir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("glaze-registry-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn discovers_valid_entries_and_ids() {
        let root = tempdir("basic");
        write(
            &root,
            "applications/firefox.desktop",
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\n",
        );
        write(
            &root,
            "applications/kde/kate.desktop",
            "[Desktop Entry]\nType=Application\nName=Kate\nExec=kate\n",
        );
        let reg = AppRegistry::discover_in(std::slice::from_ref(&root), opts());
        let ids: Vec<&str> = reg.apps().iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"firefox"));
        assert!(ids.contains(&"kde-kate"));
        assert_eq!(reg.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn user_dir_shadows_system_dir() {
        let user = tempdir("shadow-user");
        let sys = tempdir("shadow-sys");
        write(
            &user,
            "applications/editor.desktop",
            "[Desktop Entry]\nType=Application\nName=UserEditor\nExec=ue\n",
        );
        write(
            &sys,
            "applications/editor.desktop",
            "[Desktop Entry]\nType=Application\nName=SystemEditor\nExec=se\n",
        );
        let reg = AppRegistry::discover_in(&[user.clone(), sys.clone()], opts());
        assert_eq!(reg.get("editor").unwrap().name, "UserEditor");
        let _ = fs::remove_dir_all(&user);
        let _ = fs::remove_dir_all(&sys);
    }

    #[test]
    fn skips_hidden_nondisplay_and_non_application() {
        let root = tempdir("filter");
        write(
            &root,
            "applications/hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=H\nExec=h\nHidden=true\n",
        );
        write(
            &root,
            "applications/nodisp.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=n\nNoDisplay=true\n",
        );
        write(
            &root,
            "applications/link.desktop",
            "[Desktop Entry]\nType=Link\nName=L\nURL=https://x\n",
        );
        write(
            &root,
            "applications/broken.desktop",
            "[Desktop Entry]\nType=Application\nName=B\n",
        );
        let reg = AppRegistry::discover_in(std::slice::from_ref(&root), opts());
        assert!(
            reg.is_empty(),
            "expected no visible apps, got {:?}",
            reg.apps()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn honours_only_show_in_and_not_show_in() {
        let root = tempdir("showin");
        write(
            &root,
            "applications/gnome-only.desktop",
            "[Desktop Entry]\nType=Application\nName=G\nExec=g\nOnlyShowIn=GNOME;\n",
        );
        write(
            &root,
            "applications/never-glaze.desktop",
            "[Desktop Entry]\nType=Application\nName=X\nExec=x\nNotShowIn=Glaze;\n",
        );
        write(
            &root,
            "applications/any.desktop",
            "[Desktop Entry]\nType=Application\nName=A\nExec=a\n",
        );
        let reg = AppRegistry::discover_in(std::slice::from_ref(&root), opts());
        let ids: Vec<&str> = reg.apps().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["any"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_directory_is_not_an_error() {
        let reg = AppRegistry::discover_in(&[PathBuf::from("/definitely/not/here")], opts());
        assert!(reg.is_empty());
    }
}
