//! `zwlr_foreign_toplevel_manager_v1` (version 3) server: the protocol a
//! taskbar uses to list, activate, minimise, maximise and close windows.
//!
//! Smithay 0.7 ships only `ext-foreign-toplevel-list` (read-only), which we
//! also serve. This module adds the wlr management protocol so the Glaze
//! taskbar (and `wlrctl`, Waybar, ...) can *drive* windows without Sway.
//! Minimise/restore really hide and show windows here (Sway cannot).
//! `set_rectangle` is honoured: it becomes the target of the minimise
//! animation (the window flies into its taskbar button).

use std::collections::HashMap;

use glaze_wm::Rect;
use smithay::desktop::{layer_map_for_output, WindowSurfaceType};
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use tracing::{debug, info};

use crate::state::State;

const VERSION: u32 = 3;

// zwlr_foreign_toplevel_handle_v1.state values.
const STATE_MAXIMIZED: u32 = 0;
const STATE_MINIMIZED: u32 = 1;
const STATE_ACTIVATED: u32 = 2;
const STATE_FULLSCREEN: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    title: String,
    app_id: String,
    states: Vec<u32>,
}

#[derive(Default)]
pub struct WlrForeignToplevelState {
    managers: Vec<ZwlrForeignToplevelManagerV1>,
    handles: Vec<(String, ZwlrForeignToplevelHandleV1)>,
    last: HashMap<String, Snapshot>,
    rects: HashMap<String, Rect>,
}

impl WlrForeignToplevelState {
    pub fn new(dh: &DisplayHandle) -> Self {
        dh.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(VERSION, ());
        Self::default()
    }

    /// The taskbar button rectangle a client reported for window `id`, in
    /// global coordinates.
    pub fn rectangle(&self, id: &str) -> Option<Rect> {
        self.rects.get(id).copied().filter(|r| r.w > 0 && r.h > 0)
    }
}

fn states_bytes(states: &[u32]) -> Vec<u8> {
    states.iter().flat_map(|s| s.to_ne_bytes()).collect()
}

impl State {
    fn wlr_snapshot(&self, id: &str) -> Option<Snapshot> {
        let w = self.wm.get(id)?;
        let mut states = Vec::new();
        if w.maximized {
            states.push(STATE_MAXIMIZED);
        }
        if w.minimized {
            states.push(STATE_MINIMIZED);
        }
        if self.wm.focused() == Some(id) {
            states.push(STATE_ACTIVATED);
        }
        if w.fullscreen {
            states.push(STATE_FULLSCREEN);
        }
        Some(Snapshot {
            title: w.title.clone(),
            app_id: w.app_id.clone().unwrap_or_default(),
            states,
        })
    }

    fn wlr_send_all(&self, handle: &ZwlrForeignToplevelHandleV1, snap: &Snapshot) {
        handle.title(snap.title.clone());
        handle.app_id(snap.app_id.clone());
        handle.state(states_bytes(&snap.states));
        handle.done();
    }

    fn wlr_create_handle(&mut self, manager: &ZwlrForeignToplevelManagerV1, id: &str) {
        let Some(client) = manager.client() else {
            return;
        };
        let Ok(handle) = client.create_resource::<ZwlrForeignToplevelHandleV1, String, State>(
            &self.display_handle,
            manager.version(),
            id.to_owned(),
        ) else {
            return;
        };
        manager.toplevel(&handle);
        if let Some(output) = &self.output {
            for wl_output in output.client_outputs(&client) {
                handle.output_enter(&wl_output);
            }
        }
        if let Some(snap) = self.wlr_snapshot(id) {
            self.wlr_send_all(&handle, &snap);
        }
        self.wlr_foreign_toplevel
            .handles
            .push((id.to_owned(), handle));
    }

    /// A window was mapped: announce it to every manager.
    pub fn wlr_foreign_toplevel_new(&mut self, id: &str) {
        let managers = self.wlr_foreign_toplevel.managers.clone();
        for m in &managers {
            self.wlr_create_handle(m, id);
        }
        if let Some(snap) = self.wlr_snapshot(id) {
            self.wlr_foreign_toplevel.last.insert(id.to_owned(), snap);
        }
    }

    /// A window is gone.
    pub fn wlr_foreign_toplevel_closed(&mut self, id: &str) {
        let st = &mut self.wlr_foreign_toplevel;
        st.handles.retain(|(hid, h)| {
            if hid == id {
                h.closed();
                false
            } else {
                true
            }
        });
        st.last.remove(id);
        st.rects.remove(id);
    }

    /// Re-send a window's state if it changed.
    pub fn wlr_foreign_toplevel_refresh(&mut self, id: &str) {
        let Some(snap) = self.wlr_snapshot(id) else {
            return;
        };
        if self.wlr_foreign_toplevel.last.get(id) == Some(&snap) {
            return;
        }
        for (hid, h) in &self.wlr_foreign_toplevel.handles {
            if hid == id {
                self.wlr_send_all(h, &snap);
            }
        }
        self.wlr_foreign_toplevel.last.insert(id.to_owned(), snap);
    }

    pub fn wlr_foreign_toplevel_refresh_all(&mut self) {
        let ids: Vec<String> = self.wm.windows().iter().map(|w| w.id.clone()).collect();
        for id in ids {
            self.wlr_foreign_toplevel_refresh(&id);
        }
    }

    fn surface_origin(&self, surface: &WlSurface) -> Option<(i32, i32)> {
        let output = self.output.as_ref()?;
        let og = self.space.output_geometry(output)?;
        let map = layer_map_for_output(output);
        if let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::ALL) {
            let g = map.layer_geometry(layer)?;
            return Some((og.loc.x + g.loc.x, og.loc.y + g.loc.y));
        }
        drop(map);
        let i = self.entry_for_surface(surface)?;
        let loc = self.space.element_location(&self.windows[i].window)?;
        Some((loc.x, loc.y))
    }
}

impl GlobalDispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn bind(
        state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let manager = data_init.init(resource, ());
        info!("wlr-foreign-toplevel manager bound");
        let ids: Vec<String> = state.wm.windows().iter().map(|w| w.id.clone()).collect();
        for id in ids {
            state.wlr_create_handle(&manager, &id);
        }
        state.wlr_foreign_toplevel.managers.push(manager);
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        resource: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            resource.finished();
            state
                .wlr_foreign_toplevel
                .managers
                .retain(|m| m != resource);
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        resource: &ZwlrForeignToplevelManagerV1,
        _data: &(),
    ) {
        state
            .wlr_foreign_toplevel
            .managers
            .retain(|m| m != resource);
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, String> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        _resource: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        id: &String,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::Request;
        debug!(id = %id, ?request, "foreign-toplevel request");
        match request {
            Request::SetMaximized => state.set_maximized(id, true),
            Request::UnsetMaximized => state.set_maximized(id, false),
            Request::SetMinimized => {
                state.minimize_window(id);
            }
            Request::UnsetMinimized => {
                info!(id = %id, "restore (foreign-toplevel)");
                state.activate_window(id);
            }
            Request::Activate { .. } => {
                info!(id = %id, "activate (foreign-toplevel)");
                state.activate_window(id);
            }
            Request::Close => {
                info!(id = %id, "close (foreign-toplevel)");
                state.close_window(id);
            }
            Request::SetRectangle {
                surface,
                x,
                y,
                width,
                height,
            } => {
                if let Some((ox, oy)) = state.surface_origin(&surface) {
                    state
                        .wlr_foreign_toplevel
                        .rects
                        .insert(id.clone(), Rect::new(ox + x, oy + y, width, height));
                }
            }
            Request::SetFullscreen { .. } => state.set_fullscreen(id, true),
            Request::UnsetFullscreen => state.set_fullscreen(id, false),
            Request::Destroy => {}
            _ => {}
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        resource: &ZwlrForeignToplevelHandleV1,
        _data: &String,
    ) {
        state
            .wlr_foreign_toplevel
            .handles
            .retain(|(_, h)| h != resource);
    }
}
