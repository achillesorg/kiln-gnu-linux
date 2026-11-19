//! `zwlr_layer_shell_v1`: the Glaze shell's panel, desktop and menus (and
//! `swaybg` for a wallpaper when nested).

use smithay::desktop::{layer_map_for_output, LayerSurface, PopupKind};
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::wayland::shell::wlr_layer::{
    Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler, WlrLayerShellState,
};
use smithay::wayland::shell::xdg::PopupSurface;
use tracing::{info, warn};

use crate::state::State;

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        let requested = wl_output
            .as_ref()
            .and_then(smithay::output::Output::from_resource);
        let Some(output) = requested.or_else(|| self.output.clone()) else {
            warn!("layer surface before any output; ignoring");
            return;
        };
        info!(?layer, namespace, "layer surface");
        let mut map = layer_map_for_output(&output);
        if let Err(e) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            warn!(%e, "failed to map layer surface");
        }
    }

    fn new_popup(&mut self, _parent: WlrLayerSurface, popup: PopupSurface) {
        self.unconstrain_popup(&popup);
        if let Err(e) = self.popups.track_popup(PopupKind::Xdg(popup)) {
            warn!(%e, "failed to track layer popup");
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        for output in self.outputs.clone() {
            let found = {
                let map = layer_map_for_output(&output);
                let layer = map
                    .layers()
                    .find(|l| l.layer_surface() == &surface)
                    .cloned();
                layer.map(|l| {
                    let g = map.layer_geometry(&l);
                    (l, g)
                })
            };
            if let Some((layer, geo)) = found {
                // Toasts and other shell sheets fade and sink out from a
                // snapshot (the bar, the desktop and the transparent click
                // catcher just go; see `render::layer_animates`).
                let ns = layer.namespace().to_owned();
                if let (Some(g), true) = (geo, crate::render::layer_animates(&ns)) {
                    let out_loc = self
                        .space
                        .output_geometry(&output)
                        .map(|o| o.loc)
                        .unwrap_or_default();
                    let rest = glaze_wm::Rect::new(
                        out_loc.x + g.loc.x,
                        out_loc.y + g.loc.y,
                        g.size.w,
                        g.size.h,
                    );
                    let surfaces = self
                        .backend
                        .as_mut()
                        .map(|b| b.capture_surface(surface.wl_surface()))
                        .unwrap_or_default();
                    let now = self.now();
                    let anim = crate::anim::WindowAnim::close_popup(rest, now, &self.motion);
                    info!(namespace = %ns, surfaces = surfaces.len(), "layer close animation");
                    if !surfaces.is_empty() && !anim.is_done(now) {
                        self.ghosts.push(crate::state::Ghost {
                            rest,
                            insets: crate::frame::Insets::NONE,
                            surfaces,
                            title_buffer: None,
                            parts: Default::default(),
                            anim,
                        });
                    }
                }
                layer_map_for_output(&output).unmap_layer(&layer);
                // A later surface with the same id fades in again.
                self.layer_seen
                    .remove(&smithay::reexports::wayland_server::Resource::id(
                        surface.wl_surface(),
                    ));
            }
        }
        // Give keyboard focus back to the windows.
        let keyboard = self.seat.get_keyboard().expect("keyboard");
        if keyboard
            .current_focus()
            .is_some_and(|f| &f == surface.wl_surface())
        {
            keyboard.set_focus(self, None, smithay::utils::SERIAL_COUNTER.next_serial());
        }
        self.update_work_area();
        self.apply_layout(false);
    }
}
