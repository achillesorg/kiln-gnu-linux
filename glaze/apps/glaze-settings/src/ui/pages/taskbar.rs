//! Taskbar: edge, density, height, auto-hide, buttons, section order, tray,
//! clock, float/tile toggle and per-monitor overrides. Everything is a
//! `[panel]` key of config.toml, reloaded live by glaze-shell.

use gtk4 as gtk;
use gtk4::prelude::*;

use glaze_common::config::{
    Autohide, ButtonStyle, Density, GroupingMode, LabelMode, LayoutScope, PanelPosition,
    PanelSection, TrayItem,
};

use crate::catalog::Page;
use crate::store::{value_of, Change, Doc};
use crate::ui::widgets::{self, Card, OrderItem, PageBuilder};
use crate::ui::App;
use crate::validate;

fn section_label(s: PanelSection) -> &'static str {
    match s {
        PanelSection::Start => "Start button",
        PanelSection::Pinned => "Pinned apps",
        PanelSection::Tasks => "Window buttons",
        PanelSection::Spacer => "Flexible space",
        PanelSection::LayoutToggle => "Float/tile toggle",
        PanelSection::Tray => "Tray",
        PanelSection::Clock => "Clock",
        PanelSection::ShowDesktop => "Show desktop",
    }
}

fn tray_label(t: TrayItem) -> &'static str {
    match t {
        TrayItem::StatusNotifier => "App icons (StatusNotifier)",
        TrayItem::Network => "Network",
        TrayItem::Volume => "Volume",
        TrayItem::Battery => "Battery",
        TrayItem::Notifications => "Notifications bell",
    }
}

fn slug<T: serde::Serialize>(v: &T) -> String {
    value_of(v).as_str().unwrap_or_default().to_string()
}

/// Schematic preview of the bar on a screen, drawn with the tokens.
fn preview(app: &App) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::new();
    d.set_content_width(240);
    d.set_content_height(140);
    d.add_css_class("fs-preview-box");
    d.update_property(&[gtk::accessible::Property::Label("Taskbar preview")]);
    let a = app.clone();
    d.set_draw_func(move |_, cr, w, h| {
        let cfg = a.cfg();
        let t = a.tokens();
        let p = &t.palette;
        let set = |c: glaze_design::Color| {
            cr.set_source_rgba(
                c.r as f64 / 255.0,
                c.g as f64 / 255.0,
                c.b as f64 / 255.0,
                c.a as f64 / 255.0,
            )
        };
        let (w, h) = (w as f64, h as f64);
        set(p.accent_soft);
        cr.rectangle(0.0, 0.0, w, h);
        let _ = cr.fill();
        // A window sheet.
        set(p.surface_raised);
        cr.rectangle(w * 0.28, h * 0.22, w * 0.42, h * 0.42);
        let _ = cr.fill();
        set(p.border);
        cr.set_line_width(1.0);
        cr.rectangle(w * 0.28 + 0.5, h * 0.22 + 0.5, w * 0.42, h * 0.42);
        let _ = cr.stroke();
        let thick = (cfg.panel_height(&t) as f64 / 1080.0 * h * 2.4).clamp(8.0, 26.0);
        let pos = cfg.panel.position;
        let (bx, by, bw, bh) = match pos {
            PanelPosition::Bottom => (0.0, h - thick, w, thick),
            PanelPosition::Top => (0.0, 0.0, w, thick),
            PanelPosition::Left => (0.0, 0.0, thick, h),
            PanelPosition::Right => (w - thick, 0.0, thick, h),
        };
        if cfg.panel.autohide != Autohide::Never {
            cr.set_dash(&[3.0, 3.0], 0.0);
        }
        set(p.panel);
        cr.rectangle(bx, by, bw, bh);
        let _ = cr.fill_preserve();
        set(p.border);
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
        // Sections in order along the bar.
        let vertical = pos.is_vertical();
        let len = if vertical { h } else { w };
        let mut at = 4.0;
        let unit = thick - 6.0;
        for s in &cfg.panel.sections {
            let (size, color) = match s {
                PanelSection::Start => (unit * 1.8, Some(p.accent)),
                PanelSection::Pinned => (unit * 2.0, Some(p.border)),
                PanelSection::Tasks => (unit * 4.0, Some(p.surface_raised)),
                PanelSection::Spacer => {
                    let rest: f64 = cfg
                        .panel
                        .sections
                        .iter()
                        .filter(|x| **x != PanelSection::Spacer)
                        .count() as f64
                        * unit
                        * 1.6;
                    ((len - rest - 40.0).max(6.0), None)
                }
                PanelSection::LayoutToggle => (unit * 1.4, Some(p.accent_soft)),
                PanelSection::Tray => (unit * 2.0, Some(p.surface_sunken)),
                PanelSection::Clock => (unit * 1.6, Some(p.text_secondary)),
                PanelSection::ShowDesktop => (unit * 0.4, Some(p.border)),
            };
            if let Some(c) = color {
                set(c);
                if vertical {
                    cr.rectangle(bx + 3.0, at, bw - 6.0, size);
                } else {
                    cr.rectangle(at, by + 3.0, size, bh - 6.0);
                }
                let _ = cr.fill();
            }
            at += size + 3.0;
            if at > len {
                break;
            }
        }
    });
    app.previews.borrow_mut().push(d.downgrade());
    d
}

pub fn build(app: &App) -> gtk::Widget {
    let cfg = app.cfg();
    let pc = &cfg.panel;
    let page = PageBuilder::new(app, Page::Taskbar);

    // Layout ---------------------------------------------------------------
    let layout = Card::new(
        "Layout",
        Some("Changes reach the running taskbar within a moment."),
    );
    let pv = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pv.set_halign(gtk::Align::Center);
    pv.set_margin_top(4);
    pv.set_margin_bottom(8);
    pv.append(&preview(app));
    layout.add(&pv);
    layout.add(
        &widgets::enum_choice(
            app,
            "panel.position",
            Doc::Main,
            "panel.position",
            &[
                (PanelPosition::Top, "Top"),
                (PanelPosition::Bottom, "Bottom"),
                (PanelPosition::Left, "Left"),
                (PanelPosition::Right, "Right"),
            ],
            pc.position,
            None,
        )
        .root,
    );
    layout.add(
        &widgets::enum_choice(
            app,
            "panel.density",
            Doc::Main,
            "panel.density",
            &[
                (Density::Compact, "Compact"),
                (Density::Normal, "Normal"),
                (Density::Comfortable, "Comfortable"),
            ],
            pc.density,
            Some("Scales the bar and its buttons together."),
        )
        .root,
    );
    {
        let auto = gtk::CheckButton::with_label("Automatic");
        auto.set_active(pc.height.is_none());
        let theme_h = cfg.tokens().metrics.panel_height;
        let (r, sp) = widgets::spin(
            app,
            "panel.height",
            24.0,
            96.0,
            pc.height.unwrap_or(theme_h) as f64,
            Some("Thickness in pixels; Automatic follows the theme and density."),
            |a, v| {
                a.set(
                    Doc::Main,
                    "panel.height",
                    toml::Value::Integer(v),
                    "Taskbar height",
                )
            },
        );
        sp.set_sensitive(pc.height.is_some());
        r.line
            .insert_child_after(&auto, Some(&r.line.first_child().unwrap()));
        let a = app.clone();
        let sp2 = sp.clone();
        auto.connect_toggled(move |c| {
            if a.building.get() {
                return;
            }
            sp2.set_sensitive(!c.is_active());
            let _ = if c.is_active() {
                a.remove(Doc::Main, "panel.height", "Taskbar height: automatic")
            } else {
                a.set(
                    Doc::Main,
                    "panel.height",
                    toml::Value::Integer(sp2.value() as i64),
                    "Taskbar height",
                )
            };
        });
        layout.add(&r.root);
    }
    layout.add(
        &widgets::enum_choice(
            app,
            "panel.autohide",
            Doc::Main,
            "panel.autohide",
            &[
                (Autohide::Never, "Never"),
                (Autohide::Smart, "Smart"),
                (Autohide::Always, "Always"),
            ],
            pc.autohide,
            Some("Hidden bars leave a 3 px edge; touch it to bring the bar back."),
        )
        .root,
    );
    page.add(&layout.frame);

    // Buttons ---------------------------------------------------------------
    let buttons = Card::new("Window buttons", None);
    buttons.add(
        &widgets::enum_choice(
            app,
            "panel.button_style",
            Doc::Main,
            "panel.button_style",
            &[
                (ButtonStyle::Paper, "Paper"),
                (ButtonStyle::Flat, "Flat"),
                (ButtonStyle::Outline, "Outline"),
            ],
            pc.button_style,
            None,
        )
        .root,
    );
    {
        let cur = pc.label_mode();
        let r = widgets::choice(
            app,
            "panel.labels",
            &["Titles", "Icons only", "Active only"],
            match cur {
                LabelMode::Full => 0,
                LabelMode::IconOnly => 1,
                LabelMode::ActiveOnly => 2,
            },
            None,
            |a, i| {
                let m = [LabelMode::Full, LabelMode::IconOnly, LabelMode::ActiveOnly][i];
                a.apply(
                    vec![
                        Change::Set(Doc::Main, "panel.labels".into(), value_of(&m)),
                        Change::Set(Doc::Main, "panel.show_labels".into(), true.into()),
                    ],
                    "Window labels",
                    false,
                )
            },
        );
        buttons.add(&r.root);
    }
    buttons.add(
        &widgets::enum_choice(
            app,
            "panel.grouping",
            Doc::Main,
            "panel.grouping",
            &[
                (GroupingMode::Always, "Always"),
                (GroupingMode::Auto, "When full"),
                (GroupingMode::Never, "Never"),
            ],
            pc.grouping,
            Some("Grouped windows of one app share a button with a count."),
        )
        .root,
    );
    buttons.add(
        &widgets::switch(
            app,
            "panel.default_pins",
            Doc::Main,
            "panel.default_pins",
            pc.default_pins,
            Some("While nothing is pinned, show a file manager, terminal and browser."),
        )
        .root,
    );
    page.add(&buttons.frame);

    // Sections ---------------------------------------------------------------
    let sections = Card::new(
        "Sections",
        Some("Drag, use the arrows or Alt+Up/Down to reorder; untick to hide."),
    );
    {
        let mut items: Vec<OrderItem> = pc
            .sections
            .iter()
            .map(|s| OrderItem {
                key: slug(s),
                label: section_label(*s).into(),
                on: true,
                removable: false,
            })
            .collect();
        for s in PanelSection::DEFAULT_ORDER {
            if !pc.sections.contains(&s) {
                items.push(OrderItem {
                    key: slug(&s),
                    label: section_label(s).into(),
                    on: false,
                    removable: false,
                });
            }
        }
        let ol = widgets::order_list(app, items, true, |a, items| {
            let v: Vec<toml::Value> = items
                .iter()
                .filter(|i| i.on)
                .map(|i| toml::Value::String(i.key.clone()))
                .collect();
            if !items.iter().any(|i| i.on && i.key == "start") {
                a.toast("Without the Start button the menu is only reachable by keyboard (Super+Space).", false);
            }
            a.set(
                Doc::Main,
                "panel.sections",
                toml::Value::Array(v),
                "Taskbar section order",
            )
        });
        let r = widgets::row(
            app,
            "panel.sections",
            None,
            &gtk::Box::new(gtk::Orientation::Horizontal, 0),
        );
        r.root.append(&ol.list);
        if let Some(first) = ol.list.row_at_index(0) {
            app.register("panel.sections", &r.root, &first);
        }
        sections.add(&r.root);
    }
    page.add(&sections.frame);

    // Tray ---------------------------------------------------------------------
    let tray = Card::new(
        "Tray",
        Some("Items only appear when they have real state (no battery, no battery icon)."),
    );
    {
        let mut items: Vec<OrderItem> = pc
            .tray_items
            .iter()
            .map(|s| OrderItem {
                key: slug(s),
                label: tray_label(*s).into(),
                on: true,
                removable: false,
            })
            .collect();
        for s in TrayItem::DEFAULT {
            if !pc.tray_items.contains(&s) {
                items.push(OrderItem {
                    key: slug(&s),
                    label: tray_label(s).into(),
                    on: false,
                    removable: false,
                });
            }
        }
        let ol = widgets::order_list(app, items, true, |a, items| {
            let v: Vec<toml::Value> = items
                .iter()
                .filter(|i| i.on)
                .map(|i| toml::Value::String(i.key.clone()))
                .collect();
            a.set(
                Doc::Main,
                "panel.tray_items",
                toml::Value::Array(v),
                "Tray items",
            )
        });
        let r = widgets::row(
            app,
            "panel.tray_items",
            None,
            &gtk::Box::new(gtk::Orientation::Horizontal, 0),
        );
        r.root.append(&ol.list);
        if let Some(first) = ol.list.row_at_index(0) {
            app.register("panel.tray_items", &r.root, &first);
        }
        tray.add(&r.root);
    }
    page.add(&tray.frame);

    // Clock --------------------------------------------------------------------
    let clock = Card::new("Clock", None);
    clock.add(
        &widgets::switch(
            app,
            "panel.clock_24h",
            Doc::Main,
            "panel.clock_24h",
            pc.clock_24h,
            None,
        )
        .root,
    );
    clock.add(
        &widgets::switch(
            app,
            "panel.show_seconds",
            Doc::Main,
            "panel.show_seconds",
            pc.show_seconds,
            None,
        )
        .root,
    );
    clock.add(
        &widgets::switch(
            app,
            "panel.show_date",
            Doc::Main,
            "panel.show_date",
            pc.show_date,
            None,
        )
        .root,
    );
    {
        let sample = (2026, 11, 12, 3, 9, 5, 7);
        let fmt = pc.date_format.clone().unwrap_or_default();
        let shown = if fmt.is_empty() {
            "%a %d %b".to_string()
        } else {
            fmt.clone()
        };
        let sub = format!(
            "Preview: {}. Tokens: %a %A %d %b %m %y %Y %H %M %S. Empty uses %a %d %b.",
            validate::preview_date(&shown, sample)
        );
        let (r, e) = widgets::entry(
            app,
            "panel.date_format",
            &fmt,
            "%a %d %b",
            Some(&sub),
            validate::date_format,
            |a, s| {
                if s.trim().is_empty() {
                    a.remove(Doc::Main, "panel.date_format", "Date format: default")
                } else {
                    a.set(Doc::Main, "panel.date_format", s.into(), "Date format")
                }
            },
        );
        let subl = r.sub.clone();
        e.connect_changed(move |e| {
            let t = e.text();
            let f = if t.is_empty() {
                "%a %d %b".to_string()
            } else {
                t.to_string()
            };
            if validate::date_format(&f).is_ok() {
                subl.set_text(&format!(
                    "Preview: {}. Tokens: %a %A %d %b %m %y %Y %H %M %S. Empty uses %a %d %b.",
                    validate::preview_date(&f, sample)
                ));
            }
        });
        clock.add(&r.root);
    }
    page.add(&clock.frame);

    // Float / tile --------------------------------------------------------------
    let ft = Card::new("Float/tile toggle", None);
    ft.add(
        &widgets::switch(
            app,
            "panel.show_layout_toggle",
            Doc::Main,
            "panel.show_layout_toggle",
            pc.show_layout_toggle,
            Some("The segmented stacking/tiling switch next to the tray."),
        )
        .root,
    );
    ft.add(
        &widgets::enum_choice(
            app,
            "panel.layout_scope",
            Doc::Main,
            "panel.layout_scope",
            &[
                (LayoutScope::Workspace, "This workspace"),
                (LayoutScope::Global, "All workspaces"),
            ],
            pc.layout_scope,
            None,
        )
        .root,
    );
    page.add(&ft.frame);

    // Per monitor -------------------------------------------------------------------
    let mon = Card::new(
        "Monitors",
        Some("Override the taskbar per output. Inherit uses the settings above."),
    );
    let outputs = monitor_names(app);
    if outputs.is_empty() {
        mon.add(&widgets::empty_state(
            app,
            "display",
            "No outputs found",
            "The display server did not report any outputs.",
        ));
        app.register("panel.monitors", &mon.frame, &mon.frame);
    }
    for (i, name) in outputs.iter().enumerate() {
        let o = pc.monitors.get(name).cloned().unwrap_or_default();
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let show = gtk::Switch::new();
        show.set_active(o.enabled.unwrap_or(true));
        show.set_tooltip_text(Some("Show the taskbar on this output"));
        show.update_property(&[gtk::accessible::Property::Label(&format!(
            "Taskbar on {name}"
        ))]);
        let edges = ["Inherit edge", "Top", "Bottom", "Left", "Right"];
        let edge = gtk::DropDown::from_strings(&edges);
        edge.set_selected(match o.position {
            None => 0,
            Some(PanelPosition::Top) => 1,
            Some(PanelPosition::Bottom) => 2,
            Some(PanelPosition::Left) => 3,
            Some(PanelPosition::Right) => 4,
        });
        edge.update_property(&[gtk::accessible::Property::Label(&format!("Edge on {name}"))]);
        let dens =
            gtk::DropDown::from_strings(&["Inherit density", "Compact", "Normal", "Comfortable"]);
        dens.set_selected(match o.density {
            None => 0,
            Some(Density::Compact) => 1,
            Some(Density::Normal) => 2,
            Some(Density::Comfortable) => 3,
        });
        dens.update_property(&[gtk::accessible::Property::Label(&format!(
            "Density on {name}"
        ))]);
        bx.append(&edge);
        bx.append(&dens);
        bx.append(&show);
        let id: &'static str = if i == 0 {
            "panel.monitors"
        } else {
            Box::leak(format!("panel.monitors.{name}").into_boxed_str())
        };
        let r = widgets::row(app, "panel.monitors", Some(&format!("Output {name}")), &bx);
        if i == 0 {
            app.register(id, &r.root, &edge);
        }
        let n1 = name.clone();
        let a = app.clone();
        show.connect_active_notify(move |s| {
            if a.building.get() {
                return;
            }
            let p = format!("panel.monitors.{n1}.enabled");
            let _ = if s.is_active() {
                a.remove(Doc::Main, &p, &format!("Taskbar on {n1}"))
            } else {
                a.set(
                    Doc::Main,
                    &p,
                    false.into(),
                    &format!("Taskbar hidden on {n1}"),
                )
            };
        });
        let n2 = name.clone();
        let a = app.clone();
        edge.connect_selected_notify(move |d| {
            if a.building.get() {
                return;
            }
            let p = format!("panel.monitors.{n2}.position");
            let vals = [
                PanelPosition::Top,
                PanelPosition::Bottom,
                PanelPosition::Left,
                PanelPosition::Right,
            ];
            let _ = match d.selected() {
                0 => a.remove(Doc::Main, &p, &format!("Edge on {n2}: inherit")),
                i => a.set(
                    Doc::Main,
                    &p,
                    value_of(&vals[(i - 1) as usize]),
                    &format!("Edge on {n2}"),
                ),
            };
        });
        let n3 = name.clone();
        let a = app.clone();
        dens.connect_selected_notify(move |d| {
            if a.building.get() {
                return;
            }
            let p = format!("panel.monitors.{n3}.density");
            let vals = [Density::Compact, Density::Normal, Density::Comfortable];
            let _ = match d.selected() {
                0 => a.remove(Doc::Main, &p, &format!("Density on {n3}: inherit")),
                i => a.set(
                    Doc::Main,
                    &p,
                    value_of(&vals[(i - 1) as usize]),
                    &format!("Density on {n3}"),
                ),
            };
        });
        mon.add(&r.root);
    }
    page.add(&mon.frame);
    page.finish()
}

/// Output connector names from GDK (falls back to the configured overrides).
pub fn monitor_names(app: &App) -> Vec<String> {
    let mut v = Vec::new();
    let display = gtk::prelude::WidgetExt::display(&app.window);
    let list = display.monitors();
    for i in 0..list.n_items() {
        if let Some(m) = list
            .item(i)
            .and_then(|o| o.downcast::<gtk::gdk::Monitor>().ok())
        {
            if let Some(c) = m.connector() {
                v.push(c.to_string());
            }
        }
    }
    if v.is_empty() {
        v = app.cfg().panel.monitors.keys().cloned().collect();
    }
    v
}
