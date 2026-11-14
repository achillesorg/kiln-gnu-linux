//! MRU + frequency blend for the launcher's "recent apps" section.
//!
//! Pure recency is jittery (one accidental launch jumps an app to the front)
//! and pure frequency is stale (an app used heavily months ago outranks the one
//! used this morning). Glaze blends the two with **frecency**: an exponential
//! decay on age plus a logarithmic frequency bonus.
//!
//! ```text
//! age   = max(0, now - last_used)
//! decay = 0.5 ^ (age / half_life)          // 1.0 now, 0.5 one half-life ago
//! freq  = ln(max(1, count))                // 0 now, grows slowly with use
//! score = decay + freq
//! ```
//!
//! The half-life is chosen by the caller (see [`DEFAULT_HALF_LIFE_SECS`]), and
//! `now` is always injected so results are deterministic in tests.

use crate::state::LauncherState;

/// Default half-life for the decay term: one week.
pub const DEFAULT_HALF_LIFE_SECS: u64 = 60 * 60 * 24 * 7;

/// Frecency score for a single application id. Returns `0.0` for ids that have
/// never been launched or recorded.
pub fn frecency(app_id: &str, state: &LauncherState, now_unix: u64, half_life_secs: u64) -> f64 {
    match state.recent.iter().find(|r| r.app_id == app_id) {
        Some(entry) => score(entry.last_used_unix, entry.count, now_unix, half_life_secs),
        // Known only through the lifetime counter (no launch timestamp): treat
        // its age as maximally old so it ranks below anything with real recency.
        None => match state.launch_counts.get(app_id) {
            Some(count) => score(0, *count, now_unix, half_life_secs),
            None => 0.0,
        },
    }
}

fn score(last_used_unix: u64, count: u32, now_unix: u64, half_life_secs: u64) -> f64 {
    let age = now_unix.saturating_sub(last_used_unix) as f64;
    let half_life = half_life_secs.max(1) as f64;
    let decay = 0.5_f64.powf(age / half_life);
    let freq = f64::from(count.max(1)).ln();
    decay + freq
}

/// Rank every known application id (union of the recent list and lifetime
/// counters) by frecency, best first.
///
/// Ties break deterministically by most-recent launch, then higher count, then
/// alphabetically, so the order never depends on map iteration or float noise.
pub fn blended_order(state: &LauncherState, now_unix: u64, half_life_secs: u64) -> Vec<String> {
    // Union of ids, collected deterministically from the BTreeMap plus recent.
    let mut ids: Vec<String> = state.launch_counts.keys().cloned().collect();
    for entry in &state.recent {
        if !ids.iter().any(|id| id == &entry.app_id) {
            ids.push(entry.app_id.clone());
        }
    }

    let last_used = |id: &str| {
        state
            .recent
            .iter()
            .find(|r| r.app_id == id)
            .map(|r| r.last_used_unix)
            .unwrap_or(0)
    };
    let count = |id: &str| state.launch_counts.get(id).copied().unwrap_or(0);

    ids.sort_by(|a, b| {
        let sa = frecency(a, state, now_unix, half_life_secs);
        let sb = frecency(b, state, now_unix, half_life_secs);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(last_used(b).cmp(&last_used(a)))
            .then(count(b).cmp(&count(a)))
            .then(a.cmp(b))
    });
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEK: u64 = DEFAULT_HALF_LIFE_SECS;

    #[test]
    fn unknown_app_scores_zero() {
        let state = LauncherState::default();
        assert_eq!(frecency("ghost", &state, 10_000, WEEK), 0.0);
    }

    #[test]
    fn freshness_beats_old_frequent_at_equal_count() {
        let now = 100_000_000u64;
        let mut s = LauncherState::default();
        s.record_launch("fresh", now);
        s.record_launch("stale", now - 10 * WEEK);
        let order = blended_order(&s, now, WEEK);
        assert_eq!(order, vec!["fresh", "stale"]);
    }

    #[test]
    fn heavy_frequent_use_outranks_old_single_launch() {
        let mut s = LauncherState::default();
        // Used 40 times, last a week ago.
        for i in 0..40 {
            s.record_launch("workhorse", 1_000_000 - WEEK + i);
        }
        // Used once, right now.
        s.record_launch("oneshot", 1_000_000);
        // decay: workhorse = 0.5, oneshot = 1.0; freq: ln(40)~3.69 vs ln(1)=0.
        let order = blended_order(&s, 1_000_000, WEEK);
        assert_eq!(order, vec!["workhorse", "oneshot"]);
    }

    #[test]
    fn decay_halves_each_half_life() {
        let mut s = LauncherState::default();
        s.record_launch("app", 1_000_000);
        let now = 1_000_000u64;
        let s0 = frecency("app", &s, now, WEEK);
        let s1 = frecency("app", &s, now + WEEK, WEEK);
        let s2 = frecency("app", &s, now + 2 * WEEK, WEEK);
        // Only the decay term changes; freq = ln(1) = 0.
        assert!((s0 - 1.0).abs() < 1e-9, "s0 = {s0}");
        assert!((s1 - 0.5).abs() < 1e-9, "s1 = {s1}");
        assert!((s2 - 0.25).abs() < 1e-9, "s2 = {s2}");
    }

    #[test]
    fn future_timestamps_do_not_underflow() {
        let mut s = LauncherState::default();
        // Clock skew: launch stamped after `now`.
        s.record_launch("skewed", 5_000);
        let sc = frecency("skewed", &s, 1_000, WEEK);
        assert!((sc - 1.0).abs() < 1e-9, "got {sc}");
    }

    #[test]
    fn ordering_is_deterministic_and_injectable() {
        let mut s = LauncherState::default();
        s.record_launch("a", 1_000);
        s.record_launch("b", 2_000);
        s.record_launch("c", 3_000);
        let order1 = blended_order(&s, 3_500, WEEK);
        let order2 = blended_order(&s, 3_500, WEEK);
        assert_eq!(order1, order2);
        // Most recent at this injected time wins (all counts equal).
        assert_eq!(order1, vec!["c", "b", "a"]);

        // Move the injected clock a long way forward: still deterministic and
        // recency ordering preserved.
        let far = blended_order(&s, 3_000 + 100 * WEEK, WEEK);
        assert_eq!(far, vec!["c", "b", "a"]);
    }

    #[test]
    fn ordering_includes_counter_only_ids() {
        let mut s = LauncherState::default();
        // An id known only through the lifetime counter has no timestamp, so it
        // is treated as maximally old.
        s.launch_counts.insert("counted".into(), 1);
        s.record_launch("recent", WEEK);
        let order = blended_order(&s, WEEK, WEEK);
        assert!(order.contains(&"counted".to_string()));
        assert!(order.contains(&"recent".to_string()));
        // Equal counts: the app with a fresh timestamp outranks the stale one.
        assert_eq!(order[0], "recent");
    }

    #[test]
    fn tie_break_is_alphabetical() {
        let mut s = LauncherState::default();
        s.record_launch("zzz", 1_000);
        s.record_launch("aaa", 1_000);
        assert_eq!(blended_order(&s, 1_000, WEEK), vec!["aaa", "zzz"]);
    }
}
