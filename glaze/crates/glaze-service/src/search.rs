//! Launcher search ranking.
//!
//! Deliberately deterministic and explainable (no ML, no fake results): every
//! hit is a real registered app, ranked by the documented scoring function
//! below, and the matched character indices in the display name are returned so
//! the UI can highlight them (`GLAZE.md` §6.2).
//!
//! Ranking model:
//! 1. The **name** is scored by the strongest strategy that matches:
//!    exact > prefix > word-prefix > substring > subsequence.
//! 2. **Secondary fields** (keywords, generic name, comment, desktop id) add a
//!    smaller contribution. They also *gate* multi-term queries: every term must
//!    match the name or some secondary field, so "text editor" cannot match an
//!    app that only knows "text".
//! 3. A tiny length penalty breaks ties in favour of shorter, more specific
//!    names.

use glaze_common::model::{AppEntry, SearchHit};

/// Score weights — named so behaviour is asserted, not magic numbers.
pub mod weight {
    pub const EXACT_NAME: i32 = 1000;
    pub const NAME_PREFIX: i32 = 600;
    pub const NAME_WORD_PREFIX: i32 = 450;
    pub const NAME_SUBSTRING: i32 = 300;
    pub const NAME_SUBSEQUENCE: i32 = 200;
    pub const SUBSEQUENCE_GAP_PENALTY: i32 = 6;
    pub const KEYWORD_EXACT: i32 = 350;
    pub const KEYWORD_PREFIX: i32 = 180;
    pub const GENERIC_SUBSTRING: i32 = 120;
    pub const COMMENT_SUBSTRING: i32 = 60;
    pub const ID_SUBSTRING: i32 = 40;
    /// Secondary contributions count at most this fraction of the name score.
    pub const SECONDARY_DIVISOR: i32 = 5;
}

/// Search `apps` for `query`, returning at most `limit` ranked hits.
///
/// An empty query returns the first `limit` apps in registry order with score 0
/// (the shell uses this to show the full alphabetical list).
pub fn search_apps(apps: &[AppEntry], query: &str, limit: usize) -> Vec<SearchHit> {
    let query = query.trim();
    if query.is_empty() {
        return apps
            .iter()
            .take(limit)
            .map(|a| SearchHit {
                app_id: a.id.clone(),
                name: a.name.clone(),
                icon: a.icon.clone(),
                score: 0,
                name_match_indices: Vec::new(),
            })
            .collect();
    }

    let needle = query.to_lowercase();
    let terms: Vec<&str> = needle.split_whitespace().collect();

    let mut hits: Vec<SearchHit> = Vec::new();
    for app in apps {
        if let Some((score, indices)) = score_app(app, &needle, &terms) {
            hits.push(SearchHit {
                app_id: app.id.clone(),
                name: app.name.clone(),
                icon: app.icon.clone(),
                score,
                name_match_indices: indices,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.app_id.cmp(&b.app_id))
    });
    hits.truncate(limit);
    hits
}

/// Score the name by the strongest matching strategy; `0` means no name match.
fn name_score(name_lower: &str, needle: &str) -> (i32, Vec<usize>) {
    if name_lower == needle {
        let n = name_lower.chars().count();
        return (weight::EXACT_NAME, (0..n).collect());
    }
    if let Some(rest) = name_lower.strip_prefix(needle) {
        let _ = rest;
        let n = needle.chars().count();
        return (weight::NAME_PREFIX, (0..n).collect());
    }
    if let Some(idx) = name_word_prefix(name_lower, needle) {
        return (weight::NAME_WORD_PREFIX, idx);
    }
    if name_lower.contains(needle) {
        return (
            weight::NAME_SUBSTRING,
            substring_indices(name_lower, needle),
        );
    }
    if let Some(idx) = subsequence_indices(name_lower, needle) {
        let gaps = idx
            .last()
            .copied()
            .unwrap_or(0)
            .saturating_sub(idx.len() - 1);
        let score = weight::NAME_SUBSEQUENCE - (gaps as i32 * weight::SUBSEQUENCE_GAP_PENALTY);
        return (score.max(1), idx);
    }
    (0, Vec::new())
}

/// Score one app against the lowered query and its terms. Returns `None` when
/// the app does not match all terms.
fn score_app(app: &AppEntry, needle: &str, terms: &[&str]) -> Option<(i32, Vec<usize>)> {
    let name_lower = app.name.to_lowercase();
    let (name, indices) = name_score(&name_lower, needle);
    let name_len = app.name.chars().count() as i32;

    let keywords_lower: Vec<String> = app.keywords.iter().map(|k| k.to_lowercase()).collect();
    let generic_lower = app.generic_name.as_ref().map(|g| g.to_lowercase());
    let comment_lower = app.comment.as_ref().map(|c| c.to_lowercase());
    let id_lower = app.id.to_lowercase();

    let mut secondary = 0i32;
    for term in terms {
        // A name match (of any strength) satisfies every term gate.
        let mut matched = name > 0;
        if keywords_lower.iter().any(|k| k == term) {
            secondary += weight::KEYWORD_EXACT;
            matched = true;
        } else if keywords_lower.iter().any(|k| k.starts_with(term)) {
            secondary += weight::KEYWORD_PREFIX;
            matched = true;
        }
        if generic_lower
            .as_deref()
            .map(|g| g.contains(term))
            .unwrap_or(false)
        {
            secondary += weight::GENERIC_SUBSTRING;
            matched = true;
        }
        if comment_lower
            .as_deref()
            .map(|c| c.contains(term))
            .unwrap_or(false)
        {
            secondary += weight::COMMENT_SUBSTRING;
            matched = true;
        }
        if id_lower.contains(term) {
            secondary += weight::ID_SUBSTRING;
            matched = true;
        }
        if !matched {
            return None;
        }
    }

    let base = if name > 0 {
        name + secondary / weight::SECONDARY_DIVISOR
    } else {
        secondary
    };
    Some((base - name_len, indices))
}

/// Find a word within `name` that starts with `needle`; return its indices.
fn name_word_prefix(name_lower: &str, needle: &str) -> Option<Vec<usize>> {
    let chars: Vec<char> = name_lower.chars().collect();
    let needle_len = needle.chars().count();
    for i in 0..chars.len() {
        let at_word_start = i == 0 || !chars[i - 1].is_alphanumeric();
        if !at_word_start {
            continue;
        }
        let slice: String = chars[i..].iter().take(needle_len).collect();
        if slice == needle {
            return Some((i..i + needle_len).collect());
        }
    }
    None
}

fn substring_indices(name_lower: &str, needle: &str) -> Vec<usize> {
    match name_lower.find(needle) {
        Some(byte_pos) => {
            let start = name_lower[..byte_pos].chars().count();
            (start..start + needle.chars().count()).collect()
        }
        None => Vec::new(),
    }
}

/// Greedy subsequence match with character indices, or `None`.
fn subsequence_indices(hay: &str, needle: &str) -> Option<Vec<usize>> {
    let hay: Vec<char> = hay.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() {
        return Some(Vec::new());
    }
    let mut indices = Vec::with_capacity(needle.len());
    let mut hi = 0;
    for &nc in &needle {
        let mut found = false;
        while hi < hay.len() {
            if hay[hi] == nc {
                indices.push(hi);
                hi += 1;
                found = true;
                break;
            }
            hi += 1;
        }
        if !found {
            return None;
        }
    }
    Some(indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, comment: &str, keywords: &[&str]) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: Some(comment.into()),
            generic_name: None,
            exec: format!("{id} %u"),
            try_exec: None,
            icon: Some(id.into()),
            categories: Vec::new(),
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    fn sample() -> Vec<AppEntry> {
        vec![
            app(
                "firefox",
                "Firefox",
                "Browse the web",
                &["browser", "internet"],
            ),
            app(
                "files",
                "Files",
                "Access and organize files",
                &["file manager", "folder"],
            ),
            app(
                "gedit",
                "Text Editor",
                "Edit text files",
                &["text", "editor"],
            ),
            app(
                "glaze-settings",
                "Glaze Settings",
                "Configure Glaze",
                &["settings", "control"],
            ),
        ]
    }

    #[test]
    fn empty_query_returns_registry_order() {
        let apps = sample();
        let hits = search_apps(&apps, "   ", 10);
        assert_eq!(hits.len(), 4);
        assert!(hits.iter().all(|h| h.score == 0));
    }

    #[test]
    fn exact_name_ranks_first() {
        let apps = sample();
        let hits = search_apps(&apps, "firefox", 10);
        assert_eq!(hits[0].app_id, "firefox");
        assert!(hits[0].score >= weight::EXACT_NAME);
        // The whole name is highlighted for an exact match.
        assert_eq!(hits[0].name_match_indices, (0..7).collect::<Vec<_>>());
    }

    #[test]
    fn name_prefix_matches_rank_above_unrelated() {
        let apps = sample();
        let hits = search_apps(&apps, "fire", 10);
        assert_eq!(hits[0].app_id, "firefox");
        // "Files" does not contain "fire", so it must not be a hit.
        assert!(!hits.iter().any(|h| h.app_id == "files"));
    }

    #[test]
    fn two_char_prefix_returns_both_files_and_firefox() {
        let apps = sample();
        let hits = search_apps(&apps, "fi", 10);
        let ids: Vec<&str> = hits.iter().map(|h| h.app_id.as_str()).collect();
        assert!(ids.contains(&"files"));
        assert!(ids.contains(&"firefox"));
    }

    #[test]
    fn keyword_search_finds_app() {
        let apps = sample();
        let hits = search_apps(&apps, "browser", 10);
        assert_eq!(hits[0].app_id, "firefox");
    }

    #[test]
    fn multiple_terms_must_all_match() {
        let apps = sample();
        let hits = search_apps(&apps, "text editor", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].app_id, "gedit");
    }

    #[test]
    fn subsequence_match_returns_indices() {
        let apps = sample();
        let hits = search_apps(&apps, "ffx", 10);
        assert_eq!(hits[0].app_id, "firefox");
        // "firefox": f(0) i(1) r(2) e(3) f(4) o(5) x(6)
        assert_eq!(hits[0].name_match_indices, vec![0, 4, 6]);
    }

    #[test]
    fn unrelated_query_yields_no_hits() {
        let apps = sample();
        assert!(search_apps(&apps, "zzzzq", 10).is_empty());
    }

    #[test]
    fn limit_is_respected() {
        let apps = sample();
        assert_eq!(search_apps(&apps, "e", 2).len(), 2);
    }

    #[test]
    fn matching_is_case_insensitive() {
        let apps = sample();
        assert_eq!(search_apps(&apps, "FIREFOX", 5)[0].app_id, "firefox");
    }

    #[test]
    fn word_prefix_matches_middle_words() {
        let apps = sample();
        let hits = search_apps(&apps, "settings", 10);
        assert_eq!(hits[0].app_id, "glaze-settings");
    }
}
