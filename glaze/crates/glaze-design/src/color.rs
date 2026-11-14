//! Colors with precise serialisation for GTK CSS and honest WCAG contrast maths.
//!
//! Every theme role in [`crate::theme`] is one of these values so that no raw
//! colour literal ever leaks into an individual widget (see `GLAZE.md` §3.4).
//! Moved here from `glaze-common::color` (which re-exports it) in design system v2.

use std::fmt;

/// An 8-bit RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Error produced when parsing a colour from a hexadecimal string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorParseError {
    Empty,
    BadLength(usize),
    BadDigit(char),
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColorParseError::Empty => write!(f, "empty colour string"),
            ColorParseError::BadLength(n) => {
                write!(f, "expected 3, 6 or 8 hex digits but found {n}")
            }
            ColorParseError::BadDigit(c) => write!(f, "invalid hex digit '{c}'"),
        }
    }
}

impl std::error::Error for ColorParseError {}

impl Color {
    /// Fully opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Colour with explicit alpha (0 transparent .. 255 opaque).
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (the leading `#` is optional).
    pub fn from_hex(input: &str) -> Result<Self, ColorParseError> {
        let s = input.trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.is_empty() {
            return Err(ColorParseError::Empty);
        }
        for c in s.chars() {
            if !c.is_ascii_hexdigit() {
                return Err(ColorParseError::BadDigit(c));
            }
        }
        let nibble = |c: u8| -> u8 { c.to_ascii_lowercase() };
        match s.len() {
            3 => {
                let b = s.as_bytes();
                let dup = |c: u8| -> u8 {
                    let v = (nibble(c) as char).to_digit(16).unwrap() as u8;
                    v * 16 + v
                };
                Ok(Color::rgb(dup(b[0]), dup(b[1]), dup(b[2])))
            }
            6 => {
                let v = u32::from_str_radix(s, 16).map_err(|_| ColorParseError::BadDigit('?'))?;
                Ok(Color::rgb(
                    ((v >> 16) & 0xff) as u8,
                    ((v >> 8) & 0xff) as u8,
                    (v & 0xff) as u8,
                ))
            }
            8 => {
                let v = u64::from_str_radix(s, 16).map_err(|_| ColorParseError::BadDigit('?'))?;
                Ok(Color::rgba(
                    ((v >> 24) & 0xff) as u8,
                    ((v >> 16) & 0xff) as u8,
                    ((v >> 8) & 0xff) as u8,
                    (v & 0xff) as u8,
                ))
            }
            other => Err(ColorParseError::BadLength(other)),
        }
    }

    /// Same colour with a different alpha.
    pub const fn with_alpha(self, a: u8) -> Self {
        Color { a, ..self }
    }

    /// Fully opaque RGB representation, discarding alpha.
    pub const fn to_rgb(self) -> Self {
        Color { a: 255, ..self }
    }

    /// Serialise as `#rrggbb` or `#rrggbbaa` (lowercase, alpha only when < 255).
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Serialise as a CSS/GTK `rgba(r, g, b, a)` string (alpha in `0.0..=1.0`).
    pub fn to_css(self) -> String {
        let a = self.a as f64 / 255.0;
        // Trim float noise: 3 decimal places is plenty for a colour channel.
        format!("rgba({}, {}, {}, {:.3})", self.r, self.g, self.b, a)
    }

    /// Blend toward `other` by `t` in `0.0..=1.0` (0 = self, 1 = other).
    pub fn mix(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| -> u8 {
            (a as f32 + (b as f32 - a as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Color {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }

    /// Move toward white by `t` in `0.0..=1.0`.
    pub fn lighten(self, t: f32) -> Color {
        self.mix(Color::rgb(255, 255, 255), t)
    }

    /// Move toward black by `t` in `0.0..=1.0`.
    pub fn darken(self, t: f32) -> Color {
        self.mix(Color::rgb(0, 0, 0), t)
    }

    /// Source-over composite of `self` (with its alpha, further scaled by
    /// `opacity` in `0.0..=1.0`) onto an opaque `backdrop`. The result is opaque.
    /// Used to check text contrast against textured or translucent surfaces.
    pub fn over(self, backdrop: Color, opacity: f32) -> Color {
        let a = (self.a as f32 / 255.0) * opacity.clamp(0.0, 1.0);
        backdrop.to_rgb().mix(self.to_rgb(), a)
    }

    /// WCAG 2.x relative luminance (alpha ignored).
    pub fn relative_luminance(self) -> f64 {
        fn channel(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG 2.x contrast ratio between two colours (1.0 .. 21.0).
    pub fn contrast_ratio(self, other: Color) -> f64 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();
        let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_digit_hex() {
        assert_eq!(
            Color::from_hex("#4C6FFF").unwrap(),
            Color::rgb(0x4c, 0x6f, 0xff)
        );
        assert_eq!(
            Color::from_hex("4c6fff").unwrap(),
            Color::rgb(0x4c, 0x6f, 0xff)
        );
    }

    #[test]
    fn parses_short_and_alpha_hex() {
        assert_eq!(Color::from_hex("#fff").unwrap(), Color::rgb(255, 255, 255));
        assert_eq!(
            Color::from_hex("#00000080").unwrap(),
            Color::rgba(0, 0, 0, 0x80)
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Color::from_hex("").unwrap_err(), ColorParseError::Empty);
        assert_eq!(
            Color::from_hex("#12345").unwrap_err(),
            ColorParseError::BadLength(5)
        );
        assert!(matches!(
            Color::from_hex("#zzzzzz").unwrap_err(),
            ColorParseError::BadDigit(_)
        ));
    }

    #[test]
    fn round_trips_hex() {
        for s in ["#000000", "#ffffff", "#4c6fff", "#00000080", "#ff0000ff"] {
            let c = Color::from_hex(s).unwrap();
            assert_eq!(Color::from_hex(&c.to_hex()).unwrap(), c);
        }
    }

    #[test]
    fn css_is_gtk_parseable_shape() {
        let css = Color::rgba(10, 20, 30, 128).to_css();
        assert!(css.starts_with("rgba(10, 20, 30,"));
        assert!(css.ends_with(')'));
    }

    #[test]
    fn contrast_matches_wcag_reference() {
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        // The canonical black/white ratio is 21:1.
        assert!((black.contrast_ratio(white) - 21.0).abs() < 0.01);
        assert!((black.contrast_ratio(black) - 1.0).abs() < 0.001);
    }

    #[test]
    fn over_composites_with_alpha() {
        let paper = Color::rgb(250, 250, 250);
        let ink = Color::rgba(0, 0, 0, 255);
        assert_eq!(ink.over(paper, 0.0), paper);
        assert_eq!(ink.over(paper, 1.0), Color::rgb(0, 0, 0));
        let half = Color::rgba(0, 0, 0, 128).over(paper, 1.0);
        assert!(half.r > 120 && half.r < 130 && half.a == 255);
    }

    #[test]
    fn mix_endpoints_are_exact() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(255, 128, 64);
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
    }
}
