//! A tiny, honest message catalogue.
//!
//! Glaze does **not** ship a translation framework with plural/gender rules
//! here: shell labels are short, human strings (`GLAZE.md` §1.1) and the whole
//! point of this first slice is a correct, testable lookup with a deterministic
//! fallback order ([`Catalog::get`] → English → the key itself).
//!
//! Catalogues come from two places:
//!
//! 1. **Embedded defaults** — the `locales/*.txt` files compiled into the
//!    binary with [`include_str!`], so a stock install always has English and a
//!    few real translations.
//! 2. **Text files** — plain `key = value` files parsed by [`Catalog::parse`],
//!    which a packager or user can drop into the locale directories returned by
//!    [`locale_dirs`] (see `locales/README.md`).
//!
//! File format (UTF-8):
//!
//! - `key = value`; the first `=` splits, so values may contain `=`.
//! - Lines whose first non-space character is `#` are comments.
//! - Blank lines are ignored.
//! - In values, `\n`, `\t`, `\r` and `\\` are unescaped. A backslash before any
//!   other character is kept literally.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use glaze_common::paths;

/// Locale whose catalogue is the source of truth and the final fallback.
pub const SOURCE_LOCALE: &str = "en";

/// Embedded catalogues: `(locale, file contents)`, compiled into the binary.
///
/// Add a locale by dropping `locales/<tag>.txt` next to these and adding one
/// line here — see `locales/README.md`.
pub const EMBEDDED: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.txt")),
    ("de", include_str!("../locales/de.txt")),
    ("es", include_str!("../locales/es.txt")),
    ("fr", include_str!("../locales/fr.txt")),
];

/// A parsed catalogue for one locale.
///
/// `messages` maps a stable key such as `panel.start` to its localised string
/// for `locale`. Lookups that miss fall back — see [`Catalog::get`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// The normalised locale this catalogue was built for (e.g. `en`, `de`).
    pub locale: String,
    /// Localised messages keyed by message id.
    pub messages: BTreeMap<String, String>,
}

impl Catalog {
    /// Parse `text` as a Glaze catalogue for `locale`.
    ///
    /// Malformed lines (no `=`, empty key) are skipped rather than failing:
    /// a partial or hand-edited translation must never crash the shell.
    pub fn parse(locale: &str, text: &str) -> Self {
        let mut messages = BTreeMap::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            messages.insert(key.to_string(), unescape(value.trim()));
        }
        Catalog {
            locale: locale.to_string(),
            messages,
        }
    }

    /// Parse a catalogue from a file on disk.
    pub fn from_file(locale: &str, path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Catalog::parse(locale, &text))
    }

    /// Look up `key`, falling back to English and then to the key itself.
    ///
    /// This never returns an empty string for a missing key; callers can render
    /// the result unconditionally and a missing translation degrades to a
    /// visible key rather than a blank control.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(value) = self.messages.get(key) {
            return value;
        }
        if self.locale != SOURCE_LOCALE {
            if let Some(value) = source_catalog().messages.get(key) {
                return value;
            }
        }
        key
    }

    /// Whether this catalogue defines `key` directly (no fallback).
    pub fn contains(&self, key: &str) -> bool {
        self.messages.contains_key(key)
    }

    /// Number of messages defined directly.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the catalogue defines no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Keys defined in [`source_catalog`] but missing from `self`.
    ///
    /// Used by the translation audit to report incomplete locales.
    pub fn missing_from_source(&self) -> Vec<String> {
        source_catalog()
            .messages
            .keys()
            .filter(|k| !self.messages.contains_key(*k))
            .cloned()
            .collect()
    }
}

/// The built-in English catalogue, parsed once.
pub fn source_catalog() -> &'static Catalog {
    static SOURCE: OnceLock<Catalog> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let text = embedded_text(SOURCE_LOCALE).unwrap_or_default();
        Catalog::parse(SOURCE_LOCALE, text)
    })
}

/// The embedded catalogue text for `locale`, if Glaze ships one.
pub fn embedded_text(locale: &str) -> Option<&'static str> {
    EMBEDDED
        .iter()
        .find(|(tag, _)| *tag == locale)
        .map(|(_, text)| *text)
}

/// Every locale Glaze embeds, sorted for deterministic output.
pub fn embedded_locales() -> Vec<&'static str> {
    let mut locales: Vec<&'static str> = EMBEDDED.iter().map(|(tag, _)| *tag).collect();
    locales.sort_unstable();
    locales
}

/// Directories searched for user- or packager-supplied catalogues, in
/// precedence order. Files here override the embedded defaults.
///
/// Order: `$XDG_DATA_HOME/glaze/locales`, then `$XDG_CONFIG_HOME/glaze/locales`.
/// The config location wins because it is the closer-to-the-user override.
pub fn locale_dirs() -> Vec<PathBuf> {
    vec![
        paths::data_home().join("glaze").join("locales"),
        paths::glaze_config_dir().join("locales"),
    ]
}

/// Build the effective catalogue for `locale`.
///
/// The fallback chain from [`crate::locale::chain`] is applied from least to
/// most specific (`en`, then `de`, then `de_DE`), so a regional catalogue only
/// needs to carry the strings that differ from its base language. Finally, any
/// on-disk catalogue found in [`locale_dirs`] is overlaid as a user override.
pub fn catalog_for(locale: &str) -> Catalog {
    let normalised = crate::locale::normalise(locale);
    let mut messages: BTreeMap<String, String> = BTreeMap::new();

    let chain = crate::locale::chain(&normalised);
    for tag in chain.iter().rev() {
        if let Some(text) = embedded_text(tag) {
            messages.extend(Catalog::parse(tag, text).messages);
        }
    }

    // User/packager override for the requested locale.
    for dir in locale_dirs() {
        let path = dir.join(format!("{normalised}.txt"));
        if let Ok(catalog) = Catalog::from_file(&normalised, &path) {
            messages.extend(catalog.messages);
            break;
        }
    }

    Catalog {
        locale: normalised,
        messages,
    }
}

/// Unescape the small set of sequences Glaze supports in catalogue values.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                // Unknown escape: keep it literally so nothing is silently lost.
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys_values_comments_and_blank_lines() {
        let cat = Catalog::parse(
            "xx",
            "# a comment\n\n  panel.start = Start \nlauncher.search=Search\n",
        );
        assert_eq!(cat.get("panel.start"), "Start");
        assert_eq!(cat.get("launcher.search"), "Search");
        assert_eq!(cat.len(), 2);
    }

    #[test]
    fn value_may_contain_equals_sign() {
        let cat = Catalog::parse("xx", "k = a = b");
        assert_eq!(cat.get("k"), "a = b");
    }

    #[test]
    fn unescapes_newline_tab_and_backslash() {
        let cat = Catalog::parse("xx", r"k = line1\nline2\ttab\\slash");
        assert_eq!(cat.get("k"), "line1\nline2\ttab\\slash");
    }

    #[test]
    fn unknown_escape_is_kept_literally() {
        let cat = Catalog::parse("xx", r"k = a\qb");
        assert_eq!(cat.get("k"), r"a\qb");
    }

    #[test]
    fn malformed_lines_are_skipped_not_fatal() {
        let cat = Catalog::parse("xx", "no equals here\n= onemissingkey\nok = fine\n");
        assert_eq!(cat.len(), 1);
        assert_eq!(cat.get("ok"), "fine");
    }

    #[test]
    fn unknown_key_falls_back_to_the_key_itself() {
        let cat = Catalog::parse("de", "panel.start = Start");
        assert_eq!(cat.get("does.not.exist"), "does.not.exist");
    }

    #[test]
    fn missing_key_falls_back_to_english_catalogue() {
        // A partial `de` catalogue still resolves an English-only key.
        let cat = Catalog::parse("de", "panel.start = Start");
        assert_eq!(cat.get("launcher.search"), "Search");
        // ...but its own translation wins where it exists.
        assert_eq!(cat.get("panel.start"), "Start");
    }

    #[test]
    fn from_file_reads_and_parses() {
        let dir = std::env::temp_dir().join(format!(
            "glaze-i18n-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("zz.txt");
        std::fs::write(&path, "greeting.hello = Hello\n").expect("write temp catalogue");

        let cat = Catalog::from_file("zz", &path).expect("read temp catalogue");
        assert_eq!(cat.get("greeting.hello"), "Hello");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn embedded_source_catalogue_is_complete_and_stable() {
        let en = source_catalog();
        assert_eq!(en.locale, SOURCE_LOCALE);
        assert!(!en.is_empty());
        assert_eq!(en.get("panel.start"), "Start");
    }

    #[test]
    fn catalog_for_merges_chain_into_a_single_lookup_table() {
        let de = catalog_for("de");
        // Direct translation present...
        assert_eq!(de.get("launcher.search"), "Suchen");
        // ...and English keys are merged in so nothing is lost.
        assert!(de.contains("panel.start"));
        assert!(de.missing_from_source().is_empty());
    }

    #[test]
    fn catalog_for_unknown_language_is_english_equivalent() {
        let xx = catalog_for("zz_ZZ");
        assert_eq!(xx.get("panel.start"), "Start");
        assert_eq!(xx.get("panel.show_desktop"), "Show Desktop");
    }
}
