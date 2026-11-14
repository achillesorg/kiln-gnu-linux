//! Automated WCAG 2.2 contrast audit over the theme palettes.
//!
//! Every required foreground/background pair the shell actually renders is
//! mapped from the CSS emitted by [`glaze_common::theme::ThemeTokens`]
//! (`crates/glaze-common/src/theme.rs`) and evaluated **from the real palette**
//! for every `(ThemeKind, ColorScheme)`. Ratios are computed with
//! [`glaze_common::color::Color::contrast_ratio`]; nothing is hard-coded.
//!
//! `GLAZE.md` §3.3 requires that "contrast and boundaries must remain readable
//! without shadows" and §6.5 targets WCAG 2.2 AA.

use glaze_common::config::{ColorScheme, ThemeKind};
use glaze_common::theme::{ThemeRole, ThemeTokens};
use serde::Serialize;

/// WCAG 2.2 AA minimum contrast for normal-size text.
pub const AA_NORMAL_TEXT: f64 = 4.5;
/// WCAG 2.2 AA minimum contrast for large text / secondary text.
pub const AA_LARGE_TEXT: f64 = 3.0;

/// A required foreground/background colour pair, with where the shell uses it.
pub struct Requirement {
    /// Foreground (text/accent) role.
    pub fg: ThemeRole,
    /// Background (surface) role.
    pub bg: ThemeRole,
    /// Minimum acceptable WCAG contrast ratio.
    pub required: f64,
    /// Human note about the shell CSS rule that produces this pairing.
    pub usage: &'static str,
}

/// The pairs the shell CSS actually renders, mapped from `theme.rs`.
///
/// `text_secondary` is graded at the large/secondary-text threshold; the
/// semantic accent roles are graded as normal text because `theme.rs` exposes
/// them as generic colour helpers (".glaze-danger", ".glaze-success") that may
/// tint labels.
pub const REQUIRED_PAIRS: &[Requirement] = &[
    Requirement {
        fg: ThemeRole::TextPrimary,
        bg: ThemeRole::Surface,
        required: AA_NORMAL_TEXT,
        usage: "panel/taskbar text on the panel surface",
    },
    Requirement {
        fg: ThemeRole::TextPrimary,
        bg: ThemeRole::SurfaceRaised,
        required: AA_NORMAL_TEXT,
        usage: "start button / launcher / menu text on raised surfaces",
    },
    Requirement {
        fg: ThemeRole::TextPrimary,
        bg: ThemeRole::SurfaceSunken,
        required: AA_NORMAL_TEXT,
        usage: "launcher search entry text on the sunken field",
    },
    Requirement {
        fg: ThemeRole::TextSecondary,
        bg: ThemeRole::Surface,
        required: AA_LARGE_TEXT,
        usage: "unfocused task button and empty-state secondary text",
    },
    Requirement {
        fg: ThemeRole::AccentText,
        bg: ThemeRole::Accent,
        required: AA_NORMAL_TEXT,
        usage: "labels on selected launcher rows and focused task buttons",
    },
    Requirement {
        fg: ThemeRole::Danger,
        bg: ThemeRole::SurfaceRaised,
        required: AA_NORMAL_TEXT,
        usage: ".glaze-danger status text on raised surfaces",
    },
    Requirement {
        fg: ThemeRole::Success,
        bg: ThemeRole::SurfaceRaised,
        required: AA_NORMAL_TEXT,
        usage: ".glaze-success status text on raised surfaces",
    },
    Requirement {
        fg: ThemeRole::TextPrimary,
        bg: ThemeRole::Panel,
        required: AA_NORMAL_TEXT,
        usage: "taskbar labels on the panel fill (design v2)",
    },
    Requirement {
        fg: ThemeRole::TextSecondary,
        bg: ThemeRole::Panel,
        required: AA_LARGE_TEXT,
        usage: "clock date and minimised task labels on the panel",
    },
    Requirement {
        fg: ThemeRole::AccentSoftText,
        bg: ThemeRole::AccentSoft,
        required: AA_NORMAL_TEXT,
        usage: "group-count pill and menu hover text on the soft accent",
    },
    Requirement {
        fg: ThemeRole::Ink,
        bg: ThemeRole::Panel,
        required: AA_LARGE_TEXT,
        usage: "ink glyphs (pinned, tray, toggle icons) on the panel (non-text, 3:1)",
    },
];

/// The result of evaluating one [`Requirement`] against one theme palette.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContrastCheck {
    pub theme: ThemeKind,
    pub scheme: ColorScheme,
    pub foreground_role: &'static str,
    pub background_role: &'static str,
    pub ratio: f64,
    pub required: f64,
    pub pass: bool,
}

impl ContrastCheck {
    /// Short human label, e.g. `Glaze Clay (light)`.
    pub fn theme_label(&self) -> String {
        format!("{} ({})", self.theme.label(), self.scheme.label())
    }
}

/// The full audit across every theme/scheme combination.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditReport {
    pub checks: Vec<ContrastCheck>,
    pub failures: usize,
}

impl AuditReport {
    /// Number of checks that passed.
    pub fn passed(&self) -> usize {
        self.checks.len() - self.failures
    }

    /// True when every required pair clears its threshold in every theme.
    pub fn all_pass(&self) -> bool {
        self.failures == 0
    }

    /// Iterate only the failing checks.
    pub fn failed_checks(&self) -> impl Iterator<Item = &ContrastCheck> {
        self.checks.iter().filter(|c| !c.pass)
    }
}

fn evaluate(theme: ThemeKind, scheme: ColorScheme, req: &Requirement) -> ContrastCheck {
    let tokens = ThemeTokens::for_kind(theme, scheme);
    let fg = tokens.palette.get(req.fg);
    let bg = tokens.palette.get(req.bg);
    let ratio = fg.contrast_ratio(bg);
    ContrastCheck {
        theme,
        scheme,
        foreground_role: req.fg.key(),
        background_role: req.bg.key(),
        ratio,
        required: req.required,
        // Tiny epsilon so a mathematically-exact ratio (e.g. 4.5) passes.
        pass: ratio + 1e-9 >= req.required,
    }
}

/// Evaluate every required pair for every `(ThemeKind, ColorScheme)`.
pub fn audit_all_themes() -> AuditReport {
    let mut checks =
        Vec::with_capacity(ThemeKind::ALL.len() * ColorScheme::ALL.len() * REQUIRED_PAIRS.len());
    for theme in ThemeKind::ALL {
        for scheme in ColorScheme::ALL {
            for req in REQUIRED_PAIRS {
                checks.push(evaluate(theme, scheme, req));
            }
        }
    }
    let failures = checks.iter().filter(|c| !c.pass).count();
    AuditReport { checks, failures }
}

/// Render the audit as pretty JSON (for CI artefacts).
pub fn report_to_json(report: &AuditReport) -> String {
    serde_json::to_string_pretty(report)
        .unwrap_or_else(|e| format!("{{\"error\":\"failed to serialise audit: {e}\"}}"))
}

/// Render the audit as a human-readable fixed-width table.
///
/// One line per check; failing rows carry `FAIL`, passing rows `PASS`, so the
/// output can be counted by tooling.
pub fn report_to_text(report: &AuditReport) -> String {
    let mut out = String::with_capacity(1024 + report.checks.len() * 80);
    out.push_str("Glaze contrast audit — WCAG 2.2 AA (computed from live palette data)\n");
    out.push_str(&format!(
        "{:<8} {:<6} {:<14} {:<16} {:>6}  {:>5}  {}\n",
        "theme", "scheme", "foreground", "background", "ratio", "req", "result"
    ));
    for c in &report.checks {
        out.push_str(&format!(
            "{:<8} {:<6} {:<14} {:<16} {:>6.2}  {:>5.1}  {}\n",
            c.theme.slug(),
            c.scheme.slug(),
            c.foreground_role,
            c.background_role,
            c.ratio,
            c.required,
            if c.pass { "PASS" } else { "FAIL" },
        ));
    }
    out.push_str(&format!(
        "{} checks, {} passed, {} failed\n",
        report.checks.len(),
        report.passed(),
        report.failures
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_covers_every_theme_and_required_pair() {
        let report = audit_all_themes();
        let expected = ThemeKind::ALL.len() * ColorScheme::ALL.len() * REQUIRED_PAIRS.len();
        assert_eq!(expected, 44, "11 pairs x 4 theme/scheme combinations");
        assert_eq!(report.checks.len(), expected);
        assert_eq!(report.passed() + report.failures, report.checks.len());
    }

    /// The shell's own text colours must clear AA in every theme. This is the
    /// gate the design system commits to (`GLAZE.md` §6.5, `theme.rs` tests).
    #[test]
    fn shell_text_pairs_meet_wcag_aa() {
        let report = audit_all_themes();
        for check in &report.checks {
            let is_core_text = matches!(
                check.foreground_role,
                "text_primary" | "text_secondary" | "accent_text"
            );
            if is_core_text {
                assert!(
                    check.pass,
                    "{}: {} on {} = {:.3} < required {:.1}",
                    check.theme_label(),
                    check.foreground_role,
                    check.background_role,
                    check.ratio,
                    check.required
                );
            }
        }
    }

    /// The semantic status accents (`danger`/`success`) are exposed as text
    /// helpers, so they must clear WCAG AA as text on raised surfaces too.
    ///
    /// The palette was fixed in `glaze-common` (Clay-light accents darkened,
    /// Classic-dark accents lightened); this test is now a hard gate that fails
    /// if any pair regresses.
    #[test]
    fn semantic_accents_meet_wcag_aa() {
        let report = audit_all_themes();
        let failing: Vec<String> = report
            .failed_checks()
            .map(|c| {
                format!(
                    "{}: {} on {} = {:.2} < {:.1}",
                    c.theme_label(),
                    c.foreground_role,
                    c.background_role,
                    c.ratio,
                    c.required
                )
            })
            .collect();
        assert!(
            failing.is_empty(),
            "contrast regressions detected:\n{}",
            failing.join("\n")
        );
        assert_eq!(report.failures, 0);
        assert!(report.all_pass());
    }

    #[test]
    fn text_report_has_one_row_per_check() {
        let report = audit_all_themes();
        let text = report_to_text(&report);
        let rows = text
            .lines()
            .filter(|l| l.contains("PASS") || l.contains("FAIL"))
            .count();
        assert_eq!(rows, report.checks.len());
        assert!(text.contains("WCAG 2.2 AA"));
        // Header/summary must not be counted as rows.
        assert!(text.contains("44 checks"));
    }

    #[test]
    fn json_report_parses_and_matches_counts() {
        let report = audit_all_themes();
        let json = report_to_json(&report);
        let value: serde_json::Value = serde_json::from_str(&json).expect("audit JSON parses");
        assert_eq!(
            value["checks"].as_array().unwrap().len(),
            report.checks.len()
        );
        assert_eq!(
            value["failures"].as_u64().unwrap() as usize,
            report.failures
        );
        // Field names on the wire are kebab-case enums from glaze-common.
        assert_eq!(value["checks"][0]["theme"], "clay");
        assert_eq!(value["checks"][0]["scheme"], "light");
    }

    /// Ratios must equal a fresh computation, never a stored constant.
    #[test]
    fn ratios_recompute_from_the_palette() {
        let tokens = ThemeTokens::default_clay();
        let report = audit_all_themes();
        let first = &report.checks[0];
        let expected = tokens
            .palette
            .text_primary
            .contrast_ratio(tokens.palette.surface);
        assert!((first.ratio - expected).abs() < 1e-12);
    }
}
