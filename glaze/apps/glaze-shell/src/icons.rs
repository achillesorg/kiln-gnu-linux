//! Ink icon loader for agent C's stroke icons (`assets/icons/ink/<variant>/`).
//!
//! GTK's symbolic recolouring fills every shape and breaks stroke outlines,
//! so the SVG source is loaded as a normal image with `currentColor`
//! substituted by the token colour, rasterised by gdk-pixbuf (librsvg) at the
//! exact pixel size. Results are cached per (name, size, colour, accent). If
//! the icon set is not installed, the cairo glyphs in [`crate::ink`] are used.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use glaze_common::brand::{Brand, Logo, OsRelease};
use glaze_common::color::Color;
use glaze_common::config::BrandingConfig;
use gtk4::prelude::*;
use gtk4::{gdk, gdk_pixbuf};

use crate::ink::{self, Glyph};

/// Variant used by the taskbar (mockup hairline weight).
pub const TASKBAR_VARIANT: &str = "stroke-1.25";

thread_local! {
    static CACHE: RefCell<HashMap<(String, i32, String, String), gdk::Texture>> =
        RefCell::new(HashMap::new());
    static DIRS: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

fn icon_dirs() -> Vec<PathBuf> {
    DIRS.with(|d| {
        d.borrow_mut()
            .get_or_insert_with(|| {
                let mut v = Vec::new();
                if let Some(p) = std::env::var_os("GLAZE_ICON_DIR") {
                    v.push(PathBuf::from(p));
                }
                v.push(glaze_common::paths::data_home().join("glaze/assets/icons/ink"));
                for d in glaze_common::paths::data_dirs() {
                    v.push(d.join("glaze/assets/icons/ink"));
                }
                if let Ok(exe) = std::env::current_exe() {
                    for up in exe.ancestors().skip(1).take(6) {
                        v.push(up.join("assets/icons/ink"));
                    }
                }
                v
            })
            .clone()
    })
}

/// Read an icon's SVG source.
fn source(name: &str, variant: &str) -> Option<String> {
    icon_dirs()
        .into_iter()
        .map(|d| d.join(variant).join(format!("{name}.svg")))
        .find_map(|p| std::fs::read_to_string(p).ok())
}

/// Render `svg` to a texture at `size` px.
pub fn render_svg(svg: &str, size: i32) -> Option<gdk::Texture> {
    render_svg_size(svg, size, size)
}

/// Render `svg` to a texture of `w` x `h` px.
pub fn render_svg_size(svg: &str, w: i32, h: i32) -> Option<gdk::Texture> {
    let loader = gdk_pixbuf::PixbufLoader::with_type("svg").ok()?;
    loader.set_size(w, h);
    loader.write(svg.as_bytes()).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}

/// A texture for ink icon `name`, coloured `color`, with the `glaze-badge`
/// shapes (bell dot) in `accent`.
pub fn texture(name: &str, size: i32, color: Color, accent: Option<Color>) -> Option<gdk::Texture> {
    let hex = color.to_rgb().to_hex();
    let acc = accent.map(|c| c.to_rgb().to_hex()).unwrap_or_default();
    let key = (name.to_string(), size, hex.clone(), acc.clone());
    if let Some(t) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Some(t);
    }
    let mut svg = source(name, TASKBAR_VARIANT)?;
    if !acc.is_empty() {
        svg = svg.replace(
            "class=\"glaze-badge\" cx",
            &format!("class=\"glaze-badge\" style=\"fill:{acc}\" cx"),
        );
        svg = svg.replace(
            "class=\"glaze-badge\" fill=\"currentColor\"",
            &format!("class=\"glaze-badge\" fill=\"{acc}\""),
        );
    }
    svg = svg.replace("currentColor", &hex);
    let tex = render_svg(&svg, size)?;
    CACHE.with(|c| c.borrow_mut().insert(key, tex.clone()));
    Some(tex)
}

/// An icon widget: the ink SVG when installed, else the cairo glyph.
pub fn icon(name: &str, fallback: Glyph, size: i32, color: Color, stroke: f64) -> gtk4::Widget {
    match texture(name, size, color, None) {
        Some(tex) => picture(&tex, size),
        None => ink::glyph(fallback, size, stroke).upcast(),
    }
}

/// An icon with an accent badge (bell dot).
pub fn icon_with_accent(
    name: &str,
    fallback: Glyph,
    size: i32,
    color: Color,
    accent: Color,
    stroke: f64,
) -> gtk4::Widget {
    match texture(name, size, color, Some(accent)) {
        Some(tex) => picture(&tex, size),
        None => ink::glyph(fallback, size, stroke).upcast(),
    }
}

fn picture(tex: &gdk::Texture, size: i32) -> gtk4::Widget {
    let img = gtk4::Image::from_paintable(Some(tex));
    img.set_pixel_size(size);
    img.set_valign(gtk4::Align::Center);
    img.set_halign(gtk4::Align::Center);
    img.add_css_class("glaze-glyph");
    img.upcast()
}

/// The Kiln brand symbol (small cut) for the start menu header.
pub fn kiln_symbol(size: i32, dark: bool) -> Option<gtk4::Widget> {
    let svg = if dark {
        include_str!("../resources/kiln-symbol-small-on-dark.svg")
    } else {
        include_str!("../resources/kiln-symbol-small.svg")
    };
    render_svg(svg, size).map(|t| picture(&t, size))
}

/// The neutral Glaze symbol (full colour) for distros other than Kiln.
pub fn glaze_symbol(size: i32, dark: bool) -> Option<gtk4::Widget> {
    let svg = if dark {
        include_str!("../resources/glaze-symbol-on-dark.svg")
    } else {
        include_str!("../resources/glaze-symbol.svg")
    };
    render_svg(svg, size).map(|t| picture(&t, size))
}

thread_local! {
    static OS: OsRelease = OsRelease::load();
}

/// Branding for this machine from os-release plus `[branding]`.
pub fn brand(cfg: &BrandingConfig) -> Brand {
    OS.with(|os| Brand::resolve(os, cfg))
}

/// A user-chosen image file (`[branding] logo = <path>`), aspect preserved.
fn file_logo(path: &std::path::Path, size: i32) -> Option<gtk4::Widget> {
    let pb = gdk_pixbuf::Pixbuf::from_file_at_scale(path, size, size, true).ok()?;
    Some(picture(&gdk::Texture::for_pixbuf(&pb), size))
}

/// Full-colour brand symbol for headers (start menu): Kiln symbol, neutral
/// Glaze symbol, or the configured file. Falls back to the Glaze symbol when
/// a file cannot be read.
pub fn brand_symbol(brand: &Brand, size: i32, dark: bool) -> Option<gtk4::Widget> {
    match &brand.logo {
        Logo::Kiln => kiln_symbol(size, dark),
        Logo::Glaze => glaze_symbol(size, dark),
        Logo::File(p) => file_logo(p, size).or_else(|| glaze_symbol(size, dark)),
    }
}

/// Icon-theme icon named by os-release `LOGO=`, when the theme has it.
fn theme_icon(name: &str, size: i32) -> Option<gtk4::Widget> {
    let theme = gtk4::IconTheme::for_display(&gdk::Display::default()?);
    if !theme.has_icon(name) {
        return None;
    }
    let p = theme.lookup_icon(
        name,
        &[],
        size,
        1,
        gtk4::TextDirection::None,
        gtk4::IconLookupFlags::empty(),
    );
    let img = gtk4::Image::from_paintable(Some(&p));
    img.set_pixel_size(size);
    img.set_valign(gtk4::Align::Center);
    img.set_halign(gtk4::Align::Center);
    img.add_css_class("glaze-glyph");
    Some(img.upcast())
}

/// Monochrome Start-button icon. Kiln keeps the ink "start" glyph; other
/// distros get the os-release `LOGO` theme icon when installed, else the
/// neutral Glaze mark. An explicit `[branding] logo` file or `glaze` wins.
pub fn start_icon(brand: &Brand, size: i32, color: Color, stroke: f64) -> gtk4::Widget {
    let dots = || icons_start(size, color, stroke);
    match &brand.logo {
        Logo::Kiln => dots(),
        Logo::File(p) => file_logo(p, size).unwrap_or_else(dots),
        Logo::Glaze => {
            if let Some(w) = brand.os_icon.as_deref().and_then(|n| theme_icon(n, size)) {
                return w;
            }
            let svg = include_str!("../resources/glaze-mark.svg")
                .replace("currentColor", &color.to_rgb().to_hex());
            match render_svg(&svg, size) {
                Some(t) => picture(&t, size),
                None => dots(),
            }
        }
    }
}

fn icons_start(size: i32, color: Color, stroke: f64) -> gtk4::Widget {
    icon("start", Glyph::StartDots, size, color, stroke)
}

/// Ink icon name for an app, when the ink set has one.
pub fn app_icon_name(app_id: &str, categories: &[String]) -> Option<&'static str> {
    match ink::app_glyph(app_id, categories)? {
        Glyph::Terminal => Some("terminal"),
        Glyph::Folder => Some("folder"),
        Glyph::Globe => Some("browser"),
        Glyph::Settings => Some("settings"),
        _ => None,
    }
}

/// Glyph fallback paired with an ink icon name.
pub fn fallback_for(name: &str) -> Glyph {
    match name {
        "terminal" => Glyph::Terminal,
        "folder" => Glyph::Folder,
        "browser" => Glyph::Globe,
        "settings" => Glyph::Settings,
        "layout-float" => Glyph::Float,
        "layout-tile" => Glyph::Tile,
        "network-wifi" | "network-wired" | "network-off" => Glyph::Wifi,
        "volume-high" | "volume-medium" | "volume-low" | "volume-mute" => Glyph::Volume,
        "bell" | "bell-dot" => Glyph::Bell,
        "window-close" => Glyph::Close,
        "pin" | "unpin" => Glyph::Pin,
        "power" => Glyph::Power,
        "lock" => Glyph::Lock,
        "sleep" => Glyph::Moon,
        "restart" => Glyph::Restart,
        "logout" => Glyph::Logout,
        "search" => Glyph::Search,
        "start" => Glyph::StartDots,
        _ => Glyph::Window,
    }
}
