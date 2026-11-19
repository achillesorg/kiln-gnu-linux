//! `ext_session_lock_v1`: the screen locker (swaylock today, the future
//! kiln-lock UI later) locks the session. While locked Glaze draws only the
//! lock surfaces over a flat paper background, sends input only to them and
//! ignores every shortcut except VT switching, so no client and no shortcut
//! can bypass it. If the locker dies the session stays locked (paper
//! background only) until a new locker unlocks it (Super+L starts one).

use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};
use tracing::{info, warn};

use crate::state::State;

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        info!("session locked");
        self.locked = true;
        self.locked_since = self.now();
        self.lock_surfaces.retain(|(_, s)| s.alive());
        // Nothing but the lock (and the paper background) is drawn from the
        // next frame on; keyboard focus leaves every client.
        let keyboard = self.seat.get_keyboard().expect("keyboard");
        keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
        self.needs_redraw = true;
        confirmation.lock();
    }

    fn unlock(&mut self) {
        info!("session unlocked");
        self.locked = false;
        self.lock_surfaces.clear();
        self.needs_redraw = true;
        self.apply_layout(false);
    }

    fn new_surface(&mut self, surface: LockSurface, wl_output: WlOutput) {
        let Some(output) = smithay::output::Output::from_resource(&wl_output) else {
            warn!("lock surface for an unknown output");
            return;
        };
        let size = self
            .space
            .output_geometry(&output)
            .map(|g| g.size)
            .unwrap_or_else(|| (1280, 800).into());
        surface.with_pending_state(|s| {
            s.size = Some((size.w as u32, size.h as u32).into());
        });
        surface.send_configure();
        let keyboard = self.seat.get_keyboard().expect("keyboard");
        keyboard.set_focus(
            self,
            Some(surface.wl_surface().clone()),
            SERIAL_COUNTER.next_serial(),
        );
        self.lock_surfaces.push((output, surface));
        self.needs_redraw = true;
    }
}

impl State {
    /// Locked but the locker died: start a new one (at most every 5 s). The
    /// screen meanwhile shows only the paper background.
    pub fn check_locker(&mut self) {
        if !self.locked || self.lock_surfaces.iter().any(|(_, s)| s.alive()) {
            return;
        }
        if self
            .last_locker_spawn
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(5))
        {
            return;
        }
        warn!("the screen locker is gone while locked; starting a new one");
        self.start_locker();
    }

    /// Super+L / the shell's Lock: start the configured screen locker.
    /// `GLAZE_LOCKER` (a shell command) wins; else `kiln-lock-ui` when
    /// installed (the shared Kiln lock service, future); else swaylock in
    /// Glaze paper colours.
    pub fn start_locker(&mut self) {
        if self.locked && self.lock_surfaces.iter().any(|(_, s)| s.alive()) {
            return;
        }
        let cmd = std::env::var("GLAZE_LOCKER")
            .ok()
            .filter(|c| !c.trim().is_empty())
            .or_else(|| program_on_path("kiln-lock-ui").then(|| "kiln-lock-ui".to_owned()))
            .or_else(|| {
                program_on_path("swaylock").then(|| {
                    let dark = self.palette.title_bg.luma() < 0.4;
                    let (bg, ring, text) = if dark {
                        ("2B2926", "D97757", "F3EFE6")
                    } else {
                        ("F5F2EA", "D97757", "2B2926")
                    };
                    format!(
                        "swaylock -c {bg} --inside-color {bg} --ring-color {ring} \
                         --key-hl-color {text} --text-color {text} --line-color {bg} \
                         --separator-color {bg} --indicator-radius 72 --indicator-thickness 6"
                    )
                })
            });
        match cmd {
            Some(cmd) => {
                info!(cmd = %cmd, "starting the screen locker");
                self.last_locker_spawn = Some(std::time::Instant::now());
                self.spawn(&cmd);
            }
            None => warn!("no screen locker (set GLAZE_LOCKER or install swaylock); not locking"),
        }
    }
}

fn program_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

smithay::delegate_session_lock!(State);
