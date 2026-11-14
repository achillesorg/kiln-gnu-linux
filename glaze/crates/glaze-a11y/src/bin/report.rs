//! `glaze-a11y-report` — print the accessibility contrast audit.
//!
//! Usage:
//! - `glaze-a11y-report`          human table (contrast + focus + reduced motion)
//! - `glaze-a11y-report --json`   machine-readable JSON (the contrast audit)
//! - `glaze-a11y-report --strict` exit non-zero when any check fails (for CI)
//!
//! Ratios are computed from the live `glaze-common` palette data on every run;
//! nothing is cached or hard-coded.

use std::process::ExitCode;

use glaze_a11y::focus::{audit_focus_indicators, focus_report_to_text};
use glaze_a11y::{audit_all_themes, report_to_json, report_to_text};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let as_json = args.iter().any(|a| a == "--json");
    let strict = args.iter().any(|a| a == "--strict");

    let contrast = audit_all_themes();
    let focus = audit_focus_indicators();

    if as_json {
        println!("{}", report_to_json(&contrast));
    } else {
        print!("{}", report_to_text(&contrast));
        print!("{}", focus_report_to_text(&focus));
        let total_failures = contrast.failures + focus.failures;
        println!(
            "\nSummary: {} contrast checks ({} failed), {} focus checks ({} failed), \
             reduced-motion honoured by {}/{} themes.",
            contrast.checks.len(),
            contrast.failures,
            focus.checks.len(),
            focus.failures,
            focus.reduced_motion.themes_honouring_reduced_motion,
            focus.reduced_motion.themes_checked,
        );
        if total_failures == 0 {
            println!("Result: PASS — every required pair meets WCAG 2.2 AA.");
        } else {
            println!("Result: FAIL — {total_failures} check(s) below their WCAG 2.2 AA threshold.");
        }
    }

    if strict && !contrast.all_pass() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
