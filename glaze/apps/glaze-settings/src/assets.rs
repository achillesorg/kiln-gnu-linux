//! Embedded artwork: ink glyphs (Glaze ink set plus settings-only glyphs in
//! the same 24 px / 1.5 px style), the Kiln logo and paper texture tiles.
//! Embedding keeps the binary self-contained when it is copied to
//! `~/.local/bin`.
//!
//! Glyphs are plain SVG with `currentColor`; [`colored`] substitutes a token
//! colour before the SVG is rasterised (GTK symbolic recolouring breaks the
//! ink strokes).

/// (name, svg) for every glyph.
pub const ICONS: &[(&str, &str)] = &[
    (
        "appearance",
        include_str!("../resources/icons/appearance.svg"),
    ),
    (
        "wallpaper",
        include_str!("../resources/icons/wallpaper.svg"),
    ),
    (
        "animation",
        include_str!("../resources/icons/animation.svg"),
    ),
    ("taskbar", include_str!("../resources/icons/taskbar.svg")),
    ("display", include_str!("../resources/icons/display.svg")),
    ("about", include_str!("../resources/icons/about.svg")),
    ("keyboard", include_str!("../resources/icons/keyboard.svg")),
    ("undo", include_str!("../resources/icons/undo.svg")),
    ("import", include_str!("../resources/icons/import.svg")),
    ("export", include_str!("../resources/icons/export.svg")),
    ("reset", include_str!("../resources/icons/reset.svg")),
    ("warning", include_str!("../resources/icons/warning.svg")),
    ("grip", include_str!("../resources/icons/grip.svg")),
    ("plus", include_str!("../resources/icons/plus.svg")),
    ("check", include_str!("../resources/icons/check.svg")),
    ("close", include_str!("../resources/icons/close.svg")),
    ("arrow-up", include_str!("../resources/icons/arrow-up.svg")),
    (
        "arrow-down",
        include_str!("../resources/icons/arrow-down.svg"),
    ),
    ("sun", include_str!("../resources/icons/sun.svg")),
    ("moon", include_str!("../resources/icons/moon.svg")),
    (
        "start",
        include_str!("../../../assets/icons/ink/stroke-1.5/start.svg"),
    ),
    (
        "layout-tile",
        include_str!("../../../assets/icons/ink/stroke-1.5/layout-tile.svg"),
    ),
    (
        "layout-float",
        include_str!("../../../assets/icons/ink/stroke-1.5/layout-float.svg"),
    ),
    (
        "volume-high",
        include_str!("../../../assets/icons/ink/stroke-1.5/volume-high.svg"),
    ),
    (
        "volume-mute",
        include_str!("../../../assets/icons/ink/stroke-1.5/volume-mute.svg"),
    ),
    (
        "network-wired",
        include_str!("../../../assets/icons/ink/stroke-1.5/network-wired.svg"),
    ),
    (
        "network-wifi",
        include_str!("../../../assets/icons/ink/stroke-1.5/network-wifi.svg"),
    ),
    (
        "network-off",
        include_str!("../../../assets/icons/ink/stroke-1.5/network-off.svg"),
    ),
    (
        "power",
        include_str!("../../../assets/icons/ink/stroke-1.5/power.svg"),
    ),
    (
        "apps",
        include_str!("../../../assets/icons/ink/stroke-1.5/apps.svg"),
    ),
    (
        "search",
        include_str!("../../../assets/icons/ink/stroke-1.5/search.svg"),
    ),
    (
        "settings",
        include_str!("../../../assets/icons/ink/stroke-1.5/settings.svg"),
    ),
    (
        "battery-full",
        include_str!("../../../assets/icons/ink/stroke-1.5/battery-full.svg"),
    ),
    (
        "battery-charging",
        include_str!("../../../assets/icons/ink/stroke-1.5/battery-charging.svg"),
    ),
    (
        "battery-empty",
        include_str!("../../../assets/icons/ink/stroke-1.5/battery-empty.svg"),
    ),
    (
        "folder",
        include_str!("../../../assets/icons/ink/stroke-1.5/folder.svg"),
    ),
    (
        "terminal",
        include_str!("../../../assets/icons/ink/stroke-1.5/terminal.svg"),
    ),
    (
        "browser",
        include_str!("../../../assets/icons/ink/stroke-1.5/browser.svg"),
    ),
    (
        "pin",
        include_str!("../../../assets/icons/ink/stroke-1.5/pin.svg"),
    ),
    (
        "clock",
        include_str!("../../../assets/icons/ink/stroke-1.5/clock.svg"),
    ),
    (
        "chevron-right",
        include_str!("../../../assets/icons/ink/stroke-1.5/chevron-right.svg"),
    ),
    (
        "bell",
        include_str!("../../../assets/icons/ink/stroke-1.5/bell.svg"),
    ),
    (
        "lock",
        include_str!("../../../assets/icons/ink/stroke-1.5/lock.svg"),
    ),
];

/// The SVG source of a glyph.
pub fn icon(name: &str) -> Option<&'static str> {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Substitute the stroke colour (`#rrggbb`) for `currentColor`.
pub fn colored(svg: &str, hex: &str) -> String {
    svg.replace("currentColor", hex)
}

/// Kiln logo variants (copied from the approved brand kit, concept C).
pub const KILN_LOCKUP_FULL: &str = include_str!("../resources/brand/kiln-lockup-full.svg");
pub const KILN_LOCKUP_FULL_ON_DARK: &str =
    include_str!("../resources/brand/kiln-lockup-full-on-dark.svg");

/// Neutral Glaze symbol (24 px grid, full colour) for non-Kiln systems.
pub const GLAZE_SYMBOL: &str = include_str!("../resources/brand/glaze-symbol.svg");
pub const GLAZE_SYMBOL_ON_DARK: &str = include_str!("../resources/brand/glaze-symbol-on-dark.svg");

/// Paper texture tiles (`assets/textures`, generated by scripts/gen-textures.py).
pub const TEXTURES: &[(&str, &[u8])] = &[
    (
        "grain-light.png",
        include_bytes!("../../../assets/textures/grain-light.png"),
    ),
    (
        "grain-dark.png",
        include_bytes!("../../../assets/textures/grain-dark.png"),
    ),
    (
        "fibre-light.png",
        include_bytes!("../../../assets/textures/fibre-light.png"),
    ),
    (
        "fibre-dark.png",
        include_bytes!("../../../assets/textures/fibre-dark.png"),
    ),
    (
        "laid-light.png",
        include_bytes!("../../../assets/textures/laid-light.png"),
    ),
    (
        "laid-dark.png",
        include_bytes!("../../../assets/textures/laid-dark.png"),
    ),
];

pub fn texture(file: &str) -> Option<&'static [u8]> {
    TEXTURES.iter().find(|(n, _)| *n == file).map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_icon_exists_and_uses_current_color() {
        for p in crate::catalog::Page::ALL {
            let svg = icon(p.icon()).unwrap_or_else(|| panic!("missing icon {}", p.icon()));
            assert!(svg.contains("currentColor"));
            assert!(colored(svg, "#123456").contains("#123456"));
            assert!(!colored(svg, "#123456").contains("currentColor"));
        }
        for t in glaze_design::texture::SETS.iter().filter(|s| **s != "none") {
            for s in glaze_design::ColorScheme::ALL {
                let f = glaze_design::texture::tile_file(t, s).unwrap();
                assert!(
                    texture(&f).is_some_and(|b| b.starts_with(b"\x89PNG")),
                    "{f}"
                );
            }
        }
        assert!(KILN_LOCKUP_FULL.contains("Kiln GNU/Linux"));
    }
}
