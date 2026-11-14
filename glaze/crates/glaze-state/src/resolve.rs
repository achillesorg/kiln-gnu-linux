//! Map stored launcher state back onto the real application registry.
//!
//! Two responsibilities:
//! * resolve pinned ids to the [`AppEntry`] values that actually exist, and
//!   report ids that have gone stale so the UI can offer to clean them;
//! * map recent ids to registry entries, **dropping** any that no longer
//!   exist. Glaze never fabricates an app to fill a slot (see `GLAZE.md` §6.2).

use glaze_common::model::AppEntry;

use crate::state::LauncherState;

fn find<'a>(apps: &'a [AppEntry], id: &str) -> Option<&'a AppEntry> {
    apps.iter().find(|a| a.id == id)
}

/// Resolve pinned ids against the registry.
///
/// Returns `(apps, stale)`:
/// * `apps` — the pinned entries that exist, in the order the user pinned them,
///   with duplicate ids collapsed;
/// * `stale` — ids that no longer match any installed application, so the shell
///   can show a "remove missing favourite" affordance and rewrite the config.
pub fn resolve_pinned(apps: &[AppEntry], pinned_ids: &[String]) -> (Vec<AppEntry>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut stale = Vec::new();
    let mut seen = Vec::new();
    for id in pinned_ids {
        if seen.iter().any(|s: &String| s == id) {
            continue;
        }
        seen.push(id.clone());
        match find(apps, id) {
            Some(app) => resolved.push(app.clone()),
            None => stale.push(id.clone()),
        }
    }
    (resolved, stale)
}

/// Map the state's recent ids to real registry entries.
///
/// The stored order (most-recent-first) is preserved, unknown ids are dropped,
/// and duplicates in the registry cannot produce duplicates here because ids
/// are unique. `limit` caps the number of *returned* apps: dropped entries do
/// not consume a slot, so the list is filled from further down the history.
pub fn recent_apps(apps: &[AppEntry], state: &LauncherState, limit: usize) -> Vec<AppEntry> {
    state
        .recent_ids(usize::MAX)
        .into_iter()
        .filter_map(|id| find(apps, &id).cloned())
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: id.to_string(),
            comment: None,
            generic_name: None,
            exec: format!("{id} %U"),
            try_exec: None,
            icon: None,
            categories: Vec::new(),
            keywords: Vec::new(),
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/tmp/{id}.desktop"),
        }
    }

    fn registry() -> Vec<AppEntry> {
        vec![app("firefox"), app("kate"), app("glaze-settings")]
    }

    #[test]
    fn resolve_pinned_keeps_order_and_reports_stale() {
        let apps = registry();
        let pinned = vec![
            "kate".to_string(),
            "ghost".to_string(),
            "firefox".to_string(),
            "kate".to_string(), // duplicate collapses
            "removed".to_string(),
        ];
        let (resolved, stale) = resolve_pinned(&apps, &pinned);
        let ids: Vec<&str> = resolved.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["kate", "firefox"]);
        assert_eq!(stale, vec!["ghost", "removed"]);
        // Returned apps are real registry clones, not synthesised.
        assert_eq!(resolved[0], app("kate"));
    }

    #[test]
    fn resolve_pinned_empty_input() {
        let (resolved, stale) = resolve_pinned(&registry(), &[]);
        assert!(resolved.is_empty());
        assert!(stale.is_empty());
    }

    #[test]
    fn recent_apps_drops_unknown_ids() {
        let apps = registry();
        let mut state = LauncherState::default();
        state.record_launch("firefox", 10);
        state.record_launch("uninstalled", 20); // unknown on this machine
        state.record_launch("kate", 30);

        let recent = recent_apps(&apps, &state, 10);
        let ids: Vec<&str> = recent.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["kate", "firefox"]);
        assert!(!ids.contains(&"uninstalled"));
    }

    #[test]
    fn recent_apps_fills_limit_after_dropping() {
        let apps = registry();
        let mut state = LauncherState::default();
        state.record_launch("firefox", 10);
        state.record_launch("gone-a", 20);
        state.record_launch("gone-b", 30);
        state.record_launch("kate", 40);
        // Limit 2 should yield the two live apps, not stop at the dropped ones.
        let recent = recent_apps(&apps, &state, 2);
        let ids: Vec<&str> = recent.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["kate", "firefox"]);
    }

    #[test]
    fn recent_apps_respects_limit() {
        let apps = registry();
        let mut state = LauncherState::default();
        state.record_launch("firefox", 10);
        state.record_launch("kate", 20);
        state.record_launch("glaze-settings", 30);
        let recent = recent_apps(&apps, &state, 2);
        let ids: Vec<&str> = recent.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["glaze-settings", "kate"]);
    }
}
