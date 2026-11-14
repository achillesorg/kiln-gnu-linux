//! Adversarial edge cases across the portable models.
//!
//! These are "bug hunt" tests: they probe the degenerate inputs a real session
//! will eventually produce (a 1-pixel output, a query full of regex
//! metacharacters, a `.desktop` file with single quotes, a clock at 24:00) and
//! assert that the model stays **safe and non-panicking**. A failing assertion
//! here is a real defect, not a formatting preference.

use glaze_common::config::{ColorScheme, GlazeConfig, ThemeKind};
use glaze_common::model::{AppEntry, WindowInfo, WindowKind};
use glaze_common::theme::ThemeTokens;
use glaze_service::desktop::tokenize_exec;
use glaze_service::search::search_apps;

use glaze_desktop::grid::{grid_positions, GridConfig};
use glaze_shell_core::clock::ClockParts;
use glaze_shell_core::launcher::LauncherState;
use glaze_wm::geometry::{detect_zone, snap_zone, Rect as WmRect, SnapZone};
use glaze_wm::keybindings::{Binding, Key, Keymap, Mods};
use glaze_wm::tiling::{tile, Layout};

fn app(id: &str, name: &str) -> AppEntry {
    AppEntry {
        id: id.into(),
        name: name.into(),
        comment: None,
        generic_name: None,
        exec: format!("/usr/bin/{id} %u"),
        try_exec: None,
        icon: None,
        categories: vec![],
        keywords: vec![],
        terminal: false,
        no_display: false,
        working_dir: None,
        source_path: format!("/usr/share/applications/{id}.desktop"),
    }
}

// --- window management ------------------------------------------------------

#[test]
fn tiling_handles_zero_and_one_window_and_huge_gaps() {
    let area = WmRect::new(0, 0, 1920, 1080);
    let layout = Layout::MasterStack {
        master_count: 1,
        master_ratio: 0.5,
    };

    assert!(
        tile(area, 0, layout, 0).is_empty(),
        "no windows -> no rects"
    );
    assert_eq!(tile(area, 1, layout, 0).len(), 1);

    // A gap larger than the area must not panic or produce negative sizes.
    for rect in tile(area, 3, layout, 10_000) {
        assert!(rect.w >= 0 && rect.h >= 0, "negative size: {rect:?}");
    }
}

#[test]
fn tiling_clamps_a_non_numeric_master_ratio() {
    let area = WmRect::new(0, 0, 800, 600);
    for ratio in [f64::NAN, -1.0, 0.0, 2.0] {
        let rects = tile(
            area,
            2,
            Layout::MasterStack {
                master_count: 1,
                master_ratio: ratio,
            },
            0,
        );
        assert_eq!(rects.len(), 2);
        for rect in rects {
            assert!(
                rect.w >= 0 && rect.h >= 0 && rect.x + rect.w <= 800,
                "unclamped rect {rect:?} for ratio {ratio}"
            );
        }
    }
}

#[test]
fn snapping_a_degenerate_work_area_is_safe() {
    for area in [
        WmRect::new(0, 0, 0, 0),
        WmRect::new(0, 0, 1, 1),
        WmRect::new(0, 0, -5, -5),
    ] {
        for zone in [
            SnapZone::LeftHalf,
            SnapZone::BottomRight,
            SnapZone::Maximize,
            SnapZone::Center,
        ] {
            let rect = snap_zone(zone, area, 0);
            assert!(
                rect.w >= 0 && rect.h >= 0,
                "zone {zone:?} on {area:?} gave negative size {rect:?}"
            );
        }
        // Edge detection must not panic anywhere inside (or outside) the area.
        let _ = detect_zone((0, 0), area);
        let _ = detect_zone((-100, 99_999), area);
    }
}

#[test]
fn keybinding_parsing_rejects_degenerate_input() {
    assert!(Binding::parse("super++q").is_err(), "empty token");
    assert!(
        Binding::parse("super+super+q").is_err(),
        "duplicate modifier"
    );
    assert!(Binding::parse("super+").is_err(), "modifier only");
    assert!(Binding::parse("").is_err(), "empty");
    assert!(Binding::parse("super+ä").is_err(), "non-ascii key");
    // A keymap with no bindings resolves nothing rather than panicking.
    assert_eq!(Keymap::new().action_for(Mods::SUPER, &Key::Char('q')), None);
}

// --- desktop grid -----------------------------------------------------------

#[test]
fn the_icon_grid_never_overflows_or_panics() {
    let cfg = GridConfig::default();
    for (w, h) in [(0, 0), (-10, -10), (1, 1), (1920, 1080), (10_000, 10_000)] {
        let rects = grid_positions(500, w, h, &cfg);
        for rect in &rects {
            assert!(
                rect.w >= 0 && rect.h >= 0,
                "negative cell {rect:?} in {w}x{h}"
            );
            if w > 0 && h > 0 {
                assert!(rect.fits_within(w, h), "cell {rect:?} overflows {w}x{h}");
            }
        }
    }
    // A zero-area screen fits nothing.
    assert!(grid_positions(10, 0, 0, &cfg).is_empty());
}

// --- searching --------------------------------------------------------------

#[test]
fn search_treats_regex_metacharacters_literally() {
    let apps = vec![
        app("org.mozilla.firefox", "Firefox"),
        app("org.gnome.Terminal", "Terminal [Beta]"),
    ];
    // None of these may panic or match everything.
    for query in ["[a-z]*", ".*", "((", "\\", "^$", "???"] {
        let hits = search_apps(&apps, query, 10);
        assert!(
            hits.len() <= apps.len(),
            "query {query:?} produced more hits than apps"
        );
    }
    // A literal bracket in the name still matches literally.
    let hits = search_apps(&apps, "[Beta]", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].app_id, "org.gnome.Terminal");
}

#[test]
fn search_with_an_empty_registry_and_zero_limit_is_safe() {
    assert!(search_apps(&[], "anything", 10).is_empty());
    let apps = vec![app("a", "Alpha")];
    assert!(
        search_apps(&apps, "", 0).is_empty(),
        "limit 0 yields nothing"
    );
    assert_eq!(search_apps(&apps, "", 1).len(), 1);
}

#[test]
fn launcher_navigation_on_an_empty_list_does_not_panic() {
    let mut state = LauncherState::new();
    state.open(&[], 8);
    assert!(state.selected().is_none());
    state.move_selection(1);
    state.move_selection(-1);
    state.move_selection(i32::MIN);
    assert!(state.selected().is_none());
    assert!(state.activate_selected().is_none());
}

// --- exec tokenising --------------------------------------------------------

#[test]
fn exec_tokenising_handles_quotes_percent_and_escapes() {
    let ctx = Default::default();
    // A trailing lone `%` and single quotes are kept literal (single quotes are
    // NOT special in a .desktop Exec line).
    let argv = tokenize_exec("app % 'quoted'", &ctx).unwrap();
    assert_eq!(argv, vec!["app", "'quoted'"]);

    // An escaped quote inside a quoted argument.
    let argv = tokenize_exec(r#"app "say \"hi\"""#, &ctx).unwrap();
    assert_eq!(argv, vec!["app", "say \"hi\""]);

    // A lone backslash is an error, not a silent drop.
    assert!(tokenize_exec("app \\", &ctx).is_err());
}

// --- clock, i18n, theme, cache ----------------------------------------------

#[test]
fn the_clock_normalises_out_of_range_parts() {
    use glaze_shell_core::clock::format_time;
    // 24:00 and negative/overflowed fields must not panic or print garbage.
    for (h, m, s) in [(24, 0, 0), (25, 90, 90), (-1, -1, -1), (0, 0, 0)] {
        let time = format_time(&ClockParts::new(2026, 1, 1, h, m, s), true, true);
        assert!(!time.is_empty());
        assert!(
            time.chars().all(|c| c.is_ascii_digit() || c == ':'),
            "odd time {time:?}"
        );
    }
}

#[test]
fn localization_never_returns_an_empty_string() {
    glaze_i18n::messages::install_from_env();
    for key in glaze_i18n::messages::Keys::ALL {
        let value = glaze_i18n::messages::tr(key);
        assert!(!value.trim().is_empty(), "empty translation for {key}");
    }
    // An unknown key falls back to the key itself, never empty.
    assert_eq!(glaze_i18n::messages::tr("nope.nope"), "nope.nope");
}

#[test]
fn every_theme_variant_defines_every_token_and_serialises() {
    for theme in ThemeKind::ALL {
        for scheme in ColorScheme::ALL {
            let tokens = ThemeTokens::for_kind(theme, scheme);
            let css = tokens.to_gtk_css();
            assert!(css.contains("@define-color glaze_canvas"));
            assert!(css.contains("@define-color glaze_accent"));
            // Reduced motion is a separate, valid emission.
            let reduced = tokens.to_gtk_css_with(glaze_common::theme::CssOptions {
                reduced_motion: true,
                focus_rings: false,
                ..glaze_common::theme::CssOptions::default()
            });
            assert!(reduced.contains("0ms"));
            assert!(glaze_a11y::audit::audit_all_themes().failures == 0);
        }
    }
}

#[test]
fn a_default_config_round_trips_and_the_state_file_is_per_user() {
    let cfg = GlazeConfig::default();
    let text = cfg.to_toml_string().unwrap();
    assert_eq!(GlazeConfig::from_toml_str(&text).unwrap(), cfg);
    // The restore file must live under the state directory, not the config one.
    let session = glaze_session::restore::RunningApps::default_path();
    let config = glaze_common::paths::config_file();
    assert_ne!(session, config);
    assert!(session.to_string_lossy().contains("session.json"));
}

#[test]
fn a_cache_signature_is_stable_for_an_unchanged_tree() {
    use glaze_cache::signature;
    let dirs: Vec<std::path::PathBuf> = Vec::new();
    let first = signature::signature(&dirs);
    let second = signature::signature(&dirs);
    assert!(first.is_valid_for(&dirs));
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap(),
        "signature must be deterministic"
    );
}

#[test]
fn a_window_with_no_app_id_is_still_groupable() {
    use glaze_shell_core::tasks::group_windows;
    let windows = vec![
        WindowInfo {
            id: "w1".into(),
            app_id: None,
            title: "Untitled".into(),
            workspace: Some(0),
            kind: WindowKind::Normal,
            focused: true,
            minimized: false,
            floating: false,
        },
        WindowInfo {
            id: "w2".into(),
            app_id: Some("glaze-shell".into()),
            title: "panel".into(),
            workspace: Some(0),
            kind: WindowKind::Internal,
            focused: false,
            minimized: false,
            floating: false,
        },
    ];
    let groups = group_windows(&windows);
    // The internal surface is skipped; the app-less window still gets a group.
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].window_ids, vec!["w1".to_string()]);
}
