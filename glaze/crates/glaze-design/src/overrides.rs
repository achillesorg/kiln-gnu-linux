//! Theme files and token overrides.
//!
//! A **theme file** is a shareable TOML document that overrides any subset of
//! the built-in tokens. The user config embeds the same shape under
//! `[appearance.overrides]`. Load order (later wins):
//!
//! ```text
//! built-in tokens (kind + scheme)  <  theme file  <  user config overrides
//! ```
//!
//! ```toml
//! [meta]
//! name = "Warm Clay"
//! schema = 1
//! base = "clay"            # clay | classic (optional)
//!
//! [palette.light]          # any ThemeRole key, "#rrggbb" or "#rrggbbaa"
//! accent = "#c8643f"
//! [palette.dark]
//! accent = "#e08a6c"
//! [palette]                # applies to both schemes (scheme tables win)
//! border = "#cbbfa9"
//!
//! [metrics]                # see METRIC_RANGES for every key and its range
//! panel_height = 44
//! control_radius = 6
//! stroke_width = 1.25
//!
//! [typography]
//! ui_family = "\"Inter\", \"Noto Sans\""
//! display_family = "\"Noto Serif\""
//! clock_size_pt = 10
//!
//! [shadow]
//! strength = 0.5
//! popover = { x = 2, y = 2, blur = 0, spread = 0 }
//! ```
//!
//! Parsing is done from a `toml::Value` by hand so that **every** bad key or
//! value produces an [`Issue`] with its exact path while the rest of the file
//! still applies.

use std::collections::BTreeMap;

use toml::Value;

use crate::color::Color;
use crate::issues::Issue;
use crate::kinds::{ColorScheme, ThemeKind};
use crate::theme::{Shadow, ThemeMetrics, ThemeRole, ThemeTokens};

/// Current theme-file schema version.
pub const THEME_SCHEMA: i64 = 1;

/// Integer metric keys with their inclusive ranges.
pub const INT_METRICS: &[(&str, i64, i64)] = &[
    ("panel_height", 24, 96),
    ("control_height", 16, 80),
    ("control_radius", 0, 16),
    ("menu_radius", 0, 20),
    ("window_radius", 0, 20),
    ("overlay_radius", 0, 24),
    ("panel_padding", 0, 24),
    ("spacing", 0, 32),
    ("touch_target", 48, 96),
    ("motion_micro_ms", 0, 2000),
    ("motion_menu_ms", 0, 2000),
    ("motion_workspace_ms", 0, 4000),
    ("icon_size", 12, 48),
    ("task_button_width", 60, 400),
    ("task_icon_width", 24, 96),
    ("underline_height", 0, 6),
];

/// Float metric keys with their inclusive ranges.
pub const FLOAT_METRICS: &[(&str, f64, f64)] = &[
    ("font_size_pt", 6.0, 24.0),
    ("stroke_width", 0.0, 3.0),
    ("icon_stroke", 0.5, 3.0),
];

/// Parsed overrides. Every field is optional; absent means "keep the layer
/// below".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeOverrides {
    pub name: Option<String>,
    pub base: Option<ThemeKind>,
    /// Applied to both schemes, before the scheme-specific tables.
    pub palette_any: BTreeMap<&'static str, Color>,
    pub palette_light: BTreeMap<&'static str, Color>,
    pub palette_dark: BTreeMap<&'static str, Color>,
    pub int_metrics: BTreeMap<&'static str, i64>,
    pub float_metrics: BTreeMap<&'static str, f64>,
    pub ui_family: Option<String>,
    pub display_family: Option<String>,
    pub mono_family: Option<String>,
    pub clock_size_pt: Option<f64>,
    pub small_size_pt: Option<f64>,
    pub emphasis_weight: Option<u16>,
    pub shadow_strength: Option<f64>,
    pub shadow_button: Option<Shadow>,
    pub shadow_popover: Option<Shadow>,
    pub shadow_window: Option<Shadow>,
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "a string",
        Value::Integer(_) => "an integer",
        Value::Float(_) => "a number",
        Value::Boolean(_) => "a boolean",
        Value::Datetime(_) => "a date",
        Value::Array(_) => "an array",
        Value::Table(_) => "a table",
    }
}

/// A font family list may not contain characters that could escape the CSS
/// declaration it is written into.
pub fn font_family_is_safe(s: &str) -> bool {
    !s.trim().is_empty()
        && !s
            .chars()
            .any(|c| matches!(c, ';' | '{' | '}' | '\\' | '\n'))
}

impl ThemeOverrides {
    /// Parse a whole theme file.
    pub fn from_toml_str(src: &str) -> (Self, Vec<Issue>) {
        match src.parse::<toml::Table>() {
            Ok(table) => Self::from_table(&table),
            Err(e) => (
                Self::default(),
                vec![Issue::error("", format!("not valid TOML: {}", e.message()))],
            ),
        }
    }

    /// Parse from an already-parsed table (e.g. `[appearance.overrides]`).
    pub fn from_table(table: &toml::Table) -> (Self, Vec<Issue>) {
        let mut o = ThemeOverrides::default();
        let mut issues = Vec::new();
        for (key, value) in table {
            match key.as_str() {
                "meta" => o.parse_meta(value, &mut issues),
                "palette" => o.parse_palette(value, &mut issues),
                "metrics" => o.parse_metrics(value, &mut issues),
                "typography" => o.parse_typography(value, &mut issues),
                "shadow" => o.parse_shadow(value, &mut issues),
                other => issues.push(Issue::warning(
                    other,
                    "unknown section (expected meta, palette, metrics, typography, shadow); ignored",
                )),
            }
        }
        (o, issues)
    }

    fn parse_meta(&mut self, v: &Value, issues: &mut Vec<Issue>) {
        let Some(t) = v.as_table() else {
            issues.push(Issue::error("meta", "must be a table"));
            return;
        };
        for (k, v) in t {
            let path = format!("meta.{k}");
            match k.as_str() {
                "name" => match v.as_str() {
                    Some(s) => self.name = Some(s.to_string()),
                    None => issues.push(Issue::error(path, "must be a string")),
                },
                "base" => match v.as_str().and_then(ThemeKind::parse) {
                    Some(kind) => self.base = Some(kind),
                    None => issues.push(Issue::error(path, "must be \"clay\" or \"classic\"")),
                },
                "schema" => match v.as_integer() {
                    Some(n) if n <= THEME_SCHEMA => {}
                    Some(n) => issues.push(Issue::warning(
                        path,
                        format!("schema {n} is newer than this Glaze ({THEME_SCHEMA}); unknown keys are ignored"),
                    )),
                    None => issues.push(Issue::error(path, "must be an integer")),
                },
                "author" | "description" | "license" => {}
                _ => issues.push(Issue::warning(path, "unknown key; ignored")),
            }
        }
    }

    fn parse_palette(&mut self, v: &Value, issues: &mut Vec<Issue>) {
        let Some(t) = v.as_table() else {
            issues.push(Issue::error("palette", "must be a table"));
            return;
        };
        for (k, v) in t {
            match (k.as_str(), v) {
                ("light", Value::Table(sub)) => {
                    parse_colors(sub, "palette.light", &mut self.palette_light, issues)
                }
                ("dark", Value::Table(sub)) => {
                    parse_colors(sub, "palette.dark", &mut self.palette_dark, issues)
                }
                _ => {
                    let mut one = toml::Table::new();
                    one.insert(k.clone(), v.clone());
                    parse_colors(&one, "palette", &mut self.palette_any, issues);
                }
            }
        }
    }

    fn parse_metrics(&mut self, v: &Value, issues: &mut Vec<Issue>) {
        let Some(t) = v.as_table() else {
            issues.push(Issue::error("metrics", "must be a table"));
            return;
        };
        for (k, v) in t {
            let path = format!("metrics.{k}");
            if let Some(&(name, lo, hi)) = INT_METRICS.iter().find(|(n, _, _)| *n == k) {
                match v.as_integer() {
                    Some(n) if (lo..=hi).contains(&n) => {
                        self.int_metrics.insert(name, n);
                    }
                    Some(n) => issues.push(Issue::error(
                        path,
                        format!("{n} is outside {lo}..={hi}; ignored"),
                    )),
                    None => issues.push(Issue::error(
                        path,
                        format!("expected an integer, found {}; ignored", type_name(v)),
                    )),
                }
            } else if let Some(&(name, lo, hi)) = FLOAT_METRICS.iter().find(|(n, _, _)| *n == k) {
                match as_f64(v) {
                    Some(n) if (lo..=hi).contains(&n) => {
                        self.float_metrics.insert(name, n);
                    }
                    Some(n) => issues.push(Issue::error(
                        path,
                        format!("{n} is outside {lo}..={hi}; ignored"),
                    )),
                    None => issues.push(Issue::error(
                        path,
                        format!("expected a number, found {}; ignored", type_name(v)),
                    )),
                }
            } else {
                issues.push(Issue::warning(path, "unknown metric; ignored"));
            }
        }
    }

    fn parse_typography(&mut self, v: &Value, issues: &mut Vec<Issue>) {
        let Some(t) = v.as_table() else {
            issues.push(Issue::error("typography", "must be a table"));
            return;
        };
        for (k, v) in t {
            let path = format!("typography.{k}");
            match k.as_str() {
                "ui_family" | "display_family" | "mono_family" => match v.as_str() {
                    Some(s) if font_family_is_safe(s) => {
                        let slot = match k.as_str() {
                            "ui_family" => &mut self.ui_family,
                            "display_family" => &mut self.display_family,
                            _ => &mut self.mono_family,
                        };
                        *slot = Some(s.trim().to_string());
                    }
                    Some(_) => issues.push(Issue::error(
                        path,
                        "font family list is empty or contains ; { } \\; ignored",
                    )),
                    None => issues.push(Issue::error(path, "must be a string")),
                },
                "clock_size_pt" | "small_size_pt" => match as_f64(v) {
                    Some(n) if (5.0..=32.0).contains(&n) => {
                        if k == "clock_size_pt" {
                            self.clock_size_pt = Some(n);
                        } else {
                            self.small_size_pt = Some(n);
                        }
                    }
                    _ => issues.push(Issue::error(path, "must be a number in 5..=32; ignored")),
                },
                "emphasis_weight" => match v.as_integer() {
                    Some(n) if (100..=900).contains(&n) => self.emphasis_weight = Some(n as u16),
                    _ => issues.push(Issue::error(
                        path,
                        "must be an integer in 100..=900; ignored",
                    )),
                },
                _ => issues.push(Issue::warning(path, "unknown key; ignored")),
            }
        }
    }

    fn parse_shadow(&mut self, v: &Value, issues: &mut Vec<Issue>) {
        let Some(t) = v.as_table() else {
            issues.push(Issue::error("shadow", "must be a table"));
            return;
        };
        for (k, v) in t {
            let path = format!("shadow.{k}");
            match k.as_str() {
                "strength" => match as_f64(v) {
                    Some(n) if (0.0..=2.0).contains(&n) => self.shadow_strength = Some(n),
                    _ => issues.push(Issue::error(path, "must be a number in 0..=2; ignored")),
                },
                "button" | "popover" | "window" => {
                    if let Some(s) = parse_shadow_token(v, &path, issues) {
                        match k.as_str() {
                            "button" => self.shadow_button = Some(s),
                            "popover" => self.shadow_popover = Some(s),
                            _ => self.shadow_window = Some(s),
                        }
                    }
                }
                _ => issues.push(Issue::warning(path, "unknown key; ignored")),
            }
        }
    }

    /// Apply these overrides to `tokens` in place. Returns warnings produced by
    /// applying (e.g. Clay blur forced to 0).
    pub fn apply(&self, tokens: &mut ThemeTokens) -> Vec<Issue> {
        let mut issues = Vec::new();
        let scheme_map = match tokens.scheme {
            ColorScheme::Light => &self.palette_light,
            ColorScheme::Dark => &self.palette_dark,
        };
        let mut touched_accent = false;
        let mut touched_accent_text = false;
        for map in [&self.palette_any, scheme_map] {
            for (key, color) in map {
                if let Some(role) = ThemeRole::from_key(key) {
                    tokens.palette.set(role, *color);
                    touched_accent |= role == ThemeRole::Accent;
                    touched_accent_text |= role == ThemeRole::AccentText;
                }
            }
        }
        if touched_accent && !touched_accent_text {
            derive_accent_text(tokens);
        }
        let m = &mut tokens.metrics;
        for (k, v) in &self.int_metrics {
            set_int_metric(m, k, *v);
        }
        for (k, v) in &self.float_metrics {
            set_float_metric(m, k, *v);
        }
        if m.control_height > m.panel_height {
            issues.push(Issue::warning(
                "metrics.control_height",
                format!(
                    "{} is taller than panel_height {}; clamped",
                    m.control_height, m.panel_height
                ),
            ));
            m.control_height = m.panel_height;
        }
        let ty = &mut tokens.typography;
        if let Some(s) = &self.ui_family {
            ty.ui_family = s.clone();
        }
        if let Some(s) = &self.display_family {
            ty.display_family = s.clone();
        }
        if let Some(s) = &self.mono_family {
            ty.mono_family = s.clone();
        }
        if let Some(n) = self.clock_size_pt {
            ty.clock_size_pt = n;
        }
        if let Some(n) = self.small_size_pt {
            ty.small_size_pt = n;
        }
        if let Some(n) = self.emphasis_weight {
            ty.emphasis_weight = n;
        }
        let sh = &mut tokens.shadows;
        if let Some(n) = self.shadow_strength {
            sh.strength = n;
        }
        for (slot, value, name) in [
            (&mut sh.button, self.shadow_button, "button"),
            (&mut sh.popover, self.shadow_popover, "popover"),
            (&mut sh.window, self.shadow_window, "window"),
        ] {
            if let Some(mut s) = value {
                if tokens.kind == ThemeKind::Clay && s.blur > 0 {
                    issues.push(Issue::warning(
                        format!("shadow.{name}.blur"),
                        "Clay shadows are flat paper offsets; blur forced to 0",
                    ));
                    s.blur = 0;
                }
                *slot = s;
            }
        }
        issues
    }
}

fn parse_colors(
    t: &toml::Table,
    prefix: &str,
    out: &mut BTreeMap<&'static str, Color>,
    issues: &mut Vec<Issue>,
) {
    for (k, v) in t {
        let path = format!("{prefix}.{k}");
        let Some(role) = ThemeRole::from_key(k) else {
            issues.push(Issue::warning(path, "unknown colour role; ignored"));
            continue;
        };
        match v.as_str().map(Color::from_hex) {
            Some(Ok(c)) => {
                out.insert(role.key(), c);
            }
            Some(Err(e)) => issues.push(Issue::error(
                path,
                format!("invalid colour {v} ({e}); ignored"),
            )),
            None => issues.push(Issue::error(
                path,
                format!(
                    "expected a \"#rrggbb\" string, found {}; ignored",
                    type_name(v)
                ),
            )),
        }
    }
}

fn parse_shadow_token(v: &Value, path: &str, issues: &mut Vec<Issue>) -> Option<Shadow> {
    if v.as_str() == Some("none") {
        return Some(Shadow::NONE);
    }
    let Some(t) = v.as_table() else {
        issues.push(Issue::error(
            path,
            "must be \"none\" or { x, y, blur, spread }; ignored",
        ));
        return None;
    };
    let mut s = Shadow::NONE;
    for (k, v) in t {
        let n = match v.as_integer() {
            Some(n) if (-8..=16).contains(&n) => n as i32,
            _ => {
                issues.push(Issue::error(
                    format!("{path}.{k}"),
                    "must be an integer in -8..=16; shadow ignored",
                ));
                return None;
            }
        };
        match k.as_str() {
            "x" => s.x = n,
            "y" => s.y = n,
            "blur" => s.blur = n.max(0),
            "spread" => s.spread = n,
            _ => issues.push(Issue::warning(
                format!("{path}.{k}"),
                "unknown key; ignored",
            )),
        }
    }
    Some(s)
}

/// Set an integer metric by key (keys from [`INT_METRICS`]).
pub fn set_int_metric(m: &mut ThemeMetrics, key: &str, v: i64) {
    let v32 = v as i32;
    match key {
        "panel_height" => m.panel_height = v32,
        "control_height" => m.control_height = v32,
        "control_radius" => m.control_radius = v32,
        "menu_radius" => m.menu_radius = v32,
        "window_radius" => m.window_radius = v32,
        "overlay_radius" => m.overlay_radius = v32,
        "panel_padding" => m.panel_padding = v32,
        "spacing" => m.spacing = v32,
        "touch_target" => m.touch_target = v32,
        "motion_micro_ms" => m.motion_micro_ms = v as u32,
        "motion_menu_ms" => m.motion_menu_ms = v as u32,
        "motion_workspace_ms" => m.motion_workspace_ms = v as u32,
        "icon_size" => m.icon_size = v32,
        "task_button_width" => m.task_button_width = v32,
        "task_icon_width" => m.task_icon_width = v32,
        "underline_height" => m.underline_height = v32,
        _ => {}
    }
}

/// Set a float metric by key (keys from [`FLOAT_METRICS`]).
pub fn set_float_metric(m: &mut ThemeMetrics, key: &str, v: f64) {
    match key {
        "font_size_pt" => m.font_size_pt = v,
        "stroke_width" => m.stroke_width = v,
        "icon_stroke" => m.icon_stroke = v,
        _ => {}
    }
}

/// Re-derive `accent_text` (and the soft accent) after the accent changed, so
/// a user-picked accent never produces unreadable labels.
pub fn derive_accent_text(tokens: &mut ThemeTokens) {
    let p = &mut tokens.palette;
    let dark = Color::rgb(0x1C, 0x14, 0x10);
    let light = Color::rgb(0xFF, 0xFF, 0xFF);
    p.accent_text = ThemeTokens::best_text_on(p.accent, dark, light);
    p.accent_soft = match tokens.scheme {
        ColorScheme::Light => p.accent.mix(p.surface, 0.72),
        ColorScheme::Dark => p.accent.mix(p.surface, 0.70),
    };
    let soft_dark = p.accent.mix(Color::rgb(0, 0, 0), 0.55);
    let soft_light = p.accent.mix(Color::rgb(255, 255, 255), 0.65);
    p.accent_soft_text = ThemeTokens::best_text_on(p.accent_soft, soft_dark, soft_light);
}

/// Core contrast pairs checked after overrides are applied. Failures are
/// warnings: the user's choice is applied, but they are told it is hard to
/// read (`glaze-a11y` holds the full audit of the built-in palettes).
pub const CONTRAST_PAIRS: &[(ThemeRole, ThemeRole, f64)] = &[
    (ThemeRole::TextPrimary, ThemeRole::Panel, 4.5),
    (ThemeRole::TextPrimary, ThemeRole::Surface, 4.5),
    (ThemeRole::TextPrimary, ThemeRole::SurfaceRaised, 4.5),
    (ThemeRole::TextSecondary, ThemeRole::Panel, 3.0),
    (ThemeRole::AccentText, ThemeRole::Accent, 4.5),
    (ThemeRole::AccentSoftText, ThemeRole::AccentSoft, 4.5),
    (ThemeRole::Ink, ThemeRole::Panel, 3.0),
];

/// Contrast warnings for a (possibly overridden) token set.
pub fn contrast_issues(tokens: &ThemeTokens) -> Vec<Issue> {
    CONTRAST_PAIRS
        .iter()
        .filter_map(|(fg, bg, min)| {
            let r = tokens
                .palette
                .get(*fg)
                .contrast_ratio(tokens.palette.get(*bg));
            (r + 1e-9 < *min).then(|| {
                Issue::warning(
                    format!("palette.{}", fg.key()),
                    format!(
                        "contrast {r:.2}:1 on {} is below WCAG AA {min}:1 ({} scheme)",
                        bg.key(),
                        tokens.scheme.slug()
                    ),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
[meta]
name = "Warm Clay"
schema = 1
base = "clay"

[palette]
border = "#cbbfa9"
[palette.light]
accent = "#c8643f"
[palette.dark]
accent = "#e08a6c"

[metrics]
panel_height = 44
control_radius = 6
stroke_width = 1.25

[typography]
display_family = "\"Noto Serif\""
clock_size_pt = 10

[shadow]
strength = 0.5
popover = { x = 2, y = 2, blur = 0, spread = 0 }
"##;

    #[test]
    fn sample_theme_file_parses_cleanly_and_applies() {
        let (o, issues) = ThemeOverrides::from_toml_str(SAMPLE);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(o.name.as_deref(), Some("Warm Clay"));
        assert_eq!(o.base, Some(ThemeKind::Clay));
        let mut t = ThemeTokens::default_clay();
        assert!(o.apply(&mut t).is_empty());
        assert_eq!(t.palette.accent, Color::from_hex("#c8643f").unwrap());
        assert_eq!(t.palette.border, Color::from_hex("#cbbfa9").unwrap());
        assert_eq!(t.metrics.panel_height, 44);
        assert_eq!(t.metrics.control_radius, 6);
        assert!((t.metrics.stroke_width - 1.25).abs() < 1e-9);
        assert_eq!(t.typography.display_family, "\"Noto Serif\"");
        assert!((t.shadows.strength - 0.5).abs() < 1e-9);
        let mut d = ThemeTokens::for_kind(ThemeKind::Clay, ColorScheme::Dark);
        o.apply(&mut d);
        assert_eq!(d.palette.accent, Color::from_hex("#e08a6c").unwrap());
    }

    #[test]
    fn bad_values_are_reported_with_paths_and_the_rest_applies() {
        let src = r##"
[palette.light]
accent = "#zzzzzz"
surface = 12
sparkle = "#ffffff"
[metrics]
panel_height = 300
control_radius = "big"
wobble = 3
[typography]
ui_family = "Inter; } * { color: red"
[shadow]
popover = { x = 2, y = 2, blur = 4 }
[extra]
"##;
        let (o, issues) = ThemeOverrides::from_toml_str(src);
        let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
        for p in [
            "palette.light.accent",
            "palette.light.surface",
            "palette.light.sparkle",
            "metrics.panel_height",
            "metrics.control_radius",
            "metrics.wobble",
            "typography.ui_family",
            "extra",
        ] {
            assert!(paths.contains(&p), "missing issue for {p}: {paths:?}");
        }
        let msg = issues
            .iter()
            .find(|i| i.path == "metrics.panel_height")
            .unwrap()
            .to_string();
        assert!(msg.contains("outside 24..=96"), "{msg}");
        // The blurred popover shadow parses but is flattened for Clay.
        let mut t = ThemeTokens::default_clay();
        let applied = o.apply(&mut t);
        assert!(applied.iter().any(|i| i.path == "shadow.popover.blur"));
        assert_eq!(t.shadows.popover.blur, 0);
        assert_eq!(t.metrics.panel_height, 40, "invalid value left default");
        // Classic keeps the blur.
        let mut c = ThemeTokens::for_kind(ThemeKind::Classic, ColorScheme::Light);
        o.apply(&mut c);
        assert_eq!(c.shadows.popover.blur, 4);
    }

    #[test]
    fn syntax_errors_are_one_clear_error() {
        let (_, issues) = ThemeOverrides::from_toml_str("[metrics\npanel_height = 4");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_error());
        assert!(issues[0].message.starts_with("not valid TOML"));
    }

    #[test]
    fn user_accent_derives_readable_text() {
        for accent in ["#1a3d8f", "#f5d76e", "#2e7d32", "#d97757"] {
            let mut o = ThemeOverrides::default();
            o.palette_any
                .insert("accent", Color::from_hex(accent).unwrap());
            for scheme in ColorScheme::ALL {
                let mut t = ThemeTokens::for_kind(ThemeKind::Clay, scheme);
                o.apply(&mut t);
                let r = t.palette.accent_text.contrast_ratio(t.palette.accent);
                assert!(r >= 4.5, "{accent} {scheme:?}: {r:.2}");
                assert!(
                    contrast_issues(&t).is_empty(),
                    "{accent} {scheme:?}: {:?}",
                    contrast_issues(&t)
                );
            }
        }
    }

    #[test]
    fn contrast_issues_flag_unreadable_overrides() {
        let mut t = ThemeTokens::default_clay();
        t.palette.text_primary = t.palette.panel;
        let issues = contrast_issues(&t);
        assert!(issues.iter().any(|i| i.path == "palette.text_primary"));
        assert!(issues.iter().all(|i| !i.is_error()));
        assert!(contrast_issues(&ThemeTokens::default_clay()).is_empty());
    }

    #[test]
    fn control_taller_than_panel_is_clamped() {
        let (o, issues) =
            ThemeOverrides::from_toml_str("[metrics]\npanel_height = 30\ncontrol_height = 40\n");
        assert!(issues.is_empty());
        let mut t = ThemeTokens::default_clay();
        let applied = o.apply(&mut t);
        assert_eq!(t.metrics.control_height, 30);
        assert_eq!(applied.len(), 1);
    }

    #[test]
    fn every_metric_key_is_settable() {
        for (k, lo, _) in INT_METRICS {
            let mut m = ThemeMetrics::clay();
            let before = m;
            set_int_metric(&mut m, k, *lo + 1);
            assert_ne!(m, before, "{k} not wired");
        }
        for (k, lo, _) in FLOAT_METRICS {
            let mut m = ThemeMetrics::clay();
            let before = m;
            set_float_metric(&mut m, k, *lo + 0.123);
            assert_ne!(m, before, "{k} not wired");
        }
    }
}
