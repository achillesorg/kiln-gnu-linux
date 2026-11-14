//! Validation issues for theme files and configuration.
//!
//! Every problem names the exact key path and says what happened to the value
//! (ignored, clamped, replaced), so a user editing TOML by hand gets an
//! actionable message instead of a silently ignored setting.

use std::fmt;

/// How serious an issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The value was applied but is questionable (e.g. low contrast) or was
    /// adjusted (e.g. a Clay shadow blur forced to 0, an unknown key ignored).
    Warning,
    /// The value could not be used; the previous layer's value stays.
    Error,
}

/// One validation problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    /// Dotted key path, e.g. `panel.height` or `palette.dark.accent`.
    pub path: String,
    pub message: String,
}

impl Issue {
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn warning(path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            severity: Severity::Warning,
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// Prefix every path (used when a theme-override table is embedded in a
    /// larger file, e.g. `appearance.overrides.`).
    pub fn prefixed(mut self, prefix: &str) -> Self {
        if !prefix.is_empty() {
            self.path = if self.path.is_empty() {
                prefix.trim_end_matches('.').to_string()
            } else {
                format!("{prefix}{}", self.path)
            };
        }
        self
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        if self.path.is_empty() {
            write!(f, "{level}: {}", self.message)
        } else {
            write!(f, "{level}: {}: {}", self.path, self.message)
        }
    }
}

/// True when any issue is an error.
pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(Issue::is_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_path() {
        let i = Issue::error("panel.height", "300 is outside 24..=96; ignored");
        assert_eq!(
            i.to_string(),
            "error: panel.height: 300 is outside 24..=96; ignored"
        );
        let w = Issue::warning("accent", "low contrast").prefixed("appearance.");
        assert_eq!(w.path, "appearance.accent");
        assert!(!w.is_error());
        assert!(has_errors(&[w, i]));
    }
}
