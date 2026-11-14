//! The session/power action catalogue.
//!
//! `GLAZE.md` §5 (P1) requires "Proper shutdown/restart/logout confirmation, no
//! destructive shortcuts". This module classifies each action so the shell can
//! decide (a) whether it needs a confirmation dialog and (b) whether the action
//! is honestly available in the current session.

use crate::env::SessionEnv;

/// A user-visible session or power action offered by the Glaze shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAction {
    /// End the current user session.
    LogOut,
    /// Reboot the machine.
    Restart,
    /// Power the machine off.
    ShutDown,
    /// Suspend the machine to RAM.
    Suspend,
    /// Suspend the machine to disk.
    Hibernate,
    /// Lock the screen without ending the session.
    Lock,
    /// Switch to another user's session.
    SwitchUser,
}

impl SessionAction {
    /// Every action, in the order the shell should present them (safe actions
    /// first, destructive ones last).
    pub const ALL: [SessionAction; 7] = [
        SessionAction::Lock,
        SessionAction::Suspend,
        SessionAction::Hibernate,
        SessionAction::LogOut,
        SessionAction::SwitchUser,
        SessionAction::Restart,
        SessionAction::ShutDown,
    ];

    /// Stable, machine-readable identifier (logging, IPC, tests).
    pub fn id(self) -> &'static str {
        match self {
            SessionAction::LogOut => "log_out",
            SessionAction::Restart => "restart",
            SessionAction::ShutDown => "shut_down",
            SessionAction::Suspend => "suspend",
            SessionAction::Hibernate => "hibernate",
            SessionAction::Lock => "lock",
            SessionAction::SwitchUser => "switch_user",
        }
    }

    /// Stable i18n key for the action's menu label (`glaze-i18n` catalogue).
    pub fn label_key(self) -> &'static str {
        match self {
            SessionAction::LogOut => "menu.log_out",
            SessionAction::Restart => "menu.restart",
            SessionAction::ShutDown => "menu.power_off",
            SessionAction::Suspend => "menu.suspend",
            SessionAction::Hibernate => "menu.hibernate",
            SessionAction::Lock => "menu.lock",
            SessionAction::SwitchUser => "menu.switch_user",
        }
    }

    /// Stable i18n key for the confirmation dialog body.
    ///
    /// Returns `""` for non-destructive actions, which must never prompt
    /// (`GLAZE.md` §5 P1).
    pub fn confirmation_key(self) -> &'static str {
        match self {
            SessionAction::LogOut => "confirm.log_out",
            SessionAction::Restart => "confirm.restart",
            SessionAction::ShutDown => "confirm.power_off",
            _ => "",
        }
    }

    /// `true` for actions that end the user's session or the machine and must
    /// therefore be confirmed before running.
    pub fn is_destructive(self) -> bool {
        matches!(
            self,
            SessionAction::LogOut | SessionAction::Restart | SessionAction::ShutDown
        )
    }

    /// Actions whose success depends on hardware the Glaze session cannot check
    /// (no battery/sleep-capability probe yet). They are still offered — the
    /// underlying `systemctl` call fails gracefully — but the shell may flag
    /// them (`GLAZE.md` §4.3).
    pub fn is_hardware_dependent(self) -> bool {
        matches!(self, SessionAction::Suspend | SessionAction::Hibernate)
    }

    /// Whether Glaze can honestly offer this action in `env`.
    ///
    /// Equivalent to `unavailable_reason(action, env).is_none()`; when this is
    /// `false` the shell must hide or disable the affordance rather than pretend
    /// it works.
    pub fn is_available_on(self, env: &SessionEnv) -> bool {
        crate::command::unavailable_reason(self, env).is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> SessionEnv {
        SessionEnv::new(Some("3".into()), true, true, true, true)
    }

    #[test]
    fn destructive_set_is_exactly_logout_restart_shutdown() {
        assert!(SessionAction::LogOut.is_destructive());
        assert!(SessionAction::Restart.is_destructive());
        assert!(SessionAction::ShutDown.is_destructive());
        assert!(!SessionAction::Lock.is_destructive());
        assert!(!SessionAction::Suspend.is_destructive());
        assert!(!SessionAction::Hibernate.is_destructive());
        assert!(!SessionAction::SwitchUser.is_destructive());
    }

    #[test]
    fn every_action_has_a_label_and_id() {
        for action in SessionAction::ALL {
            assert!(!action.label_key().is_empty(), "{action:?}");
            assert!(!action.id().is_empty(), "{action:?}");
        }
        // ids are unique.
        let mut ids: Vec<&str> = SessionAction::ALL.iter().map(|a| a.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SessionAction::ALL.len());
    }

    #[test]
    fn confirmation_key_is_non_empty_only_for_destructive_actions() {
        for action in SessionAction::ALL {
            if action.is_destructive() {
                assert!(!action.confirmation_key().is_empty(), "{action:?}");
            } else {
                assert_eq!(action.confirmation_key(), "", "{action:?}");
            }
        }
    }

    #[test]
    fn suspend_and_hibernate_are_hardware_dependent() {
        assert!(SessionAction::Suspend.is_hardware_dependent());
        assert!(SessionAction::Hibernate.is_hardware_dependent());
        assert!(!SessionAction::ShutDown.is_hardware_dependent());
        assert!(!SessionAction::Lock.is_hardware_dependent());
    }

    #[test]
    fn lock_needs_a_lock_screen() {
        let no_lock = SessionEnv::new(Some("1".into()), true, true, true, false);
        assert!(!SessionAction::Lock.is_available_on(&no_lock));
        assert!(SessionAction::Lock.is_available_on(&full()));
    }

    #[test]
    fn power_actions_need_logind() {
        let bare = SessionEnv::unsupported();
        assert!(!SessionAction::ShutDown.is_available_on(&bare));
        assert!(!SessionAction::Restart.is_available_on(&bare));
        assert!(!SessionAction::Suspend.is_available_on(&bare));
        assert!(!SessionAction::Hibernate.is_available_on(&bare));
        assert!(SessionAction::ShutDown.is_available_on(&full()));
        assert!(SessionAction::Restart.is_available_on(&full()));
    }

    #[test]
    fn logout_needs_a_session_handle() {
        // logind present but no session id and no user manager: cannot act.
        let no_handle = SessionEnv::new(None, true, false, true, true);
        assert!(!SessionAction::LogOut.is_available_on(&no_handle));
        // `systemd --user` is an acceptable alternative handle.
        let user_only = SessionEnv::new(None, false, true, true, false);
        assert!(SessionAction::LogOut.is_available_on(&user_only));
        // logind session id alone is enough.
        let logind_only = SessionEnv::new(Some("42".into()), true, false, true, true);
        assert!(SessionAction::LogOut.is_available_on(&logind_only));
    }

    #[test]
    fn switch_user_is_never_claimed_available() {
        assert!(!SessionAction::SwitchUser.is_available_on(&full()));
        assert!(!SessionAction::SwitchUser.is_available_on(&SessionEnv::unsupported()));
    }

    #[test]
    fn availability_agrees_with_unavailable_reason() {
        let envs = [
            SessionEnv::unsupported(),
            full(),
            SessionEnv::new(None, true, false, true, true),
            SessionEnv::new(None, false, true, true, false),
        ];
        for env in envs {
            for action in SessionAction::ALL {
                assert_eq!(
                    action.is_available_on(&env),
                    crate::command::unavailable_reason(action, &env).is_none(),
                    "{action:?} in {env:?}"
                );
            }
        }
    }
}
