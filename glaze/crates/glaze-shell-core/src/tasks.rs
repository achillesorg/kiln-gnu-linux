//! Grouping live windows into taskbar entries.
//!
//! The taskbar shows one button per *application group*, not per window
//! (`GLAZE.md` §3.4: "Active app clearly distinguished from merely pinned
//! apps"). This module turns the flat `WindowInfo` list reported by a
//! compositor backend (`glaze-backend`, see `docs/adr/ADR-0002-...`) into
//! stable, ordered [`TaskGroup`]s. It contains **no** fabricated data: an empty
//! input list yields an empty output list.

use std::collections::HashMap;

use glaze_common::model::{WindowInfo, WindowKind};

/// One taskbar button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGroup {
    /// Stable group key: the `app_id` when present, else the window id.
    pub key: String,
    /// The Wayland `app_id` / X11 `WM_CLASS`, when the backend reports one.
    pub app_id: Option<String>,
    /// Best available label (focused title, else first title, else the key).
    pub title: String,
    /// Window ids that belong to this group, in first-seen order.
    pub window_ids: Vec<String>,
    /// True when any window in the group is focused.
    pub focused: bool,
    /// True when every window in the group is minimised.
    pub minimized: bool,
    /// Number of windows in the group.
    pub count: usize,
}

/// Group windows by application id, preserving first-seen order.
///
/// Windows of [`WindowKind::Internal`] (e.g. Glaze's own layer surfaces) are
/// skipped: they are never user-facing toplevels. Windows without an `app_id`
/// fall back to one group per window, so an unknown app is still reachable.
pub fn group_windows(windows: &[WindowInfo]) -> Vec<TaskGroup> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<GroupAccumulator> = Vec::new();

    for win in windows {
        if win.kind == WindowKind::Internal {
            continue;
        }
        let (key, app_id) = match win.app_id.as_deref().map(str::trim) {
            Some(id) if !id.is_empty() => (id.to_string(), Some(id.to_string())),
            _ => (win.id.clone(), None),
        };

        let slot = match index.get(&key) {
            Some(&i) => i,
            None => {
                let i = groups.len();
                groups.push(GroupAccumulator::new(key.clone(), app_id));
                index.insert(key, i);
                i
            }
        };
        groups[slot].push(win);
    }

    groups.into_iter().map(GroupAccumulator::finish).collect()
}

#[derive(Debug)]
struct GroupAccumulator {
    key: String,
    app_id: Option<String>,
    window_ids: Vec<String>,
    focused_title: Option<String>,
    first_title: Option<String>,
    focused: bool,
    minimized: bool,
}

impl GroupAccumulator {
    fn new(key: String, app_id: Option<String>) -> Self {
        GroupAccumulator {
            key,
            app_id,
            window_ids: Vec::new(),
            focused_title: None,
            first_title: None,
            focused: false,
            minimized: true,
        }
    }

    fn push(&mut self, win: &WindowInfo) {
        self.window_ids.push(win.id.clone());
        let title = win.title.trim();
        if !title.is_empty() {
            if self.first_title.is_none() {
                self.first_title = Some(title.to_string());
            }
            if win.focused && self.focused_title.is_none() {
                self.focused_title = Some(title.to_string());
            }
        }
        if win.focused {
            self.focused = true;
        }
        // The group is minimised only while every member is.
        self.minimized = self.minimized && win.minimized;
    }

    fn finish(self) -> TaskGroup {
        let count = self.window_ids.len();
        let title = self
            .focused_title
            .or(self.first_title)
            .unwrap_or_else(|| self.key.clone());
        TaskGroup {
            title,
            app_id: self.app_id,
            key: self.key,
            window_ids: self.window_ids,
            focused: self.focused,
            minimized: self.minimized && count > 0,
            count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: &str, app: Option<&str>, title: &str, kind: WindowKind) -> WindowInfo {
        WindowInfo {
            id: id.into(),
            app_id: app.map(str::to_string),
            title: title.into(),
            workspace: Some(0),
            kind,
            focused: false,
            minimized: false,
            floating: false,
        }
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(group_windows(&[]).is_empty());
    }

    #[test]
    fn groups_windows_of_the_same_app() {
        let windows = vec![
            win("1", Some("firefox"), "Tab A", WindowKind::Normal),
            win("2", Some("firefox"), "Tab B", WindowKind::Normal),
            win("3", Some("org.gnome.Terminal"), "bash", WindowKind::Normal),
        ];
        let groups = group_windows(&windows);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "firefox");
        assert_eq!(groups[0].app_id.as_deref(), Some("firefox"));
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[0].window_ids, vec!["1", "2"]);
        assert_eq!(groups[1].key, "org.gnome.Terminal");
        assert_eq!(groups[1].count, 1);
    }

    #[test]
    fn preserves_first_seen_order() {
        let windows = vec![
            win("1", Some("zeta"), "Z", WindowKind::Normal),
            win("2", Some("alpha"), "A", WindowKind::Normal),
            win("3", Some("zeta"), "Z2", WindowKind::Normal),
        ];
        let keys: Vec<_> = group_windows(&windows).into_iter().map(|g| g.key).collect();
        assert_eq!(keys, vec!["zeta", "alpha"]);
    }

    #[test]
    fn focused_window_supplies_the_title_and_focus_flag() {
        let a = win("1", Some("app"), "First", WindowKind::Normal);
        let mut b = win("2", Some("app"), "Focused", WindowKind::Normal);
        b.focused = true;
        let groups = group_windows(&[a.clone(), b]);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].focused);
        assert_eq!(groups[0].title, "Focused");

        let only_first = group_windows(&[a]);
        assert!(!only_first[0].focused);
        assert_eq!(only_first[0].title, "First");
    }

    #[test]
    fn group_is_minimized_only_when_all_windows_are() {
        let mut a = win("1", Some("app"), "A", WindowKind::Normal);
        let mut b = win("2", Some("app"), "B", WindowKind::Normal);
        a.minimized = true;
        b.minimized = true;
        assert!(group_windows(&[a.clone(), b.clone()])[0].minimized);

        b.minimized = false;
        assert!(!group_windows(&[a, b])[0].minimized);
    }

    #[test]
    fn internal_windows_are_skipped() {
        let windows = vec![
            win(
                "layer",
                Some("glaze-panel"),
                "Glaze panel",
                WindowKind::Internal,
            ),
            win("1", Some("app"), "Real", WindowKind::Normal),
        ];
        let groups = group_windows(&windows);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "app");
    }

    #[test]
    fn windows_without_app_id_get_their_own_group() {
        let windows = vec![
            win("10", None, "One", WindowKind::Normal),
            win("11", None, "Two", WindowKind::Normal),
        ];
        let groups = group_windows(&windows);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "10");
        assert!(groups[0].app_id.is_none());
        assert_eq!(groups[0].title, "One");
        assert_eq!(groups[1].key, "11");
    }

    #[test]
    fn blank_title_falls_back_to_the_key() {
        let windows = vec![win("42", Some("app"), "   ", WindowKind::Normal)];
        let groups = group_windows(&windows);
        assert_eq!(groups[0].title, "app");
    }
}
