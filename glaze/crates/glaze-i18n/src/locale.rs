//! POSIX locale detection and normalisation.
//!
//! Glaze reads the same environment variables every POSIX program uses —
//! `LC_ALL`, `LC_MESSAGES`, `LANG`, first non-empty wins — and reduces the raw
//! POSIX string to the `language[_TERRITORY]` tag Glaze keys its catalogues on:
//!
//! ```text
//! en_GB.UTF-8   -> en_GB      (codeset stripped)
//! sr_RS@latin   -> sr_RS      (modifier stripped)
//! C / POSIX / "" -> en        (the C locale has no messages of its own)
//! en-us         -> en_US      (separator and case normalised)
//! ```
//!
//! [`chain`] turns a tag into the ordered fallback list used by the catalogue
//! loader: `sr_RS -> ["sr_RS", "sr", "en"]`.

use std::env;

use crate::catalog::SOURCE_LOCALE;

/// The locale Glaze falls back to when nothing usable is configured.
pub const SOURCE: &str = SOURCE_LOCALE;

/// Detect the user's locale from the standard POSIX environment variables.
///
/// Returns a normalised tag; `en` when none of the variables is set/usable.
pub fn detect() -> String {
    detect_with(|name| env::var(name).ok())
}

/// Testable core of [`detect`] with an injectable environment lookup.
///
/// Kept separate so the precedence and empty-value rules can be unit-tested
/// without mutating process-wide environment variables.
fn detect_with<F>(lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = lookup(var) {
            if !value.trim().is_empty() {
                return normalise(&value);
            }
        }
    }
    SOURCE.to_string()
}

/// Normalise a raw POSIX locale string to a Glaze tag.
///
/// Strips the codeset (`.` suffix) and modifier (`@` suffix), maps the `C` and
/// `POSIX` locales to [`SOURCE`], normalises the separator to `_`, lowercases
/// the language subtag and uppercases the territory subtag.
pub fn normalise(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return SOURCE.to_string();
    }

    let base = trimmed
        .split('@')
        .next()
        .unwrap_or(trimmed)
        .split('.')
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches(['_', '-']);

    if base.is_empty() {
        return SOURCE.to_string();
    }
    if base.eq_ignore_ascii_case("C") || base.eq_ignore_ascii_case("POSIX") {
        return SOURCE.to_string();
    }

    let mut out = String::with_capacity(base.len());
    let mut parts = base.split(['_', '-']).filter(|p| !p.is_empty());
    if let Some(language) = parts.next() {
        out.push_str(&language.to_ascii_lowercase());
    }
    for part in parts {
        out.push('_');
        if part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()) {
            // A 4-letter alphabetic subtag is a script (e.g. `Latn`): title case.
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
                for c in chars {
                    out.push(c.to_ascii_lowercase());
                }
            }
        } else {
            // Everything else (territory like `GB`) is uppercased.
            out.push_str(&part.to_ascii_uppercase());
        }
    }

    if out.is_empty() {
        SOURCE.to_string()
    } else {
        out
    }
}

/// Build the fallback chain for a locale, most specific first, ending in
/// [`SOURCE`], with duplicates removed.
///
/// ```text
/// sr_RS -> ["sr_RS", "sr", "en"]
/// en    -> ["en"]
/// C     -> ["en"]
/// ```
pub fn chain(locale: &str) -> Vec<String> {
    let full = normalise(locale);
    let mut out: Vec<String> = Vec::new();
    push_unique(&mut out, &full);
    if let Some(language) = full.split('_').next() {
        push_unique(&mut out, language);
    }
    push_unique(&mut out, SOURCE);
    out
}

fn push_unique(out: &mut Vec<String>, tag: &str) {
    if !tag.is_empty() && !out.iter().any(|existing| existing == tag) {
        out.push(tag.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup_from<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn strips_codeset() {
        assert_eq!(normalise("en_GB.UTF-8"), "en_GB");
        assert_eq!(normalise("de_DE.utf8"), "de_DE");
        assert_eq!(normalise("fr_FR.ISO-8859-1"), "fr_FR");
    }

    #[test]
    fn strips_modifier() {
        assert_eq!(normalise("sr_RS@latin"), "sr_RS");
        assert_eq!(normalise("ca_ES@valencia"), "ca_ES");
    }

    #[test]
    fn strips_codeset_and_modifier_together() {
        assert_eq!(normalise("sr_RS.UTF-8@latin"), "sr_RS");
    }

    #[test]
    fn c_and_posix_map_to_source() {
        assert_eq!(normalise("C"), "en");
        assert_eq!(normalise("POSIX"), "en");
        assert_eq!(normalise("c"), "en");
        assert_eq!(normalise(""), "en");
        assert_eq!(normalise("   "), "en");
        assert_eq!(normalise("C.UTF-8"), "en");
    }

    #[test]
    fn normalises_case_and_separator() {
        assert_eq!(normalise("en"), "en");
        assert_eq!(normalise("EN"), "en");
        assert_eq!(normalise("en-us"), "en_US");
        assert_eq!(normalise("pt_br"), "pt_BR");
        assert_eq!(normalise("zh-hant"), "zh_Hant");
    }

    #[test]
    fn chain_ends_in_source_without_duplicates() {
        assert_eq!(chain("sr_RS"), vec!["sr_RS", "sr", "en"]);
        assert_eq!(chain("en_GB"), vec!["en_GB", "en"]);
        assert_eq!(chain("de"), vec!["de", "en"]);
        assert_eq!(chain("en"), vec!["en"]);
        assert_eq!(chain("C"), vec!["en"]);
        assert_eq!(chain(""), vec!["en"]);
    }

    #[test]
    fn chain_output_preserves_priority_order() {
        // The full tag must come first and the source locale last.
        let c = chain("pt_BR");
        assert_eq!(c.first().map(String::as_str), Some("pt_BR"));
        assert_eq!(c.last().map(String::as_str), Some("en"));
        let mut sorted_check = c.clone();
        sorted_check.dedup();
        assert_eq!(sorted_check, c, "no duplicate entries");
    }

    #[test]
    fn detect_uses_first_non_empty_variable_in_order() {
        let env = lookup_from(&[
            ("LC_ALL", "de_DE.UTF-8"),
            ("LC_MESSAGES", "fr_FR"),
            ("LANG", "en_US"),
        ]);
        assert_eq!(detect_with(env), "de_DE");
    }

    #[test]
    fn detect_skips_empty_values() {
        let env = lookup_from(&[
            ("LC_ALL", ""),
            ("LC_MESSAGES", "   "),
            ("LANG", "es_ES.UTF-8"),
        ]);
        assert_eq!(detect_with(env), "es_ES");
    }

    #[test]
    fn detect_falls_back_to_source_when_unset_or_c() {
        assert_eq!(detect_with(|_| None), "en");
        let env = lookup_from(&[("LC_ALL", "C"), ("LANG", "de_DE")]);
        assert_eq!(detect_with(env), "en", "LC_ALL=C maps to en, not LANG");
        let posix = lookup_from(&[("LANG", "POSIX")]);
        assert_eq!(detect_with(posix), "en");
    }

    #[test]
    fn real_detect_returns_valid_tag() {
        // Whatever this host has set, detect() must never fail or be empty.
        let detected = detect();
        assert!(!detected.is_empty());
        assert_eq!(detected, normalise(&detected));
    }
}
