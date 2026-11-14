//! `glaze-i18n` — locale detection and a small, honest message catalogue.
//!
//! This crate is deliberately portable and tiny: shell labels are short, human
//! strings (`GLAZE.md` §1.1), so the first slice is a correct lookup with a
//! deterministic fallback order rather than a full ICU stack.
//!
//! Module map:
//!
//! - [`locale`]   — POSIX locale detection, normalisation and fallback chain.
//! - [`catalog`]  — `key = value` catalogues, embedded + on-disk, English-first
//!   fallback.
//! - [`messages`] — the typed [`Keys`] list and the runtime [`tr`] accessor.
//!
//! Quick start (shell startup / render):
//!
//! ```
//! use glaze_i18n::{tr, Keys};
//!
//! glaze_i18n::install_from_env();
//! let label = tr(Keys::START); // "Start" in English, localised otherwise
//! assert!(!label.is_empty());
//! ```

pub mod catalog;
pub mod locale;
pub mod messages;

pub use catalog::{
    catalog_for, embedded_locales, source_catalog, Catalog, EMBEDDED, SOURCE_LOCALE,
};
pub use locale::{chain, detect, normalise};
pub use messages::{active_locale, audit, audit_locale, install, install_from_env, tr, Keys};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
