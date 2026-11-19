//! XWayland: X11 programs (VLC 3's Qt interface, xterm, games) on the Glaze
//! compositor, through Smithay's XWayland launcher and X11 window manager.
//!
//! - Xwayland starts with the compositor; its display (`:N`) is exported to
//!   our own children and written to `$XDG_RUNTIME_DIR/glaze/xwayland-display`
//!   so `glaze-session` hands it to the shell (and so to every app it
//!   launches).
//! - Managed X11 windows become normal Glaze windows (`WindowEntry` with an
//!   X11 `Window`): paper frames (unless Motif hints ask for none), taskbar
//!   buttons, minimise/maximise/snap/tiling, move/resize requests.
//! - Override-redirect windows (menus, tooltips, drag icons) are drawn above
//!   the windows at the place X11 put them, without frames.
//! - Clipboard and primary selection are bridged both ways.

use std::os::fd::OwnedFd;
use std::process::Stdio;

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle, SERIAL_COUNTER};
use smithay::wayland::selection::data_device::{
    clear_data_device_selection, request_data_device_client_selection, set_data_device_selection,
};
use smithay::wayland::selection::primary_selection::{
    clear_primary_selection, request_primary_client_selection, set_primary_selection,
};
use smithay::wayland::selection::SelectionTarget;
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge as X11ResizeEdge, XwmId};
use smithay::xwayland::{X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler};
use tracing::{info, warn};

use crate::frame::Edges;
use crate::state::{State, WindowEntry};

/// File (under `$XDG_RUNTIME_DIR/glaze/`) holding the XWayland display, `:N`.
pub const DISPLAY_FILE: &str = "xwayland-display";

/// Start Xwayland (unless `GLAZE_XWAYLAND=0`).
pub fn start(state: &mut State) {
    if std::env::var("GLAZE_XWAYLAND").is_ok_and(|v| v == "0") {
        info!("XWayland disabled (GLAZE_XWAYLAND=0)");
        return;
    }
    let spawned = XWayland::spawn(
        &state.display_handle,
        None,
        std::iter::empty::<(String, String)>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    );
    let (xwayland, client) = match spawned {
        Ok(x) => x,
        Err(e) => {
            warn!(%e, "XWayland not started (is the xwayland package installed?)");
            return;
        }
    };
    let handle = state.loop_handle.clone();
    let res = state
        .loop_handle
        .insert_source(xwayland, move |event, _, state| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => match X11Wm::start_wm(handle.clone(), x11_socket, client.clone()) {
                Ok(wm) => {
                    info!(display = display_number, "XWayland ready");
                    state.xwm = Some(wm);
                    state.xdisplay = Some(display_number);
                    let path = glaze_common::paths::runtime_dir()
                        .join("glaze")
                        .join(DISPLAY_FILE);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&path, format!(":{display_number}\n"));
                }
                Err(e) => warn!(%e, "X11 window manager failed to start"),
            },
            XWaylandEvent::Error => warn!("XWayland failed to start"),
        });
    if let Err(e) = res {
        warn!(%e, "XWayland source");
    }
}

impl XWaylandShellHandler for State {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }
}

impl State {
    fn entry_for_x11(&self, window: &X11Surface) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.x11().is_some_and(|x| x == window))
    }

    fn id_for_x11(&self, window: &X11Surface) -> Option<String> {
        self.entry_for_x11(window)
            .and_then(|i| self.windows[i].id.clone())
    }

    /// A commit on an X11 window's surface: manage it on its first buffer.
    pub fn x11_committed(&mut self, i: usize) {
        if self.windows[i].id.is_some() {
            return;
        }
        let has_buffer = self.windows[i].wl_surface().is_some_and(|s| {
            smithay::backend::renderer::utils::with_renderer_surface_state(&s, |st| {
                st.buffer().is_some()
            })
            .unwrap_or(false)
        });
        if has_buffer {
            self.manage_window(i);
        }
    }

    fn unmanage_x11(&mut self, window: &X11Surface) {
        self.x11_override.retain(|w| w != window);
        let Some(i) = self.entry_for_x11(window) else {
            self.needs_redraw = true;
            return;
        };
        let mut entry = self.windows.remove(i);
        // Close animation from a snapshot of the last frame (as for Wayland
        // windows), so X11 windows sink and fade out too.
        let close = entry.id.as_ref().and_then(|_| self.close_anim(&entry));
        if let Some((rest, anim)) = close {
            let surfaces = self
                .backend
                .as_mut()
                .map(|b| b.capture_window(&entry.window))
                .unwrap_or_default();
            if !surfaces.is_empty() {
                self.ghosts.push(crate::state::Ghost {
                    rest,
                    insets: entry.insets(&self.wm),
                    surfaces,
                    title_buffer: entry.title_buffer.clone(),
                    parts: std::mem::take(&mut entry.parts),
                    anim,
                });
            }
        }
        self.space.unmap_elem(&entry.window);
        if let Some(handle) = &entry.ext_handle {
            self.foreign_toplevel_list_state.remove_toplevel(handle);
        }
        if let Some(id) = entry.id.as_deref() {
            info!(id = %id, "X11 window unmapped");
            self.wlr_foreign_toplevel_closed(id);
            self.wm.remove_window(id);
        }
        self.apply_layout(true);
    }

    /// Override-redirect windows above everything, for rendering/hit tests.
    pub fn x11_override_windows(&self) -> impl Iterator<Item = &X11Surface> {
        self.x11_override.iter().filter(|w| w.is_mapped())
    }
}

impl XwmHandler for State {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().expect("X11 window manager")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(e) = window.set_mapped(true) {
            warn!(?e, "X11 map failed");
            return;
        }
        if self.entry_for_x11(&window).is_some() {
            return;
        }
        let mut entry = WindowEntry::new(smithay::desktop::Window::new_x11_window(window.clone()));
        // X11 programs expect the window manager to draw the frame; Motif
        // "no decorations" (splash screens, custom-chrome apps) opt out.
        // (Smithay's `is_decorated()` means "draws its own decorations".)
        entry.ssd = !window.is_decorated();
        // Show it where the toolkit asked until it is managed (first buffer).
        let geo = window.geometry();
        let _ = window.configure(geo);
        self.windows.push(entry);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        if !self.x11_override.contains(&window) {
            self.x11_override.push(window);
        }
        self.needs_redraw = true;
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.unmanage_x11(&window);
        if !window.is_override_redirect() {
            let _ = window.set_mapped(false);
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.unmanage_x11(&window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        match self.id_for_x11(&window) {
            // Managed: a size request becomes the stacking size; the WM keeps
            // the position (and tiling/maximise win).
            Some(id) => {
                if let (Some(w), Some(h)) = (w, h) {
                    let i = self.entry_for_x11(&window).expect("managed");
                    let insets = self.windows[i].insets(&self.wm);
                    self.wm.client_resized(
                        &id,
                        (
                            w as i32 + insets.left + insets.right,
                            h as i32 + insets.top + insets.bottom,
                        ),
                    );
                    self.apply_layout(false);
                }
            }
            // Not managed yet: grant what the toolkit asked.
            None => {
                let mut geo = window.geometry();
                if let Some(x) = x {
                    geo.loc.x = x;
                }
                if let Some(y) = y {
                    geo.loc.y = y;
                }
                if let Some(w) = w {
                    geo.size.w = w as i32;
                }
                if let Some(h) = h {
                    geo.size.h = h as i32;
                }
                let _ = window.configure(geo);
            }
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        self.needs_redraw = true;
    }

    fn property_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _property: smithay::xwayland::xwm::WmWindowProperty,
    ) {
        if let Some(id) = self.id_for_x11(&window) {
            let title = window.title();
            self.wm.set_title(&id, title.clone());
            if let Some(i) = self.entry_for_x11(&window) {
                if let Some(h) = &self.windows[i].ext_handle {
                    h.send_title(&title);
                    h.send_done();
                }
            }
            self.wlr_foreign_toplevel_refresh(&id);
            self.refresh_titles();
            self.ipc_dirty = true;
        }
    }

    fn maximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.id_for_x11(&window) {
            self.set_maximized(&id, true);
        }
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.id_for_x11(&window) {
            self.set_maximized(&id, false);
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.id_for_x11(&window) {
            self.set_fullscreen(&id, true);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.id_for_x11(&window) {
            self.set_fullscreen(&id, false);
        }
    }

    fn minimize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.id_for_x11(&window) {
            self.minimize_window(&id);
        }
    }

    fn unminimize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.id_for_x11(&window) {
            self.activate_window(&id);
        }
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _button: u32,
        edge: X11ResizeEdge,
    ) {
        let Some(id) = self.id_for_x11(&window) else {
            return;
        };
        let e = match edge {
            X11ResizeEdge::Top => Edges {
                top: true,
                ..Default::default()
            },
            X11ResizeEdge::Bottom => Edges {
                bottom: true,
                ..Default::default()
            },
            X11ResizeEdge::Left => Edges {
                left: true,
                ..Default::default()
            },
            X11ResizeEdge::Right => Edges {
                right: true,
                ..Default::default()
            },
            X11ResizeEdge::TopLeft => Edges {
                top: true,
                left: true,
                ..Default::default()
            },
            X11ResizeEdge::TopRight => Edges {
                top: true,
                right: true,
                ..Default::default()
            },
            X11ResizeEdge::BottomLeft => Edges {
                bottom: true,
                left: true,
                ..Default::default()
            },
            X11ResizeEdge::BottomRight => Edges {
                bottom: true,
                right: true,
                ..Default::default()
            },
        };
        let seat = self.seat.clone();
        crate::input::start_resize(self, &seat, &id, e, SERIAL_COUNTER.next_serial());
    }

    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        if let Some(id) = self.id_for_x11(&window) {
            let seat = self.seat.clone();
            crate::input::start_move(self, &seat, &id, SERIAL_COUNTER.next_serial());
        }
    }

    // --- clipboard bridge (X11 -> Wayland) ---------------------------------

    fn allow_selection_access(&mut self, _xwm: XwmId, _selection: SelectionTarget) -> bool {
        // An X11 client may read the selection when an X11 window has focus.
        let keyboard = self.seat.get_keyboard().expect("keyboard");
        keyboard.current_focus().is_some_and(|f: WlSurface| {
            self.entry_for_surface(&f)
                .is_some_and(|i| self.windows[i].x11().is_some())
        })
    }

    fn send_selection(
        &mut self,
        _xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        let res = match selection {
            SelectionTarget::Clipboard => {
                request_data_device_client_selection(&self.seat, mime_type, fd)
                    .map_err(|e| e.to_string())
            }
            SelectionTarget::Primary => request_primary_client_selection(&self.seat, mime_type, fd)
                .map_err(|e| e.to_string()),
        };
        if let Err(e) = res {
            warn!(%e, ?selection, "X11 read of the Wayland selection failed");
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        match selection {
            SelectionTarget::Clipboard => {
                set_data_device_selection(&self.display_handle, &self.seat, mime_types, ())
            }
            SelectionTarget::Primary => {
                set_primary_selection(&self.display_handle, &self.seat, mime_types, ())
            }
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        match selection {
            SelectionTarget::Clipboard => {
                clear_data_device_selection(&self.display_handle, &self.seat)
            }
            SelectionTarget::Primary => clear_primary_selection(&self.display_handle, &self.seat),
        }
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        warn!("XWayland disconnected");
        self.xwm = None;
    }
}

smithay::delegate_xwayland_shell!(State);
