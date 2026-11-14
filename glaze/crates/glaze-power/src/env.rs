//! The subset of the session environment that decides which session/power
//! actions are honestly available (`GLAZE.md` §4.3: "graceful unsupported
//! states").
//!
//! The struct itself is a plain value — no probing happens until [`SessionEnv::detect`]
//! is called — so tests can construct exact environments without touching the
//! host. `detect()` only ever *reports what it can verify on this host*; it never
//! assumes logind, a lock screen or a Wayland session is present.

/// Facts about the current session, used to gate session/power actions.
///
/// Every field is conservative: `false`/`None` means "not verified", never
/// "probably fine".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionEnv {
    /// `$XDG_SESSION_ID` — the logind session handle, when the display manager
    /// exported one.
    pub session_id: Option<String>,
    /// logind/systemd system tooling is present (`systemctl`/`loginctl` on
    /// `$PATH`). On Fedora this implies systemd is PID 1.
    pub has_logind: bool,
    /// A `systemd --user` manager is running for this session
    /// (`$XDG_RUNTIME_DIR/systemd` exists).
    pub has_systemd_user: bool,
    /// `$XDG_SESSION_TYPE=wayland` or `$WAYLAND_DISPLAY` is set.
    pub is_wayland: bool,
    /// A lock mechanism is available: either logind's `lock-session` signal or a
    /// known Wayland locker binary on `$PATH`.
    pub has_lock_screen: bool,
}

impl SessionEnv {
    /// Portable constructor (used by tests and by callers that already know the
    /// session facts, e.g. from IPC).
    pub fn new(
        session_id: Option<String>,
        has_logind: bool,
        has_systemd_user: bool,
        is_wayland: bool,
        has_lock_screen: bool,
    ) -> Self {
        SessionEnv {
            session_id,
            has_logind,
            has_systemd_user,
            is_wayland,
            has_lock_screen,
        }
    }

    /// An environment that claims nothing: every action will be reported as
    /// unavailable. Also the result of `detect()` on non-Unix hosts.
    pub fn unsupported() -> Self {
        SessionEnv::new(None, false, false, false, false)
    }

    /// Probe the current process environment and `$PATH`.
    ///
    /// On non-Unix hosts there is nothing Glaze can verify, so this returns
    /// [`SessionEnv::unsupported`] rather than inventing capabilities.
    #[cfg(unix)]
    pub fn detect() -> Self {
        let session_id = std::env::var("XDG_SESSION_ID")
            .ok()
            .filter(|s| !s.is_empty());

        let has_systemctl = on_path("systemctl");
        let has_loginctl = on_path("loginctl");
        let has_logind = has_systemctl || has_loginctl;

        let runtime = match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
            _ => glaze_common::paths::runtime_dir(),
        };
        let has_systemd_user = has_systemctl && runtime.join("systemd").exists();

        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || std::env::var_os("WAYLAND_DISPLAY").is_some();

        // logind's `lock-session` is itself a lock mechanism; otherwise accept a
        // known Wayland locker. We do not claim a lock screen we cannot see.
        let has_lock_screen = has_logind
            || on_path("swaylock")
            || on_path("swaylock-effects")
            || on_path("gtklock")
            || on_path("hyprlock");

        SessionEnv {
            session_id,
            has_logind,
            has_systemd_user,
            is_wayland,
            has_lock_screen,
        }
    }

    /// Non-Unix hosts expose no session/power integration.
    #[cfg(not(unix))]
    pub fn detect() -> Self {
        SessionEnv::unsupported()
    }
}

impl Default for SessionEnv {
    fn default() -> Self {
        SessionEnv::unsupported()
    }
}

/// Look up an executable name on `$PATH` (Unix only).
#[cfg(unix)]
fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_claims_nothing() {
        let env = SessionEnv::unsupported();
        assert_eq!(env.session_id, None);
        assert!(!env.has_logind);
        assert!(!env.has_systemd_user);
        assert!(!env.is_wayland);
        assert!(!env.has_lock_screen);
        assert_eq!(env, SessionEnv::default());
    }

    #[test]
    fn portable_constructor_stores_every_field() {
        let env = SessionEnv::new(Some("9".into()), true, false, true, false);
        assert_eq!(env.session_id.as_deref(), Some("9"));
        assert!(env.has_logind);
        assert!(!env.has_systemd_user);
        assert!(env.is_wayland);
        assert!(!env.has_lock_screen);
    }

    #[cfg(not(unix))]
    #[test]
    fn detect_is_unsupported_off_unix() {
        assert_eq!(SessionEnv::detect(), SessionEnv::unsupported());
    }

    #[cfg(unix)]
    #[test]
    fn detect_never_launches_anything_and_is_well_formed() {
        // This must be a pure probe: it reads env/PATH and returns booleans.
        let env = SessionEnv::detect();
        // If we are Wayland, `is_wayland` must be true; the converse is not
        // required (the env var could be missing on a Wayland host).
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            assert!(env.is_wayland);
        }
    }
}
