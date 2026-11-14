//! Desktop icons and how they resolve to a real launch command.
//!
//! A [`DesktopIcon`] is the portable description of one item on the desktop
//! surface. Nothing here invents desktop contents: application icons are built
//! only from [`AppEntry`] records the registry really discovered, and a shortcut
//! that points at an application which is not installed produces **no** icon.
//!
//! [`resolve_target`] turns an icon into the argv with which it should be
//! launched — application icons go through `glaze-service`'s shell-free
//! `Exec` handling (`GLAZE.md` §4.1, §7.9), file/home/trash icons open through
//! `xdg-open`, and user shortcuts run their literal command.

use std::path::PathBuf;

use glaze_common::model::AppEntry;
use glaze_common::paths;
use glaze_service::launch::build_command;

use crate::shortcuts::Shortcut;

/// What double-clicking (or activating) a desktop icon should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconTarget {
    /// Launch an installed application by its desktop-file id.
    Application(String),
    /// Open a file or directory with the user's default application.
    File(PathBuf),
    /// Open the trash.
    Trash,
    /// Open the user's home directory.
    Home,
    /// Run a user-authored command (already tokenised; never a shell string).
    Custom {
        /// Literal argv. `cmd[0]` is the program.
        cmd: Vec<String>,
    },
}

/// One item on the desktop surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopIcon {
    /// Desktop-file id when this icon represents an installed application.
    pub app_id: Option<String>,
    /// Display label shown under the icon.
    pub label: String,
    /// Icon name or absolute path, resolved by the view layer.
    pub icon: Option<String>,
    /// What activation does.
    pub target: IconTarget,
}

impl DesktopIcon {
    /// A stable, unique key used by the selection model and persisted shortcuts.
    ///
    /// Two icons describing the same thing share a key; two icons describing
    /// different things never do. `/`-separated `kind` prefixes keep the
    /// namespaces distinct.
    pub fn key(&self) -> String {
        match &self.target {
            IconTarget::Application(id) => format!("app:{id}"),
            IconTarget::File(path) => format!("file:{}", path.display()),
            IconTarget::Trash => "trash".to_string(),
            IconTarget::Home => "home".to_string(),
            // Unit Separator keeps the key unique even when arguments contain
            // spaces or punctuation.
            IconTarget::Custom { cmd } => format!("cmd:\u{1f}{}", cmd.join("\u{1f}")),
        }
    }
}

/// Build the desktop icon list from real application entries and user shortcuts.
///
/// Order is deterministic: shortcuts in the order given, then the standard Home
/// and Trash icons when requested. Rules:
///
/// - A shortcut with an `app_id` becomes an application icon **only if** that id
///   exists in `apps`; an unknown id is skipped rather than faked.
/// - A shortcut with a non-empty `cmd` (and no `app_id`) becomes a
///   [`IconTarget::Custom`] icon.
/// - A shortcut with neither is ignored.
/// - An empty shortcut label falls back to the application name, or to the
///   program name for a custom command.
pub fn build_icons(
    apps: &[AppEntry],
    shortcuts: &[Shortcut],
    include_home: bool,
    include_trash: bool,
) -> Vec<DesktopIcon> {
    let mut icons = Vec::with_capacity(shortcuts.len() + 2);
    for s in shortcuts {
        if let Some(id) = s.app_id.as_deref() {
            let Some(app) = apps.iter().find(|a| a.id == id) else {
                continue; // Not installed — never fabricate an application.
            };
            icons.push(DesktopIcon {
                app_id: Some(id.to_string()),
                label: pick_label(&s.label, &app.name),
                icon: s.icon.clone().or_else(|| app.icon.clone()),
                target: IconTarget::Application(id.to_string()),
            });
        } else if !s.cmd.is_empty() {
            let fallback = s.cmd.first().cloned().unwrap_or_default();
            icons.push(DesktopIcon {
                app_id: None,
                label: pick_label(&s.label, &fallback),
                icon: s.icon.clone(),
                target: IconTarget::Custom { cmd: s.cmd.clone() },
            });
        }
    }
    if include_home {
        icons.push(DesktopIcon {
            app_id: None,
            label: "Home".to_string(),
            icon: Some("user-home".to_string()),
            target: IconTarget::Home,
        });
    }
    if include_trash {
        icons.push(DesktopIcon {
            app_id: None,
            label: "Trash".to_string(),
            icon: Some("user-trash".to_string()),
            target: IconTarget::Trash,
        });
    }
    icons
}

/// Prefer a non-empty caller label, else the given fallback.
fn pick_label(label: &str, fallback: &str) -> String {
    if label.trim().is_empty() {
        fallback.to_string()
    } else {
        label.to_string()
    }
}

/// The argv to execute when `icon` is activated, or `None` when it cannot be
/// resolved.
///
/// - [`IconTarget::Application`] — `Exec` handling via
///   [`glaze_service::launch::build_command`]; `None` if the app is not in
///   `apps` or its `Exec` is unusable.
/// - [`IconTarget::Custom`] — the literal argv (empty argv is unresolvable).
/// - [`IconTarget::File`] — `xdg-open <path>`.
/// - [`IconTarget::Home`] — `xdg-open <home>`; `None` if home is unknown.
/// - [`IconTarget::Trash`] — `xdg-open <XDG_DATA_HOME>/Trash`.
pub fn resolve_target(icon: &DesktopIcon, apps: &[AppEntry]) -> Option<Vec<String>> {
    match &icon.target {
        IconTarget::Application(id) => apps
            .iter()
            .find(|a| &a.id == id)
            .and_then(|app| build_command(app).ok()),
        IconTarget::Custom { cmd } => {
            if cmd.is_empty() || cmd[0].is_empty() {
                None
            } else {
                Some(cmd.clone())
            }
        }
        IconTarget::File(path) => Some(xdg_open(&path.to_string_lossy())),
        IconTarget::Home => paths::home_dir().map(|h| xdg_open(&h.to_string_lossy())),
        IconTarget::Trash => Some(xdg_open(
            &paths::data_home().join("Trash").to_string_lossy(),
        )),
    }
}

/// `[xdg-open, path]` — the portable way to open a URI or path with the user's
/// default application, without ever invoking a shell.
fn xdg_open(path: &str) -> Vec<String> {
    vec!["xdg-open".to_string(), path.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::Shortcut;

    fn app(id: &str, name: &str, icon: Option<&str>) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            comment: None,
            generic_name: None,
            exec: format!("/usr/bin/{id} %U"),
            try_exec: None,
            icon: icon.map(str::to_string),
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    fn app_shortcut(id: &str) -> Shortcut {
        Shortcut {
            key: format!("app:{id}"),
            app_id: Some(id.to_string()),
            ..Shortcut::default()
        }
    }

    #[test]
    fn builds_application_icon_from_real_entry() {
        let apps = vec![app("org.mozilla.firefox", "Firefox", Some("firefox"))];
        let icons = build_icons(&apps, &[app_shortcut("org.mozilla.firefox")], false, false);
        assert_eq!(icons.len(), 1);
        let i = &icons[0];
        assert_eq!(i.app_id.as_deref(), Some("org.mozilla.firefox"));
        assert_eq!(i.label, "Firefox"); // falls back to the real app name
        assert_eq!(i.icon.as_deref(), Some("firefox"));
        assert_eq!(
            i.target,
            IconTarget::Application("org.mozilla.firefox".into())
        );
    }

    #[test]
    fn unknown_application_shortcut_is_not_fabricated() {
        let apps = vec![app("real", "Real", None)];
        let icons = build_icons(&apps, &[app_shortcut("ghost.app")], false, false);
        assert!(
            icons.is_empty(),
            "a missing app must not appear on the desktop"
        );
    }

    #[test]
    fn shortcut_label_and_icon_override_the_app() {
        let apps = vec![app("editor", "Editor", Some("editor"))];
        let s = Shortcut {
            key: "app:editor".into(),
            app_id: Some("editor".into()),
            label: "My Editor".into(),
            icon: Some("custom-icon".into()),
            ..Shortcut::default()
        };
        let icons = build_icons(&apps, &[s], false, false);
        assert_eq!(icons[0].label, "My Editor");
        assert_eq!(icons[0].icon.as_deref(), Some("custom-icon"));
    }

    #[test]
    fn custom_command_shortcut_uses_program_name_as_label() {
        let s = Shortcut {
            key: "cmd:\u{1f}htop".into(),
            cmd: vec!["htop".into()],
            ..Shortcut::default()
        };
        let icons = build_icons(&[], &[s], false, false);
        assert_eq!(icons.len(), 1);
        assert_eq!(icons[0].label, "htop");
        assert_eq!(
            icons[0].target,
            IconTarget::Custom {
                cmd: vec!["htop".into()]
            }
        );
    }

    #[test]
    fn empty_shortcuts_are_ignored() {
        // Neither app_id nor cmd.
        let icons = build_icons(&[], &[Shortcut::default()], false, false);
        assert!(icons.is_empty());
        // Empty cmd is treated as no command.
        let s = Shortcut {
            key: "x".into(),
            cmd: vec![],
            ..Shortcut::default()
        };
        assert!(build_icons(&[], &[s], false, false).is_empty());
    }

    #[test]
    fn home_and_trash_are_appended_in_order() {
        let icons = build_icons(&[], &[], true, true);
        assert_eq!(icons.len(), 2);
        assert_eq!(icons[0].target, IconTarget::Home);
        assert_eq!(icons[1].target, IconTarget::Trash);
        assert_eq!(icons[0].label, "Home");
        assert_eq!(icons[1].label, "Trash");
        assert!(build_icons(&[], &[], false, false).is_empty());
    }

    #[test]
    fn build_icons_is_deterministic() {
        let apps = vec![app("a", "A", None), app("b", "B", None)];
        let shortcuts = vec![app_shortcut("a"), app_shortcut("b")];
        let first = build_icons(&apps, &shortcuts, true, true);
        let second = build_icons(&apps, &shortcuts, true, true);
        assert_eq!(first, second);
        // Shortcut order is preserved.
        assert_eq!(first[0].target, IconTarget::Application("a".into()));
        assert_eq!(first[1].target, IconTarget::Application("b".into()));
    }

    #[test]
    fn keys_are_stable_and_distinct() {
        let app_icon = DesktopIcon {
            app_id: Some("x".into()),
            label: "X".into(),
            icon: None,
            target: IconTarget::Application("x".into()),
        };
        assert_eq!(app_icon.key(), "app:x");
        assert_eq!(app_icon.key(), app_icon.key());

        let file = DesktopIcon {
            app_id: None,
            label: "f".into(),
            icon: None,
            target: IconTarget::File(PathBuf::from("/tmp/a b.txt")),
        };
        assert_eq!(file.key(), "file:/tmp/a b.txt");

        let custom = DesktopIcon {
            app_id: None,
            label: "c".into(),
            icon: None,
            target: IconTarget::Custom {
                cmd: vec!["echo".into(), "a b".into()],
            },
        };
        assert_ne!(custom.key(), file.key());
    }

    #[test]
    fn resolves_application_through_shell_free_exec() {
        let apps = vec![app("firefox", "Firefox", None)];
        let icon = DesktopIcon {
            app_id: Some("firefox".into()),
            label: "Firefox".into(),
            icon: None,
            target: IconTarget::Application("firefox".into()),
        };
        // `%U` is expanded away; the program is the real one, never `sh`.
        let argv = resolve_target(&icon, &apps).unwrap();
        assert_eq!(argv, vec!["/usr/bin/firefox"]);
    }

    #[test]
    fn unresolvable_application_returns_none() {
        let icon = DesktopIcon {
            app_id: Some("ghost".into()),
            label: "Ghost".into(),
            icon: None,
            target: IconTarget::Application("ghost".into()),
        };
        assert!(resolve_target(&icon, &[]).is_none());
    }

    #[test]
    fn broken_exec_is_not_launchable() {
        let mut broken = app("broken", "Broken", None);
        broken.exec = "   ".into();
        let icon = DesktopIcon {
            app_id: Some("broken".into()),
            label: "Broken".into(),
            icon: None,
            target: IconTarget::Application("broken".into()),
        };
        assert!(resolve_target(&icon, &[broken]).is_none());
    }

    #[test]
    fn resolves_custom_file_and_trash_targets() {
        let custom = DesktopIcon {
            app_id: None,
            label: "c".into(),
            icon: None,
            target: IconTarget::Custom {
                cmd: vec!["glaze-settings".into(), "--page".into(), "panel".into()],
            },
        };
        assert_eq!(
            resolve_target(&custom, &[]).unwrap(),
            vec!["glaze-settings", "--page", "panel"]
        );

        let file = DesktopIcon {
            app_id: None,
            label: "f".into(),
            icon: None,
            target: IconTarget::File(PathBuf::from("/home/u/report.pdf")),
        };
        assert_eq!(
            resolve_target(&file, &[]).unwrap(),
            vec!["xdg-open", "/home/u/report.pdf"]
        );

        let trash = DesktopIcon {
            app_id: None,
            label: "t".into(),
            icon: None,
            target: IconTarget::Trash,
        };
        let argv = resolve_target(&trash, &[]).unwrap();
        assert_eq!(argv[0], "xdg-open");
        assert!(
            argv[1].ends_with("Trash"),
            "unexpected trash path: {}",
            argv[1]
        );
    }

    #[test]
    fn resolves_home_to_xdg_open_of_home() {
        let home = DesktopIcon {
            app_id: None,
            label: "Home".into(),
            icon: None,
            target: IconTarget::Home,
        };
        let argv = resolve_target(&home, &[]).expect("home is determinable on every host");
        assert_eq!(argv[0], "xdg-open");
        assert_eq!(argv.len(), 2);
        assert!(!argv[1].is_empty());
    }

    #[test]
    fn empty_custom_command_is_not_launchable() {
        let icon = DesktopIcon {
            app_id: None,
            label: "c".into(),
            icon: None,
            target: IconTarget::Custom { cmd: vec![] },
        };
        assert!(resolve_target(&icon, &[]).is_none());
    }
}
