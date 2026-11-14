//! Session integration tests: the environment a Glaze session exports and the
//! component supervisor that runs and observes real child processes.

use std::time::Duration;

use glaze_session::{default_components, Component, SessionPlan, Supervisor};

#[test]
fn session_plan_exports_the_required_environment() {
    let plan = SessionPlan::build(Some("glaze-it-session".to_string()), "sway");
    assert_eq!(plan.session_id, "glaze-it-session");

    let required = [
        ("XDG_CURRENT_DESKTOP", "Glaze"),
        ("XDG_SESSION_DESKTOP", "glaze"),
        ("XDG_SESSION_TYPE", "wayland"),
        ("XDG_SESSION_CLASS", "user"),
        ("GLAZE_SESSION", "1"),
        ("GLAZE_COMPOSITOR_BACKEND", "sway"),
        // Toolkit cooperation so GTK/Qt/Electron/Firefox prefer Wayland.
        ("QT_QPA_PLATFORM", "wayland;xcb"),
        ("GDK_BACKEND", "wayland,x11"),
        ("MOZ_ENABLE_WAYLAND", "1"),
    ];
    for (key, value) in required {
        assert_eq!(
            plan.vars.get(key).map(String::as_str),
            Some(value),
            "missing or wrong session variable {key}"
        );
    }

    // The shell form single-quotes every value, so it is safe to `eval`.
    let exports = plan.to_export_shell();
    assert!(exports.contains("export XDG_CURRENT_DESKTOP='Glaze'"));
    assert!(exports.contains("export XDG_SESSION_ID='glaze-it-session'"));
    assert!(exports.contains("export XDG_SESSION_TYPE='wayland'"));
}

#[test]
fn session_id_is_generated_when_the_display_manager_does_not_provide_one() {
    let plan = SessionPlan::build(None, "auto");
    assert!(plan.session_id.starts_with("glaze-"));
    assert_ne!(plan.session_id, "glaze-");
}

#[test]
fn default_components_cover_service_and_shell() {
    let comps = default_components("sway");
    let names: Vec<&str> = comps.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"glaze-service"));
    assert!(names.contains(&"glaze-shell"));
    // The nested developer session uses Sway by default.
    assert!(names.contains(&"sway"));
}

#[test]
fn supervisor_runs_and_observes_a_probe_process() {
    // A program guaranteed to exit immediately and portably.
    #[cfg(windows)]
    let (prog, args) = ("cmd", vec!["/C".to_string(), "exit 0".to_string()]);
    #[cfg(unix)]
    let (prog, args) = ("true", Vec::new());

    let mut supervisor = Supervisor::new();
    let failed = supervisor.start(vec![Component::new("probe", prog).with_args(args)]);
    assert!(failed.is_empty(), "probe failed to start: {failed:?}");

    // Poll until the exit is observed (bounded, no fixed sleeps in the fast path).
    let mut observed = 0;
    for _ in 0..100 {
        observed += supervisor.poll().len();
        if observed >= 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(observed >= 1, "expected to observe the probe exiting");

    let status = supervisor.status();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].name, "probe");
    assert!(status[0].restarts >= 1, "respawn should have happened");

    supervisor.stop_all();
}

#[test]
fn supervisor_reports_a_component_that_cannot_start() {
    let mut supervisor = Supervisor::new();
    let failed = supervisor.start(vec![Component::new(
        "ghost",
        "glaze-definitely-not-a-real-program-xyz",
    )
    .no_respawn()]);
    assert_eq!(failed, vec!["ghost".to_string()]);
    assert!(!supervisor.any_running());
}
