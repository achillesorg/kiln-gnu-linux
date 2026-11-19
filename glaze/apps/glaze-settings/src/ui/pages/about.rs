//! About: distro logo (Kiln only on Kiln), system and Glaze versions, hardware summary,
//! licences and the configuration files (with "reset all").

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use glaze_common::brand::{Brand, Logo};
use glaze_design::ColorScheme;

use crate::catalog::{Page, STATIC};
use crate::store::{Change, Doc};
use crate::sysinfo;
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::App;

fn set_symbol(pic: &gtk::Picture, dark: bool) {
    let svg = if dark {
        crate::assets::GLAZE_SYMBOL_ON_DARK
    } else {
        crate::assets::GLAZE_SYMBOL
    };
    let sized = svg.replacen(
        "width=\"24\" height=\"24\"",
        "width=\"192\" height=\"192\"",
        1,
    );
    if let Ok(t) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(sized.into_bytes())) {
        pic.set_paintable(Some(&t));
    }
}

/// Save a `[branding]` key, then rebuild the page so the hero follows.
fn set_branding(a: &App, key: &str, value: &str, label: &str) -> Result<(), String> {
    let v = match value.trim() {
        "" => "auto",
        v => v,
    };
    a.apply(
        vec![Change::Set(Doc::Main, key.into(), v.into())],
        label,
        false,
    )?;
    let a2 = a.clone();
    glib::idle_add_local_once(move || a2.rebuild(None));
    Ok(())
}

pub fn build(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::About);

    // Logo ---------------------------------------------------------------------
    // Kiln artwork only on Kiln (or when chosen); otherwise the neutral Glaze
    // symbol with the distro name as text.
    let cfg = app.cfg();
    let brand = Brand::detect(&cfg.branding);
    let dark = app.tokens().scheme == ColorScheme::Dark;
    let hero = gtk::Box::new(gtk::Orientation::Vertical, 10);
    hero.add_css_class("fs-card");
    let pic = gtk::Picture::new();
    let mut show_name = true;
    match &brand.logo {
        Logo::Kiln => {
            let svg = if dark {
                crate::assets::KILN_LOCKUP_FULL_ON_DARK
            } else {
                crate::assets::KILN_LOCKUP_FULL
            };
            let sized = svg.replacen(
                "width=\"1207\" height=\"256\"",
                "width=\"905\" height=\"192\"",
                1,
            );
            if let Ok(t) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(sized.into_bytes())) {
                pic.set_paintable(Some(&t));
            }
            show_name = false;
        }
        Logo::Glaze => set_symbol(&pic, dark),
        Logo::File(p) => match gdk::Texture::from_filename(p) {
            Ok(t) => pic.set_paintable(Some(&t)),
            Err(_) => set_symbol(&pic, dark),
        },
    }
    pic.set_can_shrink(true);
    pic.set_content_fit(gtk::ContentFit::Contain);
    pic.set_size_request(-1, 96);
    pic.set_margin_top(26);
    pic.set_margin_start(40);
    pic.set_margin_end(40);
    pic.set_alternative_text(Some(&brand.name));
    hero.append(&pic);
    if show_name || brand.name != "Kiln GNU/Linux" {
        let name = gtk::Label::new(Some(&brand.name));
        name.add_css_class("fs-card-title");
        name.set_wrap(true);
        name.set_justify(gtk::Justification::Center);
        hero.append(&name);
    }
    let tag = gtk::Label::new(Some(&format!("Glaze desktop {}", glaze_common::VERSION)));
    tag.add_css_class("fs-row-sub");
    tag.set_margin_bottom(20);
    hero.append(&tag);
    page.add(&hero);

    // Branding override -----------------------------------------------------------
    let brd = Card::new(
        "Branding",
        Some(
            "Auto follows /etc/os-release: Kiln artwork on Kiln, the neutral Glaze mark elsewhere.",
        ),
    );
    let (r, _) = widgets::entry(
        app,
        "branding.logo",
        &cfg.branding.logo,
        "auto",
        Some("auto, kiln, glaze, or the path of an image file."),
        |t| {
            let t = t.trim();
            let ok = t.is_empty()
                || ["auto", "kiln", "glaze"].contains(&t.to_ascii_lowercase().as_str())
                || std::path::Path::new(t).is_file();
            if ok {
                Ok(())
            } else {
                Err("Use auto, kiln, glaze or the path of an existing image.".into())
            }
        },
        |a, t| set_branding(a, "branding.logo", t, "Logo"),
    );
    app.register("branding.logo", &r.root, &r.root);
    brd.add(&r.root);
    let (r, _) = widgets::entry(
        app,
        "branding.name",
        &cfg.branding.name,
        "auto",
        Some("auto uses the name from os-release."),
        |_| Ok(()),
        |a, t| set_branding(a, "branding.name", t, "System name"),
    );
    app.register("branding.name", &r.root, &r.root);
    brd.add(&r.root);
    page.add(&brd.frame);

    // System ------------------------------------------------------------------
    let info = sysinfo::probe_about();
    let sys = Card::new("This computer", None);
    let os = if info.os_version.is_empty() {
        info.os_name.clone()
    } else {
        format!("{} (version {})", info.os_name, info.os_version)
    };
    let first = widgets::info_row("Operating system", &os);
    app.register("about.system", &first, &first);
    sys.add(&first);
    sys.add(&widgets::info_row(
        "Kernel",
        &format!("Linux {} ({})", info.kernel, info.arch),
    ));
    sys.add(&widgets::info_row(
        "Processor",
        &format!(
            "{} · {} cores",
            if info.cpu.is_empty() {
                "Unknown"
            } else {
                &info.cpu
            },
            info.cores
        ),
    ));
    sys.add(&widgets::info_row(
        "Memory",
        &sysinfo::human_kib(info.memory_kib),
    ));
    if !info.hostname.is_empty() {
        sys.add(&widgets::info_row("Computer name", &info.hostname));
    }
    sys.add(&widgets::info_row(
        "Other apps",
        &match crate::colourscheme::last_applied() {
            Some(s) => format!(
                "GTK and portal apps: {} (glaze-colour-scheme)",
                crate::colourscheme::arg(s)
            ),
            None => "GTK and portal apps: not switched by Glaze yet".into(),
        },
    ));
    sys.add(&widgets::info_row(
        "Qt apps",
        &crate::colourscheme::qt_status(),
    ));
    sys.add(&widgets::info_row(
        "Display server",
        &sysinfo::compositor_name(),
    ));
    let renderer = app
        .window
        .native()
        .and_then(|n| n.renderer())
        .map(|r| r.type_().name().trim_start_matches("Gsk").to_string())
        .unwrap_or_else(|| "GTK".into());
    sys.add(&widgets::info_row(
        "Toolkit",
        &format!(
            "GTK {}.{}.{} ({renderer})",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ),
    ));
    page.add(&sys.frame);

    let glaze = Card::new("Glaze", None);
    let r = widgets::info_row(
        "Glaze",
        &format!(
            "{} (glaze-common), design system {}",
            glaze_common::VERSION,
            glaze_design::VERSION
        ),
    );
    app.register("about.glaze", &r, &r);
    glaze.add(&r);
    glaze.add(&widgets::info_row(
        "Settings app",
        &format!("glaze-settings {}", env!("CARGO_PKG_VERSION")),
    ));
    page.add(&glaze.frame);

    let lic = Card::new("Licences", None);
    let r = widgets::info_row("Glaze", "MIT OR Apache-2.0");
    app.register("about.licences", &r, &r);
    lic.add(&r);
    lic.add(&widgets::info_row("Artwork", "Wallpapers, ink icons, paper textures, the Glaze mark and the Kiln logo are original Glaze artwork under the same licence."));
    lic.add(&widgets::info_row(
        "Fonts",
        "System fonts (Noto: SIL Open Font Licence 1.1)",
    ));
    lic.add(&widgets::info_row(
        "Base system",
        &format!(
            "{}; each package keeps its own licence (/usr/share/doc/*/copyright).",
            if brand.is_kiln {
                "Debian 13 (trixie)"
            } else {
                info.os_name.as_str()
            }
        ),
    ));
    page.add(&lic.frame);

    // Configuration ------------------------------------------------------------
    let conf = Card::new("Configuration", Some("Settings are saved as soon as they change. A backup is made before the first change of a session and before every reset."));
    let (mp, xp, bp) = {
        let s = app.store.borrow();
        (
            s.main_path().display().to_string(),
            s.extra_path().display().to_string(),
            s.backup_dir().display().to_string(),
        )
    };
    let r = widgets::info_row("Shared config", &mp);
    app.register("about.files", &r, &r);
    conf.add(&r);
    conf.add(&widgets::info_row("Settings-only keys", &xp));
    conf.add(&widgets::info_row("Backups", &bp));
    let reset = gtk::Button::with_label("Reset all settings…");
    reset.add_css_class("fs-danger");
    let rr = widgets::row(
        app,
        "about.reset_all",
        Some("Every page back to defaults. Backups are kept and Undo works."),
        &reset,
    );
    let a = app.clone();
    reset.connect_clicked(move |_| {
        let a2 = a.clone();
        widgets::confirm(
            &a,
            "Reset all settings?",
            "Taskbar, Start menu (including pinned apps), appearance, wallpaper, animation, windows and default-app choices go back to their defaults. A backup of both files is kept.",
            "Reset everything",
            true,
            move || {
                let mut ch: Vec<Change> = Vec::new();
                for p in Page::ALL {
                    ch.extend(p.reset_changes());
                }
                ch.push(Change::Remove(Doc::Main, "appearance".into()));
                ch.push(Change::Remove(Doc::Main, "branding".into()));
                if a2.apply(ch, "Reset all settings", true).is_ok() {
                    a2.rebuild(None);
                }
            },
        );
    });
    conf.add(&rr.root);
    page.add(&conf.frame);
    let _ = STATIC.len();
    page.finish()
}
