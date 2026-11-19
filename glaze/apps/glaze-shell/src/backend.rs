//! Attaching a real compositor state backend, or honestly saying we cannot.
//!
//! The panel must never invent a running-window list (`GLAZE.md` §4.3, §7.2).
//! This module is the single place that decides which [`glaze_backend::Backend`]
//! to use from `session.compositor`, tries to attach it, and asks for one real
//! window snapshot. If nothing can be attached — or the attached backend cannot
//! report windows — the result is `None` and the taskbar renders pinned items
//! only.

use std::rc::Rc;

use glaze_backend::foreign_toplevel::ForeignToplevelBackend;
use glaze_backend::sway::SwayBackend;
use glaze_backend::{
    Backend, BackendError, EventSource, Frame, FrameColors, LayoutMode, LayoutScope,
};
use glaze_common::config::{CompositorBackend, GlazeConfig};
use glaze_common::model::WindowInfo;

/// What the shell managed to attach, plus one real window snapshot.
#[derive(Default)]
pub struct Attached {
    /// The live backend, if any, shared with the UI for focus/close actions.
    pub backend: Option<Rc<dyn Backend>>,
    /// Live windows, or `None` when no backend could report them. `None` is
    /// **not** "zero windows" — it is "unknown", and the panel says so.
    pub windows: Option<Vec<WindowInfo>>,
}

impl Attached {
    /// A summary of this attach attempt for logs and the About page.
    pub fn summary(&self) -> glaze_shell_core::status::BackendSummary {
        glaze_shell_core::status::backend_summary(self.backend.as_deref())
    }
}

/// Attach the best backend the configuration allows and read one snapshot.
pub fn attach(cfg: &GlazeConfig) -> Attached {
    let Some(backend) = select_backend(cfg) else {
        eprintln!(
            "[glaze-shell] taskbar: no compositor backend attached; showing pinned apps only \
             (no windows invented)"
        );
        return Attached::default();
    };

    let windows = match backend.windows() {
        Ok(windows) => {
            eprintln!(
                "[glaze-shell] taskbar: {} reported {} live window(s)",
                backend.name(),
                windows.len()
            );
            Some(windows)
        }
        Err(e) => {
            eprintln!(
                "[glaze-shell] taskbar: {} cannot report windows ({e}); showing pinned apps only",
                backend.name()
            );
            None
        }
    };

    Attached {
        backend: Some(backend),
        windows,
    }
}

/// Choose a backend from `session.compositor`, logging every failure honestly.
fn select_backend(cfg: &GlazeConfig) -> Option<Rc<dyn Backend>> {
    match cfg.session.compositor {
        CompositorBackend::None => {
            eprintln!("[glaze-shell] session.compositor = none: window state disabled");
            None
        }
        CompositorBackend::Sway => sway_backend(),
        CompositorBackend::LayerShell => toplevel_backend(),
        CompositorBackend::Glaze => glaze_backend(),
        // Inside glaze-compositor ($GLAZE_COMPOSITOR_SOCKET set by the session
        // or the compositor) its control socket comes first; Sway otherwise.
        CompositorBackend::Auto => (if in_glaze_session() {
            glaze_backend()
        } else {
            None
        })
        .or_else(sway_backend)
        .or_else(toplevel_backend)
        .or_else(|| {
            eprintln!(
                "[glaze-shell] no supported compositor backend found (tried Sway IPC and \
                 wlr-foreign-toplevel); no window state"
            );
            None
        }),
    }
}

/// True when the session says Glaze's compositor runs it.
fn in_glaze_session() -> bool {
    std::env::var("GLAZE_COMPOSITOR_SOCKET").is_ok_and(|s| !s.is_empty())
        || std::env::var("GLAZE_COMPOSITOR_BACKEND").is_ok_and(|s| s == "glaze")
}

/// Try the Glaze compositor's control socket (`$GLAZE_COMPOSITOR_SOCKET`).
fn glaze_backend() -> Option<Rc<dyn Backend>> {
    match glaze_backend::GlazeBackend::connect() {
        Ok(backend) => {
            eprintln!("[glaze-shell] attached compositor backend: glaze-compositor");
            let shared: Rc<dyn Backend> = Rc::new(backend);
            Some(shared)
        }
        Err(e) => {
            eprintln!("[glaze-shell] glaze-compositor backend unavailable: {e}");
            None
        }
    }
}

/// Try the Sway/i3 IPC backend (`$SWAYSOCK`).
fn sway_backend() -> Option<Rc<dyn Backend>> {
    match SwayBackend::connect_from_env() {
        Ok(backend) => {
            if let Err(e) = backend.refresh() {
                eprintln!("[glaze-shell] Sway backend attached but refresh failed: {e}");
            }
            eprintln!("[glaze-shell] attached compositor backend: sway");
            let shared: Rc<dyn Backend> = Rc::new(AnimatedSway(backend));
            Some(shared)
        }
        Err(e) => {
            eprintln!("[glaze-shell] Sway IPC backend unavailable: {e}");
            None
        }
    }
}

/// Try the `wlr-foreign-toplevel-management` backend.
fn toplevel_backend() -> Option<Rc<dyn Backend>> {
    match ForeignToplevelBackend::connect() {
        Ok(backend) => {
            eprintln!("[glaze-shell] attached compositor backend: foreign-toplevel");
            let shared: Rc<dyn Backend> = Rc::new(backend);
            Some(shared)
        }
        Err(e) => {
            eprintln!("[glaze-shell] foreign-toplevel backend unavailable: {e}");
            None
        }
    }
}

/// The Sway backend with motion: minimise, restore and close go through
/// [`crate::ghost`], which animates the real window (opacity + position over
/// Sway IPC) and then performs the action. Every other call, and every call
/// under reduced motion, goes straight to [`SwayBackend`]. Wrapping here
/// covers every shell path at once (taskbar clicks, menus, the title-bar
/// buttons, the hover card, `glaze-shell --minimise`).
struct AnimatedSway(SwayBackend);

impl Backend for AnimatedSway {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn is_connected(&self) -> bool {
        self.0.is_connected()
    }
    fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
        self.0.windows()
    }
    fn workspaces(&self) -> Result<Vec<glaze_common::model::WorkspaceInfo>, BackendError> {
        self.0.workspaces()
    }
    fn activate_window(&self, id: &str) -> Result<(), BackendError> {
        self.0.activate_window(id)
    }
    fn close_window(&self, id: &str) -> Result<(), BackendError> {
        crate::ghost::close(id).unwrap_or_else(|| self.0.close_window(id))
    }
    fn refresh(&self) -> Result<(), BackendError> {
        Backend::refresh(&self.0)
    }
    fn frames(&self) -> Result<Vec<Frame>, BackendError> {
        self.0.frames()
    }
    fn is_maximized(&self, id: &str) -> bool {
        self.0.is_maximized(id)
    }
    fn toggle_maximize(&self, id: &str) -> Result<(), BackendError> {
        self.0.toggle_maximize(id)
    }
    fn set_frame_colors(&self, c: &FrameColors) -> Result<(), BackendError> {
        self.0.set_frame_colors(c)
    }
    fn minimize_window(&self, id: &str) -> Result<(), BackendError> {
        // The scratchpad forgets size and place: keep them for the restore.
        crate::ghost::remember(id);
        crate::ghost::minimise(id).unwrap_or_else(|| self.0.minimize_window(id))
    }
    fn move_to_workspace(&self, id: &str, number: u32) -> Result<(), BackendError> {
        self.0.move_to_workspace(id, number)
    }
    fn restore_window(&self, id: &str) -> Result<(), BackendError> {
        let show = || {
            self.0.restore_window(id)?;
            crate::ghost::put_back(id);
            Ok(())
        };
        crate::ghost::restore(id, show).unwrap_or_else(show)
    }
    fn layout_mode(&self) -> Result<LayoutMode, BackendError> {
        self.0.layout_mode()
    }
    fn set_layout_mode(&self, mode: LayoutMode, scope: LayoutScope) -> Result<(), BackendError> {
        self.0.set_layout_mode(mode, scope)
    }
    fn window_opened(&self, id: &str) -> Result<(), BackendError> {
        self.0.window_opened(id)
    }
    fn set_minimize_rects(
        &self,
        rects: &[(String, i32, i32, i32, i32)],
    ) -> Result<(), BackendError> {
        self.0.set_minimize_rects(rects)
    }
    fn subscribe(&self) -> Result<Box<dyn EventSource>, BackendError> {
        self.0.subscribe()
    }
}
