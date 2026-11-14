//! Installed applications (`.desktop` files) and XDG default applications
//! (`mimeapps.list`).
//!
//! Defaults are written to the user's `$XDG_CONFIG_HOME/mimeapps.list`
//! `[Default Applications]` section (what `xdg-mime default` does), which
//! every XDG-compliant launcher honours; no root is needed. Other sections
//! and unrelated lines are preserved.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// Desktop file id (`org.gnome.Nautilus.desktop`).
    pub id: String,
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub mime_types: Vec<String>,
    pub categories: Vec<String>,
    pub hidden: bool,
}

/// Parse the `[Desktop Entry]` group of a desktop file.
pub fn parse_desktop(text: &str, id: &str) -> Option<App> {
    let mut in_entry = false;
    let mut app = App {
        id: id.to_string(),
        name: String::new(),
        icon: String::new(),
        exec: String::new(),
        mime_types: Vec::new(),
        categories: Vec::new(),
        hidden: false,
    };
    let mut kind = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        let list = |v: &str| {
            v.split(';')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        match k {
            "Name" => app.name = v.to_string(),
            "Icon" => app.icon = v.to_string(),
            "Exec" => app.exec = v.to_string(),
            "Type" => kind = v.to_string(),
            "MimeType" => app.mime_types = list(v),
            "Categories" => app.categories = list(v),
            "NoDisplay" | "Hidden" if v == "true" => app.hidden = true,
            _ => {}
        }
    }
    (kind == "Application" && !app.name.is_empty()).then_some(app)
}

/// Application directories, highest precedence first.
pub fn app_dirs() -> Vec<PathBuf> {
    glaze_common::paths::data_dirs()
        .into_iter()
        .map(|d| d.join("applications"))
        .collect()
}

/// Every application in `dirs` (first id wins), sorted by name.
pub fn scan(dirs: &[PathBuf]) -> Vec<App> {
    let mut out: Vec<App> = Vec::new();
    for dir in dirs {
        walk(dir, dir, &mut out);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<App>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk(root, &p, out);
        } else if p.extension().is_some_and(|e| e == "desktop") {
            // Desktop file id: path relative to the root with '/' -> '-'.
            let id = p
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('/', "-"))
                .unwrap_or_default();
            if out.iter().any(|a| a.id == id) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Some(app) = parse_desktop(&text, &id) {
                    out.push(app);
                }
            }
        }
    }
}

/// A default-application role and the MIME types it sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Role {
    pub id: &'static str,
    pub title: &'static str,
    pub mimes: &'static [&'static str],
    /// Category fallback for apps that do not declare the MIME type.
    pub category: &'static str,
}

pub const ROLES: &[Role] = &[
    Role {
        id: "browser",
        title: "Web browser",
        mimes: &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
        ],
        category: "WebBrowser",
    },
    Role {
        id: "files",
        title: "File manager",
        mimes: &["inode/directory"],
        category: "FileManager",
    },
    Role {
        id: "editor",
        title: "Text editor",
        mimes: &["text/plain"],
        category: "TextEditor",
    },
    Role {
        id: "images",
        title: "Image viewer",
        mimes: &["image/png", "image/jpeg", "image/svg+xml"],
        category: "Viewer",
    },
    Role {
        id: "pdf",
        title: "Document viewer",
        mimes: &["application/pdf"],
        category: "Viewer",
    },
    Role {
        id: "mail",
        title: "Email",
        mimes: &["x-scheme-handler/mailto"],
        category: "Email",
    },
];

/// Apps that can fill a role (declared MIME type first).
pub fn candidates<'a>(apps: &'a [App], role: &Role) -> Vec<&'a App> {
    let primary = role.mimes[0];
    let mut v: Vec<&App> = apps
        .iter()
        .filter(|a| a.mime_types.iter().any(|m| m == primary))
        .collect();
    if v.is_empty() {
        v = apps
            .iter()
            .filter(|a| !a.hidden && a.categories.iter().any(|c| c == role.category))
            .collect();
    }
    v
}

/// Terminal emulators among the installed apps.
pub fn terminals(apps: &[App]) -> Vec<&App> {
    apps.iter()
        .filter(|a| a.categories.iter().any(|c| c == "TerminalEmulator"))
        .collect()
}

/// A `mimeapps.list` document kept as lines so unrelated content survives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MimeApps {
    lines: Vec<String>,
}

impl MimeApps {
    pub fn parse(text: &str) -> MimeApps {
        MimeApps {
            lines: text.lines().map(str::to_string).collect(),
        }
    }

    fn section_range(&self, name: &str) -> Option<(usize, usize)> {
        let header = format!("[{name}]");
        let start = self.lines.iter().position(|l| l.trim() == header)?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| l.trim().starts_with('['))
            .map(|i| start + 1 + i)
            .unwrap_or(self.lines.len());
        Some((start, end))
    }

    /// The first desktop id listed as default for `mime`.
    pub fn default_for(&self, mime: &str) -> Option<String> {
        let (s, e) = self.section_range("Default Applications")?;
        self.lines[s + 1..e].iter().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == mime)
                .then(|| {
                    v.split(';')
                        .map(str::trim)
                        .find(|x| !x.is_empty())
                        .map(str::to_string)
                })
                .flatten()
        })
    }

    /// Make `id` the default for `mime` (replacing the entry).
    pub fn set_default(&mut self, mime: &str, id: &str) {
        let line = format!("{mime}={id};");
        match self.section_range("Default Applications") {
            Some((s, e)) => {
                if let Some(i) = (s + 1..e).find(|&i| {
                    self.lines[i]
                        .split_once('=')
                        .is_some_and(|(k, _)| k.trim() == mime)
                }) {
                    self.lines[i] = line;
                } else {
                    // Insert before trailing blank lines of the section.
                    let mut at = e;
                    while at > s + 1 && self.lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    self.lines.insert(at, line);
                }
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push("[Default Applications]".into());
                self.lines.push(line);
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }
}

/// `mimeapps.list` files in XDG lookup order (user config first): for each of
/// `$XDG_CONFIG_HOME`, `$XDG_CONFIG_DIRS` (default `/etc/xdg`) and
/// `$XDG_DATA_DIRS/applications`, the desktop-specific `glaze-mimeapps.list`
/// before `mimeapps.list` (`XDG_CURRENT_DESKTOP=Glaze`).
pub fn mimeapps_files() -> Vec<PathBuf> {
    let config_dirs: Vec<PathBuf> = std::env::var("XDG_CONFIG_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.split(':')
                .filter(|d| !d.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    mimeapps_lookup(
        &glaze_common::paths::config_home(),
        &config_dirs,
        &glaze_common::paths::data_dirs(),
    )
}

/// The lookup list for explicit directories (see [`mimeapps_files`]). The
/// first entry is always the user's `mimeapps.list` (the file Settings writes).
pub fn mimeapps_lookup(
    config_home: &Path,
    config_dirs: &[PathBuf],
    data_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut v = vec![
        config_home.join("mimeapps.list"),
        config_home.join("glaze-mimeapps.list"),
    ];
    for d in config_dirs {
        v.push(d.join("glaze-mimeapps.list"));
        v.push(d.join("mimeapps.list"));
    }
    for d in data_dirs {
        let a = d.join("applications");
        v.push(a.join("glaze-mimeapps.list"));
        v.push(a.join("mimeapps.list"));
        v.push(a.join("defaults.list"));
    }
    v
}

/// The effective default for `mime` across all lookup files.
pub fn effective_default(files: &[PathBuf], mime: &str) -> Option<String> {
    files.iter().find_map(|f| {
        let text = std::fs::read_to_string(f).ok()?;
        MimeApps::parse(&text).default_for(mime)
    })
}

/// `mimeinfo.cache` files (built by update-desktop-database), in lookup order.
pub fn mimeinfo_caches() -> Vec<PathBuf> {
    let mut v = vec![glaze_common::paths::data_home().join("applications/mimeinfo.cache")];
    for d in glaze_common::paths::data_dirs() {
        v.push(d.join("applications/mimeinfo.cache"));
    }
    v
}

/// Apps listed for `mime` in a `mimeinfo.cache` text, in file order.
pub fn cache_entries(text: &str, mime: &str) -> Vec<String> {
    let prefix = format!("{mime}=");
    text.lines()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .map(|v| {
            v.split(';')
                .filter(|x| !x.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// What opens `mime` when no `mimeapps.list` names a default: like GIO, the
/// first installed app from the `mimeinfo.cache` files.
pub fn system_default(caches: &[PathBuf], mime: &str, apps: &[App]) -> Option<String> {
    caches.iter().find_map(|f| {
        let text = std::fs::read_to_string(f).ok()?;
        cache_entries(&text, mime)
            .into_iter()
            .find(|id| apps.iter().any(|a| a.id == *id))
    })
}

/// Set the user default for every MIME type of `role`.
pub fn set_role_default(user_file: &Path, role: &Role, id: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(user_file).unwrap_or_default();
    let mut m = MimeApps::parse(&text);
    for mime in role.mimes {
        m.set_default(mime, id);
    }
    crate::store::atomic_write(user_file, &m.to_text())
        .map_err(|e| format!("could not save {}: {e}", user_file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::tmp_dir;

    const FOOT: &str = "[Desktop Entry]\nType=Application\nName=Foot\nExec=foot\nIcon=foot\nCategories=System;TerminalEmulator;\n\n[Desktop Action new]\nName=New window\n";

    #[test]
    fn parses_desktop_entries() {
        let a = parse_desktop(FOOT, "foot.desktop").unwrap();
        assert_eq!(a.name, "Foot");
        assert_eq!(a.categories, vec!["System", "TerminalEmulator"]);
        assert!(!a.hidden);
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=x\n", "x").is_none());
        let h = parse_desktop(
            "[Desktop Entry]\nType=Application\nName=H\nNoDisplay=true\n",
            "h",
        )
        .unwrap();
        assert!(h.hidden);
    }

    #[test]
    fn mimeapps_lookup_order() {
        let v = mimeapps_lookup(
            Path::new("/h"),
            &[PathBuf::from("/etc/xdg")],
            &[PathBuf::from("/usr/share")],
        );
        assert_eq!(v[0], PathBuf::from("/h/mimeapps.list"));
        assert_eq!(v[2], PathBuf::from("/etc/xdg/glaze-mimeapps.list"));
        assert_eq!(v[3], PathBuf::from("/etc/xdg/mimeapps.list"));
        assert_eq!(
            v.last().unwrap(),
            &PathBuf::from("/usr/share/applications/defaults.list")
        );
    }

    #[test]
    fn mimeinfo_cache_order() {
        let text = "[MIME Cache]\ninode/directory=org.gnome.baobab.desktop;thunar.desktop;\ntext/plain=a.desktop;\n";
        assert_eq!(
            cache_entries(text, "inode/directory"),
            vec!["org.gnome.baobab.desktop", "thunar.desktop"]
        );
        assert!(cache_entries(text, "image/png").is_empty());
        assert!(cache_entries(text, "text/plai").is_empty());
    }

    #[test]
    fn scans_and_finds_candidates() {
        let d = tmp_dir("apps");
        std::fs::create_dir_all(d.join("kde")).unwrap();
        std::fs::write(d.join("foot.desktop"), FOOT).unwrap();
        std::fs::write(
            d.join("kde/browser.desktop"),
            "[Desktop Entry]\nType=Application\nName=Browser\nMimeType=text/html;x-scheme-handler/http;\n",
        )
        .unwrap();
        let apps = scan(std::slice::from_ref(&d));
        assert_eq!(apps.len(), 2);
        assert!(apps.iter().any(|a| a.id == "kde-browser.desktop"));
        let b = candidates(&apps, &ROLES[0]);
        assert_eq!(b.len(), 1);
        assert_eq!(terminals(&apps).len(), 1);
    }

    #[test]
    fn mimeapps_set_and_read_preserving_other_content() {
        let mut m = MimeApps::parse("# mine\n[Added Associations]\ntext/plain=a.desktop;\n\n[Default Applications]\ntext/plain=old.desktop;\n\n");
        assert_eq!(m.default_for("text/plain").as_deref(), Some("old.desktop"));
        m.set_default("text/plain", "new.desktop");
        m.set_default("inode/directory", "files.desktop");
        let t = m.to_text();
        assert!(t.contains("# mine"));
        assert!(t.contains("[Added Associations]\ntext/plain=a.desktop;"));
        let m2 = MimeApps::parse(&t);
        assert_eq!(m2.default_for("text/plain").as_deref(), Some("new.desktop"));
        assert_eq!(
            m2.default_for("inode/directory").as_deref(),
            Some("files.desktop")
        );
        let mut empty = MimeApps::parse("");
        empty.set_default("x/y", "z.desktop");
        assert_eq!(empty.default_for("x/y").as_deref(), Some("z.desktop"));
    }

    #[test]
    fn role_defaults_write_the_user_file() {
        let d = tmp_dir("mime");
        let f = d.join("mimeapps.list");
        set_role_default(&f, &ROLES[0], "firefox.desktop").unwrap();
        assert_eq!(
            effective_default(std::slice::from_ref(&f), "x-scheme-handler/https").as_deref(),
            Some("firefox.desktop")
        );
        assert_eq!(effective_default(&[d.join("none")], "text/html"), None);
    }
}
