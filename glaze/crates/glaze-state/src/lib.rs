//! `glaze-state` — persistent, non-configuration state for the launcher
//! (recently used applications, launch counts, MRU/frequency ordering, pinned
//! resolution). Portable and unit-tested; the file lives under
//! `$XDG_STATE_HOME/glaze/state.toml`.
//!
//! This crate owns the *derived* side of the launcher experience described in
//! `GLAZE.md` §5 (P1 "recent apps") and §6.2 (launcher). It never fabricates
//! applications: every id it stores is mapped back onto real
//! [`glaze_common::model::AppEntry`] values by [`resolve`], and unknown ids are
//! dropped rather than shown.
//!
//! Module map:
//! - [`state`]    — [`state::LauncherState`], versioned TOML persistence.
//! - [`resolve`]  — pinned/recent ids → real registry entries.
//! - [`ordering`] — frecency (decayed recency + `ln(count)`) blend.

pub mod ordering;
pub mod resolve;
pub mod state;

pub use ordering::{blended_order, frecency, DEFAULT_HALF_LIFE_SECS};
pub use resolve::{recent_apps, resolve_pinned};
pub use state::{LauncherState, RecentEntry, StateError, CURRENT_VERSION};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
