//! AMOLED variant of the dark scheme: true-black (`#000000`) backgrounds so
//! OLED pixels switch off, with elevation expressed by hairlines and faint
//! near-black lifts instead of shadows, and accents/status colours lifted
//! until they keep their contrast against black.
//!
//! Only meaningful for [`ColorScheme::Dark`]; [`apply`] is a no-op otherwise.

use crate::color::Color;
use crate::kinds::ColorScheme;
use crate::overrides::derive_accent_text;
use crate::theme::ThemeTokens;

/// Pure black.
pub const BLACK: Color = Color::rgb(0, 0, 0);

/// Minimum contrast of text/ink against black.
const TEXT_MIN: f64 = 7.0;
/// Minimum contrast of accent/status colours against black.
const COLOUR_MIN: f64 = 4.5;

/// Lighten `c` toward white until it reaches `min` contrast against `bg`.
fn ensure_contrast(c: Color, bg: Color, min: f64) -> Color {
    let mut out = c;
    for step in 1..=20 {
        if out.contrast_ratio(bg) >= min {
            break;
        }
        out = c.lighten(step as f32 * 0.05);
    }
    out
}

/// Turn dark `tokens` into the AMOLED variant. No-op for the light scheme.
pub fn apply(tokens: &mut ThemeTokens) {
    if tokens.scheme != ColorScheme::Dark {
        return;
    }
    let p = &mut tokens.palette;
    let white = Color::rgb(255, 255, 255);

    // Backgrounds: everything that fills a large area is true black. Popovers
    // and raised sheets get a faint lift so a menu over a window still reads.
    p.canvas = BLACK;
    p.panel = BLACK;
    p.surface = BLACK;
    p.surface_sunken = BLACK;
    p.surface_raised = BLACK.mix(white, 0.07);

    // Elevation by hairline: stronger border than the stock dark palette
    // (shadows vanish on black).
    p.border = p.border.lighten(0.22);
    p.shadow = Color::rgba(0, 0, 0, 0);
    p.highlight = Color::rgba(255, 255, 255, 0x1A);
    tokens.shadows.strength = 0.0;

    // Text and ink: AAA against black.
    p.text_primary = ensure_contrast(p.text_primary, BLACK, TEXT_MIN);
    p.ink = ensure_contrast(p.ink, BLACK, TEXT_MIN);
    p.text_secondary = ensure_contrast(p.text_secondary, BLACK, COLOUR_MIN + 1.0);

    // Accent and status colours.
    p.accent = ensure_contrast(p.accent, BLACK, COLOUR_MIN);
    p.focus = ensure_contrast(p.focus, BLACK, COLOUR_MIN);
    p.danger = ensure_contrast(p.danger, BLACK, COLOUR_MIN);
    p.success = ensure_contrast(p.success, BLACK, COLOUR_MIN);
    p.warning = ensure_contrast(p.warning, BLACK, COLOUR_MIN);
    derive_accent_text(tokens);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::ThemeKind;
    use crate::theme::ThemeRole;

    fn amoled(kind: ThemeKind) -> ThemeTokens {
        let mut t = ThemeTokens::for_kind(kind, ColorScheme::Dark);
        apply(&mut t);
        t
    }

    #[test]
    fn backgrounds_are_pure_black() {
        for kind in [ThemeKind::Clay, ThemeKind::Classic] {
            let t = amoled(kind);
            for r in [
                ThemeRole::Canvas,
                ThemeRole::Panel,
                ThemeRole::Surface,
                ThemeRole::SurfaceSunken,
            ] {
                assert_eq!(t.palette.get(r), BLACK, "{kind:?} {r:?}");
            }
            assert_ne!(t.palette.surface_raised, BLACK);
        }
    }

    #[test]
    fn contrast_holds_on_black() {
        for kind in [ThemeKind::Clay, ThemeKind::Classic] {
            let t = amoled(kind);
            let p = &t.palette;
            for (r, min) in [
                (ThemeRole::TextPrimary, 7.0),
                (ThemeRole::Ink, 7.0),
                (ThemeRole::TextSecondary, 5.5),
                (ThemeRole::Accent, 4.5),
                (ThemeRole::Danger, 4.5),
                (ThemeRole::Success, 4.5),
                (ThemeRole::Warning, 4.5),
                (ThemeRole::Focus, 4.5),
            ] {
                assert!(p.get(r).contrast_ratio(BLACK) >= min, "{kind:?} {r:?}");
            }
            assert!(p.accent_text.contrast_ratio(p.accent) >= 4.5, "{kind:?}");
            assert!(p.text_primary.contrast_ratio(p.surface_raised) >= 7.0);
            assert!(p.border.contrast_ratio(BLACK) >= 1.5, "{kind:?} border");
            assert_eq!(t.shadows.strength, 0.0);
        }
    }

    #[test]
    fn light_scheme_untouched() {
        let base = ThemeTokens::for_kind(ThemeKind::Clay, ColorScheme::Light);
        let mut t = base.clone();
        apply(&mut t);
        assert_eq!(t.palette, base.palette);
    }
}
