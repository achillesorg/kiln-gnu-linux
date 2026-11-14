//! `xdg_wm_base`: toplevels and popups.

use smithay::desktop::{
    find_popup_root_surface, get_popup_toplevel_coords, layer_map_for_output, PopupKeyboardGrab,
    PopupKind, PopupPointerGrab, PopupUngrabStrategy, Window, WindowSurfaceType,
};
use smithay::input::pointer::Focus;
use smithay::input::Seat;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;
use smithay::reexports::wayland_server::protocol::{wl_output, wl_seat};
use smithay::utils::{Rectangle, Serial};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use tracing::{debug, warn};

use crate::frame::Edges;
use crate::state::{State, WindowEntry};

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        debug!("new toplevel");
        let kde_ssd = self.kde_ssd.iter().any(|s| s == surface.wl_surface());
        let mut entry = WindowEntry::new(Window::new_wayland_window(surface));
        entry.ssd = kde_ssd;
        self.windows.push(entry);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.unmanage_window(&surface);
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.refresh_metadata(&surface);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.refresh_metadata(&surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        if let Err(e) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!(%e, "failed to track popup");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
        let _ = surface.send_configure();
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        if self.locked {
            return; // no popup may take the keyboard from the locker
        }
        let Some(seat) = Seat::<State>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            return;
        };
        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(id) = self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        else {
            return;
        };
        let Some(seat) = Seat::<State>::from_resource(&seat) else {
            return;
        };
        crate::input::start_move(self, &seat, &id, serial);
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: ResizeEdge,
    ) {
        let Some(id) = self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        else {
            return;
        };
        let Some(seat) = Seat::<State>::from_resource(&seat) else {
            return;
        };
        let e = Edges {
            top: matches!(
                edges,
                ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
            ),
            bottom: matches!(
                edges,
                ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight
            ),
            left: matches!(
                edges,
                ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
            ),
            right: matches!(
                edges,
                ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight
            ),
        };
        crate::input::start_resize(self, &seat, &id, e, serial);
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        match self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        {
            Some(id) => self.set_maximized(&id, true),
            None => {
                if surface.is_initial_configure_sent() {
                    surface.send_configure();
                }
            }
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        {
            self.set_maximized(&id, false);
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<wl_output::WlOutput>,
    ) {
        match self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        {
            Some(id) => self.set_fullscreen(&id, true),
            None => {
                if surface.is_initial_configure_sent() {
                    surface.send_configure();
                }
            }
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        {
            self.set_fullscreen(&id, false);
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self
            .entry_for_surface(surface.wl_surface())
            .and_then(|i| self.windows[i].id.clone())
        {
            self.minimize_window(&id);
        }
    }
}

impl State {
    /// Keep popups on screen (anvil's `unconstrain_popup`).
    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Some(output) = self.output.as_ref() else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(output) else {
            return;
        };
        // Where is the root surface in global coordinates?
        let root_loc = if let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &root))
        {
            match self.space.element_location(window) {
                Some(loc) => loc + window.geometry().loc,
                None => return,
            }
        } else {
            let map = layer_map_for_output(output);
            match map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL) {
                Some(layer) => match map.layer_geometry(layer) {
                    Some(geo) => geo.loc + output_geo.loc,
                    None => return,
                },
                None => return,
            }
        };
        let mut target = Rectangle::new(output_geo.loc, output_geo.size);
        target.loc -= get_popup_toplevel_coords(&kind);
        target.loc -= root_loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
