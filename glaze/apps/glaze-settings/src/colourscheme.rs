//! Bridge to `glaze-colour-scheme` (packaging/kilnlinux/bin), which switches
//! the light/dark preference for every toolkit: GTK 3/4 and libadwaita
//! (gsettings `color-scheme` plus settings.ini), portal-aware apps, Sway
//! frame colours, and Qt (through `~/.config/glaze/colour-scheme`, read by
//! `/etc/profile.d/kiln-theme.sh` when a Qt app starts).
//!
//! Glaze's own scheme lives in `config.toml` (`appearance.scheme`,
//! `appearance.scheme_mode`). Settings calls the tool whenever the effective
//! scheme changes so that other apps follow.

use std::path::PathBuf;

use glaze_design::ColorScheme;

/// Where the tool is installed (PATH first, then the Kiln default).
pub fn tool() -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("glaze-colour-scheme"))
            .find(|c| c.is_file())
    });
    from_path.or_else(|| {
        let p = PathBuf::from("/usr/local/bin/glaze-colour-scheme");
        p.is_file().then_some(p)
    })
}

pub fn arg(s: ColorScheme) -> &'static str {
    match s {
        ColorScheme::Dark => "dark",
        _ => "light",
    }
}

/// The scheme other apps were last switched to (`~/.config/glaze/colour-scheme`).
pub fn last_applied() -> Option<ColorScheme> {
    let p = glaze_common::paths::config_home().join("glaze/colour-scheme");
    parse_state(&std::fs::read_to_string(p).ok()?)
}

pub fn parse_state(text: &str) -> Option<ColorScheme> {
    match text.trim() {
        "dark" => Some(ColorScheme::Dark),
        "light" => Some(ColorScheme::Light),
        _ => None,
    }
}

/// Run `glaze-colour-scheme light|dark` and wait for it (it only calls
/// gsettings, edits two ini files and sends one Sway command).
pub fn apply(s: ColorScheme) -> Result<(), String> {
    let Some(t) = tool() else {
        return Err("glaze-colour-scheme is not installed; only Glaze follows the scheme".into());
    };
    let st = std::process::Command::new(&t)
        .arg(arg(s))
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("{}: {e}", t.display()))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("{} {} failed ({st})", t.display(), arg(s)))
    }
}

/// One line for the About and Appearance pages on how Qt apps follow.
pub fn qt_status() -> String {
    let profile = std::path::Path::new("/etc/profile.d/kiln-theme.sh").is_file();
    let env = std::env::var("QT_QPA_PLATFORMTHEME").unwrap_or_default();
    match (profile, env.as_str()) {
        (true, "gtk3") | (true, "gtk2") => {
            "Qt apps follow the GTK theme (QT_QPA_PLATFORMTHEME=gtk3); a scheme change reaches them when they next start."
                .into()
        }
        (true, _) => "Qt apps follow the scheme after the next login (/etc/profile.d/kiln-theme.sh).".into(),
        (false, "") => "Qt apps use their own style (no Kiln Qt theme bridge installed).".into(),
        (false, e) => format!("Qt apps use QT_QPA_PLATFORMTHEME={e}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_file() {
        assert_eq!(parse_state("dark\n"), Some(ColorScheme::Dark));
        assert_eq!(parse_state("light"), Some(ColorScheme::Light));
        assert_eq!(parse_state("purple"), None);
        assert_eq!(arg(ColorScheme::Dark), "dark");
        assert_eq!(arg(ColorScheme::Light), "light");
    }
}
