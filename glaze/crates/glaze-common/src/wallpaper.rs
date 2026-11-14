//! Bundled wallpaper manifest and selection.
//!
//! Wallpapers are original SVG artwork under `assets/wallpapers/` described by
//! `manifest.toml`. A *set* has up to four variants (light/dark x
//! landscape/portrait). [`resolve`] turns the user's config plus the current
//! scheme and orientation into a real file path, or `None` (theme canvas).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{ColorScheme, GlazeConfig};

/// Manifest file name inside a wallpaper directory.
pub const MANIFEST_FILE: &str = "manifest.toml";

/// Screen orientation a variant is authored for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Orientation {
    /// 16:9 desktop artwork.
    Landscape,
    /// 9:19.5 phone artwork.
    Portrait,
}

impl Orientation {
    /// Orientation for an output size in pixels.
    pub fn for_size(width: i32, height: i32) -> Self {
        if height > width {
            Orientation::Portrait
        } else {
            Orientation::Landscape
        }
    }
}

/// One image file of a set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub scheme: ColorScheme,
    pub orientation: Orientation,
    pub file: String,
    pub width: u32,
    pub height: u32,
}

/// A named wallpaper set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WallpaperSet {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "variant")]
    pub variants: Vec<Variant>,
}

/// The parsed `manifest.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub schema: u32,
    #[serde(default, rename = "set")]
    pub sets: Vec<WallpaperSet>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Load `<dir>/manifest.toml`; `None` when missing or invalid.
    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?;
        Self::parse(&text).ok()
    }

    pub fn set(&self, id: &str) -> Option<&WallpaperSet> {
        self.sets.iter().find(|s| s.id == id)
    }
}

impl WallpaperSet {
    /// Best variant: exact match, else the same orientation in the other
    /// scheme (never a wrong-orientation image, which would crop badly).
    pub fn pick(&self, scheme: ColorScheme, orientation: Orientation) -> Option<&Variant> {
        self.variants
            .iter()
            .find(|v| v.scheme == scheme && v.orientation == orientation)
            .or_else(|| self.variants.iter().find(|v| v.orientation == orientation))
    }
}

/// Directories searched for `manifest.toml`, best first.
///
/// `$GLAZE_WALLPAPER_DIR`, the user data dir, `/usr/local/share` and
/// `/usr/share` (`glaze/wallpapers`), then `assets/wallpapers` relative to the
/// working directory and the executable (development checkouts).
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(v) = std::env::var_os("GLAZE_WALLPAPER_DIR") {
        if !v.is_empty() {
            dirs.push(PathBuf::from(v));
        }
    }
    dirs.push(crate::paths::data_home().join("glaze/wallpapers"));
    dirs.push(PathBuf::from("/usr/local/share/glaze/wallpapers"));
    dirs.push(PathBuf::from("/usr/share/glaze/wallpapers"));
    dirs.push(PathBuf::from("assets/wallpapers"));
    if let Ok(exe) = std::env::current_exe() {
        // target/<profile>/glaze-shell -> repo root
        for up in exe.ancestors().skip(1).take(5) {
            dirs.push(up.join("assets/wallpapers"));
        }
    }
    dirs
}

/// First directory in `dirs` that has a loadable manifest.
pub fn load_manifest(dirs: &[PathBuf]) -> Option<(PathBuf, Manifest)> {
    dirs.iter()
        .find_map(|d| Manifest::load_from_dir(d).map(|m| (d.clone(), m)))
}

/// The wallpaper file to paint, or `None` for the plain theme canvas.
///
/// Order: explicit `desktop.wallpaper` file (if it exists), then the selected
/// `desktop.wallpaper_set` resolved through the manifest for `scheme` and
/// `orientation`. A variant whose file is missing yields `None`: nothing is
/// invented.
pub fn resolve(
    cfg: &GlazeConfig,
    scheme: ColorScheme,
    orientation: Orientation,
    dirs: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(p) = cfg.desktop.wallpaper.as_ref().filter(|p| p.is_file()) {
        return Some(p.clone());
    }
    let id = cfg.desktop.wallpaper_set.as_str();
    if id.is_empty() {
        return None;
    }
    let (dir, manifest) = load_manifest(dirs)?;
    let variant = manifest.set(id)?.pick(scheme, orientation)?;
    let path = dir.join(&variant.file);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
schema = 1
[[set]]
id = "demo"
title = "Demo"
[[set.variant]]
scheme = "light"
orientation = "landscape"
file = "demo-light-desktop.svg"
width = 1920
height = 1080
[[set.variant]]
scheme = "dark"
orientation = "portrait"
file = "demo-dark-mobile.svg"
width = 1080
height = 2340
"#;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("glaze-wp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn orientation_from_size() {
        assert_eq!(Orientation::for_size(1920, 1080), Orientation::Landscape);
        assert_eq!(Orientation::for_size(1080, 2340), Orientation::Portrait);
    }

    #[test]
    fn pick_prefers_exact_then_same_orientation_never_wrong_orientation() {
        let m = Manifest::parse(SAMPLE).unwrap();
        let s = m.set("demo").unwrap();
        assert_eq!(
            s.pick(ColorScheme::Light, Orientation::Landscape)
                .unwrap()
                .file,
            "demo-light-desktop.svg"
        );
        // Dark landscape is missing: falls back to the light landscape file.
        assert_eq!(
            s.pick(ColorScheme::Dark, Orientation::Landscape)
                .unwrap()
                .file,
            "demo-light-desktop.svg"
        );
        assert!(s.pick(ColorScheme::Light, Orientation::Portrait).is_some());
        let only_land = WallpaperSet {
            variants: vec![s.variants[0].clone()],
            ..s.clone()
        };
        assert!(only_land
            .pick(ColorScheme::Light, Orientation::Portrait)
            .is_none());
    }

    #[test]
    fn resolve_uses_manifest_and_requires_the_file_to_exist() {
        let dir = scratch("resolve");
        std::fs::write(dir.join(MANIFEST_FILE), SAMPLE).unwrap();
        let mut cfg = GlazeConfig::default();
        cfg.desktop.wallpaper_set = "demo".into();
        let dirs = vec![dir.clone()];
        // File not present yet: nothing is invented.
        assert_eq!(
            resolve(&cfg, ColorScheme::Light, Orientation::Landscape, &dirs),
            None
        );
        std::fs::write(dir.join("demo-light-desktop.svg"), "<svg/>").unwrap();
        assert_eq!(
            resolve(&cfg, ColorScheme::Light, Orientation::Landscape, &dirs),
            Some(dir.join("demo-light-desktop.svg"))
        );
        cfg.desktop.wallpaper_set = "unknown".into();
        assert_eq!(
            resolve(&cfg, ColorScheme::Light, Orientation::Landscape, &dirs),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn explicit_path_beats_the_set() {
        let dir = scratch("explicit");
        let file = dir.join("mine.png");
        std::fs::write(&file, "x").unwrap();
        let mut cfg = GlazeConfig::default();
        cfg.desktop.wallpaper = Some(file.clone());
        assert_eq!(
            resolve(&cfg, ColorScheme::Dark, Orientation::Portrait, &[]),
            Some(file)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real bundled manifest must be valid and every file must exist.
    #[test]
    fn bundled_manifest_is_consistent() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/wallpapers");
        let m = Manifest::load_from_dir(&dir).expect("assets/wallpapers/manifest.toml");
        assert!(m.set(crate::config::DEFAULT_WALLPAPER_SET).is_some());
        for set in &m.sets {
            for scheme in ColorScheme::ALL {
                for o in [Orientation::Landscape, Orientation::Portrait] {
                    // Single-scheme sets (e.g. dark-only night-paper) are
                    // valid: `pick` falls back to the other scheme.
                    let v = set
                        .pick(scheme, o)
                        .unwrap_or_else(|| panic!("{} cannot serve {scheme:?}/{o:?}", set.id));
                    assert!(dir.join(&v.file).is_file(), "missing {}", v.file);
                    let ratio = v.width as f64 / v.height as f64;
                    match o {
                        Orientation::Landscape => assert!((ratio - 16.0 / 9.0).abs() < 0.01),
                        Orientation::Portrait => assert!((ratio - 9.0 / 19.5).abs() < 0.01),
                    }
                }
            }
        }
    }
}
