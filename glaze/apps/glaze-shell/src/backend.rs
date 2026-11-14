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
use glaze_backend::Backend;
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
        CompositorBackend::Auto => (if in_glaze_session() { glaze_backend() } else { None })
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
            let shared: Rc<dyn Backend> = Rc::new(backend);
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
