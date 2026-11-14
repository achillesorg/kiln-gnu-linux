//! Displays: outputs from `swaymsg -t get_outputs` (Sway) or
//! `wlr-randr --json`; resolution, scale, rotation and position are staged
//! per output and applied with a 15-second "keep or revert" confirmation.
//! Changes last for the session (persisting needs shell/compositor support).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::catalog::Page;
use crate::sysinfo::{self, DisplayTool, Mode, Output, OutputChange};
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::App;

const SCALES: [f64; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 3.0];
const ROTATIONS: [(&str, &str); 4] = [
    ("normal", "Normal"),
    ("90", "90°"),
    ("180", "180°"),
    ("270", "270°"),
];

fn arrangement(app: &App, outs: &[Output]) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::new();
    d.set_content_height(150);
    d.set_hexpand(true);
    d.add_css_class("fs-preview-box");
    d.update_property(&[gtk::accessible::Property::Label("Display arrangement")]);
    let outs = outs.to_vec();
    let a = app.clone();
    d.set_draw_func(move |_, cr, w, h| {
        let t = a.tokens();
        let p = &t.palette;
        let set = |c: glaze_design::Color| {
            cr.set_source_rgb(c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0)
        };
        let rects: Vec<(f64, f64, f64, f64, &str)> = outs
            .iter()
            .filter(|o| o.active)
            .map(|o| {
                let (mw, mh) = o
                    .current
                    .as_ref()
                    .map(|m| (m.width as f64, m.height as f64))
                    .unwrap_or((1280.0, 800.0));
                let (mw, mh) = if o.transform.contains("90") || o.transform.contains("270") {
                    (mh, mw)
                } else {
                    (mw, mh)
                };
                (
                    o.x as f64,
                    o.y as f64,
                    mw / o.scale,
                    mh / o.scale,
                    o.name.as_str(),
                )
            })
            .collect();
        if rects.is_empty() {
            return;
        }
        let maxx = rects.iter().map(|r| r.0 + r.2).fold(0.0, f64::max);
        let maxy = rects.iter().map(|r| r.1 + r.3).fold(0.0, f64::max);
        let s = ((w as f64 - 40.0) / maxx).min((h as f64 - 30.0) / maxy);
        let ox = (w as f64 - maxx * s) / 2.0;
        let oy = (h as f64 - maxy * s) / 2.0;
        for (x, y, rw, rh, name) in rects {
            set(p.surface_raised);
            cr.rectangle(ox + x * s, oy + y * s, rw * s - 3.0, rh * s - 3.0);
            let _ = cr.fill_preserve();
            set(p.ink);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
            set(p.accent);
            cr.rectangle(ox + x * s, oy + (y + rh) * s - 9.0, rw * s - 3.0, 6.0);
            let _ = cr.fill();
            set(p.text_primary);
            cr.move_to(ox + x * s + 8.0, oy + y * s + 18.0);
            let _ = cr.show_text(name);
        }
    });
    d
}

pub fn build(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::Displays);
    let Some((tool, outs)) = sysinfo::probe_outputs() else {
        let c = Card::new("Displays", None);
        let e = widgets::empty_state(
            app,
            "display",
            "Display settings are not available here",
            "This session offers neither Sway IPC (swaymsg) nor wlr-output-management (wlr-randr), so outputs cannot be listed or changed from Settings.",
        );
        for id in [
            "displays.resolution",
            "displays.scale",
            "displays.rotation",
            "displays.position",
        ] {
            app.register(id, &e, &e);
        }
        c.add(&e);
        page.add(&c.frame);
        return page.finish();
    };
    let arr = Card::new(
        "Arrangement",
        Some(match tool {
            DisplayTool::Sway => "Through Sway IPC. Changes last until you log out.",
            DisplayTool::WlrRandr => "Through wlr-randr. Changes last until you log out.",
        }),
    );
    let d = arrangement(app, &outs);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bx.set_margin_start(16);
    bx.set_margin_end(16);
    bx.set_margin_bottom(10);
    bx.append(&d);
    arr.add(&bx);
    page.add(&arr.frame);

    for (i, o) in outs.iter().enumerate() {
        let title = if o.description.is_empty() {
            o.name.clone()
        } else {
            format!("{} · {}", o.name, o.description)
        };
        let card = Card::new(
            &title,
            if o.active {
                None
            } else {
                Some("This output is off.")
            },
        );
        let staged = Rc::new(RefCell::new(OutputChange {
            mode: None,
            scale: None,
            transform: None,
            position: None,
        }));
        let reg = |id: &'static str, r: &widgets::Row, w: &gtk::Widget| {
            if i == 0 {
                app.register(id, &r.root, w);
            }
        };
        // Resolution.
        let mut modes: Vec<Mode> = o.modes.clone();
        if let Some(c) = &o.current {
            if !modes.contains(c) {
                modes.insert(0, c.clone());
            }
        }
        if modes.len() <= 1 {
            // Headless or virtual outputs often report no mode list.
            for (w, h) in [(1280, 800), (1366, 768), (1600, 900), (1920, 1080)] {
                let m = Mode {
                    width: w,
                    height: h,
                    refresh: 0,
                };
                if !modes.iter().any(|x| x.width == w && x.height == h) {
                    modes.push(m);
                }
            }
        }
        let labels: Vec<String> = modes.iter().map(|m| m.label()).collect();
        let sel = o
            .current
            .as_ref()
            .and_then(|c| modes.iter().position(|m| m == c))
            .unwrap_or(0);
        let st = staged.clone();
        let m2 = modes.clone();
        let (r, dd) = widgets::dropdown(
            app,
            "displays.resolution",
            &labels,
            sel,
            None,
            false,
            move |_, k| {
                st.borrow_mut().mode = Some(m2[k].clone());
                Ok(())
            },
        );
        reg("displays.resolution", &r, dd.upcast_ref());
        card.add(&r.root);
        // Scale.
        let labels: Vec<String> = SCALES
            .iter()
            .map(|s| format!("{:.0} %", s * 100.0))
            .collect();
        let sel = SCALES
            .iter()
            .position(|s| (s - o.scale).abs() < 0.01)
            .unwrap_or(0);
        let st = staged.clone();
        let (r, dd) = widgets::dropdown(
            app,
            "displays.scale",
            &labels,
            sel,
            None,
            false,
            move |_, k| {
                st.borrow_mut().scale = Some(SCALES[k]);
                Ok(())
            },
        );
        reg("displays.scale", &r, dd.upcast_ref());
        card.add(&r.root);
        // Rotation.
        let sel = ROTATIONS
            .iter()
            .position(|(v, _)| *v == o.transform)
            .unwrap_or(0);
        let st = staged.clone();
        let r = widgets::choice(
            app,
            "displays.rotation",
            &ROTATIONS.map(|(_, l)| l),
            sel,
            None,
            move |_, k| {
                st.borrow_mut().transform = Some(ROTATIONS[k].0.to_string());
                Ok(())
            },
        );
        card.add(&r.root);
        // Position.
        let x = gtk::SpinButton::with_range(-8192.0, 16384.0, 1.0);
        x.set_value(o.x as f64);
        x.update_property(&[gtk::accessible::Property::Label("X position")]);
        let y = gtk::SpinButton::with_range(-8192.0, 16384.0, 1.0);
        y.set_value(o.y as f64);
        y.update_property(&[gtk::accessible::Property::Label("Y position")]);
        let pb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        pb.append(&gtk::Label::new(Some("x")));
        pb.append(&x);
        pb.append(&gtk::Label::new(Some("y")));
        pb.append(&y);
        let r = widgets::row(
            app,
            "displays.position",
            Some("Top-left corner in the global layout, in logical pixels."),
            &pb,
        );
        reg("displays.position", &r, x.upcast_ref());
        for sb in [&x, &y] {
            let (st, x2, y2) = (staged.clone(), x.clone(), y.clone());
            sb.connect_value_changed(move |_| {
                st.borrow_mut().position = Some((x2.value() as i32, y2.value() as i32));
            });
        }
        card.add(&r.root);
        // Apply.
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("fs-primary");
        let ab = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        ab.set_halign(gtk::Align::End);
        ab.set_margin_end(16);
        ab.set_margin_top(4);
        ab.set_margin_bottom(6);
        ab.append(&apply);
        card.add(&ab);
        let (a, out) = (app.clone(), o.clone());
        apply.connect_clicked(move |_| {
            let ch = staged.borrow().clone();
            if ch.mode.is_none()
                && ch.scale.is_none()
                && ch.transform.is_none()
                && ch.position.is_none()
            {
                a.toast("Nothing changed on this output", false);
                return;
            }
            apply_with_revert(&a, tool, &out, ch);
        });
        page.add(&card.frame);
    }
    page.finish()
}

fn run_change(tool: DisplayTool, name: &str, ch: &OutputChange) -> Result<(), String> {
    let (prog, args) = sysinfo::output_command(tool, name, ch);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    sysinfo::act(&prog, &refs)
}

fn apply_with_revert(app: &App, tool: DisplayTool, out: &Output, ch: OutputChange) {
    if let Err(e) = run_change(tool, &out.name, &ch) {
        app.toast_error(&e);
        return;
    }
    let revert = sysinfo::revert_change(out);
    let left = Rc::new(Cell::new(15));
    let body = gtk::Label::new(Some(
        "Keep these display settings? They revert automatically in 15 seconds.",
    ));
    body.set_wrap(true);
    body.set_xalign(0.0);
    let revert_btn = gtk::Button::with_label("Revert");
    let keep = gtk::Button::with_label("Keep");
    keep.add_css_class("fs-primary");
    let w = widgets::dialog(
        app,
        "Display settings",
        &body,
        vec![revert_btn.clone(), keep.clone()],
    );
    let done = Rc::new(Cell::new(false));
    let do_revert = {
        let (a, name, done, w) = (app.clone(), out.name.clone(), done.clone(), w.clone());
        let revert = revert.clone();
        Rc::new(move || {
            if done.replace(true) {
                return;
            }
            w.close();
            match run_change(tool, &name, &revert) {
                Ok(()) => a.toast("Display settings reverted", false),
                Err(e) => a.toast_error(&e),
            }
            a.rebuild(Some("displays.resolution"));
        })
    };
    {
        let r = do_revert.clone();
        revert_btn.connect_clicked(move |_| r());
    }
    {
        let r = do_revert.clone();
        w.connect_close_request(move |_| {
            r();
            glib::Propagation::Proceed
        });
    }
    {
        let (a, done, w) = (app.clone(), done.clone(), w.clone());
        keep.connect_clicked(move |_| {
            done.set(true);
            w.close();
            a.toast("Display settings kept for this session", false);
            a.rebuild(Some("displays.resolution"));
        });
    }
    let (body2, r) = (body.clone(), do_revert.clone());
    glib::timeout_add_local(Duration::from_secs(1), move || {
        if done.get() {
            return glib::ControlFlow::Break;
        }
        let n = left.get() - 1;
        left.set(n);
        if n <= 0 {
            r();
            return glib::ControlFlow::Break;
        }
        body2.set_text(&format!(
            "Keep these display settings? They revert automatically in {n} seconds."
        ));
        glib::ControlFlow::Continue
    });
}
