//! Design tokens: the single source of truth for Glaze's visual language.
//!
//! Two token families (`docs/decisions.md` Q7): **Glaze Clay** (default, flat
//! cut paper, hairline warm strokes, terracotta accent) and **Glaze Classic**
//! (Windows 2000 heritage, square, bevelled). Both share one [`ThemeTokens`]
//! structure so every Glaze surface renders from the same values. Every token
//! can be overridden from a theme file or the user config
//! ([`crate::overrides`]); the GTK CSS emitter lives in [`crate::css`].
//!
//! Design rules enforced by tests here:
//! - Clay shadows have **zero blur** and are faint, warm, tone-on-tone offsets
//!   (no heavy black offset shadows).
//! - Clay strokes are hairlines (1-1.25 px) in warm grey-brown.
//! - Text/background pairs meet WCAG AA (the full audit is `glaze-a11y`).

use crate::color::Color;
use crate::kinds::{ColorScheme, ThemeKind};

/// Named colour roles. Adding a role here is the *only* supported way to add a
/// colour to the design system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeRole {
    Canvas,
    Surface,
    SurfaceRaised,
    SurfaceSunken,
    Border,
    TextPrimary,
    TextSecondary,
    Accent,
    AccentText,
    Focus,
    Danger,
    Success,
    Warning,
    Shadow,
    Highlight,
    /// The taskbar/panel fill (slightly darker paper than `surface`).
    Panel,
    /// Icon and glyph strokes (ink).
    Ink,
    /// Soft accent tint: selected segment, group-count pill.
    AccentSoft,
    /// Text on [`ThemeRole::AccentSoft`].
    AccentSoftText,
}

impl ThemeRole {
    /// Every role, in a stable order (used by the CSS emitter and tests).
    pub const ALL: [ThemeRole; 19] = [
        ThemeRole::Canvas,
        ThemeRole::Surface,
        ThemeRole::SurfaceRaised,
        ThemeRole::SurfaceSunken,
        ThemeRole::Border,
        ThemeRole::TextPrimary,
        ThemeRole::TextSecondary,
        ThemeRole::Accent,
        ThemeRole::AccentText,
        ThemeRole::Focus,
        ThemeRole::Danger,
        ThemeRole::Success,
        ThemeRole::Warning,
        ThemeRole::Shadow,
        ThemeRole::Highlight,
        ThemeRole::Panel,
        ThemeRole::Ink,
        ThemeRole::AccentSoft,
        ThemeRole::AccentSoftText,
    ];

    /// Snake-case name used for the `@define-color` identifiers and in theme
    /// files (`[palette.light] accent = "#d97757"`).
    pub const fn key(self) -> &'static str {
        match self {
            ThemeRole::Canvas => "canvas",
            ThemeRole::Surface => "surface",
            ThemeRole::SurfaceRaised => "surface_raised",
            ThemeRole::SurfaceSunken => "surface_sunken",
            ThemeRole::Border => "border",
            ThemeRole::TextPrimary => "text_primary",
            ThemeRole::TextSecondary => "text_secondary",
            ThemeRole::Accent => "accent",
            ThemeRole::AccentText => "accent_text",
            ThemeRole::Focus => "focus",
            ThemeRole::Danger => "danger",
            ThemeRole::Success => "success",
            ThemeRole::Warning => "warning",
            ThemeRole::Shadow => "shadow",
            ThemeRole::Highlight => "highlight",
            ThemeRole::Panel => "panel",
            ThemeRole::Ink => "ink",
            ThemeRole::AccentSoft => "accent_soft",
            ThemeRole::AccentSoftText => "accent_soft_text",
        }
    }

    /// Look a role up by its [`ThemeRole::key`].
    pub fn from_key(key: &str) -> Option<ThemeRole> {
        ThemeRole::ALL.into_iter().find(|r| r.key() == key)
    }
}

/// The full palette for one theme/scheme combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub canvas: Color,
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_sunken: Color,
    pub border: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub focus: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub shadow: Color,
    pub highlight: Color,
    pub panel: Color,
    pub ink: Color,
    pub accent_soft: Color,
    pub accent_soft_text: Color,
}

impl Palette {
    /// Look up a role by enum.
    pub fn get(&self, role: ThemeRole) -> Color {
        *self.slot(role)
    }

    /// Replace one role.
    pub fn set(&mut self, role: ThemeRole, color: Color) {
        *self.slot_mut(role) = color;
    }

    fn slot(&self, role: ThemeRole) -> &Color {
        match role {
            ThemeRole::Canvas => &self.canvas,
            ThemeRole::Surface => &self.surface,
            ThemeRole::SurfaceRaised => &self.surface_raised,
            ThemeRole::SurfaceSunken => &self.surface_sunken,
            ThemeRole::Border => &self.border,
            ThemeRole::TextPrimary => &self.text_primary,
            ThemeRole::TextSecondary => &self.text_secondary,
            ThemeRole::Accent => &self.accent,
            ThemeRole::AccentText => &self.accent_text,
            ThemeRole::Focus => &self.focus,
            ThemeRole::Danger => &self.danger,
            ThemeRole::Success => &self.success,
            ThemeRole::Warning => &self.warning,
            ThemeRole::Shadow => &self.shadow,
            ThemeRole::Highlight => &self.highlight,
            ThemeRole::Panel => &self.panel,
            ThemeRole::Ink => &self.ink,
            ThemeRole::AccentSoft => &self.accent_soft,
            ThemeRole::AccentSoftText => &self.accent_soft_text,
        }
    }

    fn slot_mut(&mut self, role: ThemeRole) -> &mut Color {
        match role {
            ThemeRole::Canvas => &mut self.canvas,
            ThemeRole::Surface => &mut self.surface,
            ThemeRole::SurfaceRaised => &mut self.surface_raised,
            ThemeRole::SurfaceSunken => &mut self.surface_sunken,
            ThemeRole::Border => &mut self.border,
            ThemeRole::TextPrimary => &mut self.text_primary,
            ThemeRole::TextSecondary => &mut self.text_secondary,
            ThemeRole::Accent => &mut self.accent,
            ThemeRole::AccentText => &mut self.accent_text,
            ThemeRole::Focus => &mut self.focus,
            ThemeRole::Danger => &mut self.danger,
            ThemeRole::Success => &mut self.success,
            ThemeRole::Warning => &mut self.warning,
            ThemeRole::Shadow => &mut self.shadow,
            ThemeRole::Highlight => &mut self.highlight,
            ThemeRole::Panel => &mut self.panel,
            ThemeRole::Ink => &mut self.ink,
            ThemeRole::AccentSoft => &mut self.accent_soft,
            ThemeRole::AccentSoftText => &mut self.accent_soft_text,
        }
    }

    /// Iterate all roles with their values.
    pub fn iter(&self) -> impl Iterator<Item = (ThemeRole, Color)> + '_ {
        ThemeRole::ALL.into_iter().map(move |r| (r, self.get(r)))
    }
}

/// Smallest allowed touch target in logical px (Material/Apple guidance is 44-48).
pub const MIN_TOUCH_TARGET_PX: i32 = 48;

/// Numeric design tokens (sizes, radii, strokes, motion timings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeMetrics {
    /// Taskbar thickness (Clay: 40 px).
    pub panel_height: i32,
    /// Button/control corner radius (Clay: 5 px).
    pub control_radius: i32,
    pub menu_radius: i32,
    pub window_radius: i32,
    pub overlay_radius: i32,
    /// Button height inside the taskbar (Clay: 28 px).
    pub control_height: i32,
    /// Minimum touch hit size in the mobile layout (never below 48).
    pub touch_target: i32,
    pub panel_padding: i32,
    pub spacing: i32,
    pub font_size_pt: f64,
    pub motion_micro_ms: u32,
    pub motion_menu_ms: u32,
    pub motion_workspace_ms: u32,
    /// Hairline border width in px (Clay: 1.0).
    pub stroke_width: f64,
    /// Icon/glyph stroke width in px (Clay: 1.25).
    pub icon_stroke: f64,
    /// Glyph box size in px.
    pub icon_size: i32,
    /// Width of a labelled task button (mockup: 128 px).
    pub task_button_width: i32,
    /// Width of an icon-only task or pinned button.
    pub task_icon_width: i32,
    /// Height of the active-window accent underline.
    pub underline_height: i32,
}

/// Font families and sizes. Families are CSS family lists; the generic
/// fallback (`sans-serif`, `serif`, `monospace`) is always appended.
#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    /// UI text (labels, menus). Clay: Inter, then installed Noto Sans.
    pub ui_family: String,
    /// Display text (clock, start button word, headings). Clay: a serif.
    pub display_family: String,
    pub mono_family: String,
    /// Clock time size in pt.
    pub clock_size_pt: f64,
    /// Small secondary text (clock date) in pt.
    pub small_size_pt: f64,
    /// UI weight for active/emphasised labels (CSS numeric weight).
    pub emphasis_weight: u16,
    /// Start button word: `true` = display serif, `false` = UI sans.
    pub start_label_serif: bool,
    /// Start button word weight (CSS numeric weight).
    pub start_label_weight: u16,
    /// Start button word size in px.
    pub start_label_px: f64,
}

/// One drop shadow. Clay shadows are hard offsets: `blur == 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub x: i32,
    pub y: i32,
    pub blur: i32,
    pub spread: i32,
}

impl Shadow {
    pub const NONE: Shadow = Shadow {
        x: 0,
        y: 0,
        blur: 0,
        spread: 0,
    };

    pub fn is_none(&self) -> bool {
        self.x == 0 && self.y == 0 && self.blur == 0 && self.spread == 0
    }
}

/// Shadow tokens for each elevation. The colour is [`Palette::shadow`] scaled
/// by `strength` (0 = no shadows, 1 = theme default, 2 = maximum).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadows {
    pub strength: f64,
    /// Raised buttons (Clay: none, paper is flat).
    pub button: Shadow,
    /// Popovers, menus, the hover card.
    pub popover: Shadow,
    /// Windows / large sheets (compositor frames use this too).
    pub window: Shadow,
}

/// Highest shadow alpha the emitter will produce after `strength` scaling, so
/// no override can create a heavy offset shadow.
pub const MAX_SHADOW_ALPHA: f64 = 0.35;

impl Shadows {
    /// Every elevation, for checks.
    pub fn all(&self) -> [Shadow; 3] {
        [self.button, self.popover, self.window]
    }

    /// The effective shadow colour for a palette.
    pub fn color(&self, palette: &Palette) -> Color {
        let a =
            (palette.shadow.a as f64 / 255.0 * self.strength.clamp(0.0, 2.0)).min(MAX_SHADOW_ALPHA);
        palette.shadow.with_alpha((a * 255.0).round() as u8)
    }
}

/// A complete, ready-to-render theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeTokens {
    pub kind: ThemeKind,
    pub scheme: ColorScheme,
    pub palette: Palette,
    pub metrics: ThemeMetrics,
    pub typography: Typography,
    pub shadows: Shadows,
}

// ---------------------------------------------------------------------------
// Palette definitions
// ---------------------------------------------------------------------------

fn clay_light() -> Palette {
    // Cut paper: warm cream sheets, hairline warm grey-brown strokes, ink text,
    // terracotta accent (approved mockup taskbar-v2).
    Palette {
        canvas: Color::rgb(0xF3, 0xEF, 0xE6),
        surface: Color::rgb(0xFA, 0xF9, 0xF5),
        surface_raised: Color::rgb(0xFC, 0xFB, 0xF7),
        surface_sunken: Color::rgb(0xEC, 0xE7, 0xDB),
        border: Color::rgb(0xCF, 0xC6, 0xB4),
        text_primary: Color::rgb(0x2B, 0x29, 0x26),
        text_secondary: Color::rgb(0x6B, 0x66, 0x5C),
        // Brand terracotta as a fill; dark text on it clears WCAG AA (5.6:1).
        accent: Color::rgb(0xD9, 0x77, 0x57),
        accent_text: Color::rgb(0x2B, 0x1A, 0x12),
        focus: Color::rgb(0x9A, 0x42, 0x22),
        danger: Color::rgb(0xB0, 0x3A, 0x2E),
        success: Color::rgb(0x2F, 0x6B, 0x3F),
        warning: Color::rgb(0x8A, 0x5A, 0x12),
        // Warm, faint, tone-on-tone.
        shadow: Color::rgba(0x4A, 0x36, 0x20, 0x24),
        highlight: Color::rgba(0xFF, 0xFF, 0xFF, 0xC8),
        panel: Color::rgb(0xF5, 0xF2, 0xEA),
        ink: Color::rgb(0x3A, 0x37, 0x33),
        accent_soft: Color::rgb(0xF2, 0xD5, 0xC6),
        accent_soft_text: Color::rgb(0x7A, 0x3A, 0x22),
    }
}

fn clay_dark() -> Palette {
    // Warm charcoal paper (never a cold blue-black): #2B2926 bar, #33302C
    // raised sheets, #4A4640 hairlines, cream ink #F3EFE6; terracotta is
    // used sparingly (underline, tinted selections, the Start mark).
    Palette {
        canvas: Color::rgb(0x1F, 0x1E, 0x1C),
        surface: Color::rgb(0x2F, 0x2C, 0x29),
        surface_raised: Color::rgb(0x33, 0x30, 0x2C),
        surface_sunken: Color::rgb(0x24, 0x22, 0x1F),
        border: Color::rgb(0x4A, 0x46, 0x40),
        text_primary: Color::rgb(0xF3, 0xEF, 0xE6),
        text_secondary: Color::rgb(0xB3, 0xAC, 0x9F),
        accent: Color::rgb(0xD9, 0x77, 0x57),
        accent_text: Color::rgb(0x1C, 0x1B, 0x19),
        focus: Color::rgb(0xE8, 0x99, 0x7B),
        danger: Color::rgb(0xEE, 0x8A, 0x7C),
        success: Color::rgb(0x8B, 0xC7, 0x9A),
        warning: Color::rgb(0xE0, 0xB3, 0x5A),
        shadow: Color::rgba(0x0E, 0x0B, 0x08, 0x4C),
        highlight: Color::rgba(0xFF, 0xFF, 0xFF, 0x0F),
        panel: Color::rgb(0x2B, 0x29, 0x26),
        ink: Color::rgb(0xF3, 0xEF, 0xE6),
        accent_soft: Color::rgb(0x4A, 0x33, 0x28),
        accent_soft_text: Color::rgb(0xF4, 0xC9, 0xB4),
    }
}

fn classic_light() -> Palette {
    Palette {
        canvas: Color::rgb(0xD4, 0xD0, 0xC8),
        surface: Color::rgb(0xD4, 0xD0, 0xC8),
        surface_raised: Color::rgb(0xEF, 0xED, 0xE8),
        surface_sunken: Color::rgb(0xBF, 0xBB, 0xB2),
        border: Color::rgb(0x80, 0x80, 0x80),
        text_primary: Color::rgb(0x00, 0x00, 0x00),
        text_secondary: Color::rgb(0x40, 0x40, 0x40),
        accent: Color::rgb(0x0A, 0x24, 0x6A),
        accent_text: Color::rgb(0xFF, 0xFF, 0xFF),
        focus: Color::rgb(0x31, 0x6A, 0xC5),
        danger: Color::rgb(0xA8, 0x00, 0x00),
        success: Color::rgb(0x00, 0x64, 0x00),
        warning: Color::rgb(0x80, 0x60, 0x00),
        shadow: Color::rgba(0x00, 0x00, 0x00, 0x3C),
        highlight: Color::rgba(0xFF, 0xFF, 0xFF, 0xE6),
        panel: Color::rgb(0xD4, 0xD0, 0xC8),
        ink: Color::rgb(0x00, 0x00, 0x00),
        accent_soft: Color::rgb(0xC6, 0xCF, 0xE4),
        accent_soft_text: Color::rgb(0x0A, 0x24, 0x6A),
    }
}

fn classic_dark() -> Palette {
    Palette {
        canvas: Color::rgb(0x2B, 0x2B, 0x2B),
        surface: Color::rgb(0x33, 0x33, 0x33),
        surface_raised: Color::rgb(0x3D, 0x3D, 0x3D),
        surface_sunken: Color::rgb(0x20, 0x20, 0x20),
        border: Color::rgb(0x6E, 0x6E, 0x6E),
        text_primary: Color::rgb(0xE6, 0xE6, 0xE6),
        text_secondary: Color::rgb(0xA8, 0xA8, 0xA8),
        accent: Color::rgb(0x3B, 0x5B, 0xA5),
        accent_text: Color::rgb(0xFF, 0xFF, 0xFF),
        focus: Color::rgb(0x6C, 0x8F, 0xD6),
        danger: Color::rgb(0xF0, 0x9A, 0x9A),
        success: Color::rgb(0x6F, 0xCF, 0x6F),
        warning: Color::rgb(0xE0, 0xC0, 0x70),
        shadow: Color::rgba(0x00, 0x00, 0x00, 0x60),
        highlight: Color::rgba(0xFF, 0xFF, 0xFF, 0x28),
        panel: Color::rgb(0x33, 0x33, 0x33),
        ink: Color::rgb(0xE6, 0xE6, 0xE6),
        accent_soft: Color::rgb(0x2A, 0x38, 0x5C),
        accent_soft_text: Color::rgb(0xDC, 0xE4, 0xF8),
    }
}

impl ThemeMetrics {
    /// Clay metrics (mockup taskbar-v2: 40 px bar, 28 px buttons, 4-6 px radii).
    pub fn clay() -> Self {
        ThemeMetrics {
            panel_height: 40,
            control_radius: 5,
            menu_radius: 6,
            window_radius: 4,
            overlay_radius: 6,
            control_height: 28,
            touch_target: 48,
            panel_padding: 6,
            spacing: 8,
            font_size_pt: 9.0,
            motion_micro_ms: 140,
            motion_menu_ms: 180,
            motion_workspace_ms: 260,
            stroke_width: 1.0,
            icon_stroke: 1.5,
            icon_size: 18,
            task_button_width: 128,
            task_icon_width: 36,
            underline_height: 2,
        }
    }

    /// Classic metrics: square, bevelled, a touch tighter.
    pub fn classic() -> Self {
        ThemeMetrics {
            panel_height: 36,
            control_radius: 2,
            menu_radius: 2,
            window_radius: 2,
            overlay_radius: 2,
            control_height: 28,
            touch_target: 48,
            panel_padding: 4,
            spacing: 4,
            font_size_pt: 9.0,
            motion_micro_ms: 90,
            motion_menu_ms: 120,
            motion_workspace_ms: 180,
            stroke_width: 1.0,
            icon_stroke: 1.25,
            icon_size: 18,
            task_button_width: 150,
            task_icon_width: 36,
            underline_height: 0,
        }
    }

    /// Scale all absolute pixel metrics (for HiDPI when toolkit scaling is off).
    pub fn scaled(&self, factor: f64) -> Self {
        let f = factor.max(0.5);
        let px = |v: i32| ((v as f64) * f).round() as i32;
        ThemeMetrics {
            panel_height: px(self.panel_height),
            control_radius: px(self.control_radius),
            menu_radius: px(self.menu_radius),
            window_radius: px(self.window_radius),
            overlay_radius: px(self.overlay_radius),
            control_height: px(self.control_height),
            touch_target: px(self.touch_target).max(MIN_TOUCH_TARGET_PX),
            panel_padding: px(self.panel_padding),
            spacing: px(self.spacing),
            font_size_pt: self.font_size_pt * f,
            stroke_width: self.stroke_width * f,
            icon_stroke: self.icon_stroke * f,
            icon_size: px(self.icon_size),
            task_button_width: px(self.task_button_width),
            task_icon_width: px(self.task_icon_width),
            underline_height: px(self.underline_height),
            ..*self
        }
    }
}

impl Typography {
    pub fn clay() -> Self {
        Typography {
            ui_family: "\"Inter\", \"Noto Sans\", \"Cantarell\"".into(),
            display_family: "\"Source Serif 4\", \"Noto Serif\", \"DejaVu Serif\"".into(),
            mono_family: "\"JetBrains Mono\", \"Noto Sans Mono\", \"DejaVu Sans Mono\"".into(),
            clock_size_pt: 9.75,
            small_size_pt: 7.0,
            emphasis_weight: 600,
            start_label_serif: false,
            start_label_weight: 600,
            start_label_px: 13.0,
        }
    }

    pub fn classic() -> Self {
        Typography {
            ui_family: "\"DejaVu Sans\", \"Noto Sans\"".into(),
            display_family: "\"DejaVu Sans\", \"Noto Sans\"".into(),
            mono_family: "\"DejaVu Sans Mono\", \"Noto Sans Mono\"".into(),
            clock_size_pt: 9.0,
            small_size_pt: 7.0,
            emphasis_weight: 700,
            start_label_serif: false,
            start_label_weight: 700,
            start_label_px: 12.0,
        }
    }

    /// The UI stack with its generic fallback.
    pub fn ui_stack(&self) -> String {
        format!("{}, sans-serif", self.ui_family)
    }

    /// The display stack with its generic fallback (Clay: serif).
    pub fn display_stack(&self, kind: ThemeKind) -> String {
        match kind {
            ThemeKind::Clay => format!("{}, serif", self.display_family),
            ThemeKind::Classic => format!("{}, sans-serif", self.display_family),
        }
    }

    pub fn mono_stack(&self) -> String {
        format!("{}, monospace", self.mono_family)
    }
}

impl Shadows {
    pub fn clay() -> Self {
        Shadows {
            strength: 1.0,
            button: Shadow::NONE,
            popover: Shadow {
                x: 2,
                y: 2,
                blur: 0,
                spread: 0,
            },
            window: Shadow {
                x: 3,
                y: 3,
                blur: 0,
                spread: 0,
            },
        }
    }

    pub fn classic() -> Self {
        Shadows {
            strength: 1.0,
            button: Shadow::NONE,
            popover: Shadow {
                x: 1,
                y: 2,
                blur: 3,
                spread: 0,
            },
            window: Shadow {
                x: 2,
                y: 3,
                blur: 6,
                spread: 0,
            },
        }
    }
}

impl ThemeTokens {
    /// Build the built-in tokens for a theme kind and colour scheme.
    pub fn for_kind(kind: ThemeKind, scheme: ColorScheme) -> Self {
        let palette = match (kind, scheme) {
            (ThemeKind::Clay, ColorScheme::Light) => clay_light(),
            (ThemeKind::Clay, ColorScheme::Dark) => clay_dark(),
            (ThemeKind::Classic, ColorScheme::Light) => classic_light(),
            (ThemeKind::Classic, ColorScheme::Dark) => classic_dark(),
        };
        let (metrics, typography, shadows) = match kind {
            ThemeKind::Clay => (ThemeMetrics::clay(), Typography::clay(), Shadows::clay()),
            ThemeKind::Classic => (
                ThemeMetrics::classic(),
                Typography::classic(),
                Shadows::classic(),
            ),
        };
        ThemeTokens {
            kind,
            scheme,
            palette,
            metrics,
            typography,
            shadows,
        }
    }

    /// Glaze Clay light — the default.
    pub fn default_clay() -> Self {
        Self::for_kind(ThemeKind::Clay, ColorScheme::Light)
    }

    /// Sans stack for UI text. Generic families are the guaranteed fallback.
    pub fn ui_font_stack(&self) -> String {
        self.typography.ui_stack()
    }

    /// Serif stack for display type (Clay); Classic stays sans.
    pub fn display_font_stack(&self) -> String {
        self.typography.display_stack(self.kind)
    }

    /// A short, human label such as `Glaze Clay (light)`.
    pub fn label(&self) -> String {
        format!("{} ({})", self.kind.label(), self.scheme.label())
    }

    /// The best of `dark` / `light` text for a fill, by WCAG contrast. Used when
    /// a user overrides an accent without choosing its text colour.
    pub fn best_text_on(fill: Color, dark: Color, light: Color) -> Color {
        if dark.contrast_ratio(fill) >= light.contrast_ratio(fill) {
            dark
        } else {
            light
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_tokens() -> Vec<ThemeTokens> {
        let mut v = Vec::new();
        for kind in ThemeKind::ALL {
            for scheme in ColorScheme::ALL {
                v.push(ThemeTokens::for_kind(kind, scheme));
            }
        }
        v
    }

    #[test]
    fn every_role_has_a_distinct_key_and_round_trips() {
        let mut keys: Vec<&str> = ThemeRole::ALL.iter().map(|r| r.key()).collect();
        keys.sort_unstable();
        let len = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), len, "role keys must be unique");
        for role in ThemeRole::ALL {
            assert_eq!(ThemeRole::from_key(role.key()), Some(role));
        }
        assert_eq!(ThemeRole::from_key("nope"), None);
    }

    #[test]
    fn palette_set_and_get_agree() {
        let mut p = clay_light();
        for role in ThemeRole::ALL {
            p.set(role, Color::rgb(1, 2, 3));
            assert_eq!(p.get(role), Color::rgb(1, 2, 3));
        }
    }

    /// Brief requirement: Clay shadows never blur.
    #[test]
    fn clay_shadows_have_zero_blur_and_are_faint_and_warm() {
        for scheme in ColorScheme::ALL {
            let t = ThemeTokens::for_kind(ThemeKind::Clay, scheme);
            for s in t.shadows.all() {
                assert_eq!(s.blur, 0, "Clay shadow must not blur: {s:?}");
                assert!(s.x.abs() <= 3 && s.y.abs() <= 3, "offset too large: {s:?}");
            }
            // No heavy black: even at maximum strength the alpha is capped.
            let mut max = t.shadows;
            max.strength = 2.0;
            let c = max.color(&t.palette);
            assert!(c.a as f64 / 255.0 <= MAX_SHADOW_ALPHA + 0.003);
            if scheme == ColorScheme::Light {
                assert!(c.r > c.b, "light Clay shadow must be warm, got {c:?}");
                assert!(t.shadows.color(&t.palette).a as f64 / 255.0 <= 0.15);
            }
        }
    }

    #[test]
    fn clay_strokes_are_hairlines_and_metrics_match_the_mockup() {
        let m = ThemeMetrics::clay();
        assert!((1.0..=1.25).contains(&m.stroke_width));
        // Ink icons: 1.5 px (the asset default) after user feedback that
        // 1.25 px pinned/tray icons read too thin; still well below "heavy".
        assert!((1.0..=1.5).contains(&m.icon_stroke));
        assert_eq!(m.panel_height, 40);
        assert_eq!(m.control_height, 28);
        assert!((4..=6).contains(&m.control_radius));
        assert!((4..=6).contains(&m.menu_radius));
        let border = clay_light().border;
        assert!(
            border.r > border.g && border.g > border.b,
            "warm grey-brown"
        );
        assert!(border.relative_luminance() > 0.4, "hairline, not black ink");
    }

    #[test]
    fn clay_is_rounder_than_classic_and_taller() {
        let clay = ThemeTokens::for_kind(ThemeKind::Clay, ColorScheme::Light);
        let classic = ThemeTokens::for_kind(ThemeKind::Classic, ColorScheme::Light);
        assert!(clay.metrics.control_radius > classic.metrics.control_radius);
        assert!(clay.metrics.panel_height > classic.metrics.panel_height);
    }

    /// WCAG 2.2 AA gate for shell text on its own surfaces (`GLAZE.md` §6.5).
    #[test]
    fn shell_text_meets_wcag_aa() {
        for t in all_tokens() {
            let p = &t.palette;
            for (fg, bg, min) in [
                (p.text_primary, p.surface, 4.5),
                (p.text_primary, p.panel, 4.5),
                (p.text_primary, p.surface_raised, 4.5),
                (p.text_secondary, p.panel, 3.0),
                (p.accent_text, p.accent, 4.5),
                (p.accent_soft_text, p.accent_soft, 4.5),
                (p.ink, p.panel, 4.5),
            ] {
                let r = fg.contrast_ratio(bg);
                assert!(r >= min, "{}: {fg} on {bg} = {r:.2} < {min}", t.label());
            }
        }
    }

    #[test]
    fn clay_is_warm_paper_with_terracotta_accent() {
        let l = ThemeTokens::for_kind(ThemeKind::Clay, ColorScheme::Light).palette;
        for c in [l.canvas, l.surface, l.surface_sunken, l.panel] {
            assert!(c.r >= c.g && c.g >= c.b, "paper must be warm: {c:?}");
        }
        assert_eq!(l.accent, Color::rgb(0xD9, 0x77, 0x57));
        let d = ThemeTokens::for_kind(ThemeKind::Clay, ColorScheme::Dark).palette;
        assert_eq!(d.accent, Color::rgb(0xD9, 0x77, 0x57));
        assert!(d.surface.r >= d.surface.b, "dark paper must stay warm");
    }

    #[test]
    fn touch_targets_are_at_least_48px_even_when_scaled_down() {
        for kind in ThemeKind::ALL {
            let m = ThemeTokens::for_kind(kind, ColorScheme::Light).metrics;
            assert!(m.touch_target >= 48);
            assert!(m.scaled(0.5).touch_target >= 48);
        }
    }

    #[test]
    fn scaling_keeps_proportions() {
        let m = ThemeMetrics::clay();
        let s = m.scaled(2.0);
        assert_eq!(s.panel_height, 80);
        assert_eq!(s.control_radius, 10);
        assert!((s.stroke_width - 2.0).abs() < 1e-9);
    }

    #[test]
    fn best_text_on_picks_the_higher_contrast() {
        let dark = Color::rgb(0x2B, 0x1A, 0x12);
        let light = Color::rgb(0xFF, 0xFF, 0xFF);
        assert_eq!(
            ThemeTokens::best_text_on(Color::rgb(0xF0, 0xE0, 0xA0), dark, light),
            dark
        );
        assert_eq!(
            ThemeTokens::best_text_on(Color::rgb(0x10, 0x20, 0x60), dark, light),
            light
        );
    }

    #[test]
    fn font_stacks_end_in_generic_families() {
        let clay = ThemeTokens::default_clay();
        assert!(clay.ui_font_stack().ends_with("sans-serif"));
        assert!(clay.display_font_stack().ends_with(", serif"));
        assert!(clay.ui_font_stack().contains("Inter"));
        assert!(clay.ui_font_stack().contains("Noto Sans"));
        assert!(clay.display_font_stack().contains("Noto Serif"));
    }
}
