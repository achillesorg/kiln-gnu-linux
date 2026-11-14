//! The panel **view model**: one render-ready structure assembled from every
//! piece of shell logic, so the GTK layer in `apps/glaze-shell` is a thin binder
//! and the interesting decisions stay testable on any host.
//!
//! [`build_panel_view`] composes, in `GLAZE.md` §6.1 order:
//! start → pinned/favourites → running windows → spacer → tray → clock →
//! optional show-desktop. It merges real window state through
//! [`crate::taskbar::merge_tasks`] (never inventing windows), formats the clock
//! from injected time, and resolves tray overflow and the notification badge.
//!
//! Label strings are returned as **translation keys** (`&'static str`) so this
//! crate need not depend on `glaze-i18n`; the GTK layer calls `glaze_i18n::tr`.

use glaze_common::config::GlazeConfig;
use glaze_common::model::{AppEntry, WindowInfo};

use crate::clock::{format_clock, ClockParts};
use crate::notifications::NotificationBadge;
use crate::panel::{
    layout, task_display_mode, PanelItem, TaskDisplayMode, TASK_ICON_WIDTH_PX, TASK_LABEL_WIDTH_PX,
};
use crate::taskbar::{merge_tasks, should_show_label, RunningPolicy, TaskEntry};
use crate::tasks::group_windows;
use crate::tray::{overflow, TrayModel, TRAY_ITEM_WIDTH_PX};

/// Translation key for the start button (`glaze-i18n` `panel.start`).
pub const KEY_START: &str = "panel.start";
/// Translation key for the show-desktop affordance (`panel.show_desktop`).
pub const KEY_SHOW_DESKTOP: &str = "panel.show_desktop";

/// One taskbar button, flattened for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskView {
    pub app_id: String,
    pub label: String,
    /// Present in `launcher.pinned`.
    pub pinned: bool,
    /// A live window group is open.
    pub running: bool,
    /// The focused window belongs to this entry.
    pub focused: bool,
    pub window_count: usize,
    /// Live backend window ids, so the view can focus a real window on click
    /// (empty when the entry is a pinned shortcut that is not open).
    pub window_ids: Vec<String>,
    /// Whether the label should be drawn at the initial width budget.
    pub show_label: bool,
}

impl TaskView {
    /// True when the button should draw an "active" affordance.
    pub fn is_active(&self) -> bool {
        self.running || self.focused
    }

    /// Label visibility at a given per-button width budget.
    ///
    /// Same rule as [`crate::taskbar::should_show_label`] expressed on the view
    /// type: a **focused** button — the window the user is actually in — keeps
    /// its label down to the icon budget; every other button needs the full
    /// label budget. Hiding a label never hides the *state*: `pinned` /
    /// `running` / `focused` remain conveyed by flags and CSS.
    pub fn show_label_at(&self, available_px: i32, label_wanted: bool) -> bool {
        if !label_wanted {
            return false;
        }
        let available = available_px.max(0);
        if self.focused {
            available >= TASK_ICON_WIDTH_PX
        } else {
            available >= TASK_LABEL_WIDTH_PX
        }
    }
}

/// One tray item, flattened for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItemView {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    /// The item is asking for attention (draw a highlight).
    pub attention: bool,
}

/// A zone in the rendered panel, in draw order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelItemView {
    Start {
        label_key: &'static str,
        launcher_open: bool,
    },
    Pinned {
        entries: Vec<TaskView>,
    },
    Task {
        entries: Vec<TaskView>,
        display: TaskDisplayMode,
    },
    Spacer,
    Tray {
        items: Vec<TrayItemView>,
        overflow_count: usize,
        badge: Option<String>,
        attention: usize,
    },
    Clock {
        time: String,
        date: Option<String>,
    },
    ShowDesktop {
        label_key: &'static str,
    },
}

/// The complete, render-ready panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelView {
    pub items: Vec<PanelItemView>,
}

impl PanelView {
    /// Borrow the taskbar entries from whichever zone holds them.
    pub fn task_entries(&self) -> impl Iterator<Item = &TaskView> {
        self.items.iter().flat_map(|item| match item {
            PanelItemView::Pinned { entries } | PanelItemView::Task { entries, .. } => {
                entries.as_slice()
            }
            _ => &[],
        })
    }

    /// The clock text, when the panel has a clock zone.
    pub fn clock(&self) -> Option<(&str, Option<&str>)> {
        self.items.iter().find_map(|item| match item {
            PanelItemView::Clock { time, date } => Some((time.as_str(), date.as_deref())),
            _ => None,
        })
    }
}

/// Everything [`build_panel_view`] needs. Grouped so callers read clearly.
pub struct PanelInputs<'a> {
    pub cfg: &'a GlazeConfig,
    /// The application registry, used to show real names for pinned shortcuts.
    pub apps: &'a [AppEntry],
    /// Live windows from the compositor backend (empty when none is attached).
    pub windows: &'a [WindowInfo],
    /// Whether live windows may be trusted (`Merge`) or pins only.
    pub running_policy: RunningPolicy,
    pub tray: &'a TrayModel,
    pub badge: &'a NotificationBadge,
    /// The current time; injected so the view is deterministic in tests.
    pub clock_parts: &'a ClockParts,
    /// Width budget for the task zone, in logical px.
    pub available_task_px: i32,
    /// Width budget for the tray zone, in logical px.
    pub available_tray_px: i32,
    /// Whether the launcher is currently open (drives the start button state).
    pub launcher_open: bool,
}

/// Build the render-ready panel from configuration and live state.
pub fn build_panel_view(inputs: PanelInputs<'_>) -> PanelView {
    let groups = group_windows(inputs.windows);
    let merged = merge_tasks(&inputs.cfg.launcher.pinned, &groups, inputs.running_policy);

    // Split the merged list into the two zones GLAZE.md §6.1 describes:
    //   Pinned = pure shortcuts (not open)   Task = live windows (incl. pinned+running)
    let (pinned_entries, task_entries): (Vec<TaskEntry>, Vec<TaskEntry>) = merged
        .into_iter()
        .partition(|entry| entry.pinned && !entry.running);

    let pinned = pinned_entries
        .into_iter()
        .map(|e| to_task_view(e, inputs.apps, inputs.available_task_px, inputs.cfg))
        .collect::<Vec<_>>();

    let task_view = task_entries
        .iter()
        .map(|e| to_task_view(e.clone(), inputs.apps, inputs.available_task_px, inputs.cfg))
        .collect::<Vec<_>>();
    let display = task_display_mode(
        inputs.available_task_px,
        task_view.len(),
        inputs.cfg.panel.show_labels,
    );

    let tray_items: Vec<TrayItemView> = inputs
        .tray
        .iter()
        .map(|item| TrayItemView {
            id: item.id.clone(),
            title: item.title.clone(),
            icon: item.icon.clone(),
            attention: item.needs_attention(),
        })
        .collect();
    let overflow_count = overflow(
        tray_items.len(),
        inputs.available_tray_px,
        TRAY_ITEM_WIDTH_PX,
    );

    let clock = format_clock(inputs.clock_parts, &inputs.cfg.panel);

    let mut items: Vec<PanelItemView> = Vec::new();
    for zone in layout(inputs.cfg) {
        match zone {
            PanelItem::Start => items.push(PanelItemView::Start {
                label_key: KEY_START,
                launcher_open: inputs.launcher_open,
            }),
            PanelItem::Pinned => items.push(PanelItemView::Pinned {
                entries: pinned.clone(),
            }),
            PanelItem::Task => items.push(PanelItemView::Task {
                entries: task_view.clone(),
                display,
            }),
            PanelItem::Spacer => items.push(PanelItemView::Spacer),
            PanelItem::Tray => items.push(PanelItemView::Tray {
                items: tray_items.clone(),
                overflow_count,
                badge: inputs.badge.badge_text(),
                attention: inputs.tray.needs_attention(),
            }),
            PanelItem::Clock => items.push(PanelItemView::Clock {
                time: clock.time.clone(),
                date: clock.date.clone(),
            }),
            PanelItem::ShowDesktop => items.push(PanelItemView::ShowDesktop {
                label_key: KEY_SHOW_DESKTOP,
            }),
        }
    }

    PanelView { items }
}

/// The whole taskbar strip, as the panel draws it: pinned shortcuts followed by
/// live window groups, in zone order. The panel renders the Pinned and Task
/// zones as **one** strip, so it must take its entries from here (or
/// [`PanelView::task_entries`]) and not from the first zone alone, which would
/// drop every running window. Also used to refresh the strip when the
/// compositor's window list changes.
pub fn taskbar_entries(
    cfg: &GlazeConfig,
    apps: &[AppEntry],
    windows: &[WindowInfo],
    running_policy: RunningPolicy,
    available_task_px: i32,
) -> Vec<TaskView> {
    let view = build_panel_view(PanelInputs {
        cfg,
        apps,
        windows,
        running_policy,
        tray: &TrayModel::new(),
        badge: &NotificationBadge::new(),
        clock_parts: &ClockParts::new(1970, 1, 1, 0, 0, 0),
        available_task_px,
        available_tray_px: 0,
        launcher_open: false,
    });
    view.task_entries().cloned().collect()
}

fn to_task_view(
    entry: TaskEntry,
    apps: &[AppEntry],
    available_px: i32,
    cfg: &GlazeConfig,
) -> TaskView {
    // A pinned shortcut has no live title; prefer the registry's real name.
    let label = if entry.running {
        entry.label.clone()
    } else {
        registry_name(&entry.app_id, apps).unwrap_or_else(|| entry.label.clone())
    };
    let show_label = should_show_label(&entry, available_px, cfg.panel.show_labels);
    let window_ids = entry.window_ids.clone();
    let window_count = window_ids.len();
    TaskView {
        app_id: entry.app_id,
        label,
        pinned: entry.pinned,
        running: entry.running,
        focused: entry.focused,
        window_count,
        window_ids,
        show_label,
    }
}

/// The registry display name for a desktop id, if it is installed.
fn registry_name(app_id: &str, apps: &[AppEntry]) -> Option<String> {
    apps.iter().find(|a| a.id == app_id).map(|a| a.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::config::{ColorScheme, ThemeKind};
    use glaze_common::model::{WindowInfo, WindowKind};

    use crate::tray::TrayItem;
    use glaze_service::tray::{TrayCategory, TrayStatus};

    fn app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: None,
            generic_name: None,
            exec: format!("{id} %u"),
            try_exec: None,
            icon: Some(id.into()),
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    fn window(id: &str, app_id: &str, title: &str, focused: bool) -> WindowInfo {
        WindowInfo {
            id: id.into(),
            app_id: Some(app_id.into()),
            title: title.into(),
            workspace: Some(0),
            kind: WindowKind::Normal,
            focused,
            minimized: false,
            floating: false,
        }
    }

    fn clock() -> ClockParts {
        ClockParts::new(2026, 11, 11, 14, 5, 9)
    }

    #[test]
    fn taskbar_entries_keep_running_windows_next_to_pins() {
        let f = fixture();
        let windows = [window("7", "foot", "fede@kilnlinux: ~", true)];
        let strip = taskbar_entries(&f.cfg, &f.apps, &windows, RunningPolicy::Merge, 1200);
        let ids: Vec<&str> = strip.iter().map(|t| t.app_id.as_str()).collect();
        assert_eq!(ids, ["org.mozilla.firefox", "glaze-settings", "foot"]);
        let foot = strip.last().unwrap();
        assert!(foot.running && foot.focused && !foot.pinned);
        assert_eq!(foot.window_ids, ["7"]);
        // Pins only: no window is invented.
        let pins = taskbar_entries(&f.cfg, &f.apps, &windows, RunningPolicy::PinnedOnly, 1200);
        assert!(pins.iter().all(|t| !t.running));
    }

    struct Fixture {
        cfg: GlazeConfig,
        apps: Vec<AppEntry>,
        tray: TrayModel,
        badge: NotificationBadge,
    }

    fn fixture() -> Fixture {
        let mut cfg = GlazeConfig::default();
        cfg.appearance.theme = ThemeKind::Clay;
        cfg.appearance.scheme = ColorScheme::Light;
        cfg.launcher.pinned = vec!["org.mozilla.firefox".into(), "glaze-settings".into()];
        let apps = vec![
            app("org.mozilla.firefox", "Firefox"),
            app("glaze-settings", "Glaze Settings"),
        ];
        Fixture {
            cfg,
            apps,
            tray: TrayModel::new(),
            badge: NotificationBadge::new(),
        }
    }

    fn build<'a>(
        f: &'a Fixture,
        windows: &'a [WindowInfo],
        policy: RunningPolicy,
        tray: &'a TrayModel,
        badge: &'a NotificationBadge,
        parts: &'a ClockParts,
        launcher_open: bool,
    ) -> PanelView {
        build_panel_view(PanelInputs {
            cfg: &f.cfg,
            apps: &f.apps,
            windows,
            running_policy: policy,
            tray,
            badge,
            clock_parts: parts,
            available_task_px: 900,
            available_tray_px: 120,
            launcher_open,
        })
    }

    fn pinned_zone(view: &PanelView) -> &Vec<TaskView> {
        view.items
            .iter()
            .find_map(|i| match i {
                PanelItemView::Pinned { entries } => Some(entries),
                _ => None,
            })
            .expect("pinned zone")
    }

    fn task_zone(view: &PanelView) -> &Vec<TaskView> {
        view.items
            .iter()
            .find_map(|i| match i {
                PanelItemView::Task { entries, .. } => Some(entries),
                _ => None,
            })
            .expect("task zone")
    }

    #[test]
    fn zones_are_in_documented_order() {
        let f = fixture();
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let tags: Vec<&str> = view
            .items
            .iter()
            .map(|i| match i {
                PanelItemView::Start { .. } => "start",
                PanelItemView::Pinned { .. } => "pinned",
                PanelItemView::Task { .. } => "task",
                PanelItemView::Spacer => "spacer",
                PanelItemView::Tray { .. } => "tray",
                PanelItemView::Clock { .. } => "clock",
                PanelItemView::ShowDesktop { .. } => "show-desktop",
            })
            .collect();
        assert_eq!(
            tags,
            vec!["start", "pinned", "task", "spacer", "tray", "clock"]
        );
    }

    #[test]
    fn start_zone_carries_the_launcher_state() {
        let f = fixture();
        let closed = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let open = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            true,
        );
        assert!(matches!(
            closed.items[0],
            PanelItemView::Start {
                launcher_open: false,
                label_key: KEY_START
            }
        ));
        assert!(matches!(
            open.items[0],
            PanelItemView::Start {
                launcher_open: true,
                ..
            }
        ));
    }

    #[test]
    fn pinned_only_policy_hides_live_windows() {
        let f = fixture();
        let wins = vec![window("w1", "org.gnome.Terminal", "Terminal", true)];
        let view = build(
            &f,
            &wins,
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let tasks = task_zone(&view);
        assert!(tasks.is_empty(), "PinnedOnly must not show live windows");
        // Both pins are present as pure shortcuts.
        let pins = pinned_zone(&view);
        assert_eq!(pins.len(), 2);
        assert!(pins.iter().all(|p| p.pinned && !p.running));
    }

    #[test]
    fn pinned_shortcuts_use_registry_names() {
        let f = fixture();
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let labels: Vec<&str> = pinned_zone(&view)
            .iter()
            .map(|p| p.label.as_str())
            .collect();
        assert_eq!(labels, vec!["Firefox", "Glaze Settings"]);
    }

    #[test]
    fn label_visibility_follows_the_shared_rule() {
        let f = fixture();
        let wins = vec![window("w1", "org.mozilla.firefox", "Mozilla Firefox", true)];
        let view = build(
            &f,
            &wins,
            RunningPolicy::Merge,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );

        let focused = task_zone(&view).iter().find(|t| t.focused).unwrap();
        // A focused button keeps its label down to the icon budget.
        assert!(focused.show_label_at(TASK_ICON_WIDTH_PX, true));
        assert!(focused.show_label_at(TASK_LABEL_WIDTH_PX, true));
        // Never when labels are switched off.
        assert!(!focused.show_label_at(10_000, false));
        // The view carries the real window ids so a click can focus them.
        assert_eq!(focused.window_ids, vec!["w1".to_string()]);
        assert_eq!(focused.window_count, 1);

        let pinned = pinned_zone(&view).first().unwrap();
        assert!(!pinned.focused);
        // A non-focused button needs the full label budget.
        assert!(!pinned.show_label_at(TASK_ICON_WIDTH_PX, true));
        assert!(pinned.show_label_at(TASK_LABEL_WIDTH_PX, true));
        assert!(pinned.window_ids.is_empty(), "a closed pin has no windows");
    }

    #[test]
    fn a_pinned_app_that_is_running_shows_both_flags_in_the_task_zone() {
        let f = fixture();
        let wins = vec![window("w1", "org.mozilla.firefox", "Mozilla Firefox", true)];
        let view = build(
            &f,
            &wins,
            RunningPolicy::Merge,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let tasks = task_zone(&view);
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].pinned, "pin flag is retained when running");
        assert!(tasks[0].running && tasks[0].focused);
        assert_eq!(tasks[0].window_count, 1);
        // The pure-shortcut zone now holds only the still-closed pin.
        let pins = pinned_zone(&view);
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].app_id, "glaze-settings");
    }

    #[test]
    fn unrelated_running_app_appears_only_in_the_task_zone() {
        let f = fixture();
        let wins = vec![window("w9", "org.gnome.Terminal", "Terminal", false)];
        let view = build(
            &f,
            &wins,
            RunningPolicy::Merge,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let tasks = task_zone(&view);
        assert!(tasks
            .iter()
            .any(|t| t.app_id == "org.gnome.Terminal" && !t.pinned));
    }

    #[test]
    fn internal_windows_are_never_shown() {
        let f = fixture();
        let mut w = window("layer1", "glaze-shell", "panel", false);
        w.kind = WindowKind::Internal;
        let view = build(
            &f,
            &[w],
            RunningPolicy::Merge,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        assert!(task_zone(&view).is_empty());
    }

    #[test]
    fn tray_overflow_and_badge_are_resolved() {
        let f = fixture();
        let mut tray = TrayModel::new();
        for i in 0..10 {
            let mut item = TrayItem::new(format!("id{i}"), format!("Item {i}"));
            item.category = TrayCategory::Other;
            tray.upsert(item);
        }
        let mut badge = NotificationBadge::new();
        badge.push("n1");
        badge.push("n2");
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &tray,
            &badge,
            &clock(),
            false,
        );
        let tray_view = view
            .items
            .iter()
            .find_map(|i| match i {
                PanelItemView::Tray {
                    items,
                    overflow_count,
                    badge,
                    ..
                } => Some((items.len(), *overflow_count, badge.clone())),
                _ => None,
            })
            .unwrap();
        assert_eq!(tray_view.0, 10);
        // 120px / 24px per item = 5 fit, so 5 overflow.
        assert_eq!(tray_view.1, 5);
        assert_eq!(tray_view.2.as_deref(), Some("2"));
    }

    #[test]
    fn tray_attention_is_counted() {
        let f = fixture();
        let mut tray = TrayModel::new();
        let mut urgent = TrayItem::new("u", "Urgent");
        urgent.status = TrayStatus::NeedsAttention;
        tray.upsert(urgent);
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &tray,
            &f.badge,
            &clock(),
            false,
        );
        let attention = view
            .items
            .iter()
            .find_map(|i| match i {
                PanelItemView::Tray { attention, .. } => Some(*attention),
                _ => None,
            })
            .unwrap();
        assert_eq!(attention, 1);
    }

    #[test]
    fn dnd_suppresses_the_badge() {
        let f = fixture();
        let mut badge = NotificationBadge::new();
        badge.push("n1");
        badge.set_dnd(true);
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &badge,
            &clock(),
            false,
        );
        let badge_text = view
            .items
            .iter()
            .find_map(|i| match i {
                PanelItemView::Tray { badge, .. } => Some(badge.clone()),
                _ => None,
            })
            .unwrap();
        assert!(badge_text.is_none());
    }

    #[test]
    fn clock_formats_from_injected_time_and_config() {
        let mut f = fixture();
        f.cfg.panel.clock_24h = true;
        f.cfg.panel.show_seconds = true;
        f.cfg.panel.show_date = true;
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let (time, date) = view.clock().expect("clock zone");
        assert_eq!(time, "14:05:09");
        assert!(date.is_some(), "show_date should yield a date string");
    }

    #[test]
    fn twelve_hour_without_date() {
        let mut f = fixture();
        f.cfg.panel.clock_24h = false;
        f.cfg.panel.show_seconds = false;
        f.cfg.panel.show_date = false;
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        let (time, date) = view.clock().unwrap();
        assert_eq!(time, "2:05 PM");
        assert!(date.is_none());
    }

    #[test]
    fn task_display_mode_degrades_with_width() {
        let f = fixture();
        let wins: Vec<WindowInfo> = (0..20)
            .map(|i| window(&format!("w{i}"), &format!("app{i}"), "t", false))
            .collect();
        let mut wide = f.cfg.clone();
        wide.panel.show_labels = true;
        let f_wide = Fixture {
            cfg: wide,
            apps: vec![],
            tray: TrayModel::new(),
            badge: NotificationBadge::new(),
        };
        let view = build(
            &f_wide,
            &wins,
            RunningPolicy::Merge,
            &f_wide.tray,
            &f_wide.badge,
            &clock(),
            false,
        );
        let mode = view
            .items
            .iter()
            .find_map(|i| match i {
                PanelItemView::Task { display, .. } => Some(*display),
                _ => None,
            })
            .unwrap();
        // 900px with 20 labelled tasks (3000px needed) cannot fit labels.
        assert_ne!(mode, TaskDisplayMode::IconsAndLabels);
    }

    #[test]
    fn show_desktop_zone_follows_the_desktop_config() {
        let f = fixture();
        // Default has no wallpaper and show_icons=false -> no show-desktop zone.
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        assert!(!view
            .items
            .iter()
            .any(|i| matches!(i, PanelItemView::ShowDesktop { .. })));

        let mut with_desktop = fixture();
        with_desktop.cfg.desktop.show_icons = true;
        let view = build(
            &with_desktop,
            &[],
            RunningPolicy::PinnedOnly,
            &with_desktop.tray,
            &with_desktop.badge,
            &clock(),
            false,
        );
        assert!(view.items.iter().any(|i| matches!(
            i,
            PanelItemView::ShowDesktop {
                label_key: KEY_SHOW_DESKTOP
            }
        )));
    }

    #[test]
    fn no_task_zone_when_the_compositor_backend_is_none() {
        let mut f = fixture();
        f.cfg.session.compositor = glaze_common::config::CompositorBackend::None;
        let view = build(
            &f,
            &[],
            RunningPolicy::PinnedOnly,
            &f.tray,
            &f.badge,
            &clock(),
            false,
        );
        assert!(!view
            .items
            .iter()
            .any(|i| matches!(i, PanelItemView::Task { .. })));
    }
}
