//! Animation: style, speed (with a live demo), reduce motion, the reserved
//! stop-motion row and the frame-time log switch.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;

use glaze_common::config::MotionStyle;

use crate::catalog::Page;
use crate::store::Doc;
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::{duration_scale, reduced_motion, App};

/// A paper square that glides across a track with the current timing.
fn demo(app: &App) -> (gtk::Box, gtk::Button) {
    let area = gtk::DrawingArea::new();
    area.set_content_height(56);
    area.set_hexpand(true);
    area.add_css_class("fs-preview-box");
    area.update_property(&[gtk::accessible::Property::Label("Animation preview")]);
    let pos = Rc::new(Cell::new(0.0f64));
    {
        let a = app.clone();
        let pos = pos.clone();
        area.set_draw_func(move |_, cr, w, h| {
            let t = a.tokens();
            let p = &t.palette;
            let set = |c: glaze_design::Color| {
                cr.set_source_rgb(c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0)
            };
            let (w, h) = (w as f64, h as f64);
            set(p.border);
            cr.rectangle(16.0, h / 2.0, w - 32.0, 1.0);
            let _ = cr.fill();
            let s = 30.0;
            let x = 16.0 + (w - 32.0 - s) * pos.get();
            set(p.accent);
            cr.rectangle(x + 2.0, h / 2.0 - s / 2.0 + 2.0, s, s);
            let _ = cr.fill();
            set(p.surface_raised);
            cr.rectangle(x, h / 2.0 - s / 2.0, s, s);
            let _ = cr.fill_preserve();
            set(p.ink);
            cr.set_line_width(t.metrics.stroke_width.max(1.0));
            let _ = cr.stroke();
        });
    }
    let play = gtk::Button::with_label("Play");
    let a = app.clone();
    let ar = area.clone();
    let target = Rc::new(Cell::new(1.0f64));
    play.connect_clicked(move |_| {
        let cfg = a.cfg();
        let from = pos.get();
        let to = target.get();
        target.set(1.0 - to);
        let ms = if reduced_motion(&cfg) {
            0.0
        } else {
            420.0 * duration_scale(&cfg)
        };
        if ms <= 0.0 {
            pos.set(to);
            ar.queue_draw();
            return;
        }
        let t0 = Instant::now();
        let pos = pos.clone();
        ar.add_tick_callback(move |w, _| {
            let t = (t0.elapsed().as_secs_f64() * 1000.0 / ms).min(1.0);
            // Critically damped feel: ease-out cubic, no overshoot.
            let e = 1.0 - (1.0 - t).powi(3);
            pos.set(from + (to - from) * e);
            w.queue_draw();
            if t >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    b.set_margin_start(16);
    b.set_margin_end(16);
    b.set_margin_top(6);
    b.set_margin_bottom(8);
    b.append(&area);
    b.append(&play);
    play.set_valign(gtk::Align::Center);
    (b, play)
}

pub fn build(app: &App) -> gtk::Widget {
    let cfg = app.cfg();
    let page = PageBuilder::new(app, Page::Animation);
    let motion = Card::new(
        "Motion",
        Some("Smooth, interruptible springs everywhere. Press Play to try the current timing."),
    );
    let (d, play) = demo(app);
    motion.add(&d);
    let style = widgets::enum_choice(
        app,
        "motion.style",
        Doc::Main,
        "motion.style",
        &[
            (MotionStyle::Smooth, "Smooth"),
            (MotionStyle::Snappy, "Snappy"),
            (MotionStyle::Off, "Off"),
        ],
        if cfg.motion.style == MotionStyle::StopMotion {
            MotionStyle::Smooth
        } else {
            cfg.motion.style
        },
        None,
    );
    motion.add(&style.root);
    // Speed on a log2 scale: -2..2 => 0.25x..4x.
    let (r, s) = widgets::slider(
        app,
        "motion.speed",
        -2.0,
        2.0,
        0.25,
        cfg.motion.speed.log2(),
        Some("Applies to the shell, this window and (when supported) the compositor."),
        |v| format!("{:.2}×", 2f64.powf(v)),
        |a, v| {
            let speed = (2f64.powf(v) * 100.0).round() / 100.0;
            a.set(
                Doc::Main,
                "motion.speed",
                toml::Value::Float(speed),
                "Animation speed",
            )
        },
    );
    s.add_mark(0.0, gtk::PositionType::Bottom, Some("1×"));
    s.add_mark(-1.0, gtk::PositionType::Bottom, Some("½×"));
    s.add_mark(1.0, gtk::PositionType::Bottom, Some("2×"));
    motion.add(&r.root);
    {
        let later = gtk::Label::new(Some("Coming later"));
        later.add_css_class("fs-badge");
        later.add_css_class("fs-later");
        let sw = gtk::Switch::new();
        sw.set_sensitive(false);
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        bx.append(&later);
        bx.append(&sw);
        let r = widgets::row(app, "motion.stop_motion", Some("Stepped 8-12 fps motion with settling jitter, in light, medium or strong. Not available yet."), &bx);
        r.root.add_css_class("fs-off");
        app.register("motion.stop_motion", &r.root, &sw);
        motion.add(&r.root);
    }
    page.add(&motion.frame);
    let acc = Card::new("Accessibility", None);
    acc.add(&widgets::switch(app, "appearance.reduced_motion", Doc::Main, "appearance.reduced_motion", cfg.appearance.reduced_motion, Some("Everything changes instantly. The system setting (gtk-enable-animations) is honoured too.")).root);
    page.add(&acc.frame);
    let diag = Card::new("Diagnostics", None);
    diag.add(
        &widgets::switch(
            app,
            "motion.log_frame_times",
            Doc::Main,
            "motion.log_frame_times",
            cfg.motion.log_frame_times,
            Some("The shell logs animation frame times to its log (useful on software rendering)."),
        )
        .root,
    );
    page.add(&diag.frame);
    let _ = play;
    page.finish()
}
