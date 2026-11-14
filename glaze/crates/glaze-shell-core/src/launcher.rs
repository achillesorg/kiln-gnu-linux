//! Launcher state machine — the keyboard/testable core of the Glaze start menu.
//!
//! The GTK popover in `apps/glaze-shell` owns no decisions: it forwards input to
//! a [`LauncherState`] and renders whatever that state says. This keeps the
//! interaction contract (`GLAZE.md` §6.2) exhaustively unit-tested on any host.
//!
//! ## Keyboard semantics
//!
//! - **Escape** closes the launcher and clears query/results/selection. If it
//!   was already closed, [`LauncherState::handle_escape`] reports `false` so the
//!   caller can propagate the key (e.g. let the window handle it).
//! - **Up / Down** move the selection and **wrap** around both ends: moving up
//!   from the first result selects the last, and vice versa. With no results
//!   the selection stays at 0 and nothing is activated.
//! - **Enter** activates the selected result. [`LauncherState::activate_selected`]
//!   returns the desktop id; the caller looks it up in the real
//!   [`glaze_service::AppRegistry`] and launches it. There is no launch path when
//!   nothing is selected.
//! - Typing re-ranks via [`glaze_service::search::search_apps`] and always resets
//!   the selection to the first hit, so Enter after typing is predictable.

use glaze_common::model::{AppEntry, SearchHit};
use glaze_service::search::search_apps;

/// The complete, serialisable state of an open launcher session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LauncherState {
    /// Whether the launcher surface is visible.
    open: bool,
    /// The current search text.
    query: String,
    /// Ranked hits for `query`, at most the configured limit.
    results: Vec<SearchHit>,
    /// Index into `results` of the highlighted row.
    selected: usize,
}

impl LauncherState {
    /// A fresh, closed launcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the launcher surface should be visible.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The current search text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The current ranked results.
    pub fn results(&self) -> &[SearchHit] {
        &self.results
    }

    /// Index of the highlighted row (always a valid index while `results` is
    /// non-empty; `0` when empty).
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Open the launcher: clear the query and show the first `limit` apps in
    /// registry order (empty-query ranking). Selection resets to the top.
    pub fn open(&mut self, apps: &[AppEntry], limit: usize) {
        self.open = true;
        self.query.clear();
        self.results = search_apps(apps, "", limit);
        self.selected = 0;
    }

    /// Close the launcher and drop all transient state.
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.results.clear();
        self.selected = 0;
    }

    /// Replace the query and re-rank, resetting the selection to the first hit.
    pub fn set_query(&mut self, q: &str, apps: &[AppEntry], limit: usize) {
        self.query = q.to_string();
        self.results = search_apps(apps, &self.query, limit);
        self.selected = 0;
    }

    /// Move the selection by `delta` rows, wrapping at both ends.
    ///
    /// A no-op when there are no results (selection stays `0`).
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.results.len();
        if n == 0 {
            self.selected = 0;
            return;
        }
        let n_i = n as i64;
        let current = self.selected as i64;
        // Euclidean modulo keeps the result non-negative for negative delta.
        let next = (((current + delta as i64) % n_i) + n_i) % n_i;
        self.selected = next as usize;
    }

    /// Move the highlight to an absolute index, clamped to the result range.
    ///
    /// Used by the GUI when the user clicks a row. With no results the index
    /// stays `0` so [`LauncherState::selected`] keeps returning `None`.
    pub fn set_selected_index(&mut self, index: usize) {
        self.selected = if self.results.is_empty() {
            0
        } else {
            index.min(self.results.len() - 1)
        };
    }

    /// The highlighted hit, if any.
    pub fn selected(&self) -> Option<&SearchHit> {
        self.results.get(self.selected)
    }

    /// The desktop id of the highlighted hit, ready to launch.
    pub fn activate_selected(&self) -> Option<String> {
        self.selected().map(|hit| hit.app_id.clone())
    }

    /// Handle Escape: close if open. Returns `true` if this call closed the
    /// launcher (so the caller can swallow the key) and `false` if it was
    /// already closed (so the caller can pass the key on).
    pub fn handle_escape(&mut self) -> bool {
        if self.open {
            self.close();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: None,
            generic_name: None,
            exec: format!("/usr/bin/{id}"),
            try_exec: None,
            icon: None,
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    fn apps() -> Vec<AppEntry> {
        // Deliberately not alphabetical by name so registry order is detectable.
        vec![
            app("org.mozilla.firefox", "Firefox"),
            app("org.gnome.Files", "Files"),
            app("org.gnome.Terminal", "Terminal"),
            app("glaze-settings", "Glaze Settings"),
        ]
    }

    #[test]
    fn starts_closed_and_empty() {
        let s = LauncherState::new();
        assert!(!s.is_open());
        assert_eq!(s.query(), "");
        assert!(s.results().is_empty());
        assert_eq!(s.selected_index(), 0);
        assert!(s.selected().is_none());
        assert!(s.activate_selected().is_none());
    }

    #[test]
    fn open_clears_query_and_shows_first_limit_apps() {
        let mut s = LauncherState::new();
        s.set_query("fire", &apps(), 10);
        s.open(&apps(), 2);
        assert!(s.is_open());
        assert_eq!(s.query(), "");
        assert_eq!(s.results().len(), 2);
        assert_eq!(s.selected_index(), 0);
        // Empty-query ranking preserves registry order.
        assert_eq!(s.results()[0].app_id, "org.mozilla.firefox");
        assert_eq!(s.results()[1].app_id, "org.gnome.Files");
    }

    #[test]
    fn set_query_reranks_and_resets_selection() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10);
        s.move_selection(2);
        assert_eq!(s.selected_index(), 2);

        s.set_query("term", &apps(), 10);
        assert_eq!(s.query(), "term");
        assert_eq!(s.selected_index(), 0);
        assert_eq!(s.results().len(), 1);
        assert_eq!(s.results()[0].app_id, "org.gnome.Terminal");
    }

    #[test]
    fn selection_wraps_both_directions() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10); // 4 results
        assert_eq!(s.results().len(), 4);

        s.move_selection(-1);
        assert_eq!(s.selected_index(), 3, "up from top wraps to last");

        s.move_selection(1);
        assert_eq!(s.selected_index(), 0, "down from last wraps to first");

        s.move_selection(5);
        assert_eq!(s.selected_index(), 1, "multi-step move wraps");
    }

    #[test]
    fn selection_is_a_noop_without_results() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10);
        s.set_query("zzzzz-no-such-app", &apps(), 10);
        assert!(s.results().is_empty());
        s.move_selection(3);
        assert_eq!(s.selected_index(), 0);
        assert!(s.selected().is_none());
        assert!(s.activate_selected().is_none());
    }

    #[test]
    fn activate_returns_selected_app_id() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10);
        s.move_selection(2);
        assert_eq!(s.activate_selected().as_deref(), Some("org.gnome.Terminal"));
    }

    #[test]
    fn set_selected_index_clamps_to_the_result_range() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10); // 4 results
        s.set_selected_index(2);
        assert_eq!(s.selected_index(), 2);
        s.set_selected_index(99);
        assert_eq!(s.selected_index(), 3, "clamped to the last result");
        assert_eq!(s.activate_selected().as_deref(), Some("glaze-settings"));

        s.set_query("no-such-app-xyz", &apps(), 10);
        s.set_selected_index(5);
        assert_eq!(s.selected_index(), 0, "stays 0 with no results");
    }

    #[test]
    fn escape_closes_once_then_reports_not_handled() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10);
        assert!(s.handle_escape(), "open launcher consumes Escape");
        assert!(!s.is_open());
        assert!(s.results().is_empty());
        assert!(!s.handle_escape(), "already closed does not consume Escape");
    }

    #[test]
    fn empty_query_search_is_limited_by_limit_argument() {
        let mut s = LauncherState::new();
        s.open(&apps(), 3);
        assert_eq!(s.results().len(), 3);
    }

    #[test]
    fn close_drops_query_and_results() {
        let mut s = LauncherState::new();
        s.open(&apps(), 10);
        s.set_query("fire", &apps(), 10);
        s.close();
        assert!(!s.is_open());
        assert_eq!(s.query(), "");
        assert!(s.results().is_empty());
        assert_eq!(s.selected_index(), 0);
    }
}
