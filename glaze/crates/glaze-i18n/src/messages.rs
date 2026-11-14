//! The shell-facing translation entry point.
//!
//! Two things live here:
//!
//! - [`Keys`] — the stable, typed list of every string Glaze's shell and
//!   settings app currently use. Keys are constants so a typo is a compile
//!   error and [`audit`] can prove the English catalogue covers all of them.
//! - [`tr`] — the accessor the shell calls at render time. It reads the active
//!   catalogue (set once at startup from the POSIX environment via
//!   [`install_from_env`]) and falls back English-first.
//!
//! Keys are intentionally short, human labels (`GLAZE.md` §1.1, §3): no
//! sentence-long UI copy and no placeholders yet.

use std::sync::OnceLock;

use crate::catalog::{self, Catalog, SOURCE_LOCALE};

/// Stable message identifiers used across the Glaze shell.
///
/// Add a constant here **and** an entry in `locales/en.txt`; the [`audit`] test
/// fails otherwise.
pub struct Keys;

impl Keys {
    // Panel / taskbar.
    /// `Start` — the panel's start button.
    pub const START: &'static str = "panel.start";
    /// `Show Desktop` — minimise every window.
    pub const SHOW_DESKTOP: &'static str = "panel.show_desktop";

    // Launcher.
    /// `Search` — launcher search field label.
    pub const SEARCH: &'static str = "launcher.search";
    /// `Applications` — launcher section heading.
    pub const APPLICATIONS: &'static str = "launcher.applications";
    /// `All Applications` — the full application list.
    pub const ALL_APPLICATIONS: &'static str = "launcher.all_applications";
    /// `Recent` — recently used applications.
    pub const RECENT: &'static str = "launcher.recent";
    /// `No applications found` — empty search result.
    pub const NO_APPLICATIONS_FOUND: &'static str = "launcher.no_results";

    // Power / session menu.
    /// `Settings` — open the settings app.
    pub const SETTINGS: &'static str = "menu.settings";
    /// `Power Off` — shut the machine down.
    pub const POWER_OFF: &'static str = "menu.power_off";
    /// `Restart` — reboot the machine.
    pub const RESTART: &'static str = "menu.restart";
    /// `Log Out` — end the session.
    pub const LOG_OUT: &'static str = "menu.log_out";
    /// `Lock` — lock the session.
    pub const LOCK: &'static str = "menu.lock";

    // Desktop / icon context menus (labels used by `glaze-desktop`).
    /// `Open` — open the selected item.
    pub const MENU_OPEN: &'static str = "menu.open";
    /// `Open With` — choose an application for a file.
    pub const MENU_OPEN_WITH: &'static str = "menu.open_with";
    /// `Rename` — rename a shortcut.
    pub const MENU_RENAME: &'static str = "menu.rename";
    /// `Properties` — show item details.
    pub const MENU_PROPERTIES: &'static str = "menu.properties";
    /// `New Folder` — create a folder on the desktop.
    pub const MENU_NEW_FOLDER: &'static str = "menu.new_folder";
    /// `Paste` — paste from the clipboard.
    pub const MENU_PASTE: &'static str = "menu.paste";
    /// `Arrange Icons` — re-run the desktop grid layout.
    pub const MENU_ARRANGE_ICONS: &'static str = "menu.arrange_icons";
    /// `Change Wallpaper` — pick a new wallpaper.
    pub const MENU_CHANGE_WALLPAPER: &'static str = "menu.change_wallpaper";
    /// `Reset Wallpaper` — restore the theme canvas.
    pub const MENU_RESET_WALLPAPER: &'static str = "menu.reset_wallpaper";
    /// `Empty Trash` — permanently delete the trash contents (destructive).
    pub const MENU_EMPTY_TRASH: &'static str = "menu.empty_trash";
    /// `Edit Command` — edit a custom shortcut's command.
    pub const MENU_EDIT_COMMAND: &'static str = "menu.edit_command";

    // Settings page titles.
    /// `Settings` — settings window title.
    pub const SETTINGS_TITLE: &'static str = "settings.title";
    /// `Appearance` — appearance page.
    pub const SETTINGS_APPEARANCE: &'static str = "settings.appearance";
    /// `Panel` — panel page.
    pub const SETTINGS_PANEL: &'static str = "settings.panel";
    /// `Wallpaper` — wallpaper page.
    pub const SETTINGS_WALLPAPER: &'static str = "settings.wallpaper";
    /// `Language & Region` — language page.
    pub const SETTINGS_LANGUAGE: &'static str = "settings.language";
    /// `About` — about page.
    pub const SETTINGS_ABOUT: &'static str = "settings.about";

    /// Every key Glaze currently uses. Keep in sync with the constants above;
    /// [`audit`] enforces that each one exists in `en`.
    pub const ALL: &'static [&'static str] = &[
        Self::START,
        Self::SHOW_DESKTOP,
        Self::SEARCH,
        Self::APPLICATIONS,
        Self::ALL_APPLICATIONS,
        Self::RECENT,
        Self::NO_APPLICATIONS_FOUND,
        Self::SETTINGS,
        Self::POWER_OFF,
        Self::RESTART,
        Self::LOG_OUT,
        Self::LOCK,
        Self::MENU_OPEN,
        Self::MENU_OPEN_WITH,
        Self::MENU_RENAME,
        Self::MENU_PROPERTIES,
        Self::MENU_NEW_FOLDER,
        Self::MENU_PASTE,
        Self::MENU_ARRANGE_ICONS,
        Self::MENU_CHANGE_WALLPAPER,
        Self::MENU_RESET_WALLPAPER,
        Self::MENU_EMPTY_TRASH,
        Self::MENU_EDIT_COMMAND,
        Self::SETTINGS_TITLE,
        Self::SETTINGS_APPEARANCE,
        Self::SETTINGS_PANEL,
        Self::SETTINGS_WALLPAPER,
        Self::SETTINGS_LANGUAGE,
        Self::SETTINGS_ABOUT,
    ];
}

static ACTIVE: OnceLock<Catalog> = OnceLock::new();

/// Install the catalogue for `locale` as the process-wide active catalogue.
///
/// Returns `false` if a catalogue was already installed (installation only
/// happens once, at startup). Unknown locales still install: they simply
/// resolve everything through English.
pub fn install(locale: &str) -> bool {
    ACTIVE.set(catalog::catalog_for(locale)).is_ok()
}

/// Detect the locale from the POSIX environment and install it.
///
/// The shell calls this once during startup. [`locale::detect`] is the actual
/// detection; this is the convenience wiring.
///
/// [`locale::detect`]: crate::locale::detect
pub fn install_from_env() -> bool {
    install(&crate::locale::detect())
}

/// The active locale tag (`en` before anything is installed).
pub fn active_locale() -> &'static str {
    match ACTIVE.get() {
        Some(catalog) => catalog.locale.as_str(),
        None => SOURCE_LOCALE,
    }
}

/// Translate `key` with the active catalogue.
///
/// Falls back to English and then to the key itself, so this never returns an
/// empty string. Returns an owned `String` because the active catalogue is set
/// at runtime; shell labels are short and set once per widget anyway.
pub fn tr(key: &str) -> String {
    match ACTIVE.get() {
        Some(catalog) => catalog.get(key).to_string(),
        None => catalog::source_catalog().get(key).to_string(),
    }
}

/// Keys from [`Keys::ALL`] that are missing from the English catalogue.
///
/// An empty result means the source catalogue is complete.
pub fn audit() -> Vec<String> {
    audit_locale(SOURCE_LOCALE)
}

/// Keys from [`Keys::ALL`] that are missing from `locale`'s merged catalogue.
///
/// A fully translated locale returns an empty vector; a partial locale returns
/// the still-untranslated keys (they would fall back to English at runtime).
pub fn audit_locale(locale: &str) -> Vec<String> {
    let catalog = catalog::catalog_for(locale);
    Keys::ALL
        .iter()
        .filter(|key| !catalog.contains(key))
        .map(|key| (*key).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_finds_every_key_in_english() {
        let missing = audit();
        assert!(
            missing.is_empty(),
            "keys missing from locales/en.txt: {missing:?}"
        );
    }

    #[test]
    fn each_key_constant_is_unique_and_namespaced() {
        let mut seen = std::collections::BTreeSet::new();
        for key in Keys::ALL {
            assert!(seen.insert(*key), "duplicate key constant: {key}");
            assert!(key.contains('.'), "key is not namespaced: {key}");
            assert!(
                key.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "key should be lowercase ascii with dots/underscores: {key}"
            );
        }
        assert_eq!(seen.len(), Keys::ALL.len());
    }

    #[test]
    fn tr_without_install_returns_english() {
        // No test installs ACTIVE (see `install_is_*`), so this is the source
        // catalogue path and is deterministic.
        assert_eq!(tr(Keys::START), "Start");
        assert_eq!(tr(Keys::NO_APPLICATIONS_FOUND), "No applications found");
    }

    #[test]
    fn tr_unknown_key_returns_the_key() {
        assert_eq!(tr("this.key.does.not.exist"), "this.key.does.not.exist");
    }

    #[test]
    fn active_locale_defaults_to_english() {
        assert_eq!(active_locale(), SOURCE_LOCALE);
    }

    #[test]
    fn every_embedded_locale_loads_and_is_non_empty() {
        for locale in catalog::embedded_locales() {
            let catalog = catalog::catalog_for(locale);
            assert!(
                !catalog.is_empty(),
                "embedded locale {locale} produced an empty catalogue"
            );
            for key in Keys::ALL {
                let value = catalog.get(key);
                assert!(
                    !value.trim().is_empty(),
                    "locale {locale} has an empty value for {key}"
                );
            }
        }
    }

    #[test]
    fn shipped_translations_are_complete_for_the_current_keys() {
        for locale in catalog::embedded_locales() {
            let missing = audit_locale(locale);
            assert!(
                missing.is_empty(),
                "locale {locale} is missing: {missing:?}"
            );
        }
    }

    /// Proves each shipped `locales/<lang>.txt` file *itself* translates every
    /// key. `audit_locale` merges with English, so it cannot detect a missing
    /// translation; this reads the raw file instead.
    #[test]
    fn every_shipped_locale_file_translates_every_key() {
        for locale in catalog::embedded_locales() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("locales")
                .join(format!("{locale}.txt"));
            let cat = Catalog::from_file(locale, &path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let missing: Vec<&str> = Keys::ALL
                .iter()
                .copied()
                .filter(|k| !cat.contains(k))
                .collect();
            assert!(
                missing.is_empty(),
                "locales/{locale}.txt is missing translations for: {missing:?}"
            );
        }
    }

    #[test]
    fn translation_actually_differs_from_english() {
        // Guards against a locale file that was copied from en without editing.
        let de = catalog::catalog_for("de");
        assert_ne!(
            de.get(Keys::SEARCH),
            catalog::source_catalog().get(Keys::SEARCH)
        );
        let fr = catalog::catalog_for("fr");
        assert_ne!(
            fr.get(Keys::LOG_OUT),
            catalog::source_catalog().get(Keys::LOG_OUT)
        );
    }
}
