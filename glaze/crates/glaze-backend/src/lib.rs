//! `glaze-backend` — compositor state backends behind one abstraction.
//!
//! The shell never talks to a compositor directly; it asks a [`Backend`] for
//! [`glaze_common::model::WindowInfo`] / [`glaze_common::model::WorkspaceInfo`]
//! snapshots. Two implementations:
//!
//! - [`foreign_toplevel`] — `wlr-foreign-toplevel-management` (standard, preferred).
//! - [`sway`] — Sway IPC `get_tree` parsing (richer, Sway-specific fallback).
//!
//! Message framing/parsing is written to be **portable and unit-tested**; only
//! the socket transport is gated to Unix.
//!
//! ```text
//!  shell / glaze-service
//!         │  asks for a snapshot
//!         ▼
//!    dyn Backend ──┬── ForeignToplevelBackend  (portable registry + wayland transport)
//!                  └── SwayBackend             (portable tree parser + unix socket transport)
//! ```

pub mod glaze;
pub mod foreign_toplevel;
pub mod sway;

#[cfg(unix)]
mod sway_transport;

pub use glaze::GlazeBackend;
pub use foreign_toplevel::{ForeignToplevelBackend, HandleId, ToplevelEvent, ToplevelRegistry};
pub use sway::{SwayBackend, SwaySnapshot};

/// Event payload parsing is portable; re-exported for tests and callers.
pub use sway::parse_event;

/// A compositor state backend.
///
/// Every method except [`Backend::name`] and [`Backend::is_connected`] has a
/// default that reports an honest [`BackendError::Unsupported`]: a backend only
/// implements the capabilities it really has, and callers get a structured error
/// rather than fabricated state.
pub trait Backend: Send {
    /// Human name for logs and the settings "About" page.
    fn name(&self) -> &'static str;
    /// True when the connection to the compositor is established.
    fn is_connected(&self) -> bool;

    /// Live toplevel windows, or an error if this backend cannot report them.
    fn windows(&self) -> Result<Vec<glaze_common::model::WindowInfo>, BackendError> {
        Err(BackendError::Unsupported(
            "this backend does not report windows",
        ))
    }

    /// Workspaces, or an error if this backend cannot report them.
    fn workspaces(&self) -> Result<Vec<glaze_common::model::WorkspaceInfo>, BackendError> {
        Err(BackendError::Unsupported(
            "this backend does not report workspaces",
        ))
    }

    /// Focus a window by backend id.
    fn activate_window(&self, _id: &str) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot activate windows",
        ))
    }

    /// Politely close a window by backend id.
    fn close_window(&self, _id: &str) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot close windows",
        ))
    }

    /// Re-read state from the compositor. Backends without a live transport may
    /// leave this as a no-op.
    fn refresh(&self) -> Result<(), BackendError> {
        Ok(())
    }

    /// Geometry of every window (for shell-drawn window controls).
    fn frames(&self) -> Result<Vec<Frame>, BackendError> {
        Err(BackendError::Unsupported(
            "this backend has no window geometry",
        ))
    }

    /// True while `id` is maximised by [`Backend::toggle_maximize`].
    fn is_maximized(&self, _id: &str) -> bool {
        false
    }

    /// Maximise to the work area (above the taskbar), or restore.
    fn toggle_maximize(&self, _id: &str) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot maximise windows",
        ))
    }

    /// Recolour compositor-drawn title bars/borders from the theme tokens:
    /// `(focused_bg, focused_text, focused_border, other_bg, other_text,
    /// other_border, urgent_bg)` as `#rrggbb`.
    fn set_frame_colors(&self, _c: &FrameColors) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot recolour frames",
        ))
    }

    /// Minimise a window (Sway: move it to the scratchpad).
    fn minimize_window(&self, _id: &str) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot minimise windows",
        ))
    }

    /// Move a window to workspace `number` (1-based, created if needed).
    fn move_to_workspace(&self, _id: &str, _number: u32) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot move windows between workspaces",
        ))
    }

    /// Restore a minimised window onto the current workspace and focus it.
    fn restore_window(&self, _id: &str) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend cannot restore windows",
        ))
    }

    /// The float/tile mode of the focused workspace.
    fn layout_mode(&self) -> Result<LayoutMode, BackendError> {
        Err(BackendError::Unsupported(
            "this backend has no float/tile toggle",
        ))
    }

    /// Switch the focused workspace (or every workspace) to floating
    /// (stacking) or tiling, re-laying out its existing windows; windows
    /// opened later on it follow the same mode (see [`Backend::window_opened`]).
    fn set_layout_mode(&self, _mode: LayoutMode, _scope: LayoutScope) -> Result<(), BackendError> {
        Err(BackendError::Unsupported(
            "this backend has no float/tile toggle",
        ))
    }

    /// Tell the backend a window appeared so it can apply the workspace's
    /// layout mode to it. Default: nothing to do.
    fn window_opened(&self, _id: &str) -> Result<(), BackendError> {
        Ok(())
    }

    /// Where each window's taskbar button is: `(window id, x, y, w, h)`
    /// relative to the panel's layer surface. Minimise/restore animations
    /// use it. Backends without animations ignore it.
    fn set_minimize_rects(&self, _rects: &[(String, i32, i32, i32, i32)]) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("this backend has no minimise targets"))
    }

    /// Open a change-event stream (window/workspace changes). Callers poll
    /// its file descriptor and refresh on events instead of polling the tree.
    fn subscribe(&self) -> Result<Box<dyn EventSource>, BackendError> {
        Err(BackendError::Unsupported(
            "this backend has no event stream",
        ))
    }
}

/// Title bar colours (`#rrggbb`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameColors {
    pub focused_bg: String,
    pub focused_text: String,
    pub focused_border: String,
    pub other_bg: String,
    pub other_text: String,
    pub other_border: String,
    pub urgent_bg: String,
    /// The wide window border (resize band) of the focused window.
    pub focused_frame: String,
    /// The wide window border of other windows.
    pub other_frame: String,
}

/// A window's outer geometry including its title bar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Height of the compositor-drawn title bar (0 when there is none).
    pub title_height: i32,
}

/// Float (stacking, Windows 2000 style) or tile, per workspace or global.
/// The same interface is meant for the Glaze compositor's own backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LayoutMode {
    #[default]
    Float,
    Tile,
}

impl LayoutMode {
    pub const fn slug(self) -> &'static str {
        match self {
            LayoutMode::Float => "float",
            LayoutMode::Tile => "tile",
        }
    }
}

/// Which workspaces a layout change applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutScope {
    #[default]
    FocusedWorkspace,
    AllWorkspaces,
}

/// A compositor change event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// A window changed: `change` is the compositor's verb (`new`, `close`,
    /// `focus`, `title`, `move`, `floating`, `urgent`, ...).
    Window { change: String, id: Option<String> },
    /// A workspace changed (`focus`, `init`, `empty`, ...).
    Workspace { change: String },
    /// Anything else the stream delivered.
    Other,
}

/// A non-blocking event stream. `raw_fd` is pollable for readability; on
/// readiness call [`EventSource::read_events`] until it returns empty.
pub trait EventSource: Send {
    /// Pollable file descriptor (Unix), or -1 when there is none.
    fn raw_fd(&self) -> i32;
    /// Read every complete event currently available without blocking.
    /// `Err` means the stream is closed and should be dropped.
    fn read_events(&mut self) -> Result<Vec<BackendEvent>, BackendError>;
}

/// Errors surfaced by a [`Backend`].
///
/// The variants are deliberately limited: callers must distinguish "this
/// platform/backend genuinely cannot do that" ([`BackendError::Unsupported`])
/// from a transient transport failure, so the UI can say something honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The backend is not implemented on this platform, or the capability is
    /// absent. This is the honest answer on the Windows authoring host and for
    /// the not-yet-linked live Wayland client.
    Unsupported(&'static str),
    /// A live transport exists in principle but the connection is not established.
    NotConnected(String),
    /// The compositor does not know the requested id.
    NotFound(String),
    /// The reply did not match the expected protocol.
    Protocol(String),
    /// An I/O failure on the transport.
    Io(String),
}

impl BackendError {
    /// True for the "cannot do that here" case, which the IPC layer maps to
    /// [`glaze_common::ipc::ErrorCode::BackendUnavailable`].
    pub fn is_unsupported(&self) -> bool {
        matches!(self, BackendError::Unsupported(_))
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Unsupported(m) => write!(f, "unsupported: {m}"),
            BackendError::NotConnected(m) => write!(f, "not connected: {m}"),
            BackendError::NotFound(m) => write!(f, "not found: {m}"),
            BackendError::Protocol(m) => write!(f, "protocol error: {m}"),
            BackendError::Io(m) => write!(f, "i/o error: {m}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod placeholder_tests {
    use super::*;

    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn default_backend_methods_report_unsupported() {
        struct Minimal;
        impl Backend for Minimal {
            fn name(&self) -> &'static str {
                "minimal"
            }
            fn is_connected(&self) -> bool {
                false
            }
        }
        let b = Minimal;
        assert!(matches!(b.windows(), Err(BackendError::Unsupported(_))));
        assert!(matches!(b.workspaces(), Err(BackendError::Unsupported(_))));
        assert!(matches!(
            b.activate_window("1"),
            Err(BackendError::Unsupported(_))
        ));
        assert!(matches!(
            b.close_window("1"),
            Err(BackendError::Unsupported(_))
        ));
        assert!(b.refresh().is_ok());
    }
}
