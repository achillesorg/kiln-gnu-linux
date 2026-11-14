//! Presentation/input backends.
//!
//! - [`winit`]: nested inside a Wayland or X11 parent (development).
//! - [`udev`]: the session backend on a tty (DRM/KMS + GBM + EGL/GLES,
//!   libinput, libseat), M4.
//!
//! Everything above this module (state, input, render, policy) is
//! backend-agnostic: a backend only has to
//!
//! 1. create the `Output`s and map them into `state.space`,
//! 2. feed `InputEvent`s to [`crate::input::process_input_event`],
//! 3. call [`crate::render::output_elements`] per frame and present the
//!    damage, then [`crate::render::send_frames`], and
//! 4. snapshot windows for close animations ([`BackendData::capture_window`]).

pub mod udev;
pub mod winit;

use smithay::desktop::Window;

use crate::state::{GhostSurface, State};

pub enum BackendData {
    Winit(winit::WinitData),
    Udev(udev::UdevData),
}

impl BackendData {
    pub fn capture_window(&mut self, window: &Window) -> Vec<GhostSurface> {
        match self {
            BackendData::Winit(data) => {
                crate::render::capture_window(data.backend.renderer(), window)
            }
            BackendData::Udev(data) => match data.renderer() {
                Some(r) => crate::render::capture_window(r, window),
                None => Vec::new(),
            },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BackendData::Winit(_) => "winit",
            BackendData::Udev(_) => "udev",
        }
    }
}

/// Run after every event-loop dispatch.
pub fn post_dispatch(state: &mut State) {
    if matches!(state.backend, Some(BackendData::Udev(_))) {
        udev::render_pending(state);
    }
}

/// Ctrl+Alt+F<n>: switch VT (session backend only).
pub fn change_vt(state: &mut State, vt: i32) {
    if let Some(BackendData::Udev(data)) = state.backend.as_mut() {
        udev::change_vt(data, vt);
    }
}
