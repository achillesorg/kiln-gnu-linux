//! Glaze paper window frames (server-side decorations), portable.
//!
//! Look, from the approved mockup (`$HOME/fedeos/mockups/taskbar-v2`):
//! a flat cream title bar (28 px) with a 1 px warm-grey hairline around the
//! window, 4 px rounded top corners, a 12 px semibold title in warm ink,
//! three 9 px line glyphs (minimise, maximise, close) drawn with a 1.25 px
//! stroke, and a very faint tone-on-tone offset shadow (no blur, no heavy
//! black). The focused window gets a 2 px terracotta "paper tab" along the
//! top edge; unfocused windows wash out slightly.
//!
//! This module owns the frame *geometry* (insets, button rects, hit-testing
//! for move/resize/buttons) and *paints* the title bar into a CPU pixel buffer
//! (premultiplied ARGB8888, little-endian, i.e. what DRM calls `Argb8888`).
//! The title bar is repainted only when its width, title, focus or hover state
//! change, then uploaded once; on llvmpipe that is far cheaper than drawing
//! text every frame. Side and bottom hairlines and the shadow are solid-colour
//! elements built by the Smithay side from [`FrameGeometry`].

use std::path::Path;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use glaze_wm::Rect;

/// Title bar height including the top hairline and the separator line.
pub const TITLE_H: i32 = 28;
/// Hairline border width.
pub const BORDER: i32 = 1;
/// Invisible grab margin outside the frame for resizing (12-16 px asked for).
pub const RESIZE_GRAB: i32 = 14;
/// Corner grab size for diagonal resize.
pub const CORNER_GRAB: i32 = 22;
/// Top corner radius of the title bar.
pub const RADIUS: f64 = 4.0;
/// Title bar button hit box.
pub const BUTTON_W: i32 = 22;
pub const BUTTON_H: i32 = 22;
const BUTTON_GAP: i32 = 2;
const BUTTON_PAD_RIGHT: i32 = 4;
const GLYPH: f64 = 9.0;
const GLYPH_STROKE: f64 = 1.25;
const TITLE_PAD_LEFT: i32 = 12;
const TITLE_PX: f32 = 12.5;
/// Offset of the faint paper shadow.
pub const SHADOW_OFFSET: i32 = 3;

/// An sRGB colour with straight (non-premultiplied) alpha, 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn hex(v: u32) -> Self {
        Self {
            r: ((v >> 16) & 0xff) as f32 / 255.0,
            g: ((v >> 8) & 0xff) as f32 / 255.0,
            b: (v & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Parse `#rrggbb` (or `rrggbb`).
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().trim_start_matches('#');
        if h.len() != 6 {
            return None;
        }
        u32::from_str_radix(h, 16).ok().map(Self::hex)
    }

    /// Relative luminance (sRGB, rough), for light/dark decisions.
    pub fn luma(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Premultiplied `[r, g, b, a]` for Smithay solid elements.
    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        Rgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

/// Frame colours. Values for the light scheme come straight from the mockup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub title_bg: Rgba,
    pub title_bg_inactive: Rgba,
    pub stroke: Rgba,
    pub text: Rgba,
    pub text_inactive: Rgba,
    pub glyph: Rgba,
    pub accent: Rgba,
    pub button_hover: Rgba,
    pub button_pressed: Rgba,
    pub close_hover: Rgba,
    pub close_glyph_hover: Rgba,
    pub shadow: Rgba,
    pub shadow_inactive: Rgba,
    /// Desktop background when no wallpaper client is running.
    pub desktop: Rgba,
}

impl Palette {
    /// Glaze Clay, light.
    pub const fn clay_light() -> Self {
        Self {
            title_bg: Rgba::hex(0xF3EFE6),
            title_bg_inactive: Rgba::hex(0xF8F6F0),
            stroke: Rgba::hex(0xCFC6B4),
            text: Rgba::hex(0x2B2926),
            text_inactive: Rgba::hex(0x8A847A),
            glyph: Rgba::hex(0x3A3733),
            accent: Rgba::hex(0xD97757),
            button_hover: Rgba::hex(0xEBD9CD),
            button_pressed: Rgba::hex(0xE2C7B6),
            close_hover: Rgba::hex(0xD97757),
            close_glyph_hover: Rgba::hex(0xFCFBF7),
            shadow: Rgba::hex(0x5A4636).with_alpha(0.14),
            shadow_inactive: Rgba::hex(0x5A4636).with_alpha(0.08),
            desktop: Rgba::hex(0xD97757),
        }
    }

    /// Glaze Clay, dark (warm ink paper).
    pub const fn clay_dark() -> Self {
        Self {
            title_bg: Rgba::hex(0x2F2B27),
            title_bg_inactive: Rgba::hex(0x282521),
            stroke: Rgba::hex(0x4B453D),
            text: Rgba::hex(0xEFE9DE),
            text_inactive: Rgba::hex(0x9A9387),
            glyph: Rgba::hex(0xDCD4C7),
            accent: Rgba::hex(0xD97757),
            button_hover: Rgba::hex(0x45392F),
            button_pressed: Rgba::hex(0x55443A),
            close_hover: Rgba::hex(0xD97757),
            close_glyph_hover: Rgba::hex(0x1E1B18),
            shadow: Rgba::hex(0x000000).with_alpha(0.22),
            shadow_inactive: Rgba::hex(0x000000).with_alpha(0.12),
            desktop: Rgba::hex(0x3A2A22),
        }
    }
}

/// Live frame colours pushed by the shell (`set-frame-colors`, the same
/// tokens it uses for Sway title bars), `#rrggbb` strings.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
pub struct FrameColorsMsg {
    pub focused_bg: String,
    pub focused_text: String,
    pub focused_border: String,
    pub other_bg: String,
    pub other_text: String,
    pub other_border: String,
    #[serde(default)]
    pub urgent_bg: String,
}

impl Palette {
    /// The built-in palette for a scheme, with an optional accent override.
    pub fn for_scheme(dark: bool, accent: Option<Rgba>) -> Self {
        let mut p = if dark {
            Self::clay_dark()
        } else {
            Self::clay_light()
        };
        if let Some(a) = accent {
            p = p.with_accent(a);
        }
        p
    }

    /// Recolour everything that follows the accent.
    pub fn with_accent(mut self, accent: Rgba) -> Self {
        self.accent = accent;
        self.close_hover = accent;
        self.button_hover = self.title_bg.mix(accent, 0.22);
        self.button_pressed = self.title_bg.mix(accent, 0.36);
        self.close_glyph_hover = if accent.luma() > 0.55 {
            Rgba::hex(0x2B1A12)
        } else {
            Rgba::hex(0xFCFBF7)
        };
        self
    }

    /// Build from the shell's live tokens. Shadows and the bare-desktop
    /// colour keep the built-in values of the matching scheme.
    pub fn from_tokens(c: &FrameColorsMsg) -> Option<Self> {
        let focused_bg = Rgba::parse(&c.focused_bg)?;
        let dark = focused_bg.luma() < 0.4;
        let base = Self::for_scheme(dark, None);
        let p = Palette {
            title_bg: focused_bg,
            title_bg_inactive: Rgba::parse(&c.other_bg).unwrap_or(base.title_bg_inactive),
            text: Rgba::parse(&c.focused_text).unwrap_or(base.text),
            text_inactive: Rgba::parse(&c.other_text).unwrap_or(base.text_inactive),
            stroke: Rgba::parse(&c.other_border).unwrap_or(base.stroke),
            glyph: Rgba::parse(&c.focused_text).unwrap_or(base.glyph),
            ..base
        };
        Some(p.with_accent(Rgba::parse(&c.focused_border).unwrap_or(base.accent)))
    }
}

/// Title bar buttons, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

impl Button {
    pub const ALL: [Button; 3] = [Button::Minimize, Button::Maximize, Button::Close];
}

/// Which edges an interactive resize moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// What the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePart {
    /// The client surface: pass the event through.
    Client,
    /// The title bar (drag to move, double-click to maximise).
    Title,
    Button(Button),
    /// A resize border or corner.
    Resize(Edges),
}

/// Space the frame adds around the client geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insets {
    pub top: i32,
    pub bottom: i32,
    pub left: i32,
    pub right: i32,
}

impl Insets {
    /// Glaze server-side frame.
    pub const FRAMED: Insets = Insets {
        top: TITLE_H,
        bottom: BORDER,
        left: BORDER,
        right: BORDER,
    };
    /// Client-side decorations (or fullscreen): nothing.
    pub const NONE: Insets = Insets {
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
    };

    /// Outer rect for a client rect.
    pub fn outer(&self, client: Rect) -> Rect {
        Rect::new(
            client.x - self.left,
            client.y - self.top,
            client.w + self.left + self.right,
            client.h + self.top + self.bottom,
        )
    }

    /// Client rect for an outer rect.
    pub fn client(&self, outer: Rect) -> Rect {
        Rect::new(
            outer.x + self.left,
            outer.y + self.top,
            (outer.w - self.left - self.right).max(1),
            (outer.h - self.top - self.bottom).max(1),
        )
    }
}

/// Rect of `button` relative to the outer frame origin.
pub fn button_rect(outer_w: i32, button: Button) -> Rect {
    let idx_from_right = match button {
        Button::Close => 0,
        Button::Maximize => 1,
        Button::Minimize => 2,
    };
    let x = outer_w
        - BUTTON_PAD_RIGHT
        - BORDER
        - (idx_from_right + 1) * BUTTON_W
        - idx_from_right * BUTTON_GAP;
    let y = (TITLE_H - BUTTON_H) / 2;
    Rect::new(x, y, BUTTON_W, BUTTON_H)
}

/// Hit-test a point (`x`, `y` relative to the outer frame's origin) against a
/// framed window of outer size `w`x`h`. Points up to [`RESIZE_GRAB`] outside
/// the frame count as resize borders. Returns `None` when outside.
pub fn hit_test(w: i32, h: i32, x: i32, y: i32, resizable: bool) -> Option<FramePart> {
    let g = if resizable { RESIZE_GRAB } else { 0 };
    if x < -g || y < -g || x >= w + g || y >= h + g {
        return None;
    }
    if resizable {
        let near_left = x < BORDER.max(1) + 2;
        let near_right = x >= w - BORDER.max(1) - 2;
        let near_top = y < 3;
        let near_bottom = y >= h - BORDER.max(1) - 2;
        let outside = x < 0 || y < 0 || x >= w || y >= h;
        let corner_left = x < CORNER_GRAB;
        let corner_right = x >= w - CORNER_GRAB;
        let corner_top = y < CORNER_GRAB;
        let corner_bottom = y >= h - CORNER_GRAB;
        let mut e = Edges {
            top: near_top,
            bottom: near_bottom,
            left: near_left,
            right: near_right,
        };
        if outside || e != Edges::default() {
            // Widen to corners when close to one.
            if e.top || e.bottom {
                e.left |= corner_left;
                e.right |= corner_right;
            }
            if e.left || e.right {
                e.top |= corner_top;
                e.bottom |= corner_bottom;
            }
            if e != Edges::default() {
                return Some(FramePart::Resize(e));
            }
        }
    }
    if x < 0 || y < 0 || x >= w || y >= h {
        return None;
    }
    if y < TITLE_H {
        for b in Button::ALL {
            let r = button_rect(w, b);
            if r.contains_point(x, y) {
                return Some(FramePart::Button(b));
            }
        }
        return Some(FramePart::Title);
    }
    Some(FramePart::Client)
}

/// Pieces of a frame in output coordinates, for solid-colour elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameGeometry {
    pub title: Rect,
    pub left: Rect,
    pub right: Rect,
    pub bottom: Rect,
    /// The L-shaped faint shadow: right strip and bottom strip.
    pub shadow_right: Rect,
    pub shadow_bottom: Rect,
}

impl FrameGeometry {
    pub fn for_outer(outer: Rect) -> Self {
        let Rect { x, y, w, h } = outer;
        let s = SHADOW_OFFSET;
        Self {
            title: Rect::new(x, y, w, TITLE_H),
            left: Rect::new(x, y + TITLE_H, BORDER, (h - TITLE_H).max(0)),
            right: Rect::new(x + w - BORDER, y + TITLE_H, BORDER, (h - TITLE_H).max(0)),
            bottom: Rect::new(x, y + h - BORDER, w, BORDER),
            shadow_right: Rect::new(x + w, y + s, s, h),
            shadow_bottom: Rect::new(x + s, y + h, w - s, s),
        }
    }
}

/// Visual state of a title bar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TitleState {
    pub width: i32,
    pub title: String,
    pub focused: bool,
    pub hovered: Option<Button>,
    pub pressed: Option<Button>,
    pub maximized: bool,
}

/// Font search order for title text (first that loads wins).
pub const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/inter/Inter-SemiBold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];

/// Load the first available title font, or `None` (titles are then not
/// drawn, which is logged by the caller rather than faked).
pub fn load_title_font() -> Option<(FontVec, String)> {
    FONT_CANDIDATES.iter().find_map(|p| {
        let bytes = std::fs::read(Path::new(p)).ok()?;
        FontVec::try_from_vec(bytes)
            .ok()
            .map(|f| (f, (*p).to_owned()))
    })
}

/// A premultiplied ARGB8888 (little-endian: B, G, R, A bytes) pixel buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
        }
    }

    /// Straight-alpha colour of a pixel (for tests).
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        let (b, g, r, a) = (
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        );
        if a == 0 {
            return [0, 0, 0, 0];
        }
        let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
        [un(r), un(g), un(b), a]
    }

    /// Source-over blend `c` with extra coverage `cov` (0..=1).
    pub fn blend(&mut self, x: i32, y: i32, c: Rgba, cov: f32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let a = (c.a * cov.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        let src = [c.b * a, c.g * a, c.r * a, a];
        for (k, s) in src.iter().enumerate() {
            let dst = self.data[i + k] as f32 / 255.0;
            let out = s + dst * (1.0 - a);
            self.data[i + k] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    pub fn fill_rect(&mut self, r: Rect, c: Rgba) {
        for y in r.y.max(0)..(r.y + r.h).min(self.height) {
            for x in r.x.max(0)..(r.x + r.w).min(self.width) {
                self.blend(x, y, c, 1.0);
            }
        }
    }

    /// Anti-aliased line of `width` px between two points (pixel centres at
    /// `.5`), by distance-to-segment coverage. Cheap enough for 9 px glyphs.
    pub fn line(&mut self, (x0, y0): (f64, f64), (x1, y1): (f64, f64), width: f64, c: Rgba) {
        let half = width / 2.0;
        let minx = (x0.min(x1) - half - 1.0).floor() as i32;
        let maxx = (x0.max(x1) + half + 1.0).ceil() as i32;
        let miny = (y0.min(y1) - half - 1.0).floor() as i32;
        let maxy = (y0.max(y1) + half + 1.0).ceil() as i32;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx * dx + dy * dy;
        for py in miny..=maxy {
            for px in minx..=maxx {
                let (cx, cy) = (px as f64 + 0.5, py as f64 + 0.5);
                let t = if len2 > 0.0 {
                    (((cx - x0) * dx + (cy - y0) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (qx, qy) = (x0 + t * dx, y0 + t * dy);
                let d = ((cx - qx).powi(2) + (cy - qy).powi(2)).sqrt();
                let cov = (half + 0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, c, cov as f32);
                }
            }
        }
    }
}

/// Paint a title bar for `state` (outer width `state.width`, height
/// [`TITLE_H`]). `font` may be `None` (no text).
pub fn paint_title_bar(state: &TitleState, palette: &Palette, font: Option<&FontVec>) -> Canvas {
    let w = state.width.max(BUTTON_W * 3 + 16);
    let h = TITLE_H;
    let mut c = Canvas::new(w, h);
    let bg = if state.focused {
        palette.title_bg
    } else {
        palette.title_bg_inactive
    };
    let radius = if state.maximized { 0.0 } else { RADIUS };

    // Background with rounded top corners (coverage-based AA), hairline around.
    for y in 0..h {
        for x in 0..w {
            let cov = corner_coverage(x, y, w, radius);
            if cov > 0.0 {
                c.blend(x, y, bg, cov);
            }
        }
    }
    // Hairline: top edge, sides, and the separator under the title.
    for x in 0..w {
        let cov_top = edge_coverage(x, w, radius);
        if cov_top > 0.0 {
            c.blend(x, 0, palette.stroke, cov_top);
        }
        c.blend(x, h - 1, palette.stroke, 1.0);
    }
    for y in 0..h {
        if (y as f64) >= radius {
            c.blend(0, y, palette.stroke, 1.0);
            c.blend(w - 1, y, palette.stroke, 1.0);
        }
    }
    if radius > 0.0 {
        stroke_corner_arcs(&mut c, w, radius, palette.stroke);
    }
    if state.focused {
        // Terracotta paper tab just inside the top hairline (so it still
        // reads against a terracotta wallpaper), inset from the corners.
        let inset = radius.ceil() as i32 + 2;
        c.fill_rect(Rect::new(inset, 1, w - 2 * inset, 2), palette.accent);
    }

    // Buttons.
    for b in Button::ALL {
        let r = button_rect(w, b);
        let hovered = state.hovered == Some(b);
        let pressed = state.pressed == Some(b);
        let mut glyph = palette.glyph;
        if hovered || pressed {
            let fill = match (b, pressed) {
                (Button::Close, _) => palette.close_hover,
                (_, true) => palette.button_pressed,
                _ => palette.button_hover,
            };
            fill_round_rect(&mut c, r, 4.0, fill);
            if b == Button::Close {
                glyph = palette.close_glyph_hover;
            }
        }
        if !state.focused && !hovered {
            glyph = glyph.mix(bg, 0.45);
        }
        let cx = r.x as f64 + r.w as f64 / 2.0;
        let cy = r.y as f64 + r.h as f64 / 2.0;
        let g = GLYPH / 2.0;
        match b {
            Button::Minimize => {
                c.line((cx - g, cy + 0.5), (cx + g, cy + 0.5), GLYPH_STROKE, glyph);
            }
            Button::Maximize => {
                if state.maximized {
                    // Two offset sheets: "restore".
                    let s = g - 1.5;
                    rect_outline(&mut c, cx - s + 1.5, cy - s - 1.0, 2.0 * s, 2.0 * s, glyph);
                    rect_outline(&mut c, cx - s - 1.0, cy - s + 1.5, 2.0 * s, 2.0 * s, glyph);
                } else {
                    rect_outline(&mut c, cx - g, cy - g, 2.0 * g, 2.0 * g, glyph);
                }
            }
            Button::Close => {
                c.line((cx - g, cy - g), (cx + g, cy + g), GLYPH_STROKE, glyph);
                c.line((cx + g, cy - g), (cx - g, cy + g), GLYPH_STROKE, glyph);
            }
        }
    }

    // Title text, ellipsised before the buttons.
    if let Some(font) = font {
        let color = if state.focused {
            palette.text
        } else {
            palette.text_inactive
        };
        let max_w = (button_rect(w, Button::Minimize).x - TITLE_PAD_LEFT - 8) as f32;
        draw_text(
            &mut c,
            font,
            &state.title,
            TITLE_PAD_LEFT as f32,
            max_w,
            color,
        );
    }
    c
}

fn corner_coverage(x: i32, y: i32, w: i32, r: f64) -> f32 {
    if r <= 0.0 {
        return 1.0;
    }
    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
    let cx = if px < r {
        r
    } else if px > w as f64 - r {
        w as f64 - r
    } else {
        return 1.0;
    };
    if py > r {
        return 1.0;
    }
    let d = ((px - cx).powi(2) + (py - r).powi(2)).sqrt();
    (r - d + 0.5).clamp(0.0, 1.0) as f32
}

fn edge_coverage(x: i32, w: i32, r: f64) -> f32 {
    if (x as f64) < r || (x as f64) >= w as f64 - r {
        0.0
    } else {
        1.0
    }
}

fn stroke_corner_arcs(c: &mut Canvas, w: i32, r: f64, color: Rgba) {
    let ri = r.ceil() as i32 + 1;
    for y in 0..ri {
        for x in 0..ri {
            for &(px, cx) in &[(x, r), (w - 1 - x, w as f64 - r)] {
                let fx = px as f64 + 0.5;
                let fy = y as f64 + 0.5;
                let d = ((fx - cx).powi(2) + (fy - r).powi(2)).sqrt();
                let in_quadrant =
                    fy <= r + 0.5 && ((px as f64) < r + 0.5 || (px as f64) >= w as f64 - r - 1.5);
                if in_quadrant {
                    let cov = (1.0 - ((d - (r - 0.5)).abs())).clamp(0.0, 1.0);
                    if cov > 0.0 {
                        c.blend(px, y, color, cov as f32);
                    }
                }
            }
        }
    }
}

fn fill_round_rect(c: &mut Canvas, r: Rect, radius: f64, color: Rgba) {
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            let fx = x as f64 + 0.5 - r.x as f64;
            let fy = y as f64 + 0.5 - r.y as f64;
            let qx = fx.min(r.w as f64 - fx);
            let qy = fy.min(r.h as f64 - fy);
            let cov = if qx < radius && qy < radius {
                let d = ((radius - qx).powi(2) + (radius - qy).powi(2)).sqrt();
                (radius - d + 0.5).clamp(0.0, 1.0)
            } else {
                1.0
            };
            c.blend(x, y, color, cov as f32);
        }
    }
}

fn rect_outline(c: &mut Canvas, x: f64, y: f64, w: f64, h: f64, color: Rgba) {
    let s = GLYPH_STROKE;
    c.line((x, y), (x + w, y), s, color);
    c.line((x + w, y), (x + w, y + h), s, color);
    c.line((x + w, y + h), (x, y + h), s, color);
    c.line((x, y + h), (x, y), s, color);
}

/// Width of `text` at the title size.
pub fn text_width(font: &FontVec, text: &str) -> f32 {
    let sf = font.as_scaled(PxScale::from(TITLE_PX));
    let mut w = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            w += sf.kern(p, id);
        }
        w += sf.h_advance(id);
        prev = Some(id);
    }
    w
}

fn ellipsize(font: &FontVec, text: &str, max_w: f32) -> String {
    if text_width(font, text) <= max_w {
        return text.to_owned();
    }
    let mut out: String = text.to_owned();
    while !out.is_empty() {
        out.pop();
        let candidate = format!("{}…", out.trim_end());
        if text_width(font, &candidate) <= max_w {
            return candidate;
        }
    }
    String::new()
}

fn draw_text(c: &mut Canvas, font: &FontVec, text: &str, x0: f32, max_w: f32, color: Rgba) {
    if max_w <= 8.0 || text.is_empty() {
        return;
    }
    let text = ellipsize(font, text, max_w);
    let sf = font.as_scaled(PxScale::from(TITLE_PX));
    // Vertically centre the cap height in the title band (excluding hairlines).
    let ascent = sf.ascent();
    let descent = sf.descent();
    let baseline = ((TITLE_H as f32 - (ascent - descent)) / 2.0 + ascent).round();
    let mut x = x0;
    let mut prev = None;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        let glyph =
            id.with_scale_and_position(PxScale::from(TITLE_PX), ab_glyph::point(x, baseline));
        x += sf.h_advance(id);
        prev = Some(id);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                c.blend(px, py, color, cov);
            });
        }
    }
}

/// Draw `text` at `px` with its baseline at `baseline`, ellipsised to `max_w`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_px(
    c: &mut Canvas,
    font: &FontVec,
    text: &str,
    x0: f32,
    baseline: f32,
    max_w: f32,
    px: f32,
    color: Rgba,
) {
    if max_w <= 8.0 || text.is_empty() {
        return;
    }
    let sf = font.as_scaled(PxScale::from(px));
    let width = |t: &str| {
        let mut w = 0.0;
        let mut prev = None;
        for ch in t.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                w += sf.kern(p, id);
            }
            w += sf.h_advance(id);
            prev = Some(id);
        }
        w
    };
    let mut text = text.to_owned();
    if width(&text) > max_w {
        while !text.is_empty() {
            text.pop();
            let cand = format!("{}…", text.trim_end());
            if width(&cand) <= max_w {
                text = cand;
                break;
            }
        }
    }
    let mut x = x0;
    let mut prev = None;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(px), ab_glyph::point(x, baseline));
        x += sf.h_advance(id);
        prev = Some(id);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                c.blend(
                    bounds.min.x as i32 + gx as i32,
                    bounds.min.y as i32 + gy as i32,
                    color,
                    cov,
                );
            });
        }
    }
}

/// One row of the Alt+Tab switcher.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SwitcherRow {
    pub title: String,
    pub app: String,
    pub minimized: bool,
}

/// Switcher card geometry.
pub const SWITCHER_W: i32 = 440;
pub const SWITCHER_ROW_H: i32 = 34;
const SWITCHER_PAD: i32 = 10;
const SWITCHER_MAX_ROWS: usize = 12;

/// Paint the Alt+Tab switcher: a flat paper card (hairline edge, small
/// radius, hard 3 px offset sheet, no blur) listing windows in MRU order;
/// the previewed row gets the accent tint and a terracotta tab on the left.
pub fn paint_switcher(
    rows: &[SwitcherRow],
    selected: usize,
    palette: &Palette,
    font: Option<&FontVec>,
) -> Canvas {
    let n = rows.len().clamp(1, SWITCHER_MAX_ROWS);
    let card_w = SWITCHER_W;
    let card_h = SWITCHER_PAD * 2 + 22 + n as i32 * SWITCHER_ROW_H;
    let off = SHADOW_OFFSET;
    let mut c = Canvas::new(card_w + off, card_h + off);
    let radius = 6.0;
    // Offset sheet (hard, faint) then the card.
    fill_round_rect(
        &mut c,
        Rect::new(off, off, card_w, card_h),
        radius,
        palette.shadow,
    );
    fill_round_rect(
        &mut c,
        Rect::new(0, 0, card_w, card_h),
        radius,
        palette.stroke,
    );
    fill_round_rect(
        &mut c,
        Rect::new(1, 1, card_w - 2, card_h - 2),
        radius - 1.0,
        palette.title_bg,
    );
    // Header: "Windows" in the muted ink, plus a hairline.
    if let Some(font) = font {
        draw_text_px(
            &mut c,
            font,
            "Switch windows",
            (SWITCHER_PAD + 6) as f32,
            (SWITCHER_PAD + 14) as f32,
            (card_w - 2 * SWITCHER_PAD) as f32,
            11.5,
            palette.text_inactive,
        );
    }
    let top = SWITCHER_PAD + 22;
    c.fill_rect(
        Rect::new(SWITCHER_PAD, top - 4, card_w - 2 * SWITCHER_PAD, 1),
        palette.stroke,
    );
    let start = selected.saturating_sub(SWITCHER_MAX_ROWS - 1);
    for (i, row) in rows.iter().enumerate().skip(start).take(n) {
        let y = top + (i - start) as i32 * SWITCHER_ROW_H;
        let r = Rect::new(
            SWITCHER_PAD,
            y + 2,
            card_w - 2 * SWITCHER_PAD,
            SWITCHER_ROW_H - 4,
        );
        let is_sel = i == selected;
        if is_sel {
            fill_round_rect(&mut c, r, 4.0, palette.title_bg.mix(palette.accent, 0.18));
            c.fill_rect(Rect::new(r.x, r.y + 5, 3, r.h - 10), palette.accent);
        }
        // App chip: hairline square with the app's initial.
        let chip = Rect::new(r.x + 10, r.y + (r.h - 20) / 2, 20, 20);
        let ink = if row.minimized {
            palette.text_inactive
        } else {
            palette.text
        };
        rect_outline(
            &mut c,
            chip.x as f64 + 0.5,
            chip.y as f64 + 0.5,
            chip.w as f64 - 1.0,
            chip.h as f64 - 1.0,
            if is_sel {
                palette.accent
            } else {
                palette.glyph.mix(palette.title_bg, 0.35)
            },
        );
        if let Some(font) = font {
            let initial: String = row
                .app
                .chars()
                .find(|ch| ch.is_alphanumeric())
                .map(|ch| ch.to_uppercase().collect())
                .unwrap_or_else(|| "·".into());
            draw_text_px(
                &mut c,
                font,
                &initial,
                chip.x as f32 + 5.5,
                chip.y as f32 + 14.5,
                14.0,
                12.0,
                ink,
            );
            let label = if row.minimized {
                format!("{}  (minimised)", row.title)
            } else {
                row.title.clone()
            };
            draw_text_px(
                &mut c,
                font,
                &label,
                (chip.right() + 10) as f32,
                (r.y + r.h / 2 + 5) as f32,
                (r.right() - chip.right() - 18) as f32,
                13.0,
                if is_sel { palette.text } else { ink },
            );
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(focused: bool) -> TitleState {
        TitleState {
            width: 460,
            title: "Terminal".into(),
            focused,
            hovered: None,
            pressed: None,
            maximized: false,
        }
    }

    #[test]
    fn insets_round_trip() {
        let client = Rect::new(100, 100, 640, 400);
        let outer = Insets::FRAMED.outer(client);
        assert_eq!(outer, Rect::new(99, 72, 642, 429));
        assert_eq!(Insets::FRAMED.client(outer), client);
        assert_eq!(Insets::NONE.outer(client), client);
    }

    #[test]
    fn buttons_are_ordered_and_inside_the_title_bar() {
        let w = 460;
        let min = button_rect(w, Button::Minimize);
        let max = button_rect(w, Button::Maximize);
        let close = button_rect(w, Button::Close);
        assert!(min.x < max.x && max.x < close.x);
        assert!(close.right() <= w - BORDER);
        for r in [min, max, close] {
            assert!(r.y >= 0 && r.bottom() <= TITLE_H);
        }
    }

    #[test]
    fn hit_testing() {
        let (w, h) = (460, 300);
        assert_eq!(hit_test(w, h, 50, 10, true), Some(FramePart::Title));
        assert_eq!(hit_test(w, h, 50, 100, true), Some(FramePart::Client));
        let close = button_rect(w, Button::Close);
        assert_eq!(
            hit_test(w, h, close.x + 5, close.y + 5, true),
            Some(FramePart::Button(Button::Close))
        );
        // Outside right edge, mid height: resize right.
        assert_eq!(
            hit_test(w, h, w + 2, 150, true),
            Some(FramePart::Resize(Edges {
                right: true,
                ..Edges::default()
            }))
        );
        // Bottom-left corner.
        assert_eq!(
            hit_test(w, h, -3, h + 2, true),
            Some(FramePart::Resize(Edges {
                bottom: true,
                left: true,
                ..Edges::default()
            }))
        );
        assert_eq!(hit_test(w, h, -30, 10, true), None);
        // Not resizable: borders are part of the frame, outside is nothing.
        assert_eq!(hit_test(w, h, w + 2, 150, false), None);
    }

    #[test]
    fn frame_geometry_is_hairline() {
        let g = FrameGeometry::for_outer(Rect::new(10, 20, 300, 200));
        assert_eq!(g.title, Rect::new(10, 20, 300, TITLE_H));
        assert_eq!(g.left.w, 1);
        assert_eq!(g.right.x, 309);
        assert_eq!(g.bottom.y, 219);
        assert_eq!(g.shadow_right.x, 310);
        assert_eq!(g.shadow_bottom.y, 220);
    }

    #[test]
    fn painted_title_uses_mockup_colours() {
        let p = Palette::clay_light();
        let c = paint_title_bar(&state(false), &p, None);
        assert_eq!((c.width, c.height), (460, TITLE_H));
        // Body of the title bar: cream (inactive variant).
        assert_eq!(c.pixel(100, 14), [0xF8, 0xF6, 0xF0, 255]);
        // Separator line under the title: warm grey hairline.
        assert_eq!(c.pixel(100, TITLE_H - 1), [0xCF, 0xC6, 0xB4, 255]);
        // Rounded corner: the very corner pixel is transparent.
        assert_eq!(c.pixel(0, 0)[3], 0);
        let f = paint_title_bar(&state(true), &p, None);
        assert_eq!(f.pixel(100, 14), [0xF3, 0xEF, 0xE6, 255]);
        // Hairline on top, focus accent just below it.
        assert_eq!(f.pixel(100, 0), [0xCF, 0xC6, 0xB4, 255]);
        assert_eq!(f.pixel(100, 1), [0xD9, 0x77, 0x57, 255]);
    }

    #[test]
    fn close_hover_fills_terracotta() {
        let p = Palette::clay_light();
        let mut s = state(true);
        s.hovered = Some(Button::Close);
        let c = paint_title_bar(&s, &p, None);
        let r = button_rect(460, Button::Close);
        assert_eq!(c.pixel(r.x + 2, r.y + r.h / 2), [0xD9, 0x77, 0x57, 255]);
    }

    #[test]
    fn glyphs_are_drawn() {
        let p = Palette::clay_light();
        let c = paint_title_bar(&state(true), &p, None);
        let r = button_rect(460, Button::Close);
        let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
        let px = c.pixel(cx, cy);
        assert!(px[0] < 0x90, "close glyph centre is ink: {px:?}");
    }

    #[test]
    fn switcher_card_is_paper() {
        let p = Palette::clay_light();
        let rows = vec![
            SwitcherRow {
                title: "Terminal".into(),
                app: "foot".into(),
                minimized: false,
            },
            SwitcherRow {
                title: "Files".into(),
                app: "thunar".into(),
                minimized: true,
            },
        ];
        let c = paint_switcher(&rows, 1, &p, None);
        assert_eq!(c.width, SWITCHER_W + SHADOW_OFFSET);
        // Card body is the title-bar cream; the edge is the hairline.
        assert_eq!(c.pixel(SWITCHER_W / 2, 12), [0xF3, 0xEF, 0xE6, 255]);
        assert_eq!(c.pixel(SWITCHER_W / 2, 0), [0xCF, 0xC6, 0xB4, 255]);
        // The selected (second) row carries the terracotta tab.
        let y = 10 + 22 + SWITCHER_ROW_H + SWITCHER_ROW_H / 2;
        assert_eq!(c.pixel(11, y), [0xD9, 0x77, 0x57, 255]);
    }

    #[test]
    fn text_is_drawn_when_a_font_exists() {
        let Some((font, _)) = load_title_font() else {
            eprintln!("no font on this host; skipping");
            return;
        };
        let p = Palette::clay_light();
        let c = paint_title_bar(&state(true), &p, Some(&font));
        let inked = (TITLE_PAD_LEFT..TITLE_PAD_LEFT + 40)
            .flat_map(|x| (6..22).map(move |y| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y)[0] < 0x80)
            .count();
        assert!(inked > 20, "expected title ink, got {inked} dark pixels");
        assert!(text_width(&font, "Terminal") > 20.0);
        let long = "x".repeat(400);
        assert!(ellipsize(&font, &long, 100.0).ends_with('…'));
    }
}
