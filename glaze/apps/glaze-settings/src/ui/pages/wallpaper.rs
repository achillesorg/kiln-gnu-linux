//! Wallpaper: the bundled sets from `assets/wallpapers/manifest.toml` as a
//! thumbnail grid (light or dark preview), a plain-paper option, a custom
//! image, fit mode, variant and per-monitor choices.

use std::path::PathBuf;

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

use glaze_common::wallpaper::{self, Manifest, Orientation};
use glaze_design::ColorScheme;

use crate::catalog::Page;
use crate::store::{Change, Doc};
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::{App, IconTone};

const THUMB_W: i32 = 200;
const THUMB_H: i32 = 113;

fn load_thumb(app: &App, path: PathBuf, pic: &gtk::Picture) {
    let key = path.display().to_string();
    if let Some(t) = app.thumbs.borrow().get(&key) {
        pic.set_paintable(Some(t));
        return;
    }
    let a = app.clone();
    let pic = pic.clone();
    let p2 = path.clone();
    let handle = gio::spawn_blocking(move || {
        let pb = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&p2, THUMB_W * 2, THUMB_H * 2, true)
            .ok()?;
        Some((
            pb.read_pixel_bytes(),
            pb.width(),
            pb.height(),
            pb.rowstride(),
            pb.has_alpha(),
        ))
    });
    glib::spawn_future_local(async move {
        if let Ok(Some((bytes, w, h, stride, alpha))) = handle.await {
            let fmt = if alpha {
                gdk::MemoryFormat::R8g8b8a8
            } else {
                gdk::MemoryFormat::R8g8b8
            };
            let t: gdk::Texture =
                gdk::MemoryTexture::new(w, h, fmt, &bytes, stride as usize).upcast();
            a.thumbs.borrow_mut().insert(key, t.clone());
            pic.set_paintable(Some(&t));
        }
    });
}

fn manifest() -> Option<(PathBuf, Manifest)> {
    wallpaper::load_manifest(&wallpaper::search_dirs())
}

pub fn build(app: &App) -> gtk::Widget {
    let cfg = app.cfg();
    let page = PageBuilder::new(app, Page::Wallpaper);
    let scheme = app.tokens().scheme;
    let preview_scheme = std::rc::Rc::new(std::cell::Cell::new(scheme));

    let card = Card::new("Picture", Some("Original cut-paper artwork. Each set has light and dark versions; the one matching your colour scheme is used."));
    let Some((dir, man)) = manifest() else {
        card.add(&widgets::empty_state(app, "wallpaper", "No wallpapers installed", "assets/wallpapers/manifest.toml was not found in ~/.local/share/glaze/wallpapers or /usr/share/glaze/wallpapers."));
        app.register("desktop.wallpaper_set", &card.frame, &card.frame);
        page.add(&card.frame);
        return page.finish();
    };

    // Preview scheme toggle.
    let (seg, segb) = widgets::segmented(
        &["Light previews", "Dark previews"],
        if scheme == ColorScheme::Dark { 1 } else { 0 },
    );
    seg.set_halign(gtk::Align::End);
    seg.set_margin_end(16);
    seg.set_margin_bottom(4);
    card.add(&seg);

    let flow = gtk::FlowBox::new();
    flow.add_css_class("fs-tiles");
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(4);
    flow.set_min_children_per_line(2);
    flow.set_homogeneous(true);
    flow.set_column_spacing(4);
    flow.set_row_spacing(4);
    flow.set_margin_start(12);
    flow.set_margin_end(12);
    flow.set_margin_bottom(10);

    let current_set = if cfg.desktop.wallpaper.is_some() {
        "\u{0}custom".to_string()
    } else {
        cfg.desktop.wallpaper_set.clone()
    };
    let mut tiles: Vec<(gtk::Button, String, gtk::Picture, Option<String>)> = Vec::new();
    let mk_tile = |title: &str, sub: &str, id: &str| {
        let b = gtk::Button::new();
        b.add_css_class("fs-tile");
        if id == current_set {
            b.add_css_class("fs-selected");
        }
        let v = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let pic = gtk::Picture::new();
        pic.set_size_request(THUMB_W, THUMB_H);
        pic.set_content_fit(gtk::ContentFit::Cover);
        pic.add_css_class("fs-thumb");
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("fs-thumb-frame");
        frame.set_overflow(gtk::Overflow::Hidden);
        frame.append(&pic);
        v.append(&frame);
        let t = gtk::Label::new(Some(title));
        t.add_css_class("fs-tile-title");
        t.set_xalign(0.0);
        t.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let s = gtk::Label::new(Some(sub));
        s.add_css_class("fs-tile-sub");
        s.set_xalign(0.0);
        s.set_ellipsize(gtk::pango::EllipsizeMode::End);
        s.set_max_width_chars(28);
        v.append(&t);
        v.append(&s);
        b.set_child(Some(&v));
        b.set_tooltip_text(Some(sub));
        b.update_property(&[gtk::accessible::Property::Label(&format!(
            "Wallpaper {title}"
        ))]);
        (b, pic)
    };

    for set in &man.sets {
        let (b, pic) = mk_tile(&set.title, &set.description, &set.id);
        tiles.push((b, set.id.clone(), pic, None));
    }
    let (plain, plain_pic) = mk_tile("Plain paper", "The theme canvas colour, no picture", "");
    tiles.push((plain, String::new(), plain_pic, None));
    let custom_sub = cfg
        .desktop
        .wallpaper
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "Choose an image file…".into());
    let (custom, custom_pic) = mk_tile("Custom image", &custom_sub, "\u{0}custom");
    tiles.push((
        custom,
        "\u{0}custom".into(),
        custom_pic,
        cfg.desktop
            .wallpaper
            .as_ref()
            .map(|p| p.display().to_string()),
    ));

    let load_all = {
        let a = app.clone();
        let tiles: Vec<(String, gtk::Picture, Option<String>)> = tiles
            .iter()
            .map(|(_, id, p, c)| (id.clone(), p.clone(), c.clone()))
            .collect();
        let man = man.clone();
        let dir = dir.clone();
        move |sc: ColorScheme| {
            for (id, pic, custom) in &tiles {
                if id.is_empty() {
                    // Plain paper: a flat canvas swatch.
                    let t = a.tokens();
                    let c = if sc == t.scheme {
                        t.palette.canvas
                    } else {
                        glaze_design::ThemeTokens::for_kind(t.kind, sc)
                            .palette
                            .canvas
                    };
                    let snap = gtk::Snapshot::new();
                    snap.append_color(
                        &gdk::RGBA::new(
                            c.r as f32 / 255.0,
                            c.g as f32 / 255.0,
                            c.b as f32 / 255.0,
                            1.0,
                        ),
                        &gtk::graphene::Rect::new(0.0, 0.0, THUMB_W as f32, THUMB_H as f32),
                    );
                    if let Some(p) = snap.to_paintable(None) {
                        pic.set_paintable(Some(&p));
                    }
                } else if let Some(c) = custom {
                    load_thumb(&a, PathBuf::from(c), pic);
                } else if id != "\u{0}custom" {
                    if let Some(v) = man.set(id).and_then(|s| s.pick(sc, Orientation::Landscape)) {
                        load_thumb(&a, dir.join(&v.file), pic);
                    }
                }
            }
        }
    };
    load_all(preview_scheme.get());
    for (i, b) in segb.iter().enumerate() {
        let l = load_all.clone();
        let ps = preview_scheme.clone();
        b.connect_toggled(move |t| {
            if t.is_active() {
                let sc = if i == 0 {
                    ColorScheme::Light
                } else {
                    ColorScheme::Dark
                };
                ps.set(sc);
                l(sc);
            }
        });
    }

    let buttons: Vec<gtk::Button> = tiles.iter().map(|(b, ..)| b.clone()).collect();
    for (b, id, _, _) in &tiles {
        flow.insert(b, -1);
        let a = app.clone();
        let id = id.clone();
        let all = buttons.clone();
        let me = b.clone();
        let titles: Vec<(String, String)> = man
            .sets
            .iter()
            .map(|s| (s.id.clone(), s.title.clone()))
            .collect();
        b.connect_clicked(move |_| {
            let select = |a: &App| {
                for x in &all {
                    if x == &me {
                        x.add_css_class("fs-selected");
                    } else {
                        x.remove_css_class("fs-selected");
                    }
                }
                let _ = a;
            };
            if id == "\u{0}custom" {
                choose_custom(&a);
                return;
            }
            let title = titles
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, t)| t.clone())
                .unwrap_or_else(|| "Plain paper".into());
            let ch = vec![
                Change::Set(Doc::Main, "desktop.wallpaper_set".into(), id.clone().into()),
                Change::Remove(Doc::Main, "desktop.wallpaper".into()),
            ];
            if a.apply(ch, &format!("Wallpaper: {title}"), false).is_ok() {
                select(&a);
            }
        });
    }
    let r = widgets::row(
        app,
        "desktop.wallpaper_set",
        None,
        &gtk::Box::new(gtk::Orientation::Horizontal, 0),
    );
    r.root.append(&flow);
    let focus = buttons
        .iter()
        .find(|b| b.has_css_class("fs-selected"))
        .cloned()
        .unwrap_or_else(|| buttons[0].clone());
    app.register("desktop.wallpaper_set", &r.root, &focus);
    card.add(&r.root);
    // Custom image row (search target).
    let browse = gtk::Button::with_label("Browse…");
    let cr = widgets::row(app, "desktop.wallpaper", Some(&custom_sub), &browse);
    let a = app.clone();
    browse.connect_clicked(move |_| choose_custom(&a));
    card.add(&cr.root);
    page.add(&card.frame);

    // Options ---------------------------------------------------------------
    let opts = Card::new("Options", None);
    {
        let variant = app
            .extra("wallpaper.variant")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "follow".into());
        let vals = ["follow", "light", "dark"];
        let sel = vals.iter().position(|v| *v == variant).unwrap_or(0);
        let r = widgets::choice(
            app,
            "wallpaper.variant",
            &["Follow scheme", "Always light", "Always dark"],
            sel,
            None,
            move |a, i| {
                if i == 0 {
                    a.remove(
                        Doc::Extra,
                        "wallpaper.variant",
                        "Wallpaper variant: follow scheme",
                    )
                } else {
                    a.set(
                        Doc::Extra,
                        "wallpaper.variant",
                        vals[i].into(),
                        "Wallpaper variant",
                    )
                }
            },
        );
        opts.add(&r.root);
        let fit = app
            .extra("wallpaper.fit")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "fill".into());
        let fits = ["fill", "fit", "center", "tile", "stretch"];
        let sel = fits.iter().position(|v| *v == fit).unwrap_or(0);
        let r = widgets::choice(
            app,
            "wallpaper.fit",
            &["Fill", "Fit", "Centre", "Tile", "Stretch"],
            sel,
            Some("The shell fills the screen today."),
            move |a, i| a.set(Doc::Extra, "wallpaper.fit", fits[i].into(), "Wallpaper fit"),
        );
        opts.add(&r.root);
        opts.add(
            &widgets::switch(
                app,
                "desktop.show_icons",
                Doc::Main,
                "desktop.show_icons",
                cfg.desktop.show_icons,
                Some("Shortcut icons on the desktop."),
            )
            .root,
        );
    }
    page.add(&opts.frame);

    // Monitors --------------------------------------------------------------------
    let mon = Card::new("Monitors", Some("Pick a different set per output."));
    let outs = super::taskbar::monitor_names(app);
    let mut labels = vec!["Same as above".to_string()];
    labels.extend(man.sets.iter().map(|s| s.title.clone()));
    let ids: Vec<String> = man.sets.iter().map(|s| s.id.clone()).collect();
    for (i, o) in outs.iter().enumerate() {
        let path = format!("wallpaper.monitors.{o}");
        let cur = app
            .extra(&path)
            .and_then(|v| v.as_str().map(str::to_string));
        let sel = cur
            .and_then(|c| ids.iter().position(|x| *x == c))
            .map(|x| x + 1)
            .unwrap_or(0);
        let ids2 = ids.clone();
        let o2 = o.clone();
        let (r, dd) = widgets::dropdown(
            app,
            "wallpaper.monitors",
            &labels,
            sel,
            Some(&format!("Output {o}")),
            false,
            move |a, i| {
                let p = format!("wallpaper.monitors.{o2}");
                if i == 0 {
                    a.remove(Doc::Extra, &p, &format!("Wallpaper on {o2}: same"))
                } else {
                    a.set(
                        Doc::Extra,
                        &p,
                        ids2[i - 1].clone().into(),
                        &format!("Wallpaper on {o2}"),
                    )
                }
            },
        );
        if i == 0 {
            app.register("wallpaper.monitors", &r.root, &dd);
        }
        mon.add(&r.root);
    }
    if outs.is_empty() {
        app.register("wallpaper.monitors", &mon.frame, &mon.frame);
    }
    page.add(&mon.frame);
    page.finish()
}

fn choose_custom(app: &App) {
    let filter = gtk::FileFilter::new();
    filter.add_mime_type("image/png");
    filter.add_mime_type("image/jpeg");
    filter.add_mime_type("image/svg+xml");
    filter.set_name(Some("Images"));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let d = gtk::FileDialog::builder()
        .title("Choose a wallpaper")
        .modal(true)
        .filters(&filters)
        .build();
    let a = app.clone();
    d.open(Some(&app.window), gio::Cancellable::NONE, move |res| {
        let Ok(f) = res else { return };
        let Some(p) = f.path() else { return };
        if !p.is_file() {
            a.toast_error("that file does not exist");
            return;
        }
        if a.set(
            Doc::Main,
            "desktop.wallpaper",
            p.display().to_string().into(),
            "Wallpaper: custom image",
        )
        .is_ok()
        {
            a.rebuild(Some("desktop.wallpaper"));
        }
    });
}

#[allow(dead_code)]
fn _t(_: IconTone) {}
