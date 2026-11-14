//! `glaze-common` — the shared contract for the whole Glaze desktop.
//!
//! This crate is deliberately **portable** (no Linux/Wayland/GTK dependency) so
//! that the design tokens, configuration model, IPC protocol and shared data
//! models can be unit-tested on any host, including the Windows development
//! host used to author Glaze. The GTK4 shell, the settings app and the portable
//! service crate all consume exactly these types so the theme and behaviour stay
//! consistent across every Glaze surface (see `GLAZE.md` §3.4).
//!
//! Module map:
//! - [`color`]   — re-export of `glaze_design::color`.
//! - [`theme`]   — re-export of the `glaze-design` tokens and CSS options.
//! - [`config`]  — versioned, migration-aware user configuration.
//! - [`model`]   — applications, windows and workspaces.
//! - [`ipc`]     — versioned local request/response protocol.
//! - [`paths`]   — XDG base directory resolution.
//! - [`wallpaper`] — bundled wallpaper manifest and variant selection.

/// Re-export of [`glaze_design::color`] (moved in design system v2).
pub use glaze_design::color;
pub mod config;
pub mod config_check;
pub mod ipc;
pub mod model;
pub mod paths;
/// Re-export of the design tokens. `glaze_common::theme::*` stays valid; the
/// tokens and CSS emitter now live in `glaze-design` (`theme` + `css`).
pub mod theme {
    pub use glaze_design::css::CssOptions;
    pub use glaze_design::theme::*;
}
pub mod wallpaper;

/// Reverse-DNS application identifier prefix. See `docs/adr/ADR-0001-architecture.md`.
pub const APP_ID_PREFIX: &str = "org.glaze";

/// Human facing product name.
pub const PRODUCT_NAME: &str = "Glaze Desktop";

/// Short product name used for binaries and packages (`glaze-`).
pub const PRODUCT_SHORT: &str = "Glaze";

/// Semantic version of the whole Glaze suite.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use color::Color;
pub use config::{ColorScheme, GlazeConfig, FormFactorPref, ThemeKind};
pub use theme::{Palette, ThemeMetrics, ThemeTokens};
