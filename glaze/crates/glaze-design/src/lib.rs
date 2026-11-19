//! `glaze-design` — Glaze design system v2 (the single token source).
//!
//! Portable (no GTK/Wayland), so it builds and tests on the macOS authoring
//! host. Consumers: `glaze-common` (re-exports `color`/`theme` for old paths),
//! the GTK shell and settings app (CSS), `glaze-a11y` (contrast gate) and,
//! later, the compositor (frame parameters).
//!
//! - [`amoled`]    — true-black dark variant for OLED panels.
//! - [`color`]     — colour type, hex/CSS serialisation, WCAG contrast.
//! - [`kinds`]     — `ThemeKind` (Clay/Classic) and `ColorScheme` (light/dark).
//! - [`theme`]     — palettes, metrics, typography, shadow tokens.
//! - [`css`]       — the GTK4 CSS emitter.
//! - [`overrides`] — theme files / user token overrides with validation.
//! - [`issues`]    — validation issues with key paths.
//! - [`motion`]    — critically damped springs and frame-time stats.
//! - [`texture`]   — paper texture sets and their contrast gate.

pub mod amoled;
pub mod color;
pub mod css;
pub mod issues;
pub mod kinds;
pub mod motion;
pub mod overrides;
pub mod texture;
pub mod theme;

pub use color::Color;
pub use css::CssOptions;
pub use issues::{Issue, Severity};
pub use kinds::{ColorScheme, ThemeKind};
pub use overrides::ThemeOverrides;
pub use theme::{Palette, Shadow, Shadows, ThemeMetrics, ThemeRole, ThemeTokens, Typography};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
