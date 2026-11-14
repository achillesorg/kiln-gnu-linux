//! Import and export of shareable Glaze theme files
//! (`glaze_design::overrides` format, schema 1).
//!
//! **Export** writes the user's current look as one self-contained theme
//! file: the active theme file (if any), then `[appearance.overrides]`, then
//! the appearance shortcuts (accent, radius, stroke weight, shadow strength),
//! in the same precedence the shell applies them.
//!
//! **Import** validates a file with the same parser the shell uses, reports
//! every issue with its key path, copies the file into
//! `~/.config/glaze/themes/<slug>.toml` and selects it with
//! `appearance.theme_file`.

use std::path::{Path, PathBuf};

use glaze_common::config::GlazeConfig;
use glaze_design::overrides::{ThemeOverrides, THEME_SCHEMA};
use glaze_design::Issue;
use toml::Value;

use crate::validate;

fn merge(into: &mut toml::Table, from: &toml::Table) {
    for (k, v) in from {
        match (into.get_mut(k), v) {
            (Some(Value::Table(a)), Value::Table(b)) => merge(a, b),
            _ => {
                into.insert(k.clone(), v.clone());
            }
        }
    }
}

fn table_at<'a>(t: &'a mut toml::Table, key: &str) -> &'a mut toml::Table {
    if !matches!(t.get(key), Some(Value::Table(_))) {
        t.insert(key.to_string(), Value::Table(toml::Table::new()));
    }
    match t.get_mut(key) {
        Some(Value::Table(x)) => x,
        _ => unreachable!("inserted above"),
    }
}

/// Build a theme document from the current configuration. `theme_file_src`
/// is the text of the active theme file, when there is one.
pub fn export(cfg: &GlazeConfig, name: &str, theme_file_src: Option<&str>) -> String {
    let mut doc = theme_file_src
        .and_then(|s| s.parse::<toml::Table>().ok())
        .unwrap_or_default();
    if let Some(o) = &cfg.appearance.overrides {
        merge(&mut doc, o);
    }
    let a = &cfg.appearance;
    if let Some(acc) = &a.accent {
        table_at(&mut doc, "palette").insert("accent".into(), Value::String(acc.clone()));
    }
    if let Some(r) = a.radius {
        let m = table_at(&mut doc, "metrics");
        m.insert("control_radius".into(), Value::Integer(r as i64));
        m.insert("menu_radius".into(), Value::Integer((r + 1).min(20) as i64));
        m.insert("window_radius".into(), Value::Integer(r.min(20) as i64));
        m.insert(
            "overlay_radius".into(),
            Value::Integer((r + 1).min(24) as i64),
        );
    }
    if let Some(w) = a.stroke_weight {
        table_at(&mut doc, "metrics").insert("stroke_width".into(), Value::Float(w));
    }
    if let Some(s) = a.shadow_strength {
        table_at(&mut doc, "shadow").insert("strength".into(), Value::Float(s));
    }
    let base = theme_file_src
        .and_then(|s| ThemeOverrides::from_toml_str(s).0.base)
        .unwrap_or(a.theme);
    let meta = table_at(&mut doc, "meta");
    meta.insert("name".into(), Value::String(name.to_string()));
    meta.insert("schema".into(), Value::Integer(THEME_SCHEMA));
    meta.insert("base".into(), Value::String(base.slug().to_string()));

    // Stable, readable section order: meta first.
    let mut out = String::from("# Glaze theme file, exported by glaze-settings.\n# Import it in Settings > Appearance, or copy it to ~/.config/glaze/themes/.\n\n");
    let mut ordered = toml::Table::new();
    for key in ["meta", "palette", "metrics", "typography", "shadow"] {
        if let Some(v) = doc.remove(key) {
            ordered.insert(key.to_string(), v);
        }
    }
    for (k, v) in doc {
        ordered.insert(k, v);
    }
    // `toml` keeps insertion order only with preserve_order; emit sections by hand.
    for key in ["meta", "palette", "metrics", "typography", "shadow"] {
        if let Some(Value::Table(t)) = ordered.get(key) {
            let mut one = toml::Table::new();
            one.insert(key.to_string(), Value::Table(t.clone()));
            out.push_str(&toml::to_string_pretty(&one).unwrap_or_default());
            out.push('\n');
        }
    }
    for (k, v) in &ordered {
        if ["meta", "palette", "metrics", "typography", "shadow"].contains(&k.as_str()) {
            continue;
        }
        let mut one = toml::Table::new();
        one.insert(k.clone(), v.clone());
        out.push_str(&toml::to_string_pretty(&one).unwrap_or_default());
        out.push('\n');
    }
    out
}

/// Result of checking a theme file before import.
#[derive(Debug, Clone)]
pub struct Checked {
    pub name: String,
    pub issues: Vec<Issue>,
    /// `false` when the file is not TOML at all (nothing can be imported).
    pub parses: bool,
}

impl Checked {
    #[cfg(test)]
    pub fn errors(&self) -> usize {
        self.issues.iter().filter(|i| i.is_error()).count()
    }
}

pub fn check(src: &str, file_stem: &str) -> Checked {
    let parses = src.parse::<toml::Table>().is_ok();
    let (o, issues) = ThemeOverrides::from_toml_str(src);
    let name = o
        .name
        .clone()
        .filter(|n| validate::theme_name(n).is_ok())
        .unwrap_or_else(|| file_stem.to_string());
    Checked {
        name,
        issues,
        parses,
    }
}

/// Where an imported theme goes: `<dir>/<slug>.toml`, with `-2`, `-3`, ...
/// when a different file already has that name. Re-importing identical
/// contents reuses the existing file.
pub fn destination(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let slug = validate::theme_slug(name);
    let mut n = 1;
    loop {
        let stem = if n == 1 {
            slug.clone()
        } else {
            format!("{slug}-{n}")
        };
        let p = dir.join(format!("{stem}.toml"));
        match std::fs::read_to_string(&p) {
            Ok(existing) if existing == contents => return p,
            Ok(_) => n += 1,
            Err(_) => return p,
        }
    }
}

/// The user theme directory (`~/.config/glaze/themes`).
pub fn user_theme_dir() -> PathBuf {
    glaze_common::paths::glaze_config_dir().join("themes")
}

/// Theme files found in the standard theme directories: (name, path).
pub fn installed() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for dir in glaze_common::config_check::theme_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        for p in files {
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if !out.iter().any(|(n, _)| *n == stem) {
                out.push((stem, p));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::tmp_dir;

    #[test]
    fn export_round_trips_through_the_shell_parser() {
        let mut cfg = GlazeConfig::default();
        cfg.appearance.accent = Some("#5b7f6a".into());
        cfg.appearance.radius = Some(8);
        cfg.appearance.stroke_weight = Some(1.25);
        cfg.appearance.shadow_strength = Some(0.5);
        let mut ov = toml::Table::new();
        let mut ty = toml::Table::new();
        ty.insert("display_family".into(), "\"Noto Serif\"".into());
        ov.insert("typography".into(), Value::Table(ty));
        cfg.appearance.overrides = Some(ov);
        let text = export(&cfg, "Sage", None);
        assert!(text.starts_with("# Glaze theme file"));
        let (o, issues) = ThemeOverrides::from_toml_str(&text);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(o.name.as_deref(), Some("Sage"));
        assert_eq!(o.int_metrics.get("control_radius"), Some(&8));
        assert_eq!(o.float_metrics.get("stroke_width"), Some(&1.25));
        assert_eq!(o.shadow_strength, Some(0.5));
        assert_eq!(o.display_family.as_deref(), Some("\"Noto Serif\""));
        assert_eq!(
            o.palette_any.get("accent").map(|c| c.to_hex()),
            Some("#5b7f6a".into())
        );
        // meta comes first.
        let meta = text.find("[meta]").unwrap();
        assert!(text.find("[palette]").unwrap() > meta);
    }

    #[test]
    fn export_keeps_the_active_theme_file() {
        let cfg = GlazeConfig::default();
        let src =
            "[meta]\nname = \"Old\"\nbase = \"classic\"\n[palette.dark]\nborder = \"#444444\"\n";
        let text = export(&cfg, "New", Some(src));
        let (o, issues) = ThemeOverrides::from_toml_str(&text);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(o.name.as_deref(), Some("New"));
        assert_eq!(o.base, Some(glaze_design::ThemeKind::Classic));
        assert!(o.palette_dark.contains_key("border"));
    }

    #[test]
    fn check_reports_bad_keys_and_names() {
        let c = check(
            "[meta]\nname = \"Night Kiln\"\n[palette]\naccent = \"#zz\"\nnope = 1\n",
            "file",
        );
        assert!(c.parses);
        assert_eq!(c.name, "Night Kiln");
        assert!(c.errors() >= 1, "{:?}", c.issues);
        assert!(c.issues.iter().any(|i| i.path.contains("accent")));
        let bad = check("not toml [", "fallback");
        assert!(!bad.parses);
        assert_eq!(bad.name, "fallback");
    }

    #[test]
    fn destination_avoids_clobbering() {
        let d = tmp_dir("themes");
        let p1 = destination(&d, "Warm Clay", "a");
        assert!(p1.ends_with("warm-clay.toml"));
        std::fs::write(&p1, "a").unwrap();
        assert_eq!(destination(&d, "Warm Clay", "a"), p1);
        let p2 = destination(&d, "Warm Clay", "b");
        assert!(p2.ends_with("warm-clay-2.toml"));
    }
}
