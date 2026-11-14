//! Focus-indicator contrast checks and reduced-motion reporting.
//!
//! `GLAZE.md` §6.5 requires a "meaningful focus ring" and support for
//! reduced-motion preferences; §6.4 requires a "no-animation mode". The focus
//! ring (`--glaze_focus`) is a non-text UI component, so WCAG 1.4.11 requires a
//! **3.0** contrast ratio against the surfaces it is drawn on.

use glaze_common::config::{ColorScheme, ThemeKind};
use glaze_common::theme::{CssOptions, ThemeRole, ThemeTokens};
use serde::Serialize;

/// WCAG 2.2 AA minimum for non-text UI components (focus rings, indicators).
pub const AA_NON_TEXT: f64 = 3.0;

/// Surfaces a focus ring is drawn against.
const FOCUS_BACKGROUNDS: [ThemeRole; 3] = [
    ThemeRole::Canvas,
    ThemeRole::Surface,
    ThemeRole::SurfaceRaised,
];

/// The `focus` role evaluated against one background surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FocusCheck {
    pub theme: ThemeKind,
    pub scheme: ColorScheme,
    pub background_role: &'static str,
    pub ratio: f64,
    pub required: f64,
    pub pass: bool,
}

impl FocusCheck {
    /// Short human label, e.g. `Glaze Clay (light)`.
    pub fn theme_label(&self) -> String {
        format!("{} ({})", self.theme.label(), self.scheme.label())
    }
}

/// Whether every theme honours `prefers-reduced-motion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReducedMotionReport {
    /// Theme/scheme combinations checked.
    pub themes_checked: usize,
    /// Combinations whose reduced-motion CSS contains no non-zero duration.
    pub themes_honouring_reduced_motion: usize,
    /// True when every theme collapses all transitions to `0ms`.
    pub all_honour_reduced_motion: bool,
}

/// Focus-indicator contrast plus the reduced-motion summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FocusReport {
    pub checks: Vec<FocusCheck>,
    pub failures: usize,
    pub reduced_motion: ReducedMotionReport,
}

impl FocusReport {
    /// True when every focus ring clears 3.0 and reduced motion is honoured.
    pub fn all_pass(&self) -> bool {
        self.failures == 0 && self.reduced_motion.all_honour_reduced_motion
    }
}

/// Evaluate the focus-ring colour against every surface it can be drawn on.
pub fn audit_focus_indicators() -> FocusReport {
    let mut checks =
        Vec::with_capacity(ThemeKind::ALL.len() * ColorScheme::ALL.len() * FOCUS_BACKGROUNDS.len());
    for theme in ThemeKind::ALL {
        for scheme in ColorScheme::ALL {
            let tokens = ThemeTokens::for_kind(theme, scheme);
            for bg in FOCUS_BACKGROUNDS {
                let ratio = tokens.palette.focus.contrast_ratio(tokens.palette.get(bg));
                checks.push(FocusCheck {
                    theme,
                    scheme,
                    background_role: bg.key(),
                    ratio,
                    required: AA_NON_TEXT,
                    pass: ratio + 1e-9 >= AA_NON_TEXT,
                });
            }
        }
    }
    let failures = checks.iter().filter(|c| !c.pass).count();
    FocusReport {
        checks,
        failures,
        reduced_motion: reduced_motion_report(),
    }
}

/// Scan the emitted CSS for a digit run immediately followed by `ms` and report
/// whether any of those durations is non-zero.
fn has_nonzero_duration(css: &str) -> bool {
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if css[i..].starts_with("ms") && css[start..i].bytes().any(|b| b != b'0') {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Report whether every theme collapses transitions under reduced motion while
/// keeping real durations in the default (non-reduced) CSS.
pub fn reduced_motion_report() -> ReducedMotionReport {
    let mut themes_checked = 0;
    let mut honouring = 0;
    for theme in ThemeKind::ALL {
        for scheme in ColorScheme::ALL {
            themes_checked += 1;
            let tokens = ThemeTokens::for_kind(theme, scheme);
            let reduced = tokens.to_gtk_css_with(CssOptions {
                reduced_motion: true,
                focus_rings: true,
                ..CssOptions::default()
            });
            let normal = tokens.to_gtk_css_with(CssOptions {
                reduced_motion: false,
                focus_rings: true,
                ..CssOptions::default()
            });
            // Honoured when: reduced CSS has durations but none non-zero, and
            // the default CSS actually animates (so "0ms everywhere" is a real
            // collapse, not a theme that never animated at all).
            let honours = reduced.contains("transition")
                && !has_nonzero_duration(&reduced)
                && has_nonzero_duration(&normal);
            if honours {
                honouring += 1;
            }
        }
    }
    ReducedMotionReport {
        themes_checked,
        themes_honouring_reduced_motion: honouring,
        all_honour_reduced_motion: honouring == themes_checked,
    }
}

/// Render the focus audit + reduced-motion summary as text.
pub fn focus_report_to_text(report: &FocusReport) -> String {
    let mut out = String::with_capacity(512 + report.checks.len() * 72);
    out.push_str("\nFocus-indicator audit — WCAG 2.2 AA non-text (3.0)\n");
    out.push_str(&format!(
        "{:<8} {:<6} {:<16} {:>6}  {:>5}  {}\n",
        "theme", "scheme", "background", "ratio", "req", "result"
    ));
    for c in &report.checks {
        out.push_str(&format!(
            "{:<8} {:<6} {:<16} {:>6.2}  {:>5.1}  {}\n",
            c.theme.slug(),
            c.scheme.slug(),
            c.background_role,
            c.ratio,
            c.required,
            if c.pass { "PASS" } else { "FAIL" },
        ));
    }
    let rm = &report.reduced_motion;
    out.push_str(&format!(
        "{} checks, {} failed; reduced-motion honoured by {}/{} themes\n",
        report.checks.len(),
        report.failures,
        rm.themes_honouring_reduced_motion,
        rm.themes_checked,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_covers_all_themes_and_surfaces() {
        let report = audit_focus_indicators();
        assert_eq!(
            report.checks.len(),
            ThemeKind::ALL.len() * ColorScheme::ALL.len() * FOCUS_BACKGROUNDS.len()
        );
        assert_eq!(report.checks.len(), 12);
    }

    #[test]
    fn focus_rings_meet_non_text_contrast() {
        let report = audit_focus_indicators();
        assert_eq!(
            report.failures, 0,
            "focus ring below 3.0: {:?}",
            report.checks
        );
        assert!(report.all_pass());
    }

    #[test]
    fn reduced_motion_is_honoured_everywhere() {
        let report = reduced_motion_report();
        assert_eq!(report.themes_checked, 4);
        assert_eq!(report.themes_honouring_reduced_motion, 4);
        assert!(report.all_honour_reduced_motion);
    }

    #[test]
    fn nonzero_duration_scanner_works() {
        assert!(!has_nonzero_duration("transition: color 0ms ease;"));
        assert!(has_nonzero_duration("transition: color 130ms ease;"));
        assert!(has_nonzero_duration(
            "transition: color 0ms ease, background 190ms ease;"
        ));
        assert!(!has_nonzero_duration("no durations here"));
        // "130ms" must not be mistaken for a zero duration.
        assert!(has_nonzero_duration("130ms"));
    }

    #[test]
    fn focus_text_has_one_row_per_check() {
        let report = audit_focus_indicators();
        let text = focus_report_to_text(&report);
        let rows = text
            .lines()
            .filter(|l| l.contains("PASS") || l.contains("FAIL"))
            .count();
        assert_eq!(rows, report.checks.len());
    }
}
