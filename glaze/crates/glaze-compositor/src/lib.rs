//! `glaze-compositor` — Glaze's own Wayland compositor (Smithay 0.7).
//!
//! Structure follows Smithay's MIT-licensed `anvil` example, trimmed to what
//! Glaze needs. The runtime is Linux-only; the **policy** is portable and
//! unit-tested on every host:
//!
//! - [`wm`]      — the window-management brain (stacking order, focus,
//!   workspaces, minimise/maximise, stacking <-> tiling via `glaze-wm`).
//! - [`frame`]   — Glaze paper server-side frames: geometry, hit-testing and
//!   the CPU title-bar painter.
//! - [`anim`]    — interruptible, retargetable, time-based tweens for window
//!   open/close/minimise/restore/move, plus frame-time statistics.
//! - [`control`] — the compositor control socket protocol (tiling toggle).
//!
//! Linux-only modules: [`backend`] (winit now; DRM/libinput/libseat next, see
//! `docs/compositor.md`), [`state`], [`handlers`], [`render`], [`input`],
//! [`foreign_toplevel`] (wlr-foreign-toplevel-management server) and
//! [`linux`] (entry point).

pub const NAME: &str = "glaze-compositor";

/// True when the native compositor can run on this host.
pub fn is_available() -> bool {
    cfg!(target_os = "linux")
}

pub mod anim;
pub mod control;
pub mod frame;
pub mod wm;

#[cfg(target_os = "linux")]
pub mod backend;
#[cfg(target_os = "linux")]
pub mod cursor;
#[cfg(target_os = "linux")]
pub mod foreign_toplevel;
#[cfg(target_os = "linux")]
pub mod handlers;
#[cfg(target_os = "linux")]
pub mod input;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub mod render;
#[cfg(target_os = "linux")]
pub mod state;
#[cfg(target_os = "linux")]
pub mod xwayland;

#[cfg(test)]
mod tests {
    #[test]
    fn availability_is_platform_gated() {
        assert_eq!(super::is_available(), cfg!(target_os = "linux"));
    }
}
