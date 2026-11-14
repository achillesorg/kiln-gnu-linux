//! `glaze-shell-core` — the GTK-free brain of the shell.
//!
//! All non-visual shell behaviour lives here so it can be unit-tested on any
//! host while the GTK view layer in `apps/glaze-shell` stays thin:
//!
//! - [`bar`]      — taskbar v2 model: grouping, labels, layout, click rules, pins.
//! - [`panel`]    — panel item ordering (`GLAZE.md` §6.1) and overflow policy.
//! - [`launcher`] — the launcher state machine (query, selection, navigation,
//!   open/close). This is what makes keyboard behaviour testable.
//! - [`form_factor`] — desktop vs mobile layout selection from config/display.
//! - [`mobile`]   — touch layout model: bottom nav, gestures, paged app grid.
//! - [`clock`]    — date/time formatting from configuration (deterministic;
//!   the caller supplies the current time).
//! - [`tasks`]    — grouping of live windows into task groups.
//! - [`taskbar`]  — merging pinned apps with live groups so pinned vs active is
//!   unmistakable (`GLAZE.md` §6.1).
//! - [`tray`]     — the tray/status-item view model and overflow policy.
//! - [`notifications`] — the unread badge / Do-Not-Disturb indicator state.
//! - [`status`]   — shell status and an honest compositor-backend summary.
//! - [`socket_client`] — the control-protocol client trait, an in-process
//!   implementation (portable) and the real Unix-socket client.
//! - [`status_feed`] — turns the service's tray/notification replies into the
//!   shell's tray model and notification badge (`poll_status`).
//! - [`view`]     — the panel view model that composes all of the above into a
//!   single render-ready structure, keeping the GTK layer a thin binder.
//!
//! Nothing in this crate depends on GTK, Wayland or a running desktop, so the
//! shell's interaction rules can be verified on the Windows authoring host as
//! well as on Fedora. See `docs/adr/ADR-0001-architecture.md`.

pub mod bar;
pub mod clock;
pub mod form_factor;
pub mod launcher;
pub mod mobile;
pub mod notifications;
pub mod panel;
pub mod socket_client;
pub mod startmenu;
pub mod status;
pub mod status_feed;
pub mod taskbar;
pub mod tasks;
pub mod tray;
pub mod view;

/// Crate version, re-exported for the shell's about/status output.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
