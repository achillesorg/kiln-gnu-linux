//! Loading the user configuration with validation, theme files and the
//! effective token resolution.
//!
//! Load order for tokens (later wins):
//!
//! ```text
//! built-in (appearance.theme + scheme)  <  theme file (appearance.theme_file)
//!     <  [appearance.overrides]  <  appearance shortcuts (accent, radius, ...)
//! ```
//!
//! A bad value never takes the whole file down: TOML syntax errors keep the
//! previous configuration (the caller decides; the shell keeps its running
//! config), a key with a wrong type or unknown variant is dropped with an
//! error naming its path, unknown keys are warnings, and out-of-range values
//! are reset to their defaults with an error.

use std::path::{Path, PathBuf};

use glaze_design::issues::Issue;
use glaze_design::overrides::{contrast_issues, derive_accent_text, ThemeOverrides};
use glaze_design::{Color, ThemeTokens};
use toml::Value;

use crate::config::{GlazeConfig, PanelSection, TrayItem};

/// Known paper texture set ids (`appearance.texture`).
pub const TEXTURE_SETS: &[&str] = glaze_design::texture::SETS;

/// Result of [`load_checked`].
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: GlazeConfig,
    pub issues: Vec<Issue>,
    /// `false` when the file could not be parsed at all (syntax error or I/O);
    /// `config` is then the defaults and a live reloader should keep the
    /// previous configuration instead.
    pub usable: bool,
}

/// Read, parse, repair and validate the config at `path`. A missing file
/// yields the defaults with no issues.
pub fn load_checked(path: &Path) -> Loaded {
    match std::fs::read_to_string(path) {
        Ok(src) => parse_checked(&src),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Loaded {
            config: GlazeConfig::default(),
            issues: Vec::new(),
            usable: true,
        },
        Err(e) => Loaded {
            config: GlazeConfig::default(),
            issues: vec![Issue::error(
                "",
                format!("cannot read {}: {e}", path.display()),
            )],
            usable: false,
        },
    }
}

/// Parse a config document with per-key error recovery.
pub fn parse_checked(src: &str) -> Loaded {
    let mut issues = Vec::new();
    let mut table = match src.parse::<toml::Table>() {
        Ok(t) => t,
        Err(e) => {
            let where_ = e
                .span()
                .map(|s| {
                    let line = src[..s.start.min(src.len())].matches('\n').count() + 1;
                    format!(" (line {line})")
                })
                .unwrap_or_default();
            return Loaded {
                config: GlazeConfig::default(),
                issues: vec![Issue::error(
                    "",
                    format!("not valid TOML{where_}: {}", e.message()),
                )],
                usable: false,
            };
        }
    };

    unknown_keys(&table, &mut issues);

    // Drop keys whose value cannot deserialize, one at a time, so the rest of
    // the section survives.
    let section_names: Vec<String> = table.keys().cloned().collect();
    for section in section_names {
        if let Some(Value::Table(sec)) = table.get(&section) {
            let mut bad = Vec::new();
            for (k, v) in sec {
                let mut probe = toml::Table::new();
                let mut inner = toml::Table::new();
                inner.insert(k.clone(), v.clone());
                probe.insert(section.clone(), Value::Table(inner));
                if let Err(e) = GlazeConfig::deserialize_table(probe) {
                    bad.push((k.clone(), e));
                }
            }
            if let Some(Value::Table(sec)) = table.get_mut(&section) {
                for (k, e) in bad {
                    sec.remove(&k);
                    issues.push(Issue::error(
                        format!("{section}.{k}"),
                        format!("{}; using the default", first_line(&e)),
                    ));
                }
            }
        }
    }

    let mut config = match GlazeConfig::deserialize_table(table) {
        Ok(c) => c,
        Err(e) => {
            issues.push(Issue::error("", e));
            GlazeConfig::default()
        }
    };
    config.migrate();
    issues.extend(validate(&mut config));
    Loaded {
        config,
        issues,
        usable: true,
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(s)
        .trim()
        .to_string()
}

impl GlazeConfig {
    fn deserialize_table(t: toml::Table) -> Result<GlazeConfig, String> {
        Value::Table(t)
            .try_into::<GlazeConfig>()
            .map_err(|e| e.to_string())
    }
}

/// Keys that are valid but absent from the serialized defaults (they are
/// `Option`s or free-form tables).
const OPTIONAL_KEYS: &[&str] = &[
    "panel.height",
    "panel.date_format",
    "panel.monitors",
    "desktop.wallpaper",
    "appearance.theme_file",
    "appearance.accent",
    "appearance.radius",
    "appearance.stroke_weight",
    "appearance.shadow_strength",
    "appearance.overrides",
];

fn unknown_keys(table: &toml::Table, issues: &mut Vec<Issue>) {
    let defaults = Value::try_from(GlazeConfig::default()).expect("defaults serialize");
    let Value::Table(known) = defaults else {
        return;
    };
    for (section, value) in table {
        let Some(known_sec) = known.get(section) else {
            if section != "version" {
                issues.push(Issue::warning(section.clone(), "unknown section; ignored"));
            }
            continue;
        };
        if let (Value::Table(sec), Value::Table(known_sec)) = (value, known_sec) {
            for k in sec.keys() {
                let path = format!("{section}.{k}");
                if !known_sec.contains_key(k) && !OPTIONAL_KEYS.contains(&path.as_str()) {
                    issues.push(Issue::warning(path, "unknown key; ignored"));
                }
            }
        }
    }
}

/// Range-check and repair a configuration in place.
pub fn validate(c: &mut GlazeConfig) -> Vec<Issue> {
    let d = GlazeConfig::default();
    let mut issues = Vec::new();
    let mut range_f = |path: &str, v: &mut f64, lo: f64, hi: f64, def: f64| {
        if !(lo..=hi).contains(v) || !v.is_finite() {
            issues.push(Issue::error(
                path,
                format!("{v} is outside {lo}..={hi}; using {def}"),
            ));
            *v = def;
        }
    };
    range_f("appearance.scale", &mut c.appearance.scale, 0.5, 4.0, 1.0);
    range_f(
        "appearance.font_scale",
        &mut c.appearance.font_scale,
        0.5,
        3.0,
        1.0,
    );
    range_f(
        "appearance.texture_intensity",
        &mut c.appearance.texture_intensity,
        0.0,
        1.0,
        d.appearance.texture_intensity,
    );
    range_f("motion.speed", &mut c.motion.speed, 0.25, 4.0, 1.0);
    if let Some(v) = c.appearance.stroke_weight.as_mut() {
        let mut x = *v;
        range_f("appearance.stroke_weight", &mut x, 0.0, 3.0, 1.0);
        *v = x;
    }
    if let Some(v) = c.appearance.shadow_strength.as_mut() {
        let mut x = *v;
        range_f("appearance.shadow_strength", &mut x, 0.0, 2.0, 1.0);
        *v = x;
    }
    if !(320..=900).contains(&c.launcher.width) {
        issues.push(Issue::error(
            "launcher.width",
            format!(
                "{} is outside 320..=900; using {}",
                c.launcher.width, d.launcher.width
            ),
        ));
        c.launcher.width = d.launcher.width;
    }
    if !(360..=1000).contains(&c.launcher.height) {
        issues.push(Issue::error(
            "launcher.height",
            format!(
                "{} is outside 360..=1000; using {}",
                c.launcher.height, d.launcher.height
            ),
        ));
        c.launcher.height = d.launcher.height;
    }
    if !(16..=32).contains(&c.launcher.icon_size) {
        issues.push(Issue::error(
            "launcher.icon_size",
            format!(
                "{} is outside 16..=32; using {}",
                c.launcher.icon_size, d.launcher.icon_size
            ),
        ));
        c.launcher.icon_size = d.launcher.icon_size;
    }
    if !(2..=8).contains(&c.launcher.columns) {
        issues.push(Issue::error(
            "launcher.columns",
            format!(
                "{} is outside 2..=8; using {}",
                c.launcher.columns, d.launcher.columns
            ),
        ));
        c.launcher.columns = d.launcher.columns;
    }
    if let Some(r) = c.appearance.radius {
        if !(0..=16).contains(&r) {
            issues.push(Issue::error(
                "appearance.radius",
                format!("{r} is outside 0..=16; ignored"),
            ));
            c.appearance.radius = None;
        }
    }
    if !(300..=800).contains(&c.panel.start_label_weight) {
        issues.push(Issue::error(
            "panel.start_label_weight",
            format!(
                "{} is outside 300..=800; using 600",
                c.panel.start_label_weight
            ),
        ));
        c.panel.start_label_weight = 600;
    }
    if let Some(h) = c.panel.height {
        if !(24..=96).contains(&h) {
            issues.push(Issue::error(
                "panel.height",
                format!("{h} is outside 24..=96; using the theme height"),
            ));
            c.panel.height = None;
        }
    }
    if let Some(a) = c.appearance.accent.clone() {
        if let Err(e) = Color::from_hex(&a) {
            issues.push(Issue::error(
                "appearance.accent",
                format!("invalid colour \"{a}\" ({e}); ignored"),
            ));
            c.appearance.accent = None;
        }
    }
    if !TEXTURE_SETS.contains(&c.appearance.texture.as_str()) {
        issues.push(Issue::error(
            "appearance.texture",
            format!(
                "unknown texture set \"{}\" (known: {}); using \"{}\"",
                c.appearance.texture,
                TEXTURE_SETS.join(", "),
                d.appearance.texture
            ),
        ));
        c.appearance.texture = d.appearance.texture.clone();
    }
    dedup(&mut c.panel.sections, "panel.sections", &mut issues);
    dedup(&mut c.panel.tray_items, "panel.tray_items", &mut issues);
    if !c.panel.sections.contains(&PanelSection::Start) {
        issues.push(Issue::warning(
            "panel.sections",
            "no \"start\" section: the start menu is only reachable by keyboard",
        ));
    }
    for (name, o) in c.panel.monitors.iter_mut() {
        if let Some(h) = o.height {
            if !(24..=96).contains(&h) {
                issues.push(Issue::error(
                    format!("panel.monitors.{name}.height"),
                    format!("{h} is outside 24..=96; ignored"),
                ));
                o.height = None;
            }
        }
        if let Some(s) = o.sections.as_mut() {
            dedup(s, &format!("panel.monitors.{name}.sections"), &mut issues);
        }
        if let Some(s) = o.tray_items.as_mut() {
            dedup(s, &format!("panel.monitors.{name}.tray_items"), &mut issues);
        }
    }
    let _ = TrayItem::DEFAULT;
    issues
}

fn dedup<T: PartialEq + Copy + std::fmt::Debug>(
    v: &mut Vec<T>,
    path: &str,
    issues: &mut Vec<Issue>,
) {
    let mut seen: Vec<T> = Vec::new();
    for x in v.iter() {
        if seen.contains(x) {
            issues.push(Issue::warning(
                path,
                format!("{x:?} listed twice; the duplicate is ignored"),
            ));
        } else {
            seen.push(*x);
        }
    }
    *v = seen;
}

/// Where theme files are searched for a bare `appearance.theme_file` name.
pub fn theme_dirs() -> Vec<PathBuf> {
    let mut v = vec![
        crate::paths::glaze_config_dir().join("themes"),
        crate::paths::data_home().join("glaze/themes"),
    ];
    v.extend(
        crate::paths::data_dirs()
            .into_iter()
            .map(|d| d.join("glaze/themes")),
    );
    v
}

/// Locate and read a theme file by name or path.
pub fn read_theme_file(spec: &str, dirs: &[PathBuf]) -> Result<(PathBuf, String), Issue> {
    let candidates: Vec<PathBuf> = if spec.contains('/') || spec.ends_with(".toml") {
        vec![PathBuf::from(spec)]
    } else {
        dirs.iter()
            .map(|d| d.join(format!("{spec}.toml")))
            .collect()
    };
    for p in &candidates {
        if let Ok(src) = std::fs::read_to_string(p) {
            return Ok((p.clone(), src));
        }
    }
    Err(Issue::error(
        "appearance.theme_file",
        format!(
            "theme \"{spec}\" not found (looked in {}); using built-in tokens",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ))
}

/// Resolve the effective tokens. `theme_src` is the theme file's contents
/// (already read by the caller; `None` = no theme file).
pub fn resolve_tokens(c: &GlazeConfig, theme_src: Option<&str>) -> (ThemeTokens, Vec<Issue>) {
    let mut issues = Vec::new();
    let file = theme_src.map(|src| {
        let (o, mut i) = ThemeOverrides::from_toml_str(src);
        issues.extend(i.drain(..).map(|x| x.prefixed("theme_file:")));
        o
    });
    let kind = file
        .as_ref()
        .and_then(|o| o.base)
        .unwrap_or(c.appearance.theme);
    let mut tokens = ThemeTokens::for_kind(kind, c.appearance.scheme);
    if let Some(o) = &file {
        issues.extend(
            o.apply(&mut tokens)
                .into_iter()
                .map(|x| x.prefixed("theme_file:")),
        );
    }
    if let Some(t) = &c.appearance.overrides {
        let (o, i) = ThemeOverrides::from_table(t);
        issues.extend(i.into_iter().map(|x| x.prefixed("appearance.overrides.")));
        issues.extend(
            o.apply(&mut tokens)
                .into_iter()
                .map(|x| x.prefixed("appearance.overrides.")),
        );
    }
    // Shortcuts.
    if let Some(a) = c
        .appearance
        .accent
        .as_deref()
        .and_then(|a| Color::from_hex(a).ok())
    {
        tokens.palette.accent = a;
        derive_accent_text(&mut tokens);
    }
    if let Some(r) = c.appearance.radius {
        tokens.metrics.control_radius = r;
        tokens.metrics.menu_radius = (r + 1).min(20);
        tokens.metrics.window_radius = r.min(20);
        tokens.metrics.overlay_radius = (r + 1).min(24);
    }
    if let Some(w) = c.appearance.stroke_weight {
        tokens.metrics.stroke_width = w;
    }
    if let Some(s) = c.appearance.shadow_strength {
        tokens.shadows.strength = s;
    }
    // Density.
    let delta = c.panel.density.delta_px();
    tokens.metrics.panel_height = (tokens.metrics.panel_height + delta).max(24);
    tokens.metrics.control_height =
        (tokens.metrics.control_height + delta * 2 / 3).clamp(16, tokens.metrics.panel_height);
    if let Some(h) = c.panel.height {
        tokens.metrics.panel_height = h;
        tokens.metrics.control_height = tokens.metrics.control_height.min(h);
    }
    tokens.typography.start_label_serif =
        c.panel.start_label_style == crate::config::StartLabelStyle::Serif;
    tokens.typography.start_label_weight = c.panel.start_label_weight;
    if c.appearance.font_scale != 1.0 {
        tokens.typography.start_label_px *= c.appearance.font_scale;
        tokens.metrics.font_size_pt *= c.appearance.font_scale;
        tokens.typography.clock_size_pt *= c.appearance.font_scale;
        tokens.typography.small_size_pt *= c.appearance.font_scale;
    }
    issues.extend(contrast_issues(&tokens));
    (tokens, issues)
}

/// Read the theme file named in the config (if any) and resolve the tokens.
pub fn load_tokens(c: &GlazeConfig) -> (ThemeTokens, Vec<Issue>, Option<PathBuf>) {
    match c.appearance.theme_file.as_deref() {
        None => {
            let (t, i) = resolve_tokens(c, None);
            (t, i, None)
        }
        Some(spec) => match read_theme_file(spec, &theme_dirs()) {
            Ok((path, src)) => {
                let (t, i) = resolve_tokens(c, Some(&src));
                (t, i, Some(path))
            }
            Err(issue) => {
                let (t, mut i) = resolve_tokens(c, None);
                i.insert(0, issue);
                (t, i, None)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupingMode, LabelMode, PanelPosition};
    use glaze_design::{ColorScheme, ThemeKind};

    #[test]
    fn empty_and_missing_configs_are_clean_defaults() {
        let l = parse_checked("");
        assert!(l.usable && l.issues.is_empty(), "{:?}", l.issues);
        assert_eq!(l.config, GlazeConfig::default());
        let l = load_checked(Path::new("/nonexistent/glaze/config.toml"));
        assert!(l.usable && l.issues.is_empty());
    }

    #[test]
    fn full_customisation_round_trips() {
        let src = r##"
version = 3
[appearance]
theme = "clay"
scheme = "dark"
accent = "#3a6ea5"
radius = 4
stroke_weight = 1.25
shadow_strength = 0.5
icon_style = "system"
texture = "fibre"
texture_intensity = 0.3
reduced_effects = true
[appearance.overrides.metrics]
task_button_width = 150
[panel]
position = "top"
density = "compact"
button_style = "flat"
labels = "active-only"
grouping = "auto"
sections = ["start", "tasks", "spacer", "clock"]
tray_items = ["volume", "battery"]
show_layout_toggle = false
layout_scope = "global"
autohide = "smart"
clock_24h = false
[panel.monitors.HDMI-A-1]
position = "left"
labels = "icon-only"
[panel.monitors.Virtual-2]
enabled = false
[motion]
style = "snappy"
speed = 1.5
"##;
        let l = parse_checked(src);
        assert!(l.issues.is_empty(), "{:?}", l.issues);
        let c = &l.config;
        assert_eq!(c.panel.position, PanelPosition::Top);
        assert_eq!(c.panel.labels, LabelMode::ActiveOnly);
        assert_eq!(c.panel.grouping, GroupingMode::Auto);
        assert_eq!(c.panel.sections.len(), 4);
        let m = c.panel.for_monitor("HDMI-A-1");
        assert_eq!(m.position, PanelPosition::Left);
        assert_eq!(m.label_mode(), LabelMode::IconOnly);
        assert_eq!(m.grouping, GroupingMode::Auto, "inherited");
        assert!(!c.panel.enabled_on("Virtual-2"));
        assert!(c.panel.enabled_on("Virtual-1"));
        let back = parse_checked(&c.to_toml_string().unwrap());
        assert!(back.issues.is_empty(), "{:?}", back.issues);
        assert_eq!(&back.config, c);

        let (t, issues) = resolve_tokens(c, None);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(t.scheme, ColorScheme::Dark);
        assert_eq!(t.palette.accent, Color::from_hex("#3a6ea5").unwrap());
        assert_eq!(t.metrics.control_radius, 4);
        assert_eq!(t.metrics.task_button_width, 150);
        assert!((t.metrics.stroke_width - 1.25).abs() < 1e-9);
        assert_eq!(t.metrics.panel_height, 34, "compact density");
    }

    #[test]
    fn bad_values_are_reported_and_repaired_per_key() {
        let src = r##"
[appearance]
accent = "orange"
texture = "velvet"
texture_intensity = 7
sparkles = true
[panel]
labels = "sometimes"
height = 300
position = "top"
sections = ["start", "tasks", "start"]
[motion]
speed = 0
[mystery]
x = 1
"##;
        let l = parse_checked(src);
        assert!(l.usable);
        let paths: Vec<&str> = l.issues.iter().map(|i| i.path.as_str()).collect();
        for p in [
            "appearance.accent",
            "appearance.texture",
            "appearance.texture_intensity",
            "appearance.sparkles",
            "panel.labels",
            "panel.height",
            "panel.sections",
            "motion.speed",
            "mystery",
        ] {
            assert!(paths.contains(&p), "no issue for {p}: {paths:?}");
        }
        let c = &l.config;
        assert_eq!(c.panel.position, PanelPosition::Top, "good keys survive");
        assert_eq!(c.panel.labels, LabelMode::Full);
        assert_eq!(c.panel.height, None);
        assert_eq!(c.appearance.texture, "grain");
        assert_eq!(c.panel.sections.len(), 2);
        let msg = l
            .issues
            .iter()
            .find(|i| i.path == "panel.labels")
            .unwrap()
            .to_string();
        assert!(msg.contains("sometimes"), "{msg}");
    }

    #[test]
    fn syntax_error_is_not_usable_and_names_the_line() {
        let l = parse_checked("[panel]\nposition = \"top\n");
        assert!(!l.usable);
        assert!(l.issues[0].message.contains("line"), "{:?}", l.issues);
    }

    #[test]
    fn load_order_defaults_then_theme_file_then_user() {
        let theme = r##"
[meta]
base = "classic"
[palette]
accent = "#225588"
[metrics]
control_radius = 3
panel_height = 38
"##;
        let mut c = GlazeConfig::default();
        let (t, i) = resolve_tokens(&c, Some(theme));
        assert!(i.is_empty(), "{i:?}");
        assert_eq!(t.kind, ThemeKind::Classic, "theme file base wins");
        assert_eq!(t.metrics.control_radius, 3);
        assert_eq!(t.metrics.panel_height, 38);
        let mut user = toml::Table::new();
        user.insert(
            "metrics".into(),
            toml::Value::Table(toml::toml! { control_radius = 7 }),
        );
        c.appearance.overrides = Some(user);
        c.appearance.accent = Some("#118844".into());
        let (t, _) = resolve_tokens(&c, Some(theme));
        assert_eq!(t.metrics.control_radius, 7, "user overrides beat the file");
        assert_eq!(t.palette.accent, Color::from_hex("#118844").unwrap());
        assert_eq!(
            t.metrics.panel_height, 38,
            "file value kept where user is silent"
        );
    }

    #[test]
    fn theme_file_lookup_reports_where_it_looked() {
        let dir = std::env::temp_dir().join(format!("glaze-theme-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("paper.toml"), "[metrics]\nspacing = 4\n").unwrap();
        let (p, src) = read_theme_file("paper", std::slice::from_ref(&dir)).unwrap();
        assert!(p.ends_with("paper.toml") && src.contains("spacing"));
        let err = read_theme_file("missing", std::slice::from_ref(&dir)).unwrap_err();
        assert!(err.message.contains("missing.toml"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shipped_theme_files_are_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let mut n = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            let (t, issues) = resolve_tokens(&GlazeConfig::default(), Some(&src));
            assert!(issues.is_empty(), "{}: {issues:?}", p.display());
            assert!(t.metrics.panel_height >= 24);
            n += 1;
        }
        assert!(n >= 2, "expected shipped theme files in themes/");
    }
}
