//! `glaze-wm` — the portable window-management model shared by the compositor
//! and the shell: geometry/snapping, tiling layouts, workspace state,
//! keybindings and the Alt+Tab MRU switcher.
//!
//! `GLAZE.md` §5 P1/P2 require real window management — focus, move/resize, task
//! switching, shortcuts, multiple workspaces, tiling/snapping and alt-tab. The
//! parts of that which are **policy rather than platform** live here so they are
//! unit-tested on every host (including the Windows authoring host) and are
//! identical on x86_64 and aarch64, per `docs/adr/ADR-0005-portability-and-verification.md`.
//! Nothing in this crate performs a platform call; applying these decisions to
//! real surfaces is `#[cfg(unix)]` code elsewhere.
//!
//! Module map:
//! - [`geometry`]   — [`Rect`] plus [`snap_zone`] / [`detect_zone`].
//! - [`tiling`]     — [`Layout`] and [`tile`], exact non-overlapping rectangles.
//! - [`workspaces`] — [`WorkspaceModel`]: count, active index, reconcile.
//! - [`keybindings`]— [`Binding`] parsing/matching and the default [`Keymap`].
//! - [`alt_tab`]    — [`AltTab`]: the deterministic MRU switcher.
//! - [`layout_mode`]— the stacking <-> tiling toggle ([`LayoutPolicy`]) and
//!   [`arrange`] (stacking geometry or a dwindle/master tiling).

pub mod alt_tab;
pub mod geometry;
pub mod keybindings;
pub mod layout_mode;
pub mod tiling;
pub mod workspaces;

pub use alt_tab::AltTab;
pub use geometry::{detect_zone, snap_zone, Rect, SnapZone, SNAP_CORNER_SIZE, SNAP_EDGE_THRESHOLD};
pub use keybindings::{BindError, Binding, Key, Keymap, Mods, WmAction};
pub use layout_mode::{
    arrange, dwindle, ArrangeInput, LayoutMode, LayoutPolicy, LayoutScope, ModeRequest,
    TilingAlgorithm,
};
pub use tiling::{tile, Layout, DEFAULT_MASTER_RATIO, MAX_MASTER_RATIO, MIN_MASTER_RATIO};
pub use workspaces::WorkspaceModel;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
