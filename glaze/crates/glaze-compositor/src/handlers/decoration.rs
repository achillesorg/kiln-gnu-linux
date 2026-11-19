//! `zxdg_decoration_manager_v1`: Glaze forces server-side frames wherever the
//! client speaks the protocol (decisions.md Q5). Clients that never create a
//! decoration object (GTK4, most toolkits with CSD) keep their own.

use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::ToplevelSurface;
use tracing::{debug, info};

use crate::state::State;

impl State {
    fn force_ssd(&mut self, toplevel: &ToplevelSurface) {
        toplevel.with_pending_state(|s| s.decoration_mode = Some(Mode::ServerSide));
        if let Some(i) = self.entry_for_surface(toplevel.wl_surface()) {
            let was = self.windows[i].ssd;
            self.windows[i].ssd = true;
            if !was && self.windows[i].id.is_some() {
                self.apply_layout(false);
            }
        }
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        debug!("xdg-decoration object created: server-side frame");
        self.force_ssd(&toplevel);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        // Policy (decisions.md Q5 + M4): Glaze frames by default. A client
        // that asks for client-side decorations a second time insists, and
        // gets them (it draws its own title bar).
        if mode == Mode::ClientSide {
            if let Some(i) = self.entry_for_surface(toplevel.wl_surface()) {
                self.windows[i].csd_requests += 1;
                if self.windows[i].csd_requests >= 2 {
                    info!("client insists on client-side decorations; honouring it");
                    toplevel.with_pending_state(|s| s.decoration_mode = Some(Mode::ClientSide));
                    let was = self.windows[i].ssd;
                    self.windows[i].ssd = false;
                    if was && self.windows[i].id.is_some() {
                        self.apply_layout(false);
                    }
                    if toplevel.is_initial_configure_sent() {
                        toplevel.send_pending_configure();
                    }
                    return;
                }
            }
        }
        debug!(
            ?mode,
            "client asked for a decoration mode; Glaze answers server-side"
        );
        self.force_ssd(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.force_ssd(&toplevel);
    }
}

// --- org_kde_kwin_server_decoration (GTK 3 apps such as Thunar, Evince,
// Firefox ask through this one, not xdg-decoration) -------------------------

use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
    Mode as KdeMode, OrgKdeKwinServerDecoration,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Resource, WEnum};
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};

impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
    }

    fn new_decoration(&mut self, surface: &WlSurface, decoration: &OrgKdeKwinServerDecoration) {
        decoration.mode(KdeMode::Server);
        self.set_kde_ssd(surface, true);
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        // Undecorated windows (splash screens, GTK 3 `decorated = FALSE`)
        // ask for no decoration at all: no frame.
        if let WEnum::Value(KdeMode::None) = mode {
            decoration.mode(KdeMode::None);
            self.set_kde_ssd(surface, false);
            return;
        }
        // Same policy as xdg-decoration: Glaze frames unless the client
        // insists (asks for client-side twice).
        let mut honour_client = false;
        if let WEnum::Value(KdeMode::Client) = mode {
            if let Some(i) = self.entry_for_surface(surface) {
                self.windows[i].csd_requests += 1;
                honour_client = self.windows[i].csd_requests >= 2;
            }
        }
        if honour_client {
            info!("client insists on client-side decorations (kde protocol); honouring it");
            decoration.mode(KdeMode::Client);
            self.set_kde_ssd(surface, false);
        } else {
            decoration.mode(KdeMode::Server);
            self.set_kde_ssd(surface, true);
        }
    }

    fn release(&mut self, _decoration: &OrgKdeKwinServerDecoration, surface: &WlSurface) {
        self.set_kde_ssd(surface, false);
    }
}

impl State {
    fn set_kde_ssd(&mut self, surface: &WlSurface, on: bool) {
        // The decoration object may come before the xdg_toplevel exists.
        self.kde_ssd.retain(|s| s != surface && s.is_alive());
        if on {
            self.kde_ssd.push(surface.clone());
        }
        if let Some(i) = self.entry_for_surface(surface) {
            let was = self.windows[i].ssd;
            self.windows[i].ssd = on;
            if was != on && self.windows[i].id.is_some() {
                self.apply_layout(false);
            }
        }
    }
}

smithay::delegate_kde_decoration!(State);
