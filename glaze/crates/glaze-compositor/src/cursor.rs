//! Software cursor for the DRM backend: xcursor theme images, cached per
//! icon name, uploaded once as memory buffers.
//!
//! Theme and size come from `XCURSOR_THEME` / `XCURSOR_SIZE` (defaults
//! `Adwaita`, 24), the same variables GTK and Qt read, so the compositor's
//! pointer matches the apps'. Named icons use the CSS cursor names
//! (`default`, `ew-resize`, `grabbing`, ...) with the legacy X names as
//! fallbacks. Nested (winit), the parent session draws the pointer instead.

use std::collections::HashMap;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::input::pointer::CursorIcon;
use smithay::utils::{Logical, Point, Transform};
use tracing::{info, warn};

/// One loaded cursor image.
#[derive(Clone)]
pub struct CursorImage {
    pub buffer: MemoryRenderBuffer,
    pub hotspot: Point<i32, Logical>,
}

pub struct CursorCache {
    theme: xcursor::CursorTheme,
    theme_name: String,
    size: u32,
    cache: HashMap<String, Option<CursorImage>>,
}

impl Default for CursorCache {
    fn default() -> Self {
        Self::from_env()
    }
}

impl CursorCache {
    pub fn from_env() -> Self {
        let theme_name = std::env::var("XCURSOR_THEME")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| gtk_setting("gtk-cursor-theme-name"))
            .unwrap_or_else(|| "Adwaita".to_owned());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .or_else(|| gtk_setting("gtk-cursor-theme-size"))
            .and_then(|s| s.parse().ok())
            .filter(|s: &u32| (8..=256).contains(s))
            .unwrap_or(24);
        info!(theme = %theme_name, size, "cursor theme");
        Self {
            theme: xcursor::CursorTheme::load(&theme_name),
            theme_name,
            size,
            cache: HashMap::new(),
        }
    }

    /// The image for `icon`, falling back to `default` (and finally to a
    /// built-in arrow when no theme is installed at all).
    pub fn get(&mut self, icon: CursorIcon) -> CursorImage {
        if let Some(img) = self.lookup(icon) {
            return img;
        }
        if let Some(img) = self.lookup(CursorIcon::Default) {
            return img;
        }
        self.cache
            .entry("__builtin".into())
            .or_insert_with(|| Some(builtin_arrow()))
            .clone()
            .expect("builtin cursor")
    }

    fn lookup(&mut self, icon: CursorIcon) -> Option<CursorImage> {
        let key = icon.name().to_owned();
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let names = std::iter::once(icon.name()).chain(icon.alt_names().iter().copied());
        let mut found = None;
        for name in names {
            if let Some(img) = self.load(name) {
                found = Some(img);
                break;
            }
        }
        if found.is_none() {
            warn!(icon = %key, theme = %self.theme_name, "cursor icon not in theme");
        }
        self.cache.insert(key, found.clone());
        found
    }

    fn load(&self, name: &str) -> Option<CursorImage> {
        let path = self.theme.load_icon(name)?;
        let data = std::fs::read(path).ok()?;
        let images = xcursor::parser::parse_xcursor(&data)?;
        // Nearest nominal size, first frame (no animated cursors).
        let best = images
            .iter()
            .min_by_key(|i| (i.size as i64 - self.size as i64).abs())?;
        let buffer = MemoryRenderBuffer::from_slice(
            &best.pixels_rgba,
            Fourcc::Argb8888,
            (best.width as i32, best.height as i32),
            1,
            Transform::Normal,
            None,
        );
        Some(CursorImage {
            buffer,
            hotspot: Point::from((best.xhot as i32, best.yhot as i32)),
        })
    }
}

/// A key from `~/.config/gtk-3.0/settings.ini` (what the settings app and
/// glaze-colour-scheme write), so the compositor's pointer follows the theme
/// apps use when `XCURSOR_*` is not exported.
fn gtk_setting(key: &str) -> Option<String> {
    let path = glaze_common::paths::config_home().join("gtk-3.0/settings.ini");
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key)
            .then(|| v.trim().trim_matches('"').to_owned())
            .filter(|v| !v.is_empty())
    })
}

/// A 12x18 ink arrow with a cream edge, for systems without any cursor theme.
fn builtin_arrow() -> CursorImage {
    const W: usize = 12;
    const H: usize = 18;
    let mut px = vec![0u8; W * H * 4];
    for y in 0..H {
        let span = (y * W / H).min(W - 1);
        for x in 0..=span {
            if y > 14 && x > 4 {
                continue;
            }
            let edge = x == 0 || x == span || y == H - 1;
            let (b, g, r) = if edge {
                (0xF5, 0xF9, 0xFA)
            } else {
                (0x26, 0x29, 0x2B)
            };
            let i = (y * W + x) * 4;
            px[i..i + 4].copy_from_slice(&[b, g, r, 0xFF]);
        }
    }
    CursorImage {
        buffer: MemoryRenderBuffer::from_slice(
            &px,
            Fourcc::Argb8888,
            (W as i32, H as i32),
            1,
            Transform::Normal,
            None,
        ),
        hotspot: Point::from((0, 0)),
    }
}
