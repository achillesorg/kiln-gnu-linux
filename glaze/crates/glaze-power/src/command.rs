//! Building the real argv for a session/power action.
//!
//! Glaze **never** goes through a shell here (`GLAZE.md` §4.1: no shell
//! interpolation). Each action maps to a fixed program plus literal arguments,
//! so nothing from the environment is ever interpreted as code.
//!
//! Precedence, documented:
//!
//! - Machine power (`shutdown`/`restart`/`suspend`/`hibernate`) prefers the
//!   system `systemctl`, the canonical Fedora mechanism.
//! - Session-scoped actions (`log out`, `lock`) prefer `loginctl`, which targets
//!   exactly this session handle.
//! - `log out` falls back to `systemctl --user exit` when logind has no session
//!   id for us but a `systemd --user` manager is running.
//! - `lock` falls back to a Wayland locker (`swaylock`) when logind is absent.
//! - `switch user` uses the LightDM `dm-tool`; because Glaze cannot verify a
//!   display-manager switch command from a session, this action is *always*
//!   reported unavailable (see [`unavailable_reason`]) even though its argv is
//!   defined.
//!
//! [`command_for`] always returns the preferred argv; callers must consult
//! [`unavailable_reason`] (or [`crate::SessionAction::is_available_on`]) before
//! running it.

use crate::actions::SessionAction;
use crate::env::SessionEnv;

/// The argv Glaze would execute for `action` in `env` (program first).
///
/// Best-effort: the result is meaningful even for actions that
/// [`unavailable_reason`] reports as unavailable, so it can be logged and shown.
pub fn command_for(action: SessionAction, env: &SessionEnv) -> Vec<String> {
    match action {
        SessionAction::ShutDown => vec!["systemctl".into(), "poweroff".into()],
        SessionAction::Restart => vec!["systemctl".into(), "reboot".into()],
        SessionAction::Suspend => vec!["systemctl".into(), "suspend".into()],
        SessionAction::Hibernate => vec!["systemctl".into(), "hibernate".into()],
        SessionAction::LogOut => match (
            env.has_logind,
            env.session_id.as_deref(),
            env.has_systemd_user,
        ) {
            // Preferred: terminate exactly this logind session.
            (true, Some(id), _) => vec![
                "loginctl".into(),
                "terminate-session".into(),
                id.to_string(),
            ],
            // Fallback: stop the systemd user manager, which ends the session.
            (_, _, true) => vec!["systemctl".into(), "--user".into(), "exit".into()],
            // No verified handle; still expose a stable argv (gated unavailable).
            _ => vec!["loginctl".into(), "terminate-session".into()],
        },
        SessionAction::Lock => {
            if env.has_logind {
                let mut argv: Vec<String> = vec!["loginctl".into(), "lock-session".into()];
                if let Some(id) = env.session_id.as_deref() {
                    argv.push(id.to_string());
                }
                argv
            } else if env.is_wayland {
                vec!["swaylock".into()]
            } else {
                vec!["loginctl".into(), "lock-session".into()]
            }
        }
        SessionAction::SwitchUser => vec!["dm-tool".into(), "switch-to-greeter".into()],
    }
}

/// Why `action` cannot be offered in `env`, or `None` when it can.
///
/// `None` is a promise that Glaze has a verified mechanism; a `Some(_)` reason is
/// a plain, user-shareable explanation to show instead of a broken affordance.
pub fn unavailable_reason(action: SessionAction, env: &SessionEnv) -> Option<&'static str> {
    match action {
        SessionAction::LogOut => {
            if (env.has_logind && env.session_id.is_some()) || env.has_systemd_user {
                None
            } else {
                Some("no logind session id and no systemd user manager; cannot end the session safely")
            }
        }
        SessionAction::Restart | SessionAction::ShutDown => {
            if env.has_logind {
                None
            } else {
                Some("systemd/logind was not detected; Glaze will not guess a power command")
            }
        }
        SessionAction::Suspend | SessionAction::Hibernate => {
            if env.has_logind {
                None
            } else {
                Some("systemd/logind was not detected; cannot suspend or hibernate")
            }
        }
        SessionAction::Lock => {
            if env.has_lock_screen {
                None
            } else {
                Some("no lock screen is available in this session")
            }
        }
        SessionAction::SwitchUser => {
            Some("user switching needs a display-manager command Glaze cannot verify")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn full() -> SessionEnv {
        SessionEnv::new(Some("7".into()), true, true, true, true)
    }

    #[test]
    fn exact_argv_per_action_with_a_full_session() {
        let env = full();
        assert_eq!(
            command_for(SessionAction::ShutDown, &env),
            v(&["systemctl", "poweroff"])
        );
        assert_eq!(
            command_for(SessionAction::Restart, &env),
            v(&["systemctl", "reboot"])
        );
        assert_eq!(
            command_for(SessionAction::Suspend, &env),
            v(&["systemctl", "suspend"])
        );
        assert_eq!(
            command_for(SessionAction::Hibernate, &env),
            v(&["systemctl", "hibernate"])
        );
        assert_eq!(
            command_for(SessionAction::LogOut, &env),
            v(&["loginctl", "terminate-session", "7"])
        );
        assert_eq!(
            command_for(SessionAction::Lock, &env),
            v(&["loginctl", "lock-session", "7"])
        );
        assert_eq!(
            command_for(SessionAction::SwitchUser, &env),
            v(&["dm-tool", "switch-to-greeter"])
        );
    }

    #[test]
    fn lock_without_a_session_id_omits_the_argument() {
        let env = SessionEnv::new(None, true, false, true, true);
        assert_eq!(
            command_for(SessionAction::Lock, &env),
            v(&["loginctl", "lock-session"])
        );
    }

    #[test]
    fn logout_falls_back_to_the_user_manager_without_logind() {
        let env = SessionEnv::new(None, false, true, true, false);
        assert_eq!(
            command_for(SessionAction::LogOut, &env),
            v(&["systemctl", "--user", "exit"])
        );
    }

    #[test]
    fn logout_prefers_logind_over_the_user_manager() {
        let env = SessionEnv::new(Some("12".into()), true, true, true, true);
        assert_eq!(
            command_for(SessionAction::LogOut, &env),
            v(&["loginctl", "terminate-session", "12"])
        );
    }

    #[test]
    fn lock_falls_back_to_a_wayland_locker() {
        let wayland = SessionEnv::new(Some("3".into()), false, false, true, true);
        assert_eq!(command_for(SessionAction::Lock, &wayland), v(&["swaylock"]));

        let not_wayland = SessionEnv::new(Some("3".into()), false, false, false, true);
        assert_eq!(
            command_for(SessionAction::Lock, &not_wayland),
            v(&["loginctl", "lock-session"])
        );
    }

    #[test]
    fn no_argv_uses_a_shell_or_shell_metacharacters() {
        let envs = [
            SessionEnv::unsupported(),
            full(),
            SessionEnv::new(None, true, false, true, true),
            SessionEnv::new(None, false, true, true, false),
            SessionEnv::new(Some("3".into()), false, false, true, true),
        ];
        let forbidden = [
            ';', '|', '&', '$', '`', '>', '<', '\n', '\r', '*', '?', '\\', '"', '\'',
        ];
        for env in envs {
            for action in SessionAction::ALL {
                let argv = command_for(action, &env);
                assert!(!argv.is_empty(), "{action:?} produced an empty argv");
                for part in &argv {
                    assert!(!part.is_empty(), "{action:?} has an empty argument");
                    for c in forbidden {
                        assert!(
                            !part.contains(c),
                            "{action:?} argument {part:?} contains {c:?}"
                        );
                    }
                }
                // Never re-enter a shell.
                assert_ne!(argv[0], "sh", "{action:?}");
                assert_ne!(argv[0], "bash", "{action:?}");
                assert!(!argv.contains(&"-c".to_string()), "{action:?}");
            }
        }
    }

    #[test]
    fn unavailable_reasons_are_explicit_and_specific() {
        let bare = SessionEnv::unsupported();

        // Unsupported host: nothing is claimed.
        assert!(unavailable_reason(SessionAction::ShutDown, &bare).is_some());
        assert!(unavailable_reason(SessionAction::Restart, &bare).is_some());
        assert!(unavailable_reason(SessionAction::Suspend, &bare).is_some());
        assert!(unavailable_reason(SessionAction::Hibernate, &bare).is_some());
        assert!(unavailable_reason(SessionAction::Lock, &bare).is_some());
        assert!(unavailable_reason(SessionAction::LogOut, &bare).is_some());
        assert!(unavailable_reason(SessionAction::SwitchUser, &bare).is_some());

        // Switch user is never available, even in a fully featured session.
        assert!(unavailable_reason(SessionAction::SwitchUser, &full()).is_some());

        // Fully featured session: the rest are available.
        assert!(unavailable_reason(SessionAction::ShutDown, &full()).is_none());
        assert!(unavailable_reason(SessionAction::Restart, &full()).is_none());
        assert!(unavailable_reason(SessionAction::Suspend, &full()).is_none());
        assert!(unavailable_reason(SessionAction::Hibernate, &full()).is_none());
        assert!(unavailable_reason(SessionAction::Lock, &full()).is_none());
        assert!(unavailable_reason(SessionAction::LogOut, &full()).is_none());
    }
}
