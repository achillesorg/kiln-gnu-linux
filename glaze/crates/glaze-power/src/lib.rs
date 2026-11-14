//! `glaze-power` — session and power actions behind real confirmation gating.
//!
//! `GLAZE.md` §5 (P1) requires "Proper shutdown/restart/logout confirmation, no
//! destructive shortcuts", and §4.3 requires power/lock integrations to have
//! "proper permissions and graceful unsupported states". This crate provides
//! that contract:
//!
//! - [`SessionAction`] — the action catalogue with i18n keys, a destructive
//!   flag and honest availability.
//! - [`ConfirmState`] — a clock-injected confirmation state machine so
//!   destructive actions cannot fire by accident.
//! - [`command_for`] — the exact argv per action (never a shell).
//! - [`unavailable_reason`] — why an action is not offered, or `None`.
//! - [`SessionEnv`] — the verified session facts that decide availability.
//! - [`invoke`] — the thin, Unix-only spawner.
//!
//! Everything except [`invoke`] is portable and unit-tested on any host
//! (ADR-0005); `invoke` is `#[cfg(unix)]` and only *compiles* on the Linux
//! targets here.

pub mod actions;
pub mod command;
pub mod confirm;
pub mod env;
pub mod invoke;

pub use actions::SessionAction;
pub use command::{command_for, unavailable_reason};
pub use confirm::{ConfirmPrompt, ConfirmState, DEFAULT_TIMEOUT_SECS};
pub use env::SessionEnv;
pub use invoke::invoke;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Errors returned when running a session/power action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerError {
    /// The action is not offered in this session ([`unavailable_reason`]).
    Unsupported(&'static str),
    /// The action's process could not be spawned.
    Spawn(String),
}

impl std::fmt::Display for PowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PowerError::Unsupported(reason) => write!(f, "action unavailable: {reason}"),
            PowerError::Spawn(message) => {
                write!(f, "failed to start the action process: {message}")
            }
        }
    }
}

impl std::error::Error for PowerError {}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn power_error_is_display_and_error() {
        let err = super::PowerError::Unsupported("nope");
        assert!(err.to_string().contains("nope"));
        let _: &dyn std::error::Error = &err;
    }
}
