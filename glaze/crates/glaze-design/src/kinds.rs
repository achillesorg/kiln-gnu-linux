//! Theme family and colour scheme selectors.
//!
//! Moved from `glaze-common::config` in design system v2; `glaze-common`
//! re-exports both so existing `glaze_common::config::{ThemeKind, ColorScheme}`
//! paths keep working.

use serde::{Deserialize, Serialize};

/// Visual theme family (`GLAZE.md` §3.2, `docs/decisions.md` Q7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeKind {
    /// Flat cut paper with hairline ink strokes. The default.
    #[default]
    Clay,
    /// Heritage Windows-2000-inspired token set on the same layout.
    Classic,
}

impl ThemeKind {
    /// Stable slug used in config and CLI flags.
    pub const fn slug(self) -> &'static str {
        match self {
            ThemeKind::Clay => "clay",
            ThemeKind::Classic => "classic",
        }
    }

    /// Human display label.
    pub const fn label(self) -> &'static str {
        match self {
            ThemeKind::Clay => "Glaze Clay",
            ThemeKind::Classic => "Glaze Classic",
        }
    }

    /// Parse from a slug (`clay` / `classic`), case-insensitively.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "clay" | "glaze-clay" | "glaze_clay" => Some(ThemeKind::Clay),
            "classic" | "glaze-classic" | "glaze_classic" => Some(ThemeKind::Classic),
            _ => None,
        }
    }

    /// Both themes, in preference order.
    pub const ALL: [ThemeKind; 2] = [ThemeKind::Clay, ThemeKind::Classic];
}

/// Light/dark colour scheme. Dark is a deliberate palette, not an inversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    #[default]
    Light,
    Dark,
}

impl ColorScheme {
    pub const fn slug(self) -> &'static str {
        match self {
            ColorScheme::Light => "light",
            ColorScheme::Dark => "dark",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            ColorScheme::Light => "Light",
            ColorScheme::Dark => "Dark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => Some(ColorScheme::Light),
            "dark" => Some(ColorScheme::Dark),
            _ => None,
        }
    }

    pub const ALL: [ColorScheme; 2] = [ColorScheme::Light, ColorScheme::Dark];
}
