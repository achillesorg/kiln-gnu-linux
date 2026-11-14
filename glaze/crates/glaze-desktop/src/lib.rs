//! `glaze-desktop` — the desktop surface model (icons, selection, context menus).
//!
//! This crate is the portable brain of the desktop background described in
//! `GLAZE.md` §3.1 and §5 P1 ("Wallpaper, desktop surfaces and context menus;
//! minimal launchable desktop shortcuts"). Like `glaze-shell-core`, it is
//! deliberately free of GTK, Wayland and a running desktop so its behaviour can
//! be unit-tested on any host (see `docs/adr/ADR-0005-portability-and-verification.md`);
//! the GTK desktop layer in `apps/glaze-shell` merely renders what this model
//! decides.
//!
//! Module map:
//! - [`grid`] — the fixed-column icon packing engine and [`Rect`].
//! - [`icons`] — desktop icons and their resolution to a real launch argv.
//! - [`selection`] — click / Ctrl / Shift selection semantics.
//! - [`context_menu`] — desktop and icon context-menu models.
//! - [`shortcuts`] — versioned persistence of user desktop shortcuts.
//!
//! Nothing here fabricates desktop contents: application icons come only from
//! the registry's real [`glaze_common::model::AppEntry`] records.

pub mod context_menu;
pub mod grid;
pub mod icons;
pub mod selection;
pub mod shortcuts;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use context_menu::{desktop_menu, icon_menu, MenuItem, MenuModel};
pub use grid::{cells_per_column, grid_positions, GridConfig, Rect};
pub use icons::{build_icons, resolve_target, DesktopIcon, IconTarget};
pub use selection::Selection;
pub use shortcuts::{IconPos, Shortcut, ShortcutError, ShortcutFile};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
