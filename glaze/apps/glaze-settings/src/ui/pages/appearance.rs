//! Appearance: theme family, scheme, theme file, accent (with contrast
//! check), shape, paper texture, icons and fonts. The settings window itself
//! is the live preview: every change re-emits its CSS.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, gio};

use glaze_common::config::{GlazeConfig, IconStyle, SchemeMode};
use glaze_design::{ColorScheme, ThemeKind, ThemeTokens};

use crate::catalog::Page;
use crate::store::{value_of, Change, Doc};
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::{App, IconTone};
use crate::{themeio, validate};

/// Original preset accents (name, hex).
pub const PRESETS: &[(&str, &str)] = &[
    ("Terracotta", "#d97757"),
    ("Kiln red", "#b5502c"),
    ("Ochre", "#c9a227"),
    ("Sage", "#6f8f72"),
    ("Teal", "#2f7f7a"),
    ("Slate", "#4f6d8f"),
    ("Plum", "#8a5a83"),
    ("Ink", "#3a3733"),
];

/// Scheme mode: `appearance.scheme_mode = "auto"` in config.toml (glaze-common
/// `SchemeMode`; the shell switches live at 07:00 / 19:00). A legacy
/// sidecar value from settings 0.2 counts too until it is migrated.
pub fn scheme_mode_auto(app: &App) -> bool {
    app.cfg().appearance.scheme_mode == SchemeMode::Auto
        || app
            .extra("appearance.scheme_mode")
            .and_then(|v| v.as_str().map(|s| s == "auto"))
            .unwrap_or(false)
}

fn theme_tile(app: &App, kind: ThemeKind, selected: bool) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("fs-tile");
    if selected {
        b.add_css_class("fs-selected");
    }
    let v = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let d = gtk::DrawingArea::new();
    d.set_content_width(150);
    d.set_content_height(84);
    let a = app.clone();
    d.set_draw_func(move |_, cr, w, h| {
        let scheme = a.tokens().scheme;
        let t = ThemeTokens::for_kind(kind, scheme);
        let p = &t.palette;
        let set = |c: glaze_design::Color| {
            cr.set_source_rgb(c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0)
        };
        let (w, h) = (w as f64, h as f64);
        set(p.canvas);
        cr.rectangle(0.0, 0.0, w, h);
        let _ = cr.fill();
        // window
        let r = t.metrics.window_radius as f64;
        set(p.surface_raised);
        rounded(cr, 14.0, 10.0, w - 40.0, h - 34.0, r);
        let _ = cr.fill_preserve();
        set(p.border);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        set(p.text_primary);
        cr.rectangle(22.0, 18.0, 46.0, 4.0);
        let _ = cr.fill();
        set(p.text_secondary);
        cr.rectangle(22.0, 28.0, 70.0, 3.0);
        let _ = cr.fill();
        set(p.accent);
        rounded(cr, 22.0, 38.0, 32.0, 12.0, t.metrics.control_radius as f64);
        let _ = cr.fill();
        // taskbar
        set(p.panel);
        cr.rectangle(0.0, h - 16.0, w, 16.0);
        let _ = cr.fill();
        set(p.border);
        cr.rectangle(0.0, h - 16.0, w, 1.0);
        let _ = cr.fill();
        set(p.accent);
        rounded(
            cr,
            4.0,
            h - 13.0,
            18.0,
            10.0,
            t.metrics.control_radius as f64 / 2.0,
        );
        let _ = cr.fill();
    });
    a_push(app, &d);
    let t = gtk::Label::new(Some(kind.label()));
    t.add_css_class("fs-tile-title");
    let s = gtk::Label::new(Some(match kind {
        ThemeKind::Clay => "Cut paper, ink, terracotta",
        ThemeKind::Classic => "Windows 2000 heritage",
    }));
    s.add_css_class("fs-tile-sub");
    v.append(&d);
    v.append(&t);
    v.append(&s);
    b.set_child(Some(&v));
    b.update_property(&[gtk::accessible::Property::Label(&format!(
        "{} theme",
        kind.label()
    ))]);
    b
}

fn a_push(app: &App, d: &gtk::DrawingArea) {
    app.previews.borrow_mut().push(d.downgrade());
}

pub fn rounded(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -pi / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, pi / 2.0);
    cr.arc(x + r, y + h - r, r, pi / 2.0, pi);
    cr.arc(x + r, y + r, r, pi, 1.5 * pi);
    cr.close_path();
}

fn rgba(hex: &str) -> gdk::RGBA {
    gdk::RGBA::parse(hex).unwrap_or(gdk::RGBA::BLACK)
}

fn hex_of(c: &gdk::RGBA) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8
    )
}

/// Applies an accent hex and syncs the swatches, picker and entry.
type AccentCommit = std::rc::Rc<dyn Fn(&App, &str)>;

pub fn build(app: &App) -> gtk::Widget {
    let cfg = app.cfg();
    let ap = &cfg.appearance;
    let tokens = app.tokens();
    let page = PageBuilder::new(app, Page::Appearance);

    // Theme -------------------------------------------------------------------
    let theme = Card::new("Theme", None);
    {
        let tiles = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let kinds = [ThemeKind::Clay, ThemeKind::Classic];
        let btns: Vec<gtk::Button> = kinds
            .iter()
            .map(|k| theme_tile(app, *k, ap.theme == *k))
            .collect();
        for b in &btns {
            tiles.append(b);
        }
        let r = widgets::row(
            app,
            "appearance.theme",
            Some("Clay is the Glaze look; Classic keeps the grey, bevelled Windows 2000 style."),
            &tiles,
        );
        for (i, b) in btns.iter().enumerate() {
            let a = app.clone();
            let all = btns.clone();
            let k = kinds[i];
            b.connect_clicked(move |_| {
                if a.set(
                    Doc::Main,
                    "appearance.theme",
                    value_of(&k),
                    &format!("Theme: {}", k.label()),
                )
                .is_ok()
                {
                    for (j, x) in all.iter().enumerate() {
                        if j == i {
                            x.add_css_class("fs-selected");
                        } else {
                            x.remove_css_class("fs-selected");
                        }
                    }
                }
            });
        }
        theme.add(&r.root);
    }
    {
        let auto = scheme_mode_auto(app);
        let sel = if auto {
            2
        } else if ap.scheme == ColorScheme::Dark {
            1
        } else {
            0
        };
        let r = widgets::choice(app, "appearance.scheme", &["Light", "Dark", "Automatic"], sel, Some("Automatic is light from 07:00 to 19:00 and dark otherwise. Other apps switch too; GTK and Qt apps that are already open may need a restart."), |a, i| {
            let mut ch = vec![Change::Remove(Doc::Extra, "appearance.scheme_mode".into())];
            match i {
                0 | 1 => {
                    let scheme = if i == 0 { ColorScheme::Light } else { ColorScheme::Dark };
                    ch.push(Change::Set(Doc::Main, "appearance.scheme".into(), value_of(&scheme)));
                    ch.push(Change::Remove(Doc::Main, "appearance.scheme_mode".into()));
                }
                _ => ch.push(Change::Set(Doc::Main, "appearance.scheme_mode".into(), "auto".into())),
            }
            a.apply(ch, &format!("Colour scheme: {}", ["Light", "Dark", "Automatic"][i]), false)
        });
        theme.add(&r.root);
        theme.add(
            &widgets::switch(
                app,
                "appearance.amoled",
                Doc::Main,
                "appearance.amoled",
                ap.amoled,
                Some("True-black backgrounds for OLED screens. Applies while the scheme is dark."),
            )
            .root,
        );
    }
    {
        let installed = themeio::installed();
        let mut labels = vec!["None (built-in tokens)".to_string()];
        labels.extend(installed.iter().map(|(n, _)| n.clone()));
        let cur = ap
            .theme_file
            .as_ref()
            .and_then(|f| {
                installed
                    .iter()
                    .position(|(n, p)| n == f || p.to_string_lossy() == *f)
            })
            .map(|i| i + 1)
            .unwrap_or(0);
        let names: Vec<String> = installed.iter().map(|(n, _)| n.clone()).collect();
        let (r, _) = widgets::dropdown(app, "appearance.theme_file", &labels, cur, Some("Shareable theme files from ~/.config/glaze/themes. Import and Export are in the header."), false, move |a, i| {
            if i == 0 {
                a.remove(Doc::Main, "appearance.theme_file", "Theme file: none")
            } else {
                a.set(Doc::Main, "appearance.theme_file", names[i - 1].clone().into(), &format!("Theme file: {}", names[i - 1]))
            }
        });
        theme.add(&r.root);
    }
    page.add(&theme.frame);

    // Accent ------------------------------------------------------------------
    let colour = Card::new("Colour", None);
    {
        let current = ap
            .accent
            .clone()
            .unwrap_or_else(|| tokens.palette.accent.to_rgb().to_hex());
        let bx = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let sw_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let mut swatches = Vec::new();
        for (name, hex) in PRESETS {
            let b = gtk::Button::new();
            b.add_css_class("fs-swatch");
            if current.eq_ignore_ascii_case(hex) {
                b.add_css_class("fs-selected");
            }
            let css = gtk::CssProvider::new();
            css.load_from_string(&format!(
                "button {{ background-color: {hex}; background-image: none; }}"
            ));
            #[allow(deprecated)]
            b.style_context()
                .add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 20);
            b.set_tooltip_text(Some(name));
            b.update_property(&[gtk::accessible::Property::Label(&format!("Accent {name}"))]);
            sw_row.append(&b);
            swatches.push((b, hex.to_string()));
        }
        let picker = gtk::ColorDialogButton::new(Some(
            gtk::ColorDialog::builder()
                .with_alpha(false)
                .title("Accent colour")
                .build(),
        ));
        picker.set_rgba(&rgba(&current));
        picker.update_property(&[gtk::accessible::Property::Label("Custom accent colour")]);
        sw_row.append(&picker);
        let hex_entry = gtk::Entry::new();
        hex_entry.set_text(&current);
        hex_entry.set_width_chars(8);
        hex_entry.set_max_width_chars(8);
        hex_entry.update_property(&[gtk::accessible::Property::Label("Accent colour hex value")]);
        sw_row.append(&hex_entry);
        let reset = widgets::reset_button(app, ap.accent.is_some(), |a| {
            let _ = a.remove(Doc::Main, "appearance.accent", "Accent: theme default");
            a.rebuild(Some("appearance.accent"));
        });
        sw_row.append(&reset);
        bx.append(&sw_row);
        let warn = gtk::Label::new(None);
        warn.add_css_class("fs-warn");
        warn.set_xalign(0.0);
        warn.set_wrap(true);
        bx.append(&warn);
        let r = widgets::row(
            app,
            "appearance.accent",
            Some(
                "Used for the Start button, the active-window underline, selections and switches.",
            ),
            &gtk::Box::new(gtk::Orientation::Horizontal, 0),
        );
        r.root.append(&bx);
        app.register("appearance.accent", &r.root, &swatches[0].0);
        let show_warn = {
            let warn = warn.clone();
            move |cfg: &GlazeConfig, hex: &str| {
                let w = validate::accent_warnings(cfg, hex);
                warn.set_text(&w.join("\n"));
                warn.set_visible(!w.is_empty());
            }
        };
        show_warn(&cfg, &current);
        let err = r.error.clone();
        let commit: AccentCommit = {
            let swatches = swatches.clone();
            let picker = picker.clone();
            let hex_entry = hex_entry.clone();
            let reset = reset.clone();
            let err = err.clone();
            std::rc::Rc::new(
                move |a: &App, input: &str| match validate::accent_hex(input) {
                    Ok(hex) => {
                        if a.set(
                            Doc::Main,
                            "appearance.accent",
                            hex.clone().into(),
                            &format!("Accent {hex}"),
                        )
                        .is_ok()
                        {
                            err.set_visible(false);
                            a.building.set(true);
                            picker.set_rgba(&rgba(&hex));
                            hex_entry.set_text(&hex);
                            a.building.set(false);
                            hex_entry.remove_css_class("error");
                            for (b, h) in &swatches {
                                if h.eq_ignore_ascii_case(&hex) {
                                    b.add_css_class("fs-selected");
                                } else {
                                    b.remove_css_class("fs-selected");
                                }
                            }
                            reset.set_visible(true);
                            show_warn(&a.cfg(), &hex);
                        }
                    }
                    Err(e) => {
                        hex_entry.add_css_class("error");
                        err.set_text(&e);
                        err.set_visible(true);
                    }
                },
            )
        };
        for (b, hex) in &swatches {
            let (a, c, h) = (app.clone(), commit.clone(), hex.clone());
            b.connect_clicked(move |_| c(&a, &h));
        }
        {
            let (a, c) = (app.clone(), commit.clone());
            picker.connect_rgba_notify(move |p| {
                if !a.building.get() {
                    c(&a, &hex_of(&p.rgba()));
                }
            });
        }
        {
            let (a, c) = (app.clone(), commit.clone());
            hex_entry.connect_activate(move |e| c(&a, &e.text()));
            let err2 = err.clone();
            hex_entry.connect_changed(move |e| match validate::accent_hex(&e.text()) {
                Ok(_) => {
                    e.remove_css_class("error");
                    err2.set_visible(false);
                }
                Err(m) => {
                    e.add_css_class("error");
                    err2.set_text(&m);
                    err2.set_visible(true);
                }
            });
        }
        colour.add(&r.root);
    }
    page.add(&colour.frame);

    // Shape ---------------------------------------------------------------------
    let shape = Card::new(
        "Shape",
        Some("Flat paper: hairline strokes and hard, faint offsets. No blur at any setting."),
    );
    {
        let v = ap.radius.unwrap_or(tokens.metrics.control_radius) as f64;
        let (r, _) = widgets::slider(
            app,
            "appearance.radius",
            0.0,
            16.0,
            1.0,
            v,
            None,
            |v| format!("{v:.0} px"),
            |a, v| {
                a.set(
                    Doc::Main,
                    "appearance.radius",
                    toml::Value::Integer(v as i64),
                    "Corner radius",
                )
            },
        );
        shape.add(&r.root);
        let v = ap.stroke_weight.unwrap_or(tokens.metrics.stroke_width);
        let (r, _) = widgets::slider(
            app,
            "appearance.stroke_weight",
            0.0,
            3.0,
            0.25,
            v,
            None,
            |v| format!("{v:.2} px"),
            |a, v| {
                a.set(
                    Doc::Main,
                    "appearance.stroke_weight",
                    toml::Value::Float(v),
                    "Stroke weight",
                )
            },
        );
        shape.add(&r.root);
        let v = ap.shadow_strength.unwrap_or(1.0);
        let (r, s) = widgets::slider(
            app,
            "appearance.shadow_strength",
            0.0,
            2.0,
            0.1,
            v,
            Some("0 is no shadow, 1 the theme default."),
            |v| {
                if v < 0.05 {
                    "None".into()
                } else {
                    format!("{v:.1}×")
                }
            },
            |a, v| {
                a.set(
                    Doc::Main,
                    "appearance.shadow_strength",
                    toml::Value::Float(v),
                    "Shadow strength",
                )
            },
        );
        s.add_mark(1.0, gtk::PositionType::Bottom, None);
        shape.add(&r.root);
    }
    page.add(&shape.frame);

    // Paper ---------------------------------------------------------------------
    let paper = Card::new("Paper", None);
    {
        let sets = ["none", "grain", "fibre", "laid"];
        let labels = ["None", "Grain", "Fibre", "Laid"];
        let sel = sets.iter().position(|s| *s == ap.texture).unwrap_or(1);
        let r = widgets::choice(
            app,
            "appearance.texture",
            &labels,
            sel,
            Some("Pre-rendered tiles; cheap on software rendering."),
            move |a, i| {
                a.set(
                    Doc::Main,
                    "appearance.texture",
                    sets[i].into(),
                    &format!("Paper texture: {}", labels[i]),
                )
            },
        );
        paper.add(&r.root);
        let (r, _) = widgets::slider(
            app,
            "appearance.texture_intensity",
            0.0,
            1.0,
            0.05,
            ap.texture_intensity,
            None,
            |v| format!("{:.0} %", v * 100.0),
            |a, v| {
                a.set(
                    Doc::Main,
                    "appearance.texture_intensity",
                    toml::Value::Float(v),
                    "Texture intensity",
                )
            },
        );
        paper.add(&r.root);
        paper.add(
            &widgets::switch(
                app,
                "appearance.reduced_effects",
                Doc::Main,
                "appearance.reduced_effects",
                ap.reduced_effects,
                Some("No textures and no shadows: plain flat fills."),
            )
            .root,
        );
    }
    page.add(&paper.frame);

    // Icons ---------------------------------------------------------------------
    let icons = Card::new("Icons", None);
    icons.add(
        &widgets::enum_choice(
            app,
            "appearance.icon_style",
            Doc::Main,
            "appearance.icon_style",
            &[
                (IconStyle::Ink, "Glaze ink"),
                (IconStyle::System, "System theme"),
            ],
            ap.icon_style,
            Some("Ink glyphs where Glaze has one; the system icon theme otherwise."),
        )
        .root,
    );
    page.add(&icons.frame);

    // Fonts ---------------------------------------------------------------------
    let fonts = Card::new("Fonts", None);
    {
        let mut families: Vec<String> = app
            .window
            .pango_context()
            .list_families()
            .iter()
            .map(|f| f.name().to_string())
            .filter(|n| {
                !n.starts_with("Noto Sans ") || n == "Noto Sans Mono" || n == "Noto Sans Display"
            })
            .filter(|n| !n.starts_with("Noto Serif ") || n == "Noto Serif Display")
            .collect();
        families.sort();
        families.dedup();
        for (id, key, display) in [
            ("fonts.ui", "ui_family", false),
            ("fonts.display", "display_family", true),
        ] {
            let path = format!("appearance.overrides.typography.{key}");
            let current = app
                .main_value(&path)
                .and_then(|v| v.as_str().map(validate::first_family));
            let theme_first = validate::first_family(if display {
                &tokens.typography.display_family
            } else {
                &tokens.typography.ui_family
            });
            let mut labels = vec![format!("Theme default ({theme_first})")];
            labels.extend(families.iter().cloned());
            let sel = current
                .as_ref()
                .and_then(|c| families.iter().position(|f| f == c))
                .map(|i| i + 1)
                .unwrap_or(0);
            let fam = families.clone();
            let kind = ap.theme;
            let (r, _) = widgets::dropdown(
                app,
                id,
                &labels,
                sel,
                Some(if display {
                    "Headings, the clock and the Start button."
                } else {
                    "Labels, menus and buttons."
                }),
                true,
                move |a, i| {
                    let path = format!("appearance.overrides.typography.{key}");
                    if i == 0 {
                        a.remove(Doc::Main, &path, "Font: theme default")
                    } else {
                        let f = validate::font_family(&fam[i - 1])?;
                        a.set(
                            Doc::Main,
                            &path,
                            validate::family_list(&f, kind, display).into(),
                            &format!("Font: {f}"),
                        )
                    }
                },
            );
            fonts.add(&r.root);
        }
        let (r, _) = widgets::slider(
            app,
            "fonts.size",
            0.8,
            1.6,
            0.05,
            ap.font_scale,
            None,
            |v| format!("{:.0} %", v * 100.0),
            |a, v| {
                a.set(
                    Doc::Main,
                    "appearance.font_scale",
                    toml::Value::Float(v),
                    "Text size",
                )
            },
        );
        fonts.add(&r.root);
        let (r, _) = widgets::slider(
            app,
            "appearance.scale",
            1.0,
            2.0,
            0.25,
            ap.scale,
            Some("Extra scaling for Glaze surfaces on top of the display scale."),
            |v| format!("{v:.2}×"),
            |a, v| {
                a.set(
                    Doc::Main,
                    "appearance.scale",
                    toml::Value::Float(v),
                    "Interface scale",
                )
            },
        );
        fonts.add(&r.root);
    }
    page.add(&fonts.frame);
    page.finish()
}

// ---- theme import / export --------------------------------------------------

pub fn import_dialog(app: &App) {
    let filter = gtk::FileFilter::new();
    filter.add_pattern("*.toml");
    filter.set_name(Some("Glaze theme files (*.toml)"));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let d = gtk::FileDialog::builder()
        .title("Import a Glaze theme")
        .modal(true)
        .filters(&filters)
        .build();
    let a = app.clone();
    d.open(Some(&app.window), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        import_path(&a, &path);
    });
}

pub fn import_path(app: &App, path: &std::path::Path) {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            app.toast_error(&format!("{}: {e}", path.display()));
            return;
        }
    };
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let c = themeio::check(&src, &stem);
    let go = {
        let a = app.clone();
        let path = path.to_path_buf();
        move || {
            let r = crate::cli::import_theme(&mut a.store.borrow_mut(), &path);
            match r {
                Ok((name, _, _)) => {
                    a.after_change(&format!("Imported theme \"{name}\""));
                    a.rebuild(None);
                }
                Err(e) => a.toast_error(&e),
            }
        }
    };
    if !c.parses {
        app.toast_error(&format!("{} is not a theme file", path.display()));
        return;
    }
    if c.issues.is_empty() {
        go();
        return;
    }
    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let l = gtk::Label::new(Some(&format!(
        "“{}” has {} problem(s). Keys with errors are ignored; the rest applies.",
        c.name,
        c.issues.len()
    )));
    l.set_wrap(true);
    l.set_xalign(0.0);
    body.append(&l);
    for i in c.issues.iter().take(8) {
        let x = gtk::Label::new(Some(&format!(
            "{}: {}",
            if i.path.is_empty() { "file" } else { &i.path },
            i.message
        )));
        x.add_css_class(if i.is_error() { "fs-error" } else { "fs-warn" });
        x.set_xalign(0.0);
        x.set_wrap(true);
        body.append(&x);
    }
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("Import anyway");
    ok.add_css_class("fs-primary");
    let w = widgets::dialog(app, "Import theme", &body, vec![cancel.clone(), ok.clone()]);
    let wc = w.clone();
    cancel.connect_clicked(move |_| wc.close());
    ok.connect_clicked(move |_| {
        w.close();
        go();
    });
}

pub fn export_dialog(app: &App) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let l = gtk::Label::new(Some(
        "Save the current look (theme, accent, shape, fonts) as a shareable theme file.",
    ));
    l.set_wrap(true);
    l.set_xalign(0.0);
    body.append(&l);
    let name = gtk::Entry::new();
    name.set_text("My Glaze theme");
    name.update_property(&[gtk::accessible::Property::Label("Theme name")]);
    body.append(&name);
    let err = gtk::Label::new(None);
    err.add_css_class("fs-error");
    err.set_xalign(0.0);
    err.set_visible(false);
    body.append(&err);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("Choose file…");
    ok.add_css_class("fs-primary");
    let w = widgets::dialog(app, "Export theme", &body, vec![cancel.clone(), ok.clone()]);
    name.grab_focus();
    let wc = w.clone();
    cancel.connect_clicked(move |_| wc.close());
    {
        let (err, ok2) = (err.clone(), ok.clone());
        name.connect_changed(move |e| match validate::theme_name(&e.text()) {
            Ok(_) => {
                err.set_visible(false);
                ok2.set_sensitive(true);
            }
            Err(m) => {
                err.set_text(&m);
                err.set_visible(true);
                ok2.set_sensitive(false);
            }
        });
    }
    let a = app.clone();
    let okc = ok.clone();
    let nm = name.clone();
    name.connect_activate(move |_| okc.emit_clicked());
    ok.connect_clicked(move |_| {
        let Ok(n) = validate::theme_name(&nm.text()) else {
            return;
        };
        w.close();
        let d = gtk::FileDialog::builder()
            .title("Export theme")
            .modal(true)
            .initial_name(format!("{}.toml", validate::theme_slug(&n)))
            .build();
        let a2 = a.clone();
        d.save(Some(&a.window), gio::Cancellable::NONE, move |res| {
            let Ok(file) = res else { return };
            let Some(path) = file.path() else { return };
            export_to(&a2, &path, &n);
        });
    });
}

pub fn export_to(app: &App, path: &std::path::Path, name: &str) {
    let cfg = app.cfg();
    let src = crate::cli::active_theme_src(&cfg);
    let text = themeio::export(&cfg, name, src.as_deref());
    match crate::store::atomic_write(path, &text) {
        Ok(()) => app.toast(&format!("Exported “{name}” to {}", path.display()), false),
        Err(e) => app.toast_error(&format!("{}: {e}", path.display())),
    }
}

#[allow(dead_code)]
fn _tone(_: IconTone) {}
