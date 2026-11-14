//! Minimise / maximise / close buttons that work on Sway.
//!
//! GTK's own title buttons ask the compositor (`xdg_toplevel.set_minimized`,
//! `set_maximized`), which Sway ignores for floating windows, so on Sway the
//! buttons act through IPC instead (`crate::swaywin`): minimise moves the
//! window to the scratchpad (the Glaze taskbar button brings it back),
//! maximise fills the workspace's usable area and restores the previous
//! geometry. Elsewhere (glaze-compositor, other compositors) they use the
//! normal GTK requests.

use std::cell::Cell;
use std::process::Command;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::swaywin::{self, Rect};
use crate::ui::APP_ID;

fn on_sway() -> bool {
    std::env::var_os("SWAYSOCK").is_some()
}

fn criteria() -> String {
    format!("[app_id=\"{APP_ID}\" pid={}]", std::process::id())
}

fn swaymsg(args: &[&str]) -> Option<String> {
    let out = Command::new("swaymsg").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn geometry() -> Option<(Rect, Rect)> {
    let tree: serde_json::Value = serde_json::from_str(&swaymsg(&["-t", "get_tree"])?).ok()?;
    swaywin::find_window(&tree, APP_ID, Some(std::process::id()))
}

/// Show (from the scratchpad if minimised) and focus our window via Sway.
pub fn raise() {
    if !on_sway() {
        return;
    }
    let hidden = swaymsg(&["-t", "get_tree"])
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .is_some_and(|t| swaywin::in_scratchpad(&t, APP_ID, Some(std::process::id())));
    if hidden {
        swaymsg(&[&format!("{} scratchpad show", criteria())]);
    }
    swaymsg(&[&format!("{} focus", criteria())]);
}

fn minimize(window: &gtk::ApplicationWindow) {
    if on_sway() && swaymsg(&[&format!("{} move scratchpad", criteria())]).is_some() {
        return;
    }
    window.minimize();
}

/// Toggle; returns whether the window is now maximised.
fn toggle_maximize(window: &gtk::ApplicationWindow, saved: &Cell<Option<Rect>>) -> bool {
    if !on_sway() {
        if window.is_maximized() {
            window.unmaximize();
            return false;
        }
        window.maximize();
        return true;
    }
    let Some((win, area)) = geometry() else {
        return false;
    };
    if swaywin::is_filling(win, area) {
        let back = saved.take().unwrap_or(Rect {
            x: area.x + (area.w - 1120).max(0) / 2,
            y: area.y + (area.h - 740).max(0) / 2,
            w: 1120.min(area.w),
            h: 740.min(area.h),
        });
        swaymsg(&[&swaywin::place_command(&criteria(), back)]);
        false
    } else {
        saved.set(Some(win));
        swaymsg(&[&swaywin::place_command(&criteria(), area)]);
        true
    }
}

fn control(icon: &str, name: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("fs-wincontrol");
    b.set_tooltip_text(Some(name));
    b.update_property(&[gtk::accessible::Property::Label(name)]);
    b.set_valign(gtk::Align::Center);
    b
}

/// Build the three buttons for the end of the header bar.
pub fn build(window: &gtk::ApplicationWindow, header: &gtk::HeaderBar) -> gtk::Box {
    header.set_show_title_buttons(false);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    bx.add_css_class("fs-wincontrols");
    let min = control("window-minimize-symbolic", "Minimise");
    let max = control("window-maximize-symbolic", "Maximise");
    let close = control("window-close-symbolic", "Close");
    close.add_css_class("close");
    bx.append(&min);
    bx.append(&max);
    bx.append(&close);

    let saved: Rc<Cell<Option<Rect>>> = Rc::new(Cell::new(None));
    {
        let w = window.clone();
        min.connect_clicked(move |_| minimize(&w));
    }
    let set_icon = {
        let max = max.clone();
        move |maximised: bool| {
            let (icon, name) = if maximised {
                ("window-restore-symbolic", "Restore")
            } else {
                ("window-maximize-symbolic", "Maximise")
            };
            max.set_icon_name(icon);
            max.set_tooltip_text(Some(name));
            max.update_property(&[gtk::accessible::Property::Label(name)]);
        }
    };
    let toggle: Rc<dyn Fn()> = {
        let w = window.clone();
        let saved = saved.clone();
        Rc::new(move || set_icon(toggle_maximize(&w, &saved)))
    };
    {
        let t = toggle.clone();
        max.connect_clicked(move |_| t());
    }
    {
        let w = window.clone();
        close.connect_clicked(move |_| w.close());
    }
    if on_sway() {
        // Double-click on the title bar maximises, as on other desktops
        // (GTK's own handler only sends the request Sway ignores).
        let g = gtk::GestureClick::new();
        g.set_button(1);
        g.set_propagation_phase(gtk::PropagationPhase::Capture);
        let t = toggle.clone();
        let h = header.clone();
        g.connect_pressed(move |g, n, x, y| {
            let on_button = h.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| {
                w.is::<gtk::Button>() || w.ancestor(gtk::Button::static_type()).is_some()
            });
            if n == 2 && !on_button {
                g.set_state(gtk::EventSequenceState::Claimed);
                t();
            }
        });
        header.add_controller(g);
    }
    bx
}
