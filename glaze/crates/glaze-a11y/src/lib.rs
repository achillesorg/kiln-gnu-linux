//! `glaze-a11y` — accessible names/roles for shell affordances plus an
//! automated WCAG audit over the theme palettes.
//!
//! This crate is deliberately **portable** (no GTK/Wayland dependency) so the
//! accessibility contract can be unit-tested on any host, including the Windows
//! authoring host. Two jobs:
//!
//! - [`roles`] — the accessible name/role descriptors the GTK shell sets on its
//!   widgets (`GLAZE.md` §6.5: "meaningful focus ring and accessible names").
//! - [`audit`] — an evidence-producing WCAG 2.2 contrast audit computed from
//!   the *actual* [`glaze_common::theme::ThemeTokens`] palettes (`GLAZE.md` §3.3,
//!   §6.5). The `glaze-a11y-report` binary prints it for QA/CI.
//! - [`focus`] — focus-indicator contrast and reduced-motion reporting.

pub mod audit;
pub mod focus;
pub mod roles;

pub use audit::{
    audit_all_themes, report_to_json, report_to_text, AuditReport, ContrastCheck, AA_LARGE_TEXT,
    AA_NORMAL_TEXT, REQUIRED_PAIRS,
};
pub use focus::{
    audit_focus_indicators, focus_report_to_text, reduced_motion_report, FocusCheck, FocusReport,
    ReducedMotionReport, AA_NON_TEXT,
};
pub use roles::{find, A11yNode, A11yRole, SHELL_AFFORDANCES};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
