//! `wl_compositor` / `wl_subcompositor`, `wl_shm`, buffers: the commit path.

use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
use smithay::desktop::{layer_map_for_output, PopupKind, WindowSurfaceType};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Client;
use smithay::utils::Point;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    get_parent, is_sync_subsurface, with_states, CompositorClientState, CompositorHandler,
    CompositorState,
};
use smithay::wayland::shell::wlr_layer::LayerSurfaceData;
use smithay::wayland::shm::{ShmHandler, ShmState};

use crate::state::{ClientState, State};

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // XWayland's own client carries Smithay's data, not ours.
        if let Some(x) = client.get_data::<smithay::xwayland::XWaylandClientData>() {
            return &x.compositor_state;
        }
        &client
            .get_data::<ClientState>()
            .expect("client without ClientState")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(i) = self.entry_for_surface(&root) {
                self.windows[i].window.on_commit();
                if &root == surface {
                    if self.windows[i].x11().is_some() {
                        self.x11_committed(i);
                    } else {
                        self.toplevel_committed(i);
                    }
                }
            }
        }

        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface) {
            if !popup.is_initial_configure_sent() {
                let _ = popup.send_configure();
            }
        }

        self.layer_committed(surface);
        self.needs_redraw = true;
    }
}

impl State {
    fn toplevel_committed(&mut self, i: usize) {
        let Some(toplevel) = self.windows[i].toplevel().cloned() else {
            return;
        };
        if !toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
            return;
        }
        let has_buffer =
            with_renderer_surface_state(toplevel.wl_surface(), |s| s.buffer().is_some())
                .unwrap_or(false);
        let Some(id) = self.windows[i].id.clone() else {
            if has_buffer {
                self.manage_window(i);
            }
            return;
        };
        if !has_buffer {
            return;
        }
        // The client resized itself (or answered a configure): remember its
        // size while stacking, and keep the frame glued to the geometry.
        let insets = self.windows[i].insets(&self.wm);
        let geo = self.windows[i].window.geometry();
        let outer_w = geo.size.w + insets.left + insets.right;
        let outer_h = geo.size.h + insets.top + insets.bottom;
        let Some(rest) = self.windows[i].rest else {
            return;
        };
        let expected_loc = Point::from((
            rest.x + insets.left - geo.loc.x,
            rest.y + insets.top - geo.loc.y,
        ));
        let loc_now = self.space.element_location(&self.windows[i].window);
        let size_changed = (outer_w, outer_h) != (rest.w, rest.h);
        // Only a client that has caught up with our latest configure is
        // resizing itself; a commit still drawn at the old size (a configure
        // in flight, e.g. right after a snap) must not undo the new size.
        let caught_up = toplevel.current_state().size == toplevel.with_pending_state(|s| s.size);
        if !caught_up {
            return;
        }
        if size_changed && !self.wm.is_tiled(&id) {
            self.wm.client_resized(&id, (outer_w, outer_h));
        }
        if size_changed || loc_now.is_some_and(|l| l != expected_loc) {
            if size_changed && self.wm.is_tiled(&id) {
                // Tiled: the client chose a different size than its tile (min
                // size). Keep the tile; the frame follows the client.
                if let Some(w) = self.windows.get_mut(i) {
                    w.rest = Some(glaze_wm::Rect::new(rest.x, rest.y, outer_w, outer_h));
                }
                let window = self.windows[i].window.clone();
                if loc_now.is_some() {
                    self.space.map_element(window, expected_loc, false);
                }
                self.refresh_titles();
            } else {
                self.apply_layout(false);
            }
        }
    }

    fn layer_committed(&mut self, surface: &WlSurface) {
        // The output whose layer map holds this surface.
        let Some(output) = self
            .outputs
            .iter()
            .find(|o| {
                layer_map_for_output(o)
                    .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                    .is_some()
            })
            .cloned()
        else {
            return;
        };
        let changed = {
            let mut map = layer_map_for_output(&output);
            let Some(layer) = map
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .cloned()
            else {
                return;
            };
            let initial_configure_sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<LayerSurfaceData>()
                    .map(|d| d.lock().unwrap().initial_configure_sent)
                    .unwrap_or(true)
            });
            map.arrange();
            if !initial_configure_sent {
                layer.layer_surface().send_configure();
            }
            let st = layer.cached_state();
            let exclusive = st.keyboard_interactivity
                == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::Exclusive
                && matches!(st.layer, smithay::wayland::shell::wlr_layer::Layer::Top | smithay::wayland::shell::wlr_layer::Layer::Overlay)
                && map.layer_geometry(&layer).is_some();
            let none = st.keyboard_interactivity
                == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::None;
            (exclusive, none)
        };
        // Exclusive keyboard interactivity (the shell's menus and start
        // menu) takes the keyboard; dropping it hands focus back.
        if self.locked {
            self.update_work_area();
            return;
        }
        let keyboard = self.seat.get_keyboard().expect("keyboard");
        let has_focus = keyboard.current_focus().as_ref() == Some(surface);
        if changed.0 && !has_focus {
            keyboard.set_focus(self, Some(surface.clone()), smithay::utils::SERIAL_COUNTER.next_serial());
        } else if changed.1 && has_focus {
            keyboard.set_focus(self, None, smithay::utils::SERIAL_COUNTER.next_serial());
            self.apply_layout(false);
        }
        {
            self.update_work_area();
        }
    }
}
