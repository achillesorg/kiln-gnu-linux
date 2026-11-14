//! Desktop icon selection.
//!
//! Selection is keyed by [`DesktopIcon::key`](crate::icons::DesktopIcon::key)
//! rather than by grid index, so it survives re-arranging and re-loading the
//! desktop. The interaction rules are the familiar ones (`GLAZE.md` §3.1,
//! §6.5 "keyboard-first flows"):
//!
//! | Gesture | Effect |
//! |---|---|
//! | click | select only that icon |
//! | Ctrl+click | toggle that icon in/out |
//! | Shift+click | select the contiguous range from the anchor |
//! | Ctrl+Shift+click | add the range to the current selection |
//!
//! The icon order (the on-screen receipt order) is passed in by the caller, so
//! this module has no hidden dependency on the desktop layout.

use std::collections::BTreeSet;

/// The desktop selection set plus the range anchor used by Shift+click.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The currently selected icon keys.
    pub selected: BTreeSet<String>,
    /// Anchor for Shift-range selection: the last explicitly clicked key.
    anchor: Option<String>,
}

impl Selection {
    /// An empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a click on `key`.
    ///
    /// `ctrl` and `shift` are the modifier states at click time; `order` is the
    /// full list of icon keys in display order, used to resolve a Shift range.
    /// A Shift+click with no usable anchor (nothing clicked yet, or the anchor
    /// is no longer in `order`) degrades to a plain click.
    pub fn click(&mut self, key: &str, ctrl: bool, shift: bool, order: &[String]) {
        if shift {
            if let Some(anchor) = self.anchor.clone() {
                if let (Some(a), Some(b)) = (index_of(order, &anchor), index_of(order, key)) {
                    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                    let range: BTreeSet<String> = order[lo..=hi].iter().cloned().collect();
                    if ctrl {
                        self.selected.extend(range);
                    } else {
                        self.selected = range;
                    }
                    return; // Anchor is unchanged by a range extension.
                }
            }
            self.select_only(key);
            return;
        }

        if ctrl {
            if !self.selected.remove(key) {
                self.selected.insert(key.to_string());
            }
            self.anchor = Some(key.to_string());
            return;
        }

        self.select_only(key);
    }

    /// Clear the selection (keeps no anchor).
    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// Select every key in `order`.
    pub fn all(&mut self, order: &[String]) {
        self.selected = order.iter().cloned().collect();
        self.anchor = order.last().cloned();
    }

    /// True when `key` is selected.
    pub fn is_selected(&self, key: &str) -> bool {
        self.selected.contains(key)
    }

    /// The selected keys, in stable sorted order.
    pub fn selected_keys(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    /// Number of selected icons.
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// True when nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// The current range anchor, if any (used by tests and the view layer).
    pub fn anchor(&self) -> Option<&str> {
        self.anchor.as_deref()
    }

    fn select_only(&mut self, key: &str) {
        self.selected.clear();
        self.selected.insert(key.to_string());
        self.anchor = Some(key.to_string());
    }
}

/// Position of `key` in `order`.
fn index_of(order: &[String], key: &str) -> Option<usize> {
    order.iter().position(|k| k == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> Vec<String> {
        ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn sel(s: &Selection) -> Vec<&str> {
        s.selected.iter().map(String::as_str).collect()
    }

    #[test]
    fn starts_empty() {
        let s = Selection::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert!(!s.is_selected("a"));
        assert!(s.anchor().is_none());
    }

    #[test]
    fn plain_click_selects_only_that_icon() {
        let o = order();
        let mut s = Selection::new();
        s.click("b", false, false, &o);
        assert_eq!(sel(&s), vec!["b"]);
        s.click("d", false, false, &o);
        assert_eq!(sel(&s), vec!["d"]);
        assert_eq!(s.anchor(), Some("d"));
    }

    #[test]
    fn ctrl_click_toggles() {
        let o = order();
        let mut s = Selection::new();
        s.click("a", false, false, &o);
        s.click("c", true, false, &o);
        assert_eq!(sel(&s), vec!["a", "c"]);
        s.click("a", true, false, &o);
        assert_eq!(sel(&s), vec!["c"]);
        // Toggling on an empty selection with ctrl still selects.
        s.clear();
        s.click("e", true, false, &o);
        assert_eq!(sel(&s), vec!["e"]);
    }

    #[test]
    fn shift_click_selects_a_forward_range() {
        let o = order();
        let mut s = Selection::new();
        s.click("b", false, false, &o);
        s.click("d", false, true, &o);
        assert_eq!(sel(&s), vec!["b", "c", "d"]);
        // Anchor stayed at the original click.
        assert_eq!(s.anchor(), Some("b"));
    }

    #[test]
    fn shift_click_selects_a_backward_range() {
        let o = order();
        let mut s = Selection::new();
        s.click("d", false, false, &o);
        s.click("b", false, true, &o);
        assert_eq!(sel(&s), vec!["b", "c", "d"]);
    }

    #[test]
    fn shift_click_without_anchor_is_a_plain_click() {
        let o = order();
        let mut s = Selection::new();
        s.click("c", false, true, &o);
        assert_eq!(sel(&s), vec!["c"]);
        assert_eq!(s.anchor(), Some("c"));
    }

    #[test]
    fn shift_click_with_stale_anchor_is_a_plain_click() {
        let o = order();
        let mut s = Selection::new();
        s.click("a", false, false, &o);
        // Anchor "a" is no longer in the order: fall back to a single select.
        let other: Vec<String> = ["x", "y", "z"].iter().map(|s| s.to_string()).collect();
        s.click("y", false, true, &other);
        assert_eq!(sel(&s), vec!["y"]);
    }

    #[test]
    fn ctrl_shift_extends_the_selection() {
        let o = order();
        let mut s = Selection::new();
        s.click("a", false, false, &o);
        s.click("e", false, true, &o); // range a..e
        assert_eq!(sel(&s), vec!["a", "b", "c", "d", "e"]);
        // Remove the middle with ctrl, then add a disjoint range with ctrl+shift.
        s.click("c", true, false, &o);
        assert_eq!(sel(&s), vec!["a", "b", "d", "e"]);
    }

    #[test]
    fn ctrl_shift_unions_rather_than_replaces() {
        let o = order();
        let mut s = Selection::new();
        s.click("a", false, false, &o);
        s.click("b", true, false, &o); // explicit {a, b}, anchor b
        s.click("d", true, true, &o); // union range b..d => {a,b,c,d}
        assert_eq!(sel(&s), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn clear_and_all() {
        let o = order();
        let mut s = Selection::new();
        s.all(&o);
        assert_eq!(s.len(), 5);
        assert!(s.is_selected("c"));
        s.clear();
        assert!(s.is_empty());
        assert!(s.anchor().is_none());
        // `all` on an empty order is empty.
        s.all(&[]);
        assert!(s.is_empty());
        assert!(s.anchor().is_none());
    }

    #[test]
    fn selected_keys_are_sorted_and_unique() {
        let mut s = Selection::new();
        let o = order();
        s.click("e", false, false, &o);
        s.click("a", true, false, &o);
        assert_eq!(s.selected_keys(), vec!["a".to_string(), "e".to_string()]);
        // Duplicate order entries do not duplicate the selection.
        let dup: Vec<String> = ["a", "a", "b"].iter().map(|s| s.to_string()).collect();
        s.all(&dup);
        assert_eq!(s.selected_keys(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn click_is_deterministic() {
        let o = order();
        let mut a = Selection::new();
        let mut b = Selection::new();
        for (k, ctrl, shift) in [("a", false, false), ("c", false, true), ("e", true, false)] {
            a.click(k, ctrl, shift, &o);
            b.click(k, ctrl, shift, &o);
        }
        assert_eq!(a, b);
    }
}
