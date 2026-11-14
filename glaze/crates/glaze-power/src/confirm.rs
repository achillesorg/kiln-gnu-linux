//! A tiny, fully-testable confirmation state machine.
//!
//! Destructive actions (log out / restart / shut down) must go through an
//! explicit confirmation before they run (`GLAZE.md` §5 P1, "no destructive
//! shortcuts"). This module is deliberately clock-injected: every method takes
//! `now` so behaviour is deterministic and never depends on the wall clock
//! (ADR-0005).

use crate::actions::SessionAction;

/// Default seconds a destructive request stays valid before it expires.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// What the shell should do after requesting an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmPrompt {
    /// The action is destructive: show a confirmation dialog and, if the user
    /// agrees, call [`ConfirmState::confirm`].
    RequiresConfirmation(SessionAction),
    /// The action is safe: run it immediately, no dialog.
    Immediate(SessionAction),
}

/// Tracks at most one pending destructive action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmState {
    /// The action awaiting confirmation, if any.
    pending: Option<SessionAction>,
    /// The window during which `confirm` will accept the pending action.
    timeout_secs: u64,
    /// The `now` at which the pending action was requested (used for expiry).
    requested_at: Option<u64>,
}

impl Default for ConfirmState {
    fn default() -> Self {
        ConfirmState::new(DEFAULT_TIMEOUT_SECS)
    }
}

impl ConfirmState {
    /// Create a state with an explicit confirmation timeout in seconds.
    pub fn new(timeout_secs: u64) -> Self {
        ConfirmState {
            pending: None,
            timeout_secs,
            requested_at: None,
        }
    }

    /// Request an action at time `now`.
    ///
    /// Non-destructive actions are returned as [`ConfirmPrompt::Immediate`] and
    /// never stage a pending confirmation. Destructive actions stage a pending
    /// confirmation (replacing any previous one) and return
    /// [`ConfirmPrompt::RequiresConfirmation`].
    pub fn request(&mut self, action: SessionAction, now: u64) -> ConfirmPrompt {
        if action.is_destructive() {
            self.pending = Some(action);
            self.requested_at = Some(now);
            ConfirmPrompt::RequiresConfirmation(action)
        } else {
            ConfirmPrompt::Immediate(action)
        }
    }

    /// Resolve the pending action at time `now`.
    ///
    /// Returns the action only if a confirmation was pending and `now` is within
    /// `timeout_secs` of the request; otherwise returns `None`. Either way the
    /// pending state is cleared, so a confirmation cannot be replayed.
    pub fn confirm(&mut self, now: u64) -> Option<SessionAction> {
        let action = self.pending.take()?;
        let requested_at = self.requested_at.take().unwrap_or(now);
        if now.saturating_sub(requested_at) <= self.timeout_secs {
            Some(action)
        } else {
            None
        }
    }

    /// Discard any pending confirmation.
    pub fn cancel(&mut self) {
        self.pending = None;
        self.requested_at = None;
    }

    /// Whether a confirmation is currently pending.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The action waiting for confirmation, if any.
    pub fn pending_action(&self) -> Option<SessionAction> {
        self.pending
    }

    /// The configured confirmation window.
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    /// Seconds left before a pending confirmation expires (`0` once expired), or
    /// `None` when nothing is pending.
    pub fn remaining_secs(&self, now: u64) -> Option<u64> {
        self.pending?;
        let requested_at = self.requested_at?;
        Some(
            self.timeout_secs
                .saturating_sub(now.saturating_sub(requested_at)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_request_requires_confirmation() {
        let mut state = ConfirmState::new(30);
        assert_eq!(
            state.request(SessionAction::ShutDown, 100),
            ConfirmPrompt::RequiresConfirmation(SessionAction::ShutDown)
        );
        assert!(state.is_pending());
        assert_eq!(state.pending_action(), Some(SessionAction::ShutDown));
    }

    #[test]
    fn confirm_within_timeout_returns_action_and_clears() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::Restart, 1_000);
        assert_eq!(state.confirm(1_020), Some(SessionAction::Restart));
        assert!(!state.is_pending());
        // A second confirm has nothing to return.
        assert_eq!(state.confirm(1_021), None);
    }

    #[test]
    fn confirm_at_the_exact_deadline_is_accepted() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::LogOut, 0);
        assert_eq!(state.confirm(30), Some(SessionAction::LogOut));
    }

    #[test]
    fn expired_timeout_returns_none_and_clears() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::ShutDown, 100);
        assert_eq!(state.confirm(200), None);
        assert!(!state.is_pending());
        assert_eq!(state.pending_action(), None);
    }

    #[test]
    fn cancel_clears_pending() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::ShutDown, 5);
        state.cancel();
        assert!(!state.is_pending());
        assert_eq!(state.confirm(6), None);
    }

    #[test]
    fn non_destructive_actions_are_immediate() {
        let mut state = ConfirmState::new(30);
        assert_eq!(
            state.request(SessionAction::Lock, 10),
            ConfirmPrompt::Immediate(SessionAction::Lock)
        );
        assert!(!state.is_pending());
        assert_eq!(
            state.request(SessionAction::Suspend, 11),
            ConfirmPrompt::Immediate(SessionAction::Suspend)
        );
        assert!(!state.is_pending());
    }

    #[test]
    fn non_destructive_request_does_not_disturb_pending() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::ShutDown, 10);
        state.request(SessionAction::Lock, 12);
        assert_eq!(state.pending_action(), Some(SessionAction::ShutDown));
    }

    #[test]
    fn new_request_replaces_the_pending_action() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::ShutDown, 0);
        assert_eq!(
            state.request(SessionAction::Restart, 100),
            ConfirmPrompt::RequiresConfirmation(SessionAction::Restart)
        );
        assert_eq!(state.pending_action(), Some(SessionAction::Restart));
        // The timeout is measured from the replacement request (t=100).
        assert_eq!(state.confirm(125), Some(SessionAction::Restart));
    }

    #[test]
    fn replaced_deadline_uses_the_new_request_time() {
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::ShutDown, 0);
        state.request(SessionAction::Restart, 100);
        // 120s is long past the original request, but only 20s after the
        // replacement, so it must still confirm.
        assert_eq!(state.confirm(120), Some(SessionAction::Restart));
    }

    #[test]
    fn confirm_without_a_request_is_none() {
        let mut state = ConfirmState::new(30);
        assert_eq!(state.confirm(1), None);
    }

    #[test]
    fn remaining_secs_tracks_the_deadline() {
        let mut state = ConfirmState::new(30);
        assert_eq!(state.remaining_secs(0), None);
        state.request(SessionAction::Restart, 1_000);
        assert_eq!(state.remaining_secs(1_010), Some(20));
        assert_eq!(state.remaining_secs(1_040), Some(0));
        assert_eq!(state.remaining_secs(1_070), Some(0));
    }

    #[test]
    fn default_uses_the_standard_timeout() {
        assert_eq!(ConfirmState::default().timeout_secs(), DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn clock_going_backwards_still_confirms() {
        // Saturating arithmetic must not panic or wrongly reject.
        let mut state = ConfirmState::new(30);
        state.request(SessionAction::ShutDown, 1_000);
        assert_eq!(state.confirm(990), Some(SessionAction::ShutDown));
    }
}
