//! Notification badge / indicator model for the panel (`GLAZE.md` §3.1, §6.1).
//!
//! This is only the *indicator* state: how many notifications are unread and
//! whether Do-Not-Disturb is on. It does not own the notification server, the
//! bus name or the message bodies — those live in `glaze-service::notifications`
//! and `glaze-service::dbus`. The shell feeds ids in as notifications arrive and
//! reads back the badge text to render.
//!
//! Keeping the counting here makes it unit-testable on any host and
//! deterministic: no clock, no D-Bus, no strings from the wire.

/// Highest count rendered as a number; anything above becomes [`MAX_BADGE`]`+`.
pub const MAX_BADGE: usize = 99;

/// Unread-notification state for the panel's badge.
#[derive(Debug, Default, Clone)]
pub struct NotificationBadge {
    /// Ids of notifications that are not yet read, in arrival order.
    unread: Vec<String>,
    /// Do-Not-Disturb: when on, [`NotificationBadge::badge_text`] is suppressed
    /// (the panel still records unread ids; they just are not shown).
    pub dnd: bool,
}

impl NotificationBadge {
    /// A fresh badge: nothing unread, DND off.
    pub fn new() -> Self {
        NotificationBadge::default()
    }

    /// Record a newly delivered notification.
    ///
    /// Idempotent for the same id while it is still unread, so a republish or a
    /// replayed event cannot inflate the count. Returns `true` when the count
    /// increased (the id was not already unread).
    pub fn push(&mut self, notification_id: impl Into<String>) -> bool {
        let id = notification_id.into();
        if self.unread.contains(&id) {
            return false;
        }
        self.unread.push(id);
        true
    }

    /// Mark one notification read. Returns whether it was unread.
    pub fn mark_read(&mut self, notification_id: &str) -> bool {
        let before = self.unread.len();
        self.unread.retain(|id| id != notification_id);
        self.unread.len() != before
    }

    /// Mark everything read, clearing the badge.
    pub fn mark_all_read(&mut self) {
        self.unread.clear();
    }

    /// Number of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.unread.len()
    }

    /// The unread ids, in arrival order (for a notification list surface).
    pub fn unread_ids(&self) -> &[String] {
        &self.unread
    }

    /// True when Do-Not-Disturb is enabled.
    pub fn is_dnd(&self) -> bool {
        self.dnd
    }

    /// Set Do-Not-Disturb.
    pub fn set_dnd(&mut self, dnd: bool) {
        self.dnd = dnd;
    }

    /// The badge text to render, or `None` when the badge should be hidden.
    ///
    /// Hidden when nothing is unread or when DND is on. Counts above
    /// [`MAX_BADGE`] are capped as `"99+"` so a long-running session cannot
    /// produce an ever-widening badge (`GLAZE.md` §3.4: no layout jumps).
    pub fn badge_text(&self) -> Option<String> {
        if self.dnd {
            return None;
        }
        match self.unread.len() {
            0 => None,
            n if n > MAX_BADGE => Some(format!("{MAX_BADGE}+")),
            n => Some(n.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_badge_is_hidden() {
        let badge = NotificationBadge::new();
        assert_eq!(badge.unread_count(), 0);
        assert_eq!(badge.badge_text(), None);
        assert!(!badge.is_dnd());
    }

    #[test]
    fn push_counts_and_reports_new_ids() {
        let mut badge = NotificationBadge::new();
        assert!(badge.push("n1"));
        assert!(badge.push("n2"));
        assert_eq!(badge.unread_count(), 2);
        assert_eq!(badge.badge_text().as_deref(), Some("2"));
        assert_eq!(badge.unread_ids(), &["n1".to_string(), "n2".to_string()]);
    }

    #[test]
    fn push_is_idempotent_while_unread() {
        let mut badge = NotificationBadge::new();
        assert!(badge.push("n1"));
        assert!(!badge.push("n1"), "same id does not double-count");
        assert_eq!(badge.unread_count(), 1);
    }

    #[test]
    fn a_read_id_can_be_pushed_again() {
        let mut badge = NotificationBadge::new();
        badge.push("n1");
        badge.mark_read("n1");
        assert!(badge.push("n1"), "a fresh notification reuses the id");
        assert_eq!(badge.unread_count(), 1);
    }

    #[test]
    fn mark_read_only_affects_the_matching_id() {
        let mut badge = NotificationBadge::new();
        badge.push("n1");
        badge.push("n2");
        assert!(badge.mark_read("n1"));
        assert!(!badge.mark_read("n1"), "already read");
        assert_eq!(badge.unread_count(), 1);
        assert_eq!(badge.unread_ids(), &["n2".to_string()]);
        assert_eq!(badge.badge_text().as_deref(), Some("1"));
    }

    #[test]
    fn mark_all_read_clears_the_badge() {
        let mut badge = NotificationBadge::new();
        for i in 0..5 {
            badge.push(format!("n{i}"));
        }
        assert_eq!(badge.badge_text().as_deref(), Some("5"));
        badge.mark_all_read();
        assert_eq!(badge.unread_count(), 0);
        assert_eq!(badge.badge_text(), None);
    }

    #[test]
    fn badge_caps_at_ninety_nine_plus() {
        let mut badge = NotificationBadge::new();
        for i in 0..MAX_BADGE {
            badge.push(format!("n{i}"));
        }
        assert_eq!(badge.unread_count(), MAX_BADGE);
        assert_eq!(badge.badge_text().as_deref(), Some("99"));

        badge.push("overflow");
        assert_eq!(badge.unread_count(), MAX_BADGE + 1);
        assert_eq!(badge.badge_text().as_deref(), Some("99+"));
    }

    #[test]
    fn dnd_suppresses_the_badge_but_keeps_the_count() {
        let mut badge = NotificationBadge::new();
        badge.push("n1");
        badge.push("n2");
        badge.set_dnd(true);
        assert!(badge.is_dnd());
        assert_eq!(badge.badge_text(), None, "DND hides the badge");
        assert_eq!(badge.unread_count(), 2, "but the count is retained");

        badge.set_dnd(false);
        assert_eq!(badge.badge_text().as_deref(), Some("2"));
    }

    #[test]
    fn dnd_field_is_public_and_writable() {
        let mut badge = NotificationBadge::new();
        badge.dnd = true;
        assert!(badge.badge_text().is_none());
        badge.dnd = false;
        badge.push("n1");
        assert_eq!(badge.badge_text().as_deref(), Some("1"));
    }
}
