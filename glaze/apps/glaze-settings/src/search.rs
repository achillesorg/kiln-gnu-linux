//! Settings search: ranks catalogue entries for a free-text query.
//!
//! Every query word must match somewhere (title, group, page or keywords);
//! a word matches a field when it is a prefix of one of the field's words or,
//! for words of three letters or more, a substring of the field. Title
//! matches rank first, then keyword, group and page matches; ties keep the
//! catalogue order so results read page by page.

use crate::catalog::Entry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Index into the entry slice that was searched.
    pub index: usize,
    pub score: u32,
}

fn norm(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '-' | '_' | '/' | '.' | ',' | '(' | ')' => ' ',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn field_score(field: &str, word: &str, prefix_pts: u32, sub_pts: u32) -> u32 {
    if field.split_whitespace().any(|w| w == word) {
        return prefix_pts + 5;
    }
    if field.split_whitespace().any(|w| w.starts_with(word)) {
        return prefix_pts;
    }
    if word.chars().count() >= 3 && field.contains(word) {
        return sub_pts;
    }
    0
}

/// Rank `entries` for `query`. An empty query returns nothing.
pub fn search(entries: &[Entry], query: &str) -> Vec<Hit> {
    let q = norm(query);
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (index, e) in entries.iter().enumerate() {
        let title = norm(e.title);
        let kw = norm(e.keywords);
        let group = norm(e.group);
        let page = norm(e.page.title());
        let mut total = 0;
        let mut all = true;
        for w in &words {
            let s = field_score(&title, w, 40, 25)
                .max(field_score(&kw, w, 20, 12))
                .max(field_score(&group, w, 15, 8))
                .max(field_score(&page, w, 10, 6));
            if s == 0 {
                all = false;
                break;
            }
            total += s;
        }
        if all {
            // Whole-phrase title match is the strongest signal.
            if title.contains(q.trim()) {
                total += 30;
            }
            hits.push(Hit {
                index,
                score: total,
            });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    fn ids(q: &str) -> Vec<&'static str> {
        let all = catalog::all();
        search(&all, q)
            .into_iter()
            .map(|h| all[h.index].id)
            .collect()
    }

    #[test]
    fn empty_query_has_no_hits() {
        assert!(ids("").is_empty());
        assert!(ids("   ").is_empty());
    }

    #[test]
    fn title_matches_rank_first() {
        let r = ids("accent");
        assert_eq!(r[0], "appearance.accent");
        let r = ids("clock");
        assert!(r.starts_with(&["panel.clock_24h"]), "{r:?}");
    }

    #[test]
    fn keywords_and_synonyms_find_settings_across_pages() {
        assert!(ids("color").contains(&"appearance.accent"));
        assert!(ids("dark").contains(&"appearance.scheme"));
        assert!(ids("hidpi").contains(&"displays.scale"));
        assert!(ids("hidpi").contains(&"appearance.scale"));
        assert!(ids("alt tab").contains(&"keys.cycle-windows"));
        let tiling = ids("tiling");
        assert!(tiling.contains(&"windows.default_layout"));
        assert!(tiling.contains(&"panel.show_layout_toggle"));
        assert!(tiling.contains(&"keys.toggle-tiling"));
    }

    #[test]
    fn every_word_must_match() {
        assert!(ids("taskbar edge").contains(&"panel.position"));
        assert!(ids("zzzz").is_empty());
        assert!(ids("accent zzzz").is_empty());
    }

    #[test]
    fn prefixes_match_and_case_is_ignored() {
        assert!(ids("WALLP").contains(&"desktop.wallpaper_set"));
        assert!(ids("auto-h").contains(&"panel.autohide"));
    }

    #[test]
    fn every_entry_is_findable_by_its_title() {
        let all = catalog::all();
        for e in &all {
            let r = search(&all, e.title);
            assert!(
                r.iter().any(|h| all[h.index].id == e.id),
                "{} not found by its title",
                e.id
            );
        }
    }
}
