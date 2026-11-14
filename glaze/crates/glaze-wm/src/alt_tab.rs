//! A most-recently-used window switcher model (the `Alt+Tab` list).
//!
//! The model keeps window ids in MRU order (`order[0]` is the most recently
//! focused). A switcher session walks that list with [`AltTab::advance`] /
//! [`AltTab::retreat`] and commits the highlighted candidate with
//! [`AltTab::commit`]. It is fully deterministic and, crucially, never hands
//! back an id that was removed: closed windows disappear from the order, so the
//! switcher cannot focus a window that no longer exists.

/// The MRU-ordered switcher list plus the cursor of an in-progress session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AltTab {
    order: Vec<String>,
    cursor: usize,
}

impl AltTab {
    /// An empty switcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a newly mapped (or newly focused) window as most recent.
    ///
    /// Re-adding an existing id moves it to the front rather than duplicating it.
    pub fn push(&mut self, id: impl Into<String>) {
        let id = id.into();
        self.detach(&id);
        self.order.insert(0, id);
        self.cursor = 0;
    }

    /// Record that a window gained focus. Identical in effect to [`AltTab::push`]
    /// (focus makes a window most recent), and stays correct if the window was
    /// never explicitly pushed.
    pub fn on_focus(&mut self, id: impl Into<String>) {
        self.push(id);
    }

    /// Remove a window (it was closed). Returns `false` when it was not tracked.
    ///
    /// The cursor is adjusted so it still points at a valid entry.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(position) = self.order.iter().position(|candidate| candidate == id) else {
            return false;
        };
        self.order.remove(position);
        if self.order.is_empty() {
            self.cursor = 0;
        } else if position < self.cursor {
            self.cursor -= 1;
        }
        if self.cursor >= self.order.len() {
            self.cursor = self.order.len().saturating_sub(1);
        }
        true
    }

    /// Move to the next candidate (towards less-recently used), wrapping around.
    pub fn advance(&mut self) -> Option<&str> {
        if self.order.is_empty() {
            return None;
        }
        self.cursor = (self.cursor + 1) % self.order.len();
        self.order.get(self.cursor).map(String::as_str)
    }

    /// Move to the previous candidate, wrapping around.
    pub fn retreat(&mut self) -> Option<&str> {
        if self.order.is_empty() {
            return None;
        }
        self.cursor = if self.cursor == 0 {
            self.order.len() - 1
        } else {
            self.cursor - 1
        };
        self.order.get(self.cursor).map(String::as_str)
    }

    /// The candidate currently highlighted, if any.
    pub fn candidate(&self) -> Option<&str> {
        self.order.get(self.cursor).map(String::as_str)
    }

    /// Commit the highlighted candidate: it becomes most recent and the session
    /// cursor resets to the top. Returns the committed id, or `None` when empty.
    pub fn commit(&mut self) -> Option<String> {
        let id = self.order.get(self.cursor)?.clone();
        self.order.retain(|candidate| candidate != &id);
        self.order.insert(0, id.clone());
        self.cursor = 0;
        Some(id)
    }

    /// The MRU list, most recent first.
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// Number of tracked windows.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// True when no windows are tracked.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    fn detach(&mut self, id: &str) {
        self.order.retain(|candidate| candidate != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> AltTab {
        let mut switcher = AltTab::new();
        switcher.push("a");
        switcher.push("b");
        switcher.push("c");
        switcher
    }

    fn order(switcher: &AltTab) -> Vec<String> {
        switcher.order().to_vec()
    }

    #[test]
    fn push_keeps_most_recent_first_and_dedupes() {
        let mut switcher = seeded();
        assert_eq!(order(&switcher), vec!["c", "b", "a"]);

        switcher.push("a");
        assert_eq!(order(&switcher), vec!["a", "c", "b"]);
        assert_eq!(switcher.len(), 3, "re-pushing does not duplicate");
    }

    #[test]
    fn on_focus_moves_the_window_to_the_front() {
        let mut switcher = seeded();
        switcher.on_focus("b");
        assert_eq!(order(&switcher), vec!["b", "c", "a"]);
        // Focusing an unknown window still records it.
        switcher.on_focus("z");
        assert_eq!(order(&switcher), vec!["z", "b", "c", "a"]);
    }

    #[test]
    fn advance_walks_the_mru_list_and_wraps() {
        let mut switcher = seeded();
        assert_eq!(
            switcher.candidate(),
            Some("c"),
            "starts on the current window"
        );
        assert_eq!(switcher.advance(), Some("b"));
        assert_eq!(switcher.advance(), Some("a"));
        assert_eq!(switcher.advance(), Some("c"), "wraps back to the front");
        assert_eq!(switcher.advance(), Some("b"));
    }

    #[test]
    fn retreat_walks_backwards_and_wraps() {
        let mut switcher = seeded();
        assert_eq!(switcher.retreat(), Some("a"), "wraps to the least recent");
        assert_eq!(switcher.retreat(), Some("b"));
        assert_eq!(switcher.retreat(), Some("c"));
        assert_eq!(switcher.retreat(), Some("a"));
    }

    #[test]
    fn commit_promotes_the_candidate_and_resets_the_cursor() {
        let mut switcher = seeded();
        assert_eq!(switcher.advance(), Some("b"));
        assert_eq!(switcher.commit(), Some("b".to_owned()));
        assert_eq!(order(&switcher), vec!["b", "c", "a"]);
        assert_eq!(switcher.candidate(), Some("b"), "cursor is back at the top");
    }

    #[test]
    fn commit_on_the_front_window_keeps_it_first() {
        let mut switcher = seeded();
        assert_eq!(switcher.commit(), Some("c".to_owned()));
        assert_eq!(order(&switcher), vec!["c", "b", "a"]);
    }

    #[test]
    fn removed_ids_are_never_returned() {
        let mut switcher = seeded();
        assert!(switcher.remove("b"));
        assert_eq!(order(&switcher), vec!["c", "a"]);

        // Walk the whole ring a few times; "b" must never come back.
        for _ in 0..6 {
            let next = switcher.advance().unwrap().to_owned();
            assert_ne!(next, "b");
            assert!(next == "a" || next == "c");
        }
        for _ in 0..6 {
            let prev = switcher.retreat().unwrap().to_owned();
            assert_ne!(prev, "b");
        }
        assert_eq!(switcher.commit(), Some("c".to_owned()));
        assert_ne!(switcher.candidate(), Some("b"));
    }

    #[test]
    fn removing_the_highlighted_window_keeps_the_cursor_valid() {
        let mut switcher = seeded();
        assert_eq!(switcher.advance(), Some("b"));
        assert!(switcher.remove("b"));
        assert_eq!(order(&switcher), vec!["c", "a"]);
        assert_eq!(
            switcher.candidate(),
            Some("a"),
            "cursor fell back into range"
        );
        assert!(!switcher.remove("missing"));
    }

    #[test]
    fn removing_the_front_window_keeps_the_next_available() {
        let mut switcher = seeded();
        assert!(switcher.remove("c"));
        assert_eq!(order(&switcher), vec!["b", "a"]);
        assert_eq!(switcher.candidate(), Some("b"));
    }

    #[test]
    fn an_empty_switcher_returns_nothing() {
        let mut switcher = AltTab::new();
        assert!(switcher.is_empty());
        assert_eq!(switcher.advance(), None);
        assert_eq!(switcher.retreat(), None);
        assert_eq!(switcher.candidate(), None);
        assert_eq!(switcher.commit(), None);
        assert!(!switcher.remove("a"));
    }

    #[test]
    fn a_single_window_advances_to_itself() {
        let mut switcher = AltTab::new();
        switcher.push("only");
        assert_eq!(switcher.advance(), Some("only"));
        assert_eq!(switcher.retreat(), Some("only"));
        assert_eq!(switcher.commit(), Some("only".to_owned()));
        assert_eq!(switcher.len(), 1);
    }

    #[test]
    fn removing_everything_then_re_adding_starts_clean() {
        let mut switcher = seeded();
        assert!(switcher.remove("a"));
        assert!(switcher.remove("b"));
        assert!(switcher.remove("c"));
        assert!(switcher.is_empty());
        assert_eq!(switcher.commit(), None);

        switcher.push("fresh");
        assert_eq!(order(&switcher), vec!["fresh"]);
        assert_eq!(switcher.candidate(), Some("fresh"));
    }
}
