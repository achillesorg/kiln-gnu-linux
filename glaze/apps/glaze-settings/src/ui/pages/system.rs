//! Sound, Network and Power: live status from the system tools that exist,
//! simple controls where they are safe, and explanatory empty states when a
//! tool is missing (nothing is invented).

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::catalog::Page;
use crate::store::Doc;
use crate::sysinfo::{self, NetTool, SoundTool};
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::{App, IconTone};

pub fn sound(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::Sound);
    let card = Card::new("Output", None);
    match sysinfo::probe_volume() {
        None => {
            let e = widgets::empty_state(
                app,
                "volume-mute",
                "No sound server found",
                "Neither PipeWire (wpctl) nor PulseAudio (pactl) answered. Install pipewire-pulse and wireplumber to control volume here; the taskbar hides its volume icon until then.",
            );
            app.register("sound.volume", &e, &e);
            app.register("sound.mute", &e, &e);
            card.add(&e);
        }
        Some((tool, vol)) => {
            card.note.set_text(match tool {
                SoundTool::Wpctl => "Default output through PipeWire (wpctl).",
                SoundTool::Pactl => "Default output through PulseAudio (pactl).",
            });
            card.note.set_visible(true);
            let (r, _) = widgets::slider(
                app,
                "sound.volume",
                0.0,
                1.0,
                0.01,
                vol.level.min(1.0),
                None,
                |v| format!("{:.0} %", v * 100.0),
                move |_, v| sysinfo::set_volume(tool, v),
            );
            card.add(&r.root);
            let sw = gtk::Switch::new();
            sw.set_active(vol.muted);
            let r = widgets::row(app, "sound.mute", None, &sw);
            let err = r.error.clone();
            sw.connect_active_notify(move |s| match sysinfo::set_mute(tool, s.is_active()) {
                Ok(()) => err.set_visible(false),
                Err(e) => {
                    err.set_text(&e);
                    err.set_visible(true);
                }
            });
            card.add(&r.root);
        }
    }
    page.add(&card.frame);
    page.finish()
}

pub fn network(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::Network);
    let card = Card::new("Connections", None);
    match sysinfo::probe_network() {
        None => {
            let e = widgets::empty_state(
                app,
                "network-off",
                "No network information",
                "Neither NetworkManager (nmcli) nor iproute2 (ip) reported any interface.",
            );
            app.register("network.status", &e, &e);
            app.register("network.wifi", &e, &e);
            card.add(&e);
        }
        Some((tool, links, wifi)) => {
            card.note.set_text(match tool {
                NetTool::Nmcli => "Managed by NetworkManager.",
                NetTool::Ip => "Read-only: NetworkManager (nmcli) is not installed, so connections are configured outside Glaze (systemd-networkd or ifupdown).",
            });
            card.note.set_visible(true);
            let mut first = true;
            for l in &links {
                let state = if l.connection.is_empty() {
                    l.state.clone()
                } else {
                    format!("{} · {}", l.state, l.connection)
                };
                let addrs = if l.addresses.is_empty() {
                    "no address".to_string()
                } else {
                    l.addresses.join(", ")
                };
                let icon = if l.kind == "wifi" {
                    "network-wifi"
                } else {
                    "network-wired"
                };
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                b.add_css_class("fs-row");
                b.append(&app.icon(icon, 20, IconTone::Ink));
                let v = gtk::Box::new(gtk::Orientation::Vertical, 2);
                let t = gtk::Label::new(Some(&format!("{} ({})", l.device, l.kind)));
                t.add_css_class("fs-row-title");
                t.set_xalign(0.0);
                let s = gtk::Label::new(Some(&format!("{state} — {addrs}")));
                s.add_css_class("fs-row-sub");
                s.set_xalign(0.0);
                s.set_selectable(true);
                v.append(&t);
                v.append(&s);
                b.append(&v);
                if first {
                    app.register("network.status", &b, &b);
                    first = false;
                }
                card.add(&b);
            }
            if links.is_empty() {
                let e = widgets::empty_state(
                    app,
                    "network-off",
                    "No interfaces",
                    "Only the loopback interface exists.",
                );
                app.register("network.status", &e, &e);
                card.add(&e);
            }
            match wifi {
                Some(on) => {
                    let sw = gtk::Switch::new();
                    sw.set_active(on);
                    let r = widgets::row(
                        app,
                        "network.wifi",
                        Some("Turns the wireless radio on or off (nmcli radio wifi)."),
                        &sw,
                    );
                    let err = r.error.clone();
                    sw.connect_active_notify(move |s| {
                        match sysinfo::act(
                            "nmcli",
                            &["radio", "wifi", if s.is_active() { "on" } else { "off" }],
                        ) {
                            Ok(()) => err.set_visible(false),
                            Err(e) => {
                                err.set_text(&e);
                                err.set_visible(true);
                            }
                        }
                    });
                    card.add(&r.root);
                }
                None => {
                    let l = gtk::Label::new(Some("Needs NetworkManager"));
                    l.add_css_class("fs-row-sub");
                    let r = widgets::row(app, "network.wifi", None, &l);
                    card.add(&r.root);
                }
            }
        }
    }
    page.add(&card.frame);
    page.finish()
}

pub fn power(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::Power);
    let card = Card::new("Power source", None);
    let supplies = sysinfo::read_supplies(std::path::Path::new("/sys/class/power_supply"));
    let bats: Vec<_> = supplies.iter().filter(|s| s.kind == "Battery").collect();
    let ac = supplies
        .iter()
        .find(|s| s.kind == "Mains")
        .and_then(|s| s.online);
    if bats.is_empty() {
        let body = match ac {
            Some(true) => {
                "Running on mains power. No battery was found (a desktop or a virtual machine)."
            }
            _ => "No battery was found (a desktop or a virtual machine).",
        };
        let e = widgets::empty_state(app, "power", "No battery", body);
        app.register("power.battery", &e, &e);
        card.add(&e);
    } else {
        for (i, b) in bats.iter().enumerate() {
            let pct = b
                .capacity
                .map(|c| format!("{c} %"))
                .unwrap_or_else(|| "unknown".into());
            let lvl = gtk::LevelBar::for_interval(0.0, 100.0);
            lvl.set_value(b.capacity.unwrap_or(0) as f64);
            lvl.set_size_request(160, -1);
            let sub = format!(
                "{} · {}{}",
                pct,
                b.status,
                if ac == Some(true) { " · on mains" } else { "" }
            );
            let r = widgets::row(app, "power.battery", Some(&sub), &lvl);
            if i == 0 {
                app.register("power.battery", &r.root, &lvl);
            }
            card.add(&r.root);
        }
    }
    page.add(&card.frame);
    let saving = Card::new(
        "Energy saving",
        Some("Sleep, restart and shut down are in the Start menu's power row."),
    );
    let r = widgets::switch(
        app,
        "power.saving",
        Doc::Main,
        "appearance.reduced_effects",
        app.cfg().appearance.reduced_effects,
        Some("Flat fills without paper texture or shadows: less drawing on battery and on software rendering. Same switch as Appearance › Reduced effects."),
    );
    saving.add(&r.root);
    page.add(&saving.frame);
    page.finish()
}
