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
        let requested = wl_output.as_ref().and_then(smithay::output::Output::from_resource);
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
            let mut map = layer_map_for_output(&output);
            let layer = map
                .layers()
                .find(|l| l.layer_surface() == &surface)
                .cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
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
