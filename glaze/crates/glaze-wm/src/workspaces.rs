//! The portable workspace (virtual desktop) model.
//!
//! Glaze ships a fixed number of configurable workspaces (`GLAZE.md` §5 P1,
//! "multiple workspaces"). This model owns only the *policy* — how many
//! workspaces exist, which one is active, what they are called and where a
//! window is sent. The compositor backend remains the source of truth for which
//! windows actually live where; [`WorkspaceModel::reconcile`] widens the model
//! to cover whatever the backend reports and never invents workspaces the user
//! does not have.

use glaze_common::model::WindowInfo;

/// Workspace state: count, active index, and optional per-workspace names.
///
/// `count` is always at least 1 and `active` is always a valid index, so callers
/// never have to handle "no workspaces at all".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceModel {
    count: u32,
    active: u32,
    names: Vec<Option<String>>,
}

impl Default for WorkspaceModel {
    fn default() -> Self {
        Self::new(1)
    }
}

impl WorkspaceModel {
    /// Create a model with `count` workspaces (clamped to at least 1), the first
    /// one active and every workspace unnamed.
    pub fn new(count: u32) -> Self {
        let count = count.max(1);
        Self {
            count,
            active: 0,
            names: vec![None; count as usize],
        }
    }

    /// Number of workspaces.
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// Zero-based index of the active workspace.
    pub const fn active(&self) -> u32 {
        self.active
    }

    /// True when `index` names a real workspace.
    pub const fn is_valid(&self, index: u32) -> bool {
        index < self.count
    }

    /// Switch to `index`. Returns `false` (and does not move) for an invalid
    /// index.
    pub fn switch_to(&mut self, index: u32) -> bool {
        if self.is_valid(index) {
            self.active = index;
            true
        } else {
            false
        }
    }

    /// Move to the next workspace, wrapping past the last one back to the first.
    /// Returns the new active index.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.active = (self.active + 1) % self.count;
        self.active
    }

    /// Move to the previous workspace, wrapping from the first to the last.
    /// Returns the new active index.
    pub fn prev(&mut self) -> u32 {
        self.active = if self.active == 0 {
            self.count - 1
        } else {
            self.active - 1
        };
        self.active
    }

    /// Policy step for "move the active window to workspace `index`".
    ///
    /// The model does not own windows, so this validates both indices and
    /// returns the workspace that should be focused afterwards (the destination,
    /// so the moved window stays visible). Returns `None` when either index is
    /// not a real workspace.
    pub fn move_window_to(&mut self, index: u32, active_workspace: u32) -> Option<u32> {
        if !self.is_valid(index) || !self.is_valid(active_workspace) {
            return None;
        }
        Some(index)
    }

    /// Name of a workspace, if it has been named.
    pub fn name_at(&self, index: u32) -> Option<&str> {
        self.names
            .get(index as usize)
            .and_then(|name| name.as_deref())
    }

    /// Name of the active workspace, if it has been named.
    pub fn active_name(&self) -> Option<&str> {
        self.name_at(self.active)
    }

    /// Set (or clear, with `None`) the name of a workspace. Returns `false` for
    /// an invalid index.
    pub fn set_name(&mut self, index: u32, name: Option<String>) -> bool {
        match self.names.get_mut(index as usize) {
            Some(slot) => {
                *slot = name;
                true
            }
            None => false,
        }
    }

    /// Widen the model so it covers the highest workspace index the backend
    /// actually reports.
    ///
    /// This deliberately **never shrinks** and **never invents** workspaces: an
    /// empty window list (or windows with no reported workspace) leaves the
    /// model untouched, so the shell cannot fabricate desktops the compositor
    /// does not have.
    pub fn reconcile(&mut self, windows: &[WindowInfo]) {
        let highest = windows.iter().filter_map(|w| w.workspace).max();
        if let Some(highest) = highest {
            let needed = highest.saturating_add(1);
            if needed > self.count {
                self.count = needed;
                self.names.resize(needed as usize, None);
            }
        }
    }

    /// The shell's view of the workspaces, in index order.
    pub fn to_infos(&self) -> Vec<glaze_common::model::WorkspaceInfo> {
        (0..self.count)
            .map(|index| glaze_common::model::WorkspaceInfo {
                index,
                name: self.name_at(index).map(str::to_owned),
                focused: index == self.active,
                urgent: false,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::model::WindowKind;

    fn window(id: &str, workspace: Option<u32>) -> WindowInfo {
        WindowInfo {
            id: id.to_owned(),
            app_id: None,
            title: String::new(),
            workspace,
            kind: WindowKind::Normal,
            focused: false,
            minimized: false,
            floating: false,
        }
    }

    #[test]
    fn new_clamps_to_at_least_one_and_starts_named_none() {
        let m = WorkspaceModel::new(0);
        assert_eq!(m.count(), 1);
        assert_eq!(m.active(), 0);
        assert_eq!(m.active_name(), None);

        let m = WorkspaceModel::new(4);
        assert_eq!(m.count(), 4);
        assert_eq!(m.active_name(), None);
    }

    #[test]
    fn switch_to_rejects_invalid_indices() {
        let mut m = WorkspaceModel::new(3);
        assert!(m.switch_to(2));
        assert_eq!(m.active(), 2);
        assert!(!m.switch_to(3), "index == count is invalid");
        assert_eq!(m.active(), 2, "a rejected switch does not move");
        assert!(!m.switch_to(u32::MAX));
    }

    #[test]
    fn next_and_prev_wrap_in_both_directions() {
        let mut m = WorkspaceModel::new(3);
        assert_eq!(m.next(), 1);
        assert_eq!(m.next(), 2);
        assert_eq!(m.next(), 0, "wraps forward past the last");
        assert_eq!(m.prev(), 2, "wraps backward from the first");
        assert_eq!(m.prev(), 1);
        assert_eq!(m.prev(), 0);
    }

    #[test]
    fn a_single_workspace_never_moves() {
        let mut m = WorkspaceModel::new(1);
        assert_eq!(m.next(), 0);
        assert_eq!(m.prev(), 0);
    }

    #[test]
    fn move_window_to_validates_both_indices() {
        let mut m = WorkspaceModel::new(4);
        assert_eq!(m.move_window_to(2, 0), Some(2));
        assert_eq!(m.move_window_to(0, 2), Some(0));
        assert_eq!(m.move_window_to(4, 0), None, "destination out of range");
        assert_eq!(m.move_window_to(1, 4), None, "source out of range");
    }

    #[test]
    fn names_are_read_back_per_workspace() {
        let mut m = WorkspaceModel::new(3);
        assert!(m.set_name(1, Some("Web".to_owned())));
        assert_eq!(m.name_at(1), Some("Web"));
        m.switch_to(1);
        assert_eq!(m.active_name(), Some("Web"));
        assert!(!m.set_name(9, Some("nope".to_owned())));
        assert!(m.set_name(1, None));
        assert_eq!(m.active_name(), None);
    }

    #[test]
    fn reconcile_widens_to_the_highest_reported_index() {
        let mut m = WorkspaceModel::new(2);
        m.reconcile(&[window("a", Some(0)), window("b", Some(4))]);
        assert_eq!(m.count(), 5, "widened to cover index 4");
        assert_eq!(m.name_at(4), None, "the new workspace is unnamed");
    }

    #[test]
    fn reconcile_never_shrinks_and_never_invents() {
        let mut m = WorkspaceModel::new(5);
        // No reported workspaces at all: unchanged.
        m.reconcile(&[window("a", None)]);
        assert_eq!(m.count(), 5);
        m.reconcile(&[]);
        assert_eq!(m.count(), 5);
        // A window on a low workspace does not shrink the model either.
        m.reconcile(&[window("b", Some(1))]);
        assert_eq!(m.count(), 5);
    }

    #[test]
    fn to_infos_marks_exactly_the_active_workspace() {
        let mut m = WorkspaceModel::new(3);
        let _ = m.set_name(2, Some("Media".to_owned()));
        m.switch_to(2);
        let infos = m.to_infos();
        assert_eq!(infos.len(), 3);
        assert!(infos[2].focused);
        assert_eq!(infos[2].name.as_deref(), Some("Media"));
        assert!(!infos[0].focused && !infos[1].focused);
        assert!(infos.iter().all(|w| !w.urgent));
        assert_eq!(
            infos.iter().map(|w| w.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }
}
