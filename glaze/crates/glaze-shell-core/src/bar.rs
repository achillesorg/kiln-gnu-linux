//! Taskbar v2 model (mockup `taskbar-v2`): pinned launchers and running task
//! buttons with grouping, label modes, click behaviour and pixel layout.
//!
//! Everything the GTK layer needs to *decide* lives here and is unit-tested
//! on the authoring host: which buttons exist (stable keys, so the animation
//! engine can tell an add from a move), their widths, x positions, labels,
//! counts, and what a click does. The GTK layer only draws and animates.

use glaze_common::config::{GroupingMode, LabelMode};
use glaze_common::model::{AppEntry, WindowInfo, WindowKind};

/// One window inside a task button (for the hover card).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWindow {
    pub id: String,
    pub title: String,
    pub focused: bool,
    pub minimized: bool,
}

/// One running-window button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskButton {
    /// Stable identity across refreshes: `app:<app_id>` for a group,
    /// `win:<id>` for a single ungrouped window.
    pub key: String,
    /// App id used for the icon, launching a new window and pinning.
    pub app_id: String,
    /// Text label (window title for one window, app name for a group).
    pub label: String,
    pub windows: Vec<TaskWindow>,
    /// Any window of the button is focused.
    pub active: bool,
    /// Every window of the button is minimised.
    pub minimized: bool,
    pub show_label: bool,
    /// Layout width in px.
    pub width: i32,
}

impl TaskButton {
    pub fn count(&self) -> usize {
        self.windows.len()
    }

    pub fn is_group(&self) -> bool {
        self.windows.len() > 1
    }
}

/// Sizes the layout needs (from the design tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarMetrics {
    pub task_width: i32,
    pub icon_width: i32,
    pub spacing: i32,
}

/// Human name for an app id, from the registry when known.
pub fn app_name(app_id: &str, apps: &[AppEntry]) -> String {
    find_app(app_id, apps)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| {
            let base = app_id.rsplit('.').next().unwrap_or(app_id);
            let mut c = base.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => app_id.to_string(),
            }
        })
}

/// Match a Wayland app id / X11 class to a registry entry: exact desktop id,
/// then case-insensitive id, then the last reverse-DNS segment, then the
/// executable name.
pub fn find_app<'a>(app_id: &str, apps: &'a [AppEntry]) -> Option<&'a AppEntry> {
    let lower = app_id.to_ascii_lowercase();
    apps.iter()
        .find(|a| a.id == app_id)
        .or_else(|| apps.iter().find(|a| a.id.to_ascii_lowercase() == lower))
        .or_else(|| {
            apps.iter().find(|a| {
                a.id.rsplit('.').next().map(|s| s.to_ascii_lowercase()) == Some(lower.clone())
            })
        })
        .or_else(|| {
            apps.iter().find(|a| {
                a.exec
                    .split_whitespace()
                    .next()
                    .and_then(|p| p.rsplit('/').next())
                    .map(|p| p.to_ascii_lowercase())
                    == Some(lower.clone())
            })
        })
        .or_else(|| {
            // Reverse-DNS app ids against dashed desktop ids:
            // `org.glaze.Settings` ~ `glaze-settings`, `org.kiln.Terminal` ~
            // `kiln-terminal` (compare the last two segments, letters only).
            let squash = |s: &str| -> String {
                s.chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase()
            };
            let segs: Vec<&str> = app_id.split('.').collect();
            let tail = if segs.len() >= 2 {
                squash(&segs[segs.len() - 2..].join(""))
            } else {
                squash(app_id)
            };
            if tail.len() < 4 {
                return None;
            }
            apps.iter().find(|a| squash(&a.id) == tail)
        })
}

/// The desktop id a window's app id resolves to (itself when unknown).
pub fn resolve_app_id(app_id: &str, apps: &[AppEntry]) -> String {
    find_app(app_id, apps)
        .map(|a| a.id.clone())
        .unwrap_or_else(|| app_id.to_string())
}

/// Windows-10-style taskbar: pinned launchers and running windows share
/// one button per app. Pinned apps come first in pin order (a pinned app
/// with no window is a launcher: `windows` empty); every window of a pinned
/// app joins its pin's button; unpinned apps follow in creation order,
/// grouped per `grouping`.
pub fn merged_buttons(
    windows: &[WindowInfo],
    apps: &[AppEntry],
    pins: &[String],
    grouping: GroupingMode,
    labels: LabelMode,
    m: &BarMetrics,
    available_px: i32,
) -> Vec<TaskButton> {
    let running = task_buttons(windows, apps, grouping, labels, m, 0);
    let mut out: Vec<TaskButton> = Vec::new();
    for pin in pins {
        let mut wins = Vec::new();
        for b in running.iter().filter(|b| &b.app_id == pin) {
            wins.extend(b.windows.iter().cloned());
        }
        let mut b = TaskButton {
            key: format!("app:{pin}"),
            app_id: pin.clone(),
            label: String::new(),
            windows: wins,
            active: false,
            minimized: false,
            show_label: false,
            width: 0,
        };
        b.active = b.windows.iter().any(|w| w.focused);
        b.minimized = !b.windows.is_empty() && b.windows.iter().all(|w| w.minimized);
        b.label = match b.windows.len() {
            1 => b.windows[0].title.clone(),
            _ => app_name(pin, apps),
        };
        b.show_label = !b.windows.is_empty() && label_visible(labels, b.active);
        b.width = button_width(b.show_label, m);
        out.push(b);
    }
    for b in running.into_iter().filter(|b| !pins.contains(&b.app_id)) {
        out.push(b);
    }
    if available_px > 0 && total_width(&out, m) > available_px {
        for b in out.iter_mut() {
            b.show_label = false;
            b.width = button_width(false, m);
        }
    }
    out
}

/// Narrowest a labelled task button gets before labels are dropped.
pub const MIN_LABEL_WIDTH: i32 = 88;

fn button_width(show_label: bool, m: &BarMetrics) -> i32 {
    if show_label {
        m.task_width
    } else {
        m.icon_width
    }
}

fn label_visible(mode: LabelMode, active: bool) -> bool {
    match mode {
        LabelMode::Full => true,
        LabelMode::IconOnly => false,
        LabelMode::ActiveOnly => active,
    }
}

/// Build the running-window buttons.
///
/// `available_px` is the width of the task section (0 = unknown, no
/// shrinking). In `auto` grouping, windows stay separate until they would not
/// fit, then same-app windows are grouped. When even that does not fit,
/// labels are dropped (active button last) so state is never hidden.
pub fn task_buttons(
    windows: &[WindowInfo],
    apps: &[AppEntry],
    grouping: GroupingMode,
    labels: LabelMode,
    m: &BarMetrics,
    available_px: i32,
) -> Vec<TaskButton> {
    let mut live: Vec<&WindowInfo> = windows
        .iter()
        .filter(|w| w.kind != WindowKind::Internal)
        .collect();
    // Stable order: the compositor lists floating windows in stacking order,
    // which changes on every focus change. Buttons keep creation order
    // (numeric window ids grow monotonically on Sway); non-numeric ids keep
    // their reported order after the numeric ones.
    live.sort_by_key(|w| w.id.parse::<u64>().unwrap_or(u64::MAX));
    let mut buttons = match grouping {
        GroupingMode::Always => build(&live, apps, true, labels, m),
        GroupingMode::Never => build(&live, apps, false, labels, m),
        GroupingMode::Auto => {
            let separate = build(&live, apps, false, labels, m);
            if available_px > 0 && total_width(&separate, m) > available_px {
                build(&live, apps, true, labels, m)
            } else {
                separate
            }
        }
    };
    if available_px > 0 && total_width(&buttons, m) > available_px {
        // Windows 2000: first shrink every labelled button evenly (labels
        // ellipsize), down to MIN_LABEL_WIDTH.
        let n = buttons.len() as i32;
        let icons: i32 = buttons
            .iter()
            .filter(|b| !b.show_label)
            .map(|b| b.width)
            .sum();
        let labelled = buttons.iter().filter(|b| b.show_label).count() as i32;
        if labelled > 0 {
            let even = (available_px - icons - m.spacing * (n - 1)) / labelled;
            if even >= MIN_LABEL_WIDTH.min(m.task_width) {
                for b in buttons.iter_mut().filter(|b| b.show_label) {
                    b.width = even.min(m.task_width);
                }
                return buttons;
            }
        }
        // Drop labels on inactive buttons, then on the active one.
        for b in buttons.iter_mut().filter(|b| !b.active) {
            b.show_label = false;
            b.width = button_width(false, m);
        }
        if total_width(&buttons, m) > available_px {
            for b in buttons.iter_mut() {
                b.show_label = false;
                b.width = button_width(false, m);
            }
        }
    }
    buttons
}

fn build(
    live: &[&WindowInfo],
    apps: &[AppEntry],
    group: bool,
    labels: LabelMode,
    m: &BarMetrics,
) -> Vec<TaskButton> {
    let mut out: Vec<TaskButton> = Vec::new();
    for w in live {
        let app_id = w
            .app_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|a| resolve_app_id(a, apps));
        let tw = TaskWindow {
            id: w.id.clone(),
            title: if w.title.trim().is_empty() {
                app_id.clone().unwrap_or_else(|| w.id.clone())
            } else {
                w.title.trim().to_string()
            },
            focused: w.focused,
            minimized: w.minimized,
        };
        let key = match (&app_id, group) {
            (Some(a), true) => format!("app:{a}"),
            _ => format!("win:{}", w.id),
        };
        if let Some(b) = out.iter_mut().find(|b| b.key == key) {
            b.windows.push(tw);
            continue;
        }
        out.push(TaskButton {
            key,
            app_id: app_id.clone().unwrap_or_else(|| w.id.clone()),
            label: String::new(),
            windows: vec![tw],
            active: false,
            minimized: false,
            show_label: false,
            width: 0,
        });
    }
    for b in &mut out {
        b.active = b.windows.iter().any(|w| w.focused);
        b.minimized = b.windows.iter().all(|w| w.minimized);
        b.label = if b.is_group() {
            app_name(&b.app_id, apps)
        } else {
            b.windows[0].title.clone()
        };
        b.show_label = label_visible(labels, b.active);
        b.width = button_width(b.show_label, m);
    }
    out
}

/// Total width including spacing.
pub fn total_width(buttons: &[TaskButton], m: &BarMetrics) -> i32 {
    let w: i32 = buttons.iter().map(|b| b.width).sum();
    w + m.spacing * (buttons.len() as i32 - 1).max(0)
}

/// Target x for each button (left to right).
pub fn positions(buttons: &[TaskButton], m: &BarMetrics) -> Vec<(String, i32, i32)> {
    let mut x = 0;
    buttons
        .iter()
        .map(|b| {
            let r = (b.key.clone(), x, b.width);
            x += b.width + m.spacing;
            r
        })
        .collect()
}

/// What a primary click on a task button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAction {
    /// A pinned launcher with no window: start the app.
    Launch(String),
    Focus(String),
    Restore(Vec<String>),
    Minimize(Vec<String>),
}

/// Click rules: click focuses; click on the active button minimises (all its
/// visible windows); click on a fully minimised button restores it (all its
/// windows, the first one focused last so it ends on top).
pub fn click_action(b: &TaskButton) -> ClickAction {
    if b.windows.is_empty() {
        return ClickAction::Launch(b.app_id.clone());
    }
    if b.minimized {
        return ClickAction::Restore(b.windows.iter().map(|w| w.id.clone()).collect());
    }
    if b.active {
        return ClickAction::Minimize(
            b.windows
                .iter()
                .filter(|w| !w.minimized)
                .map(|w| w.id.clone())
                .collect(),
        );
    }
    let target = b
        .windows
        .iter()
        .find(|w| !w.minimized)
        .unwrap_or(&b.windows[0]);
    if target.minimized {
        ClickAction::Restore(vec![target.id.clone()])
    } else {
        ClickAction::Focus(target.id.clone())
    }
}

/// Category preferences for the default pins: file manager, terminal, browser.
const DEFAULT_PIN_CATEGORIES: &[(&str, &[&str])] = &[
    (
        "WebBrowser",
        &["firefox", "firefox-esr", "chromium", "epiphany"],
    ),
    (
        "FileManager",
        &["thunar", "org.gnome.Nautilus", "pcmanfm", "nemo"],
    ),
    (
        "TerminalEmulator",
        &["foot", "org.gnome.Console", "kitty", "alacritty"],
    ),
    ("Settings", &["glaze-settings"]),
];

/// Pins to show: the user's list, or (when empty and `default_pins`) a
/// browser, a file manager, a terminal and Glaze Settings picked from the
/// installed apps.
pub fn effective_pins(pinned: &[String], default_pins: bool, apps: &[AppEntry]) -> Vec<String> {
    if !pinned.is_empty() || !default_pins {
        let mut seen = Vec::new();
        for p in pinned {
            let p = p.trim().to_string();
            if !p.is_empty() && !seen.contains(&p) {
                seen.push(p);
            }
        }
        return seen;
    }
    let mut out = Vec::new();
    for (category, preferred) in DEFAULT_PIN_CATEGORIES {
        let pick = preferred
            .iter()
            .find_map(|id| apps.iter().find(|a| a.id == *id && !a.no_display))
            .or_else(|| {
                if *category == "Settings" {
                    return None;
                }
                apps.iter()
                    .find(|a| !a.no_display && a.categories.iter().any(|c| c == category))
            });
        if let Some(app) = pick {
            if !out.contains(&app.id) {
                out.push(app.id.clone());
            }
        }
    }
    out
}

/// Toggle a pin in an explicit list (used by the context menu, which then
/// writes the list to the user config).
pub fn toggle_pin(current: &[String], app_id: &str) -> Vec<String> {
    let mut v: Vec<String> = current.to_vec();
    if let Some(i) = v.iter().position(|p| p == app_id) {
        v.remove(i);
    } else {
        v.push(app_id.to_string());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: &str, app: &str, title: &str, focused: bool, minimized: bool) -> WindowInfo {
        WindowInfo {
            id: id.into(),
            app_id: Some(app.into()),
            title: title.into(),
            workspace: Some(0),
            kind: WindowKind::Normal,
            focused,
            minimized,
            floating: true,
        }
    }

    fn app(id: &str, name: &str, cats: &[&str]) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: None,
            generic_name: None,
            exec: format!("/usr/bin/{id}"),
            try_exec: None,
            icon: None,
            categories: cats.iter().map(|c| c.to_string()).collect(),
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    const M: BarMetrics = BarMetrics {
        task_width: 128,
        icon_width: 36,
        spacing: 8,
    };

    fn sample() -> Vec<WindowInfo> {
        vec![
            win("1", "foot", "Terminal", true, false),
            win("2", "nautilus", "Documents", false, false),
            win("3", "nautilus", "Downloads", false, false),
            win("4", "firefox", "Browser", false, true),
        ]
    }

    #[test]
    fn always_groups_with_counts_and_stable_keys() {
        let apps = [app("nautilus", "Files", &["FileManager"])];
        let b = task_buttons(
            &sample(),
            &apps,
            GroupingMode::Always,
            LabelMode::Full,
            &M,
            0,
        );
        let keys: Vec<&str> = b.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["app:foot", "app:nautilus", "app:firefox"]);
        assert_eq!(b[1].count(), 2);
        assert_eq!(b[1].label, "Files", "group label is the app name");
        assert_eq!(b[0].label, "Terminal", "single window label is its title");
        assert!(b[0].active && !b[1].active);
        assert!(b[2].minimized);
        assert!(b.iter().all(|b| b.width == 128 && b.show_label));
    }

    #[test]
    fn order_is_creation_order_not_stacking_order() {
        let mut ws = sample();
        // Stacking order after focusing window 1 last: it is reported last.
        ws.rotate_left(1);
        let b = task_buttons(&ws, &[], GroupingMode::Never, LabelMode::Full, &M, 0);
        let keys: Vec<&str> = b.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["win:1", "win:2", "win:3", "win:4"]);
    }

    #[test]
    fn never_gives_one_button_per_window() {
        let b = task_buttons(&sample(), &[], GroupingMode::Never, LabelMode::Full, &M, 0);
        assert_eq!(b.len(), 4);
        assert_eq!(b[1].key, "win:2");
        assert!(b.iter().all(|b| b.count() == 1));
    }

    #[test]
    fn auto_groups_only_when_it_would_not_fit() {
        let roomy = task_buttons(
            &sample(),
            &[],
            GroupingMode::Auto,
            LabelMode::Full,
            &M,
            1000,
        );
        assert_eq!(roomy.len(), 4);
        // 4*128+3*8 = 536 > 450, grouped = 3*128+2*8 = 400 fits.
        let tight = task_buttons(&sample(), &[], GroupingMode::Auto, LabelMode::Full, &M, 450);
        assert_eq!(tight.len(), 3);
        assert!(tight.iter().all(|b| b.show_label));
    }

    #[test]
    fn labels_drop_inactive_first_when_space_runs_out() {
        let b = task_buttons(
            &sample(),
            &[],
            GroupingMode::Always,
            LabelMode::Full,
            &M,
            260,
        );
        let active = b.iter().find(|b| b.active).unwrap();
        assert!(active.show_label);
        assert!(b.iter().filter(|b| !b.active).all(|b| !b.show_label));
        assert!(total_width(&b, &M) <= 260);
        let tiny = task_buttons(
            &sample(),
            &[],
            GroupingMode::Always,
            LabelMode::Full,
            &M,
            100,
        );
        assert!(tiny.iter().all(|b| !b.show_label));
    }

    #[test]
    fn buttons_shrink_evenly_before_labels_drop() {
        // 3 buttons * 128 + 16 = 400 > 340: even width (340-16)/3 = 108.
        let b = task_buttons(
            &sample(),
            &[],
            GroupingMode::Always,
            LabelMode::Full,
            &M,
            340,
        );
        assert!(b.iter().all(|b| b.show_label && b.width == 108));
        assert!(total_width(&b, &M) <= 340);
    }

    #[test]
    fn label_modes() {
        let icon = task_buttons(
            &sample(),
            &[],
            GroupingMode::Always,
            LabelMode::IconOnly,
            &M,
            0,
        );
        assert!(icon.iter().all(|b| !b.show_label && b.width == 36));
        let act = task_buttons(
            &sample(),
            &[],
            GroupingMode::Always,
            LabelMode::ActiveOnly,
            &M,
            0,
        );
        assert_eq!(act.iter().filter(|b| b.show_label).count(), 1);
        assert!(act[0].show_label);
    }

    #[test]
    fn positions_accumulate_width_and_spacing() {
        let b = task_buttons(&sample(), &[], GroupingMode::Always, LabelMode::Full, &M, 0);
        let p = positions(&b, &M);
        assert_eq!(p[0].1, 0);
        assert_eq!(p[1].1, 136);
        assert_eq!(p[2].1, 272);
        assert_eq!(total_width(&b, &M), 400);
    }

    #[test]
    fn click_focuses_minimises_and_restores() {
        let b = task_buttons(&sample(), &[], GroupingMode::Always, LabelMode::Full, &M, 0);
        assert_eq!(click_action(&b[0]), ClickAction::Minimize(vec!["1".into()]));
        assert_eq!(click_action(&b[1]), ClickAction::Focus("2".into()));
        assert_eq!(click_action(&b[2]), ClickAction::Restore(vec!["4".into()]));
        // Active group minimises all its visible windows.
        let mut ws = sample();
        ws[2].focused = true;
        ws[0].focused = false;
        let b = task_buttons(&ws, &[], GroupingMode::Always, LabelMode::Full, &M, 0);
        assert_eq!(
            click_action(&b[1]),
            ClickAction::Minimize(vec!["2".into(), "3".into()])
        );
    }

    #[test]
    fn internal_windows_are_skipped_and_empty_titles_fall_back() {
        let mut ws = vec![win("9", "foot", "  ", false, false)];
        ws.push(WindowInfo {
            kind: WindowKind::Internal,
            ..win("10", "glaze-shell", "panel", false, false)
        });
        let b = task_buttons(&ws, &[], GroupingMode::Always, LabelMode::Full, &M, 0);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].label, "foot");
    }

    #[test]
    fn default_pins_pick_file_manager_terminal_browser() {
        let apps = vec![
            app("org.gnome.Nautilus", "Files", &["FileManager"]),
            app("vim", "Vim", &["TextEditor"]),
            app("foot", "Foot", &["TerminalEmulator"]),
            app("my-browser", "Web", &["Network", "WebBrowser"]),
        ];
        assert_eq!(
            effective_pins(&[], true, &apps),
            vec!["my-browser", "org.gnome.Nautilus", "foot"]
        );
        let mut with_settings = apps.clone();
        with_settings.push(app("glaze-settings", "Glaze Settings", &["Settings"]));
        with_settings.push(app("firefox-esr", "Firefox ESR", &["WebBrowser"]));
        assert_eq!(
            effective_pins(&[], true, &with_settings),
            vec!["firefox-esr", "org.gnome.Nautilus", "foot", "glaze-settings"]
        );
        assert!(effective_pins(&[], false, &apps).is_empty());
        let user = vec!["vim".to_string(), "vim".to_string(), " ".to_string()];
        assert_eq!(effective_pins(&user, true, &apps), vec!["vim"]);
        assert_eq!(toggle_pin(&["a".into()], "b"), vec!["a", "b"]);
        assert_eq!(toggle_pin(&["a".into(), "b".into()], "a"), vec!["b"]);
    }

    #[test]
    fn pinned_and_running_merge_into_one_button() {
        let apps = vec![
            app("glaze-settings", "Glaze Settings", &["Settings"]),
            app("thunar", "Files", &["FileManager"]),
            app("foot", "Foot", &[]),
        ];
        let ws = vec![
            win("5", "org.glaze.Settings", "Settings", true, false),
            win("6", "foot", "a", false, false),
        ];
        let pins = vec!["thunar".to_string(), "glaze-settings".to_string()];
        let b = merged_buttons(&ws, &apps, &pins, GroupingMode::Always, LabelMode::IconOnly, &M, 0);
        let keys: Vec<&str> = b.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["app:thunar", "app:glaze-settings", "app:foot"]);
        assert!(b[0].windows.is_empty());
        assert_eq!(click_action(&b[0]), ClickAction::Launch("thunar".into()));
        assert_eq!(b[1].count(), 1, "the Settings window joined its pin");
        assert!(b[1].active);
    }

    #[test]
    fn app_matching_is_forgiving() {
        let apps = vec![
            app("org.gnome.Nautilus", "Files", &[]),
            app("foot", "Foot", &[]),
        ];
        assert_eq!(app_name("org.gnome.Nautilus", &apps), "Files");
        assert_eq!(app_name("nautilus", &apps), "Files");
        assert_eq!(app_name("FOOT", &apps), "Foot");
        assert_eq!(app_name("org.example.thing", &apps), "Thing");
    }
}
