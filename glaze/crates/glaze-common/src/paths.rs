//! XDG base directory resolution shared by every Glaze component.
//!
//! Everything degrades gracefully: if `XDG_CONFIG_HOME` is unset we fall back
//! to `~/.config`, and if we cannot determine a home directory at all the
//! callers still receive a deterministic relative path rather than panicking.

use std::path::{Path, PathBuf};

use crate::ipc::SOCKET_NAME;

/// The user's home directory, if it can be determined.
pub fn home_dir() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    // Windows dev host: USERPROFILE. Glaze is Linux-first, but keeping the
    // portable crates testable on Windows is deliberate.
    if let Some(h) = std::env::var_os("USERPROFILE") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    None
}

/// Resolve a base directory: explicit XDG variable, else `$HOME/<fallback>`.
fn xdg_dir(var: &str, fallback: &str, home: Option<PathBuf>) -> PathBuf {
    if let Some(v) = std::env::var_os(var) {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    match home {
        Some(h) => h.join(fallback),
        None => PathBuf::from(fallback),
    }
}

/// `$XDG_CONFIG_HOME` (default `~/.config`).
pub fn config_home() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config", home_dir())
}

/// `$XDG_DATA_HOME` (default `~/.local/share`).
pub fn data_home() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share", home_dir())
}

/// `$XDG_STATE_HOME` (default `~/.local/state`).
pub fn state_home() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state", home_dir())
}

/// One-time migration from the project's former name (FeDe): copy
/// `<xdg>/fede` to `<xdg>/glaze` for config, data and state when the new
/// directory does not exist yet (the old one is left untouched), and
/// translate a legacy `session-backend` value `fede` to `glaze`. Returns
/// the directories copied.
pub fn migrate_legacy_dirs() -> Vec<PathBuf> {
    let mut done = Vec::new();
    for base in [config_home(), data_home(), state_home()] {
        let old = base.join("fede");
        let new = base.join("glaze");
        // Merge once (files missing in the new directory only), even when
        // something already created the new directory (an installer, the
        // colour-scheme helper); a marker keeps it a one-time step.
        let marker = new.join(".migrated-from-fede");
        if old.is_dir() && !marker.exists() && copy_tree(&old, &new).is_ok() {
            let _ = std::fs::write(&marker, b"");
            done.push(new);
        }
    }
    let backend = config_home().join("glaze/session-backend");
    if let Ok(v) = std::fs::read_to_string(&backend) {
        if v.trim() == "fede" {
            let _ = std::fs::write(&backend, "glaze\n");
        }
    }
    // Paths inside migrated text files still name the old directory.
    for f in ["sway.conf", "sway-session.conf"] {
        let p = config_home().join("glaze").join(f);
        if let Ok(t) = std::fs::read_to_string(&p) {
            if t.contains("/fede/") || t.contains("fede-") {
                let t = t.replace("/.config/fede/", "/.config/glaze/").replace("fede-", "glaze-");
                let _ = std::fs::write(&p, t);
            }
        }
    }
    done
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if ty.is_symlink() {
            #[cfg(unix)]
            {
                let link = std::fs::read_link(entry.path())?;
                let _ = std::os::unix::fs::symlink(link, &target);
            }
        } else if !target.exists() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// `$XDG_CACHE_HOME` (default `~/.cache`).
pub fn cache_home() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache", home_dir())
}

/// `$XDG_RUNTIME_DIR`, or a per-user temp fallback when unset.
pub fn runtime_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("XDG_RUNTIME_DIR") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    std::env::temp_dir().join(format!("glaze-{}", current_uid()))
}

/// Best-effort numeric uid for the runtime fallback path (non-unix: 0).
pub fn current_uid() -> u32 {
    #[cfg(unix)]
    {
        // The audit login UID may be unset; use the actual process UID.
        unsafe { getuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

#[cfg(unix)]
extern "C" {
    fn getuid() -> u32;
}

/// Glaze's config directory (`$XDG_CONFIG_HOME/glaze`).
pub fn glaze_config_dir() -> PathBuf {
    config_home().join("glaze")
}

/// Path to `config.toml`.
pub fn config_file() -> PathBuf {
    glaze_config_dir().join("config.toml")
}

/// Path to the launcher/usage state file.
pub fn state_file() -> PathBuf {
    state_home().join("glaze").join("state.toml")
}

/// Filesystem path of the service control socket.
pub fn control_socket() -> PathBuf {
    runtime_dir().join("glaze").join(SOCKET_NAME)
}

/// The XDG data directories, in precedence order, used for application lookup.
pub fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(v) = std::env::var_os("XDG_DATA_DIRS") {
        for part in std::env::split_paths(&v) {
            if !part.as_os_str().is_empty() {
                dirs.push(part);
            }
        }
    }
    if dirs.is_empty() {
        dirs.push(PathBuf::from("/usr/local/share"));
        dirs.push(PathBuf::from("/usr/share"));
    }
    // User data home takes precedence over system directories.
    dirs.insert(0, data_home());
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_file_lives_under_glaze() {
        let p = config_file();
        assert!(p.ends_with("glaze/config.toml") || p.ends_with("glaze\\config.toml"));
    }

    #[test]
    fn data_dirs_includes_user_home_first() {
        let dirs = data_dirs();
        assert!(!dirs.is_empty());
        // The first entry is always the user's own data home.
        assert_eq!(dirs[0], data_home());
    }

    #[test]
    fn socket_ends_with_expected_name() {
        assert!(control_socket().ends_with(SOCKET_NAME));
    }
}
