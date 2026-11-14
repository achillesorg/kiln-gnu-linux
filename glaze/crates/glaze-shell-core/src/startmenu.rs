//! Start menu model: app categories, recent apps and keyboard navigation.
//!
//! GTK-free so the menu's behaviour is unit-tested on the authoring host.

use std::path::{Path, PathBuf};

use glaze_common::model::AppEntry;

/// Menu sections, in display order, mapped from freedesktop main categories.
pub const SECTIONS: &[(&str, &[&str])] = &[
    ("Accessories", &["Utility", "Accessibility"]),
    ("Development", &["Development"]),
    ("Education", &["Education", "Science"]),
    ("Games", &["Game"]),
    ("Graphics", &["Graphics"]),
    ("Internet", &["Network"]),
    ("Multimedia", &["AudioVideo", "Audio", "Video"]),
    ("Office", &["Office"]),
    ("Settings", &["Settings"]),
    ("System", &["System"]),
];

/// Section name for an app (first matching section, else "Other").
pub fn section_of(app: &AppEntry) -> &'static str {
    for (name, cats) in SECTIONS {
        if app.categories.iter().any(|c| cats.contains(&c.as_str())) {
            return name;
        }
    }
    "Other"
}

/// Visible apps grouped by section, sections in [`SECTIONS`] order then
/// "Other", apps sorted by name (case-insensitive). Empty sections omitted.
pub fn grouped(apps: &[AppEntry]) -> Vec<(&'static str, Vec<&AppEntry>)> {
    let mut out: Vec<(&'static str, Vec<&AppEntry>)> = SECTIONS
        .iter()
        .map(|(n, _)| (*n, Vec::new()))
        .chain(std::iter::once(("Other", Vec::new())))
        .collect();
    for app in apps.iter().filter(|a| !a.no_display) {
        let s = section_of(app);
        if let Some((_, v)) = out.iter_mut().find(|(n, _)| *n == s) {
            v.push(app);
        }
    }
    for (_, v) in out.iter_mut() {
        v.sort_by_key(|a| a.name.to_lowercase());
    }
    out.retain(|(_, v)| !v.is_empty());
    out
}

/// Most-recently-launched app ids, newest first, persisted one per line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentApps {
    ids: Vec<String>,
    cap: usize,
}

impl RecentApps {
    pub fn new(cap: usize) -> Self {
        RecentApps {
            ids: Vec::new(),
            cap: cap.max(1),
        }
    }

    pub fn default_path() -> PathBuf {
        glaze_common::paths::state_home().join("glaze/recent-apps")
    }

    pub fn load(path: &Path, cap: usize) -> Self {
        let mut r = RecentApps::new(cap);
        if let Ok(s) = std::fs::read_to_string(path) {
            for line in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if !r.ids.iter().any(|i| i == line) && r.ids.len() < r.cap {
                    r.ids.push(line.to_string());
                }
            }
        }
        r
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, self.ids.join("\n") + "\n")
    }

    /// Record a launch (moves the id to the front).
    pub fn push(&mut self, id: &str) {
        self.ids.retain(|i| i != id);
        self.ids.insert(0, id.to_string());
        self.ids.truncate(self.cap);
    }

    /// Ids that are still installed, newest first.
    pub fn installed<'a>(&self, apps: &'a [AppEntry]) -> Vec<&'a AppEntry> {
        self.ids
            .iter()
            .filter_map(|id| apps.iter().find(|a| &a.id == id && !a.no_display))
            .collect()
    }
}

/// Keyboard movement over a flat list of items where the first `grid_len`
/// items form a grid of `columns` columns (pinned apps) and the rest a list.
pub fn move_selection(
    current: usize,
    len: usize,
    grid_len: usize,
    columns: usize,
    key: Nav,
) -> usize {
    if len == 0 {
        return 0;
    }
    let cur = current.min(len - 1);
    let cols = columns.max(1);
    let in_grid = cur < grid_len;
    let next = match key {
        Nav::Left => cur.saturating_sub(1),
        Nav::Right => cur + 1,
        Nav::Down if in_grid => {
            if cur + cols < grid_len {
                cur + cols
            } else {
                grid_len
            }
        }
        Nav::Down => cur + 1,
        Nav::Up if in_grid => cur.saturating_sub(cols).min(cur),
        Nav::Up if cur == grid_len && grid_len > 0 => {
            // From the first list row back into the grid's last row.
            (grid_len - 1) / cols * cols
        }
        Nav::Up => cur.saturating_sub(1),
        Nav::Home => 0,
        Nav::End => len - 1,
    };
    next.min(len - 1)
}

/// Navigation keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, cats: &[&str]) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: None,
            generic_name: None,
            exec: id.into(),
            try_exec: None,
            icon: None,
            categories: cats.iter().map(|c| c.to_string()).collect(),
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: String::new(),
        }
    }

    #[test]
    fn groups_by_section_sorted_and_skips_hidden() {
        let mut hidden = app("h", "Hidden", &["System"]);
        hidden.no_display = true;
        let apps = vec![
            app("foot", "Foot", &["System", "TerminalEmulator"]),
            app("vim", "Vim", &["Utility", "TextEditor"]),
            app("a", "abc", &["System"]),
            app("x", "Xwayland", &[]),
            hidden,
        ];
        let g = grouped(&apps);
        let names: Vec<&str> = g.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["Accessories", "System", "Other"]);
        let sys: Vec<&str> = g[1].1.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(sys, ["abc", "Foot"]);
    }

    #[test]
    fn recent_apps_round_trip_and_dedup() {
        let dir = std::env::temp_dir().join(format!("glaze-recent-{}", std::process::id()));
        let path = dir.join("recent-apps");
        let mut r = RecentApps::new(3);
        for id in ["a", "b", "a", "c", "d"] {
            r.push(id);
        }
        r.save(&path).unwrap();
        let back = RecentApps::load(&path, 3);
        assert_eq!(back, r);
        let apps = vec![app("d", "D", &[]), app("a", "A", &[])];
        let ids: Vec<&str> = back
            .installed(&apps)
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(ids, ["d", "a"], "newest first, uninstalled dropped");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn grid_and_list_navigation() {
        // 7 pinned in 4 columns (rows 0-3, 4-6), then 5 list rows (7..12).
        let (len, grid, cols) = (12, 7, 4);
        assert_eq!(move_selection(1, len, grid, cols, Nav::Down), 5);
        assert_eq!(
            move_selection(5, len, grid, cols, Nav::Down),
            7,
            "into list"
        );
        assert_eq!(
            move_selection(7, len, grid, cols, Nav::Up),
            4,
            "back to grid"
        );
        assert_eq!(move_selection(5, len, grid, cols, Nav::Up), 1);
        assert_eq!(move_selection(0, len, grid, cols, Nav::Up), 0);
        assert_eq!(move_selection(9, len, grid, cols, Nav::Down), 10);
        assert_eq!(move_selection(11, len, grid, cols, Nav::Down), 11);
        assert_eq!(move_selection(3, len, grid, cols, Nav::Right), 4);
        assert_eq!(move_selection(0, len, grid, cols, Nav::Left), 0);
        assert_eq!(move_selection(6, len, grid, cols, Nav::End), 11);
        assert_eq!(move_selection(6, 0, grid, cols, Nav::Down), 0);
        // No grid (search results): plain list.
        assert_eq!(move_selection(0, 3, 0, cols, Nav::Down), 1);
        assert_eq!(move_selection(1, 3, 0, cols, Nav::Up), 0);
    }
}
