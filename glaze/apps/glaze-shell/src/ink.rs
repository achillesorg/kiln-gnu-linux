//! Ink-outline glyphs drawn with cairo (mockup taskbar-v2 icon set).
//!
//! Small UI glyphs stay geometric (`docs/decisions.md` Q4). They are drawn in
//! the widget's CSS `color` (the `@glaze_ink` role) with the theme's icon
//! stroke, so they follow the theme, the scheme and user overrides, and stay
//! crisp at any scale. A glyph is a 1-px-snapped path in a 20x20 design box.

use gtk4::prelude::*;
use gtk4::{cairo, gdk};

/// Named glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    StartDots,
    Folder,
    Terminal,
    Globe,
    Window,
    Float,
    Tile,
    Wifi,
    Volume,
    Bell,
    Close,
    Pin,
    Power,
    Lock,
    Moon,
    Restart,
    Logout,
    Search,
    Settings,
}

/// A drawing-area glyph. `stroke` is the line width in px.
pub fn glyph(g: Glyph, size: i32, stroke: f64) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_valign(gtk4::Align::Center);
    area.set_halign(gtk4::Align::Center);
    area.add_css_class("glaze-glyph");
    area.set_draw_func(move |area, cr, w, h| {
        let color = area.color();
        draw(cr, g, w as f64, h as f64, stroke, &color, None);
    });
    area
}

fn draw(
    cr: &cairo::Context,
    g: Glyph,
    w: f64,
    h: f64,
    stroke: f64,
    color: &gdk::RGBA,
    accent: Option<gdk::RGBA>,
) {
    let s = w.min(h) / 20.0;
    cr.translate(
        ((w - 20.0 * s) / 2.0).round(),
        ((h - 20.0 * s) / 2.0).round(),
    );
    cr.scale(s, s);
    cr.set_line_width(stroke / s);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
    let rr = |cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64| {
        cr.new_sub_path();
        cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
        cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
        cr.arc(
            x + r,
            y + h - r,
            r,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        );
        cr.arc(
            x + r,
            y + r,
            r,
            std::f64::consts::PI,
            1.5 * std::f64::consts::PI,
        );
        cr.close_path();
    };
    let pi = std::f64::consts::PI;
    match g {
        Glyph::StartDots => {
            // Two squares and two circles, filled (mockup start mark).
            rr(cr, 4.0, 4.0, 5.0, 5.0, 1.0);
            cr.arc(13.5, 6.5, 2.5, 0.0, 2.0 * pi);
            cr.new_sub_path();
            cr.arc(6.5, 13.5, 2.5, 0.0, 2.0 * pi);
            rr(cr, 11.0, 11.0, 5.0, 5.0, 1.0);
            let _ = cr.fill();
            return;
        }
        Glyph::Folder => {
            cr.move_to(4.0, 5.5);
            cr.line_to(8.0, 5.5);
            cr.line_to(9.8, 7.3);
            cr.line_to(16.0, 7.3);
            cr.line_to(16.0, 15.0);
            cr.line_to(4.0, 15.0);
            cr.close_path();
        }
        Glyph::Terminal => {
            rr(cr, 2.5, 4.0, 15.0, 12.0, 1.8);
            cr.move_to(6.0, 8.0);
            cr.line_to(8.5, 10.0);
            cr.line_to(6.0, 12.0);
            cr.move_to(10.5, 12.5);
            cr.line_to(14.0, 12.5);
        }
        Glyph::Globe => {
            cr.arc(10.0, 10.0, 7.0, 0.0, 2.0 * pi);
            cr.new_sub_path();
            cr.save().ok();
            cr.translate(10.0, 10.0);
            cr.scale(3.0 / 7.0, 1.0);
            cr.arc(0.0, 0.0, 7.0, 0.0, 2.0 * pi);
            cr.restore().ok();
            cr.move_to(3.0, 10.0);
            cr.line_to(17.0, 10.0);
        }
        Glyph::Window => {
            rr(cr, 3.0, 4.0, 14.0, 12.0, 1.8);
            cr.move_to(3.0, 7.5);
            cr.line_to(17.0, 7.5);
        }
        Glyph::Float => {
            cr.rectangle(4.0, 5.0, 9.0, 7.0);
            cr.rectangle(8.0, 8.5, 9.0, 7.0);
        }
        Glyph::Tile => {
            cr.rectangle(3.5, 4.0, 6.0, 12.0);
            cr.rectangle(9.5, 4.0, 7.0, 5.5);
            cr.rectangle(9.5, 9.5, 7.0, 6.5);
        }
        Glyph::Wifi => {
            cr.arc(10.0, 15.0, 9.0, pi * 1.25, pi * 1.75);
            cr.new_sub_path();
            cr.arc(10.0, 15.0, 5.0, pi * 1.25, pi * 1.75);
            let _ = cr.stroke();
            cr.arc(10.0, 14.5, 1.0, 0.0, 2.0 * pi);
            let _ = cr.fill();
            return;
        }
        Glyph::Volume => {
            cr.move_to(3.5, 8.0);
            cr.line_to(6.5, 8.0);
            cr.line_to(10.5, 4.5);
            cr.line_to(10.5, 15.5);
            cr.line_to(6.5, 12.0);
            cr.line_to(3.5, 12.0);
            cr.close_path();
            cr.new_sub_path();
            cr.arc(12.0, 10.0, 3.5, -pi / 3.0, pi / 3.0);
        }
        Glyph::Bell => {
            cr.move_to(5.0, 14.0);
            cr.line_to(15.0, 14.0);
            cr.line_to(13.5, 12.0);
            cr.line_to(13.5, 8.5);
            cr.arc_negative(10.0, 8.5, 3.5, 0.0, pi);
            cr.line_to(6.5, 12.0);
            cr.close_path();
            cr.move_to(8.0, 16.5);
            cr.line_to(12.0, 16.5);
            let _ = cr.stroke();
            if let Some(a) = accent {
                cr.set_source_rgba(a.red() as f64, a.green() as f64, a.blue() as f64, 1.0);
                cr.arc(15.0, 4.5, 2.6, 0.0, 2.0 * pi);
                let _ = cr.fill();
            }
            return;
        }
        Glyph::Close => {
            cr.move_to(6.0, 6.0);
            cr.line_to(14.0, 14.0);
            cr.move_to(14.0, 6.0);
            cr.line_to(6.0, 14.0);
        }
        Glyph::Pin => {
            cr.move_to(10.0, 17.0);
            cr.line_to(10.0, 12.0);
            cr.move_to(6.0, 12.0);
            cr.line_to(14.0, 12.0);
            cr.line_to(12.5, 9.5);
            cr.line_to(12.5, 4.0);
            cr.line_to(7.5, 4.0);
            cr.line_to(7.5, 9.5);
            cr.close_path();
        }
        Glyph::Power => {
            cr.arc(10.0, 10.5, 6.0, -pi * 0.3, pi * 1.3);
            cr.move_to(10.0, 3.0);
            cr.line_to(10.0, 9.5);
        }
        Glyph::Lock => {
            rr(cr, 4.5, 9.0, 11.0, 8.0, 1.5);
            cr.move_to(7.0, 9.0);
            cr.line_to(7.0, 6.5);
            cr.arc(10.0, 6.5, 3.0, pi, 2.0 * pi);
            cr.line_to(13.0, 9.0);
        }
        Glyph::Moon => {
            cr.arc(10.0, 10.0, 6.5, pi * 0.35, pi * 1.65);
            cr.arc_negative(13.0, 8.0, 5.0, pi * 1.5, pi * 0.6);
            cr.close_path();
        }
        Glyph::Restart => {
            cr.arc(10.0, 10.0, 6.0, -pi * 0.2, pi * 1.5);
            cr.move_to(15.5, 3.5);
            cr.line_to(15.0, 7.2);
            cr.line_to(11.5, 6.4);
        }
        Glyph::Logout => {
            cr.move_to(11.0, 4.0);
            cr.line_to(5.0, 4.0);
            cr.line_to(5.0, 16.0);
            cr.line_to(11.0, 16.0);
            cr.move_to(9.0, 10.0);
            cr.line_to(16.5, 10.0);
            cr.move_to(13.5, 7.0);
            cr.line_to(16.5, 10.0);
            cr.line_to(13.5, 13.0);
        }
        Glyph::Search => {
            cr.arc(8.5, 8.5, 4.8, 0.0, 2.0 * pi);
            cr.move_to(12.0, 12.0);
            cr.line_to(16.0, 16.0);
        }
        Glyph::Settings => {
            cr.arc(10.0, 10.0, 2.5, 0.0, 2.0 * pi);
            for i in 0..8 {
                let a = i as f64 * pi / 4.0;
                cr.move_to(10.0 + 5.0 * a.cos(), 10.0 + 5.0 * a.sin());
                cr.line_to(10.0 + 7.0 * a.cos(), 10.0 + 7.0 * a.sin());
            }
            cr.new_sub_path();
            cr.arc(10.0, 10.0, 5.0, 0.0, 2.0 * pi);
        }
    }
    let _ = cr.stroke();
}

/// The glyph that represents an app in ink style, if Glaze has one.
pub fn app_glyph(app_id: &str, categories: &[String]) -> Option<Glyph> {
    let id = app_id.to_ascii_lowercase();
    let has = |c: &str| categories.iter().any(|x| x == c);
    if has("TerminalEmulator")
        || ["foot", "footclient", "kitty", "alacritty", "xterm"]
            .iter()
            .any(|t| id == *t || id.ends_with(&format!(".{t}")))
        || id.contains("terminal")
        || id.contains("console")
    {
        Some(Glyph::Terminal)
    } else if has("FileManager")
        || id.contains("nautilus")
        || id.contains("thunar")
        || id.contains("files")
    {
        Some(Glyph::Folder)
    } else if has("WebBrowser")
        || id.contains("firefox")
        || id.contains("chromium")
        || id.contains("browser")
    {
        Some(Glyph::Globe)
    } else if id.contains("settings") {
        Some(Glyph::Settings)
    } else {
        None
    }
}
