//! Input validation for free-text controls, with messages meant to be shown
//! inline under the control.

use glaze_common::config::GlazeConfig;
use glaze_common::config_check;
use glaze_design::overrides::contrast_issues;
use glaze_design::{Color, ColorScheme, ThemeKind};

/// Normalise a user-typed colour to `#rrggbb` (alpha is not allowed for an
/// accent).
pub fn accent_hex(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("type a colour such as #d97757".into());
    }
    let with_hash = if s.starts_with('#') {
        s.to_string()
    } else {
        format!("#{s}")
    };
    let c = Color::from_hex(&with_hash).map_err(|e| format!("not a colour: {e}"))?;
    if c.a != 255 {
        return Err("the accent must be opaque (no alpha)".into());
    }
    Ok(c.to_rgb().to_hex())
}

/// Contrast warnings for an accent in the user's current theme and in the
/// other scheme (an accent is shared by light and dark).
pub fn accent_warnings(cfg: &GlazeConfig, hex: &str) -> Vec<String> {
    let mut out = Vec::new();
    for scheme in ColorScheme::ALL {
        let mut c = cfg.clone();
        c.appearance.accent = Some(hex.to_string());
        c.appearance.scheme = scheme;
        let (tokens, _) = config_check::resolve_tokens(&c, None);
        for issue in contrast_issues(&tokens) {
            if issue.path.contains("accent") {
                out.push(format!("{}: {}", scheme.label(), issue.message));
            }
        }
        // The active-window underline and the start button sit on the panel.
        // The underline is not the only active cue (the button is also sunken
        // and bold), so the bar is 2:1 here rather than WCAG 1.4.11's 3:1; the
        // brand terracotta on light paper is 2.7:1.
        let ratio = tokens.palette.accent.contrast_ratio(tokens.palette.panel);
        if ratio < 2.0 {
            out.push(format!(
                "{}: the accent is hard to see on the taskbar ({ratio:.1}:1 against the bar)",
                scheme.label()
            ));
        }
    }
    out.dedup();
    out
}

/// Tokens the clock date format supports (`glaze-shell-core::clock`).
pub const DATE_TOKENS: &[(char, &str)] = &[
    ('a', "short weekday (Wed)"),
    ('A', "weekday (Wednesday)"),
    ('d', "day (11)"),
    ('b', "short month (Nov)"),
    ('m', "month (11)"),
    ('y', "year (26)"),
    ('Y', "year (2026)"),
    ('H', "hour (14)"),
    ('M', "minute (05)"),
    ('S', "second (09)"),
    ('%', "a literal %"),
];

/// Validate a clock date format. Empty means "use the default".
pub fn date_format(s: &str) -> Result<(), String> {
    if s.chars().count() > 40 {
        return Err("keep the format under 40 characters".into());
    }
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '%' {
            match it.next() {
                None => return Err("the format ends with a lone %".into()),
                Some(t) if DATE_TOKENS.iter().any(|(k, _)| *k == t) => {}
                Some(t) => {
                    return Err(format!(
                        "%{t} is not supported; use %a %A %d %b %m %y %Y %H %M %S"
                    ))
                }
            }
        }
    }
    Ok(())
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Sample date for previews: (year, month, day, weekday 0=Mon, h, m, s).
pub type Sample = (i32, u32, u32, usize, u32, u32, u32);

/// Render a date format the way the taskbar clock does.
pub fn preview_date(fmt: &str, d: Sample) -> String {
    let (y, mo, day, wd, h, mi, s) = d;
    let mut out = String::new();
    let mut it = fmt.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('Y') => out.push_str(&format!("{y:04}")),
            Some('y') => out.push_str(&format!("{:02}", y.rem_euclid(100))),
            Some('m') => out.push_str(&format!("{mo:02}")),
            Some('d') => out.push_str(&format!("{day:02}")),
            Some('H') => out.push_str(&format!("{h:02}")),
            Some('M') => out.push_str(&format!("{mi:02}")),
            Some('S') => out.push_str(&format!("{s:02}")),
            Some('a') => out.push_str(&WEEKDAYS[wd % 7][..3]),
            Some('A') => out.push_str(WEEKDAYS[wd % 7]),
            Some('b') => out.push_str(MONTHS[((mo.clamp(1, 12)) - 1) as usize]),
            Some('%') => out.push('%'),
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}

/// A theme name usable as a file name (`[A-Za-z0-9 _-]`, 1..=48 chars).
pub fn theme_name(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err("give the theme a name".into());
    }
    if t.chars().count() > 48 {
        return Err("keep the name under 48 characters".into());
    }
    if let Some(c) = t
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')))
    {
        return Err(format!(
            "\"{c}\" is not allowed; use letters, digits, spaces, - and _"
        ));
    }
    Ok(t.to_string())
}

/// File stem for a theme name (`Warm Clay` -> `warm-clay`).
pub fn theme_slug(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let mut out = String::new();
    for c in s.chars() {
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "theme".into()
    } else {
        out
    }
}

/// Parse an integer within an inclusive range.
#[cfg(test)]
pub fn int_in(s: &str, lo: i64, hi: i64) -> Result<i64, String> {
    let v: i64 = s
        .trim()
        .parse()
        .map_err(|_| format!("type a whole number from {lo} to {hi}"))?;
    if !(lo..=hi).contains(&v) {
        return Err(format!("{v} is outside {lo}–{hi}"));
    }
    Ok(v)
}

/// Font family entered by the user (must not break the CSS it lands in).
pub fn font_family(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err("choose a font".into());
    }
    if !glaze_design::overrides::font_family_is_safe(t) {
        return Err("font names cannot contain ; { } \\ or line breaks".into());
    }
    Ok(t.to_string())
}

/// Quote a single family name for a CSS family list, keeping the theme's
/// fallbacks behind it.
pub fn family_list(primary: &str, kind: ThemeKind, display: bool) -> String {
    let fallback = match (kind, display) {
        (ThemeKind::Clay, true) => "\"Noto Serif\", \"DejaVu Serif\"",
        (ThemeKind::Clay, false) => "\"Noto Sans\", \"Cantarell\"",
        (ThemeKind::Classic, _) => "\"DejaVu Sans\", \"Noto Sans\"",
    };
    format!("\"{}\", {fallback}", primary.replace('"', ""))
}

/// The first family of a CSS family list, unquoted.
pub fn first_family(list: &str) -> String {
    list.split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_hex_normalises_and_rejects() {
        assert_eq!(accent_hex("D97757").unwrap(), "#d97757");
        assert_eq!(accent_hex(" #abc ").unwrap(), "#aabbcc");
        assert!(accent_hex("").is_err());
        assert!(accent_hex("#12").is_err());
        assert!(accent_hex("#d9775780").unwrap_err().contains("opaque"));
        assert!(accent_hex("tomato").is_err());
    }

    #[test]
    fn brand_accent_has_no_warnings_but_pale_accent_does() {
        let cfg = GlazeConfig::default();
        assert!(
            accent_warnings(&cfg, "#d97757").is_empty(),
            "{:?}",
            accent_warnings(&cfg, "#d97757")
        );
        let w = accent_warnings(&cfg, "#f6f1e7");
        assert!(!w.is_empty());
        assert!(w.iter().any(|m| m.contains("taskbar")), "{w:?}");
    }

    #[test]
    fn date_formats() {
        assert!(date_format("%a %d %b").is_ok());
        assert!(date_format("").is_ok());
        assert!(date_format("%Y-%m-%d %%").is_ok());
        assert!(date_format("%q").unwrap_err().contains("%q"));
        assert!(date_format("100%").unwrap_err().contains("lone"));
        let s = (2026, 11, 12, 3, 9, 5, 7);
        assert_eq!(preview_date("%a %d %b", s), "Thu 12 Nov");
        assert_eq!(
            preview_date("%A, %Y-%m-%d %H:%M:%S", s),
            "Thursday, 2026-11-12 09:05:07"
        );
    }

    #[test]
    fn theme_names_and_slugs() {
        assert_eq!(theme_name(" Warm Clay ").unwrap(), "Warm Clay");
        assert!(theme_name("").is_err());
        assert!(theme_name("a/b").unwrap_err().contains("/"));
        assert_eq!(theme_slug("Warm  Clay!"), "warm-clay");
        assert_eq!(theme_slug("///"), "theme");
    }

    #[test]
    fn numbers_and_fonts() {
        assert_eq!(int_in("40", 24, 96).unwrap(), 40);
        assert!(int_in("200", 24, 96).unwrap_err().contains("outside"));
        assert!(int_in("x", 24, 96).is_err());
        assert!(font_family("Noto Serif").is_ok());
        assert!(font_family("a; color: red").is_err());
        assert_eq!(first_family("\"Inter\", \"Noto Sans\""), "Inter");
        assert_eq!(
            family_list("Noto Serif", ThemeKind::Clay, true),
            "\"Noto Serif\", \"Noto Serif\", \"DejaVu Serif\""
        );
    }
}
