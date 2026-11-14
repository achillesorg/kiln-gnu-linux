//! Pinned-vs-running taskbar merging — the core of `GLAZE.md` §6.1:
//! **"Pinned vs active states must be visually unmistakable."**
//!
//! [`crate::tasks`] already knows how to group a flat `WindowInfo` list into
//! [`TaskGroup`]s. This module answers the next question: given the user's pinned
//! application ids and the live groups, what exactly should each taskbar button
//! be? The answer is a [`TaskEntry`] that carries *both* facts explicitly:
//!
//! ```text
//!  pinned, not running   →  pinned = true,  running = false, focused = false
//!  running, not pinned   →  pinned = false, running = true,  focused = ?
//!  pinned and running    →  pinned = true,  running = true,  focused = ?
//!  pinned, running, front→  pinned = true,  running = true,  focused = true
//! ```
//!
//! The view layer renders `pinned` / `running` / `focused` as distinct CSS
//! classes, so "merely pinned" and "actually open" can never blur together.
//!
//! ## Ordering
//!
//! Pinned items come first, in the user's pinned order; any running group that
//! is not pinned follows in stable (first-seen) group order. This matches the
//! canonical panel order (`GLAZE.md` §6.1: pinned/favourites → running windows).
//!
//! ## No fabricated windows
//!
//! When no compositor backend is attached the caller passes
//! [`RunningPolicy::PinnedOnly`]. Live groups are then ignored entirely, so a
//! taskbar can never show a window that does not exist (`GLAZE.md` §4.3, §7.2).

use crate::panel::{TASK_ICON_WIDTH_PX, TASK_LABEL_WIDTH_PX};
use crate::tasks::TaskGroup;

/// How running windows should be treated when building the taskbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningPolicy {
    /// A compositor backend reported real windows: merge live groups with pins.
    Merge,
    /// No backend (or one that cannot report windows): show pinned items only.
    /// Live groups, if any are passed, are ignored rather than trusted.
    PinnedOnly,
}

/// One taskbar button, in the panel's left-to-right order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEntry {
    /// The desktop id used to launch / match the app (`org.mozilla.firefox`).
    pub app_id: String,
    /// Best available label: the focused window title, else the group title,
    /// else the id. The GTK layer may substitute a nicer registry name.
    pub label: String,
    /// The app is in `launcher.pinned`.
    pub pinned: bool,
    /// A live window group for this app is open right now.
    pub running: bool,
    /// The focused window belongs to this entry.
    pub focused: bool,
    /// Live window ids for this entry (empty when not running).
    pub window_ids: Vec<String>,
}

impl TaskEntry {
    /// True when this button represents a live window (either pinned-and-open or
    /// a plain running app), so the view can pick an "active" affordance.
    pub fn is_active(&self) -> bool {
        self.running || self.focused
    }
}

/// Merge pinned ids with live [`TaskGroup`]s into ordered [`TaskEntry`]s.
///
/// Pinned ids are matched to a group by the group's `app_id` or its `key`, so a
/// pin named after the desktop file still matches a Wayland `app_id` that uses
/// the same name (and a group without an `app_id` falls back to its window id
/// key). Each group is consumed at most once; duplicate pinned ids are emitted
/// once.
pub fn merge_tasks(
    pinned: &[String],
    groups: &[TaskGroup],
    running_policy: RunningPolicy,
) -> Vec<TaskEntry> {
    let effective_groups: &[TaskGroup] = match running_policy {
        RunningPolicy::Merge => groups,
        RunningPolicy::PinnedOnly => &[],
    };

    let mut used = vec![false; effective_groups.len()];
    let mut emitted: Vec<&str> = Vec::new();
    let mut entries = Vec::with_capacity(pinned.len() + effective_groups.len());

    // 1. Pinned order first.
    for id in pinned {
        let id = id.trim();
        if id.is_empty() || emitted.contains(&id) {
            continue;
        }
        emitted.push(id);

        let matched = effective_groups
            .iter()
            .enumerate()
            .find(|(i, group)| !used[*i] && group_matches(group, id));

        match matched {
            Some((index, group)) => {
                // Pinned *and* running: both flags are set (the "unmistakable"
                // case — the pin stays, the active state is added).
                used[index] = true;
                entries.push(TaskEntry {
                    app_id: id.to_string(),
                    label: group.title.clone(),
                    pinned: true,
                    running: true,
                    focused: group.focused,
                    window_ids: group.window_ids.clone(),
                });
            }
            None => {
                // Pinned only: a pure launcher shortcut. No window state exists,
                // so none is invented.
                entries.push(TaskEntry {
                    app_id: id.to_string(),
                    label: id.to_string(),
                    pinned: true,
                    running: false,
                    focused: false,
                    window_ids: Vec::new(),
                });
            }
        }
    }

    // 2. Remaining running groups, in stable order.
    for (index, group) in effective_groups.iter().enumerate() {
        if used[index] {
            continue;
        }
        entries.push(TaskEntry {
            app_id: group.app_id.clone().unwrap_or_else(|| group.key.clone()),
            label: group.title.clone(),
            pinned: false,
            running: true,
            focused: group.focused,
            window_ids: group.window_ids.clone(),
        });
    }

    entries
}

/// True when a live group belongs to the pinned desktop id.
fn group_matches(group: &TaskGroup, pinned_id: &str) -> bool {
    group.app_id.as_deref() == Some(pinned_id) || group.key == pinned_id
}

/// Whether a single task button should render its text label.
///
/// This re-uses the panel's width budgets ([`TASK_ICON_WIDTH_PX`],
/// [`TASK_LABEL_WIDTH_PX`]) so the taskbar and the rest of the panel agree on
/// when text is affordable.
///
/// `available_px` is the width budget remaining for this button. A **focused**
/// entry — the one window the user is actually in — is the most valuable thing
/// on the panel, so it keeps its label down to the icon budget; every other
/// button needs the full label budget. Crucially, hiding a label never hides the
/// *state*: `pinned` / `running` / `focused` are always conveyed by the button's
/// flags and CSS, so the active window remains unmistakable even icon-only.
pub fn should_show_label(entry: &TaskEntry, available_px: i32, label_wanted: bool) -> bool {
    if !label_wanted {
        return false;
    }
    let available = available_px.max(0);
    if entry.focused {
        available >= TASK_ICON_WIDTH_PX
    } else {
        available >= TASK_LABEL_WIDTH_PX
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::model::{WindowInfo, WindowKind};

    fn win(id: &str, app: &str, title: &str, focused: bool) -> WindowInfo {
        WindowInfo {
            id: id.into(),
            app_id: Some(app.into()),
            title: title.into(),
            workspace: Some(0),
            kind: WindowKind::Normal,
            focused,
            minimized: false,
            floating: false,
        }
    }

    fn group(key: &str, title: &str, focused: bool, windows: &[&str]) -> TaskGroup {
        TaskGroup {
            key: key.into(),
            app_id: Some(key.into()),
            title: title.into(),
            window_ids: windows.iter().map(|w| (*w).to_string()).collect(),
            focused,
            minimized: false,
            count: windows.len(),
        }
    }

    fn pins(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_pins_and_no_windows_is_empty() {
        assert!(merge_tasks(&[], &[], RunningPolicy::Merge).is_empty());
    }

    #[test]
    fn pinned_but_not_running_is_pinned_only() {
        let entries = merge_tasks(&pins(&["org.mozilla.firefox"]), &[], RunningPolicy::Merge);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.app_id, "org.mozilla.firefox");
        assert_eq!(e.label, "org.mozilla.firefox", "no title available");
        assert!(e.pinned);
        assert!(!e.running);
        assert!(!e.focused);
        assert!(e.window_ids.is_empty());
    }

    #[test]
    fn running_but_not_pinned_is_running_only() {
        let g = group("firefox", "Tab A", false, &["1"]);
        let entries = merge_tasks(&[], std::slice::from_ref(&g), RunningPolicy::Merge);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.app_id, "firefox");
        assert_eq!(e.label, "Tab A");
        assert!(!e.pinned);
        assert!(e.running);
        assert!(!e.focused);
        assert_eq!(e.window_ids, vec!["1".to_string()]);
    }

    #[test]
    fn pinned_and_running_shows_both_flags() {
        let g = group("firefox", "Tab A", false, &["1", "2"]);
        let entries = merge_tasks(
            &pins(&["firefox"]),
            std::slice::from_ref(&g),
            RunningPolicy::Merge,
        );
        assert_eq!(entries.len(), 1, "the pin defaults to the same button");
        let e = &entries[0];
        assert!(e.pinned && e.running);
        assert!(!e.focused);
        assert_eq!(e.window_ids, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn pinned_running_and_focused_marks_all_three() {
        let g = group("firefox", "Active Tab", true, &["1"]);
        let entries = merge_tasks(
            &pins(&["firefox"]),
            std::slice::from_ref(&g),
            RunningPolicy::Merge,
        );
        let e = &entries[0];
        assert!(e.pinned && e.running && e.focused);
        assert_eq!(e.label, "Active Tab");
    }

    #[test]
    fn ordering_is_pinned_order_then_running_in_stable_order() {
        let pinned = pins(&["pin.b", "pin.a"]);
        let groups = vec![
            group("run.z", "Z", false, &["1"]),
            group("pin.a", "A window", false, &["2"]),
            group("run.y", "Y", false, &["3"]),
        ];
        let entries = merge_tasks(&pinned, &groups, RunningPolicy::Merge);
        let ids: Vec<&str> = entries.iter().map(|e| e.app_id.as_str()).collect();
        // pin.b (pinned-only), pin.a (merged), then run.z, run.y in group order.
        assert_eq!(ids, vec!["pin.b", "pin.a", "run.z", "run.y"]);
        assert!(entries[0].pinned && !entries[0].running);
        assert!(entries[1].pinned && entries[1].running);
        assert!(!entries[2].pinned && entries[2].running);
        assert!(!entries[3].pinned && entries[3].running);
    }

    #[test]
    fn pinned_only_policy_ignores_live_groups() {
        let groups = vec![group("firefox", "Tab", true, &["1"])];
        let entries = merge_tasks(&pins(&["firefox"]), &groups, RunningPolicy::PinnedOnly);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert!(e.pinned);
        assert!(!e.running, "no backend: running must not be invented");
        assert!(!e.focused);
        assert!(e.window_ids.is_empty());
    }

    #[test]
    fn pinned_only_policy_with_no_pins_is_empty_even_with_groups() {
        let groups = vec![group("firefox", "Tab", true, &["1"])];
        assert!(merge_tasks(&[], &groups, RunningPolicy::PinnedOnly).is_empty());
    }

    #[test]
    fn each_group_is_consumed_by_at_most_one_pin() {
        // Two pins, one group: the first pin merges, the second stays pinned-only.
        let groups = vec![group("firefox", "Tab", false, &["1"])];
        let entries = merge_tasks(
            &pins(&["firefox", "firefox-esr"]),
            &groups,
            RunningPolicy::Merge,
        );
        assert_eq!(entries.len(), 2);
        assert!(entries[0].pinned && entries[0].running);
        assert_eq!(entries[0].app_id, "firefox");
        assert!(entries[1].pinned && !entries[1].running);
        assert_eq!(entries[1].app_id, "firefox-esr");
    }

    #[test]
    fn duplicate_pinned_ids_are_emitted_once() {
        let entries = merge_tasks(
            &pins(&["firefox", "firefox", "  ", "terminal"]),
            &[],
            RunningPolicy::Merge,
        );
        let ids: Vec<&str> = entries.iter().map(|e| e.app_id.as_str()).collect();
        assert_eq!(ids, vec!["firefox", "terminal"]);
    }

    #[test]
    fn pin_matches_on_key_when_app_id_differs() {
        // A group whose key is the window id but whose app_id is None.
        let g = TaskGroup {
            key: "some-window".into(),
            app_id: None,
            title: "Untitled".into(),
            window_ids: vec!["some-window".into()],
            focused: false,
            minimized: false,
            count: 1,
        };
        let entries = merge_tasks(&pins(&["some-window"]), &[g], RunningPolicy::Merge);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].pinned && entries[0].running);
    }

    #[test]
    fn end_to_end_from_real_window_infos_groups_then_merges() {
        use crate::tasks::group_windows;
        let windows = vec![
            win("1", "firefox", "Tab A", false),
            win("2", "firefox", "Tab B", true),
            win("3", "org.gnome.Terminal", "bash", false),
        ];
        let groups = group_windows(&windows);
        let entries = merge_tasks(
            &pins(&["firefox", "glaze-settings"]),
            &groups,
            RunningPolicy::Merge,
        );
        let ids: Vec<&str> = entries.iter().map(|e| e.app_id.as_str()).collect();
        assert_eq!(ids, vec!["firefox", "glaze-settings", "org.gnome.Terminal"]);
        // firefox is pinned + running + focused (its focused tab title wins).
        assert!(entries[0].pinned && entries[0].running && entries[0].focused);
        assert_eq!(entries[0].label, "Tab B");
        assert_eq!(
            entries[0].window_ids,
            vec!["1".to_string(), "2".to_string()]
        );
        // glaze-settings: pinned only.
        assert!(entries[1].pinned && !entries[1].running);
        // terminal: running only.
        assert!(!entries[2].pinned && entries[2].running && !entries[2].focused);
    }

    #[test]
    fn is_active_reports_live_windows_only() {
        let pinned_only = TaskEntry {
            app_id: "a".into(),
            label: "a".into(),
            pinned: true,
            running: false,
            focused: false,
            window_ids: vec![],
        };
        assert!(!pinned_only.is_active());
        let running = TaskEntry {
            running: true,
            ..pinned_only.clone()
        };
        assert!(running.is_active());
        let focused = TaskEntry {
            focused: true,
            ..pinned_only
        };
        assert!(focused.is_active());
    }

    #[test]
    fn label_is_wanted_only_when_requested() {
        let entry = TaskEntry {
            app_id: "a".into(),
            label: "A".into(),
            pinned: true,
            running: false,
            focused: false,
            window_ids: vec![],
        };
        assert!(!should_show_label(&entry, 10_000, false));
        assert!(should_show_label(&entry, TASK_LABEL_WIDTH_PX, true));
    }

    #[test]
    fn non_focused_entry_needs_the_full_label_budget() {
        let entry = TaskEntry {
            app_id: "a".into(),
            label: "A".into(),
            pinned: false,
            running: true,
            focused: false,
            window_ids: vec!["1".into()],
        };
        assert!(!should_show_label(&entry, TASK_LABEL_WIDTH_PX - 1, true));
        assert!(should_show_label(&entry, TASK_LABEL_WIDTH_PX, true));
        assert!(!should_show_label(&entry, TASK_ICON_WIDTH_PX, true));
    }

    #[test]
    fn focused_entry_keeps_its_label_longest() {
        let focused = TaskEntry {
            app_id: "a".into(),
            label: "Active".into(),
            pinned: true,
            running: true,
            focused: true,
            window_ids: vec!["1".into()],
        };
        // Keeps a label down to the icon budget, unlike a non-focused button.
        assert!(should_show_label(&focused, TASK_ICON_WIDTH_PX, true));
        assert!(!should_show_label(&focused, TASK_ICON_WIDTH_PX - 1, true));
        assert!(!should_show_label(&focused, -50, true));
    }
}
