//! `glaze-session` command line entry point.
//!
//! Subcommands:
//! - `env`     — print the session environment exports.
//! - `check`   — verify that the components a session needs are present.
//! - `run`     — apply the environment and supervise the component graph.
//! - `status`  — (dev aid) print a JSON status snapshot of a running session.
//!
//! Developer-mode only: it never edits a login manager or system file, and the
//! nested compositor path runs inside a window (`GLAZE.md` §2 "Install safety").

use std::process::ExitCode;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use glaze_session::{
    default_components, default_components_with, glaze_sway_config, SessionPlan, Supervisor,
};

const USAGE: &str = "\
glaze-session — Glaze Wayland session launcher

USAGE:
    glaze-session env [--backend <auto|sway|glaze|none>]
    glaze-session check [--backend <...>]
    glaze-session run [--backend <...>] [--nested]

  run --backend glaze starts glaze-compositor on this tty and falls back to Sway
  (logged with the reason) when it is missing, produces no Wayland socket
  within GLAZE_COMPOSITOR_TIMEOUT seconds (default 20), dies right away, or
  crashed in the last 10 minutes (~/.local/state/glaze/compositor-failed).
    glaze-session status

OPTIONS:
    --backend <name>  Compositor backend to plan for (default: auto)
    --nested          Run the compositor nested inside the current session
    -h, --help        Show this help
";

fn parse_backend(args: &[String]) -> String {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--backend" {
            if let Some(v) = it.next() {
                return v.clone();
            }
        }
    }
    "auto".to_string()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn cmd_env(args: &[String]) -> ExitCode {
    let backend = parse_backend(args);
    let plan = SessionPlan::build(std::env::var("XDG_SESSION_ID").ok(), &backend);
    print!("{}", plan.to_export_shell());
    ExitCode::SUCCESS
}

fn cmd_check(args: &[String]) -> ExitCode {
    let backend = parse_backend(args);
    let mut ok = true;
    let mut seen: Vec<(String, bool)> = Vec::new();

    // Wayland display must be reachable for the shell.
    let wayland = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|s| !s.is_empty());
    seen.push(("WAYLAND_DISPLAY".into(), wayland.is_some()));

    for comp in default_components(&backend) {
        let found = glaze_service::registry::program_exists(&comp.program);
        seen.push((comp.program.clone(), found));
    }

    for (name, found) in &seen {
        let mark = if *found { "ok  " } else { "MISS" };
        println!("[{mark}] {name}");
        if !*found {
            // The compositor may legitimately be absent in nested mode; still
            // report it so the user can see what is missing.
            ok = false;
        }
    }
    if !ok {
        eprintln!(
            "\nglaze-session: one or more components are missing. \
             Install them or choose another --backend."
        );
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Seconds to wait for glaze-compositor's Wayland socket before falling back
/// to Sway (`GLAZE_COMPOSITOR_TIMEOUT`, default 20).
fn compositor_timeout() -> Duration {
    let secs = std::env::var("GLAZE_COMPOSITOR_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| (1..=120).contains(s))
        .unwrap_or(20);
    Duration::from_secs(secs)
}

/// `~/.local/state/glaze/compositor-failed`: written when glaze-compositor
/// dies abnormally during a session, so the next login falls back to Sway
/// instead of looping on a crashing compositor.
fn crash_marker() -> std::path::PathBuf {
    glaze_common::paths::state_home()
        .join("glaze")
        .join("compositor-failed")
}

/// A crash marker younger than 10 minutes, with its text.
fn recent_crash() -> Option<String> {
    let path = crash_marker();
    let age = std::fs::metadata(&path).ok()?.modified().ok()?.elapsed().ok()?;
    if age > Duration::from_secs(600) {
        return None;
    }
    Some(
        std::fs::read_to_string(&path)
            .unwrap_or_default()
            .trim()
            .to_owned(),
    )
}

fn write_crash_marker(reason: &str) {
    let path = crash_marker();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, format!("{reason}\n"));
}

/// Try to bring up glaze-compositor as the session compositor. `Err` carries
/// the reason, logged before falling back to Sway.
fn start_glaze_compositor(supervisor: &mut Supervisor) -> Result<String, String> {
    if let Some(why) = recent_crash() {
        return Err(format!(
            "it failed less than 10 minutes ago ({why}); remove {} to retry now",
            crash_marker().display()
        ));
    }
    if !glaze_service::registry::program_exists("glaze-compositor") {
        return Err("glaze-compositor is not installed (not on PATH)".into());
    }
    let spec = default_components("glaze")
        .into_iter()
        .find(|c| c.name == "glaze-compositor")
        .expect("glaze backend has a compositor component");
    supervisor.start_compositor(spec, compositor_timeout(), Duration::from_millis(1500))
}

fn cmd_run(args: &[String]) -> ExitCode {
    let mut backend = parse_backend(args);
    let nested = has_flag(args, "--nested");

    let mut plan = SessionPlan::build(std::env::var("XDG_SESSION_ID").ok(), &backend);
    plan.apply();

    let mut supervisor = Supervisor::new();
    // Hand the session identity to every child explicitly (not only through
    // this process's environment), so e.g. `XDG_SESSION_TYPE=wayland` wins over
    // the `tty` value a getty login starts with.
    supervisor.extend_env(
        std::iter::once(("XDG_SESSION_ID".to_string(), plan.session_id.clone()))
            .chain(plan.vars.iter().map(|(k, v)| (k.clone(), v.clone()))),
    );
    supervisor.update_activation_env = !nested;

    // `--backend glaze`: the Glaze compositor owns the session. Sway is the
    // fallback when glaze-compositor is missing, produces no Wayland socket in
    // time, dies right after starting, or crashed in the last 10 minutes.
    let mut compositor_up = false;
    if backend == "glaze" && !nested {
        let control = glaze_common::paths::runtime_dir()
            .join("glaze")
            .join("compositor.sock");
        supervisor.extend_env([(
            "GLAZE_COMPOSITOR_SOCKET".to_string(),
            control.display().to_string(),
        )]);
        eprintln!(
            "glaze-session: starting glaze-compositor (timeout {:?})",
            compositor_timeout()
        );
        let xdisplay_file = glaze_common::paths::runtime_dir()
            .join("glaze")
            .join("xwayland-display");
        let _ = std::fs::remove_file(&xdisplay_file);
        match start_glaze_compositor(&mut supervisor) {
            Ok(name) => {
                eprintln!("glaze-session: Glaze compositor is the session compositor ({name})");
                compositor_up = true;
                // XWayland: the compositor writes its X display once ready;
                // hand it to the shell (and so to every app it launches).
                let t0 = std::time::Instant::now();
                while t0.elapsed() < Duration::from_secs(10) {
                    if let Ok(d) = std::fs::read_to_string(&xdisplay_file) {
                        let d = d.trim().to_string();
                        if !d.is_empty() {
                            eprintln!("glaze-session: XWayland is ready: DISPLAY={d}");
                            supervisor.extend_env([("DISPLAY".to_string(), d)]);
                            break;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            Err(reason) => {
                eprintln!("glaze-session: Glaze compositor unavailable: {reason}");
                eprintln!("glaze-session: FALLING BACK TO SWAY");
                backend = "sway".to_string();
                plan = SessionPlan::build(std::env::var("XDG_SESSION_ID").ok(), &backend);
                plan.apply();
                std::env::remove_var("GLAZE_COMPOSITOR_SOCKET");
                supervisor.extend_env([
                    ("GLAZE_COMPOSITOR_BACKEND".to_string(), backend.clone()),
                    ("GLAZE_COMPOSITOR_SOCKET".to_string(), String::new()),
                ]);
            }
        }
    }

    // Glaze owns the sway config (`-c`), so `/etc/sway/config` (its `bar {}`
    // block and swaybg) never runs in a Glaze session. Nested mode below adds
    // its own config instead.
    let sway_config = if nested { None } else { glaze_sway_config() };
    if backend == "sway" && !nested {
        match &sway_config {
            Some(path) => eprintln!("glaze-session: sway config {}", path.display()),
            None => eprintln!(
                "glaze-session: no ~/.config/glaze/sway.conf (run scripts/install-session.sh); \
                 sway falls back to its distribution config"
            ),
        }
    }
    let mut components = default_components_with(&backend, sway_config.as_deref());
    if compositor_up {
        components.retain(|c| c.name != "glaze-compositor");
    }
    if nested {
        // Ensure the compositor is asked to run nested when the backend
        // supports it; the argument is harmless for the others.
        for c in components.iter_mut() {
            if c.name == "sway" {
                c.args.push("-c".into());
                c.args.push("/usr/share/glaze/sway-nested.conf".into());
            }
            if c.name == "glaze-compositor" {
                c.args = vec!["--backend".into(), "winit".into()];
            }
        }
        eprintln!("glaze-session: nested developer mode (host session untouched)");
    }

    let failed = supervisor.start(components);
    for name in &failed {
        eprintln!("glaze-session: failed to start component '{name}'");
    }
    if failed.len() == supervisor.status().len() {
        eprintln!("glaze-session: no components started; refusing to idle");
        return ExitCode::FAILURE;
    }

    install_signal_handlers();

    println!("glaze-session: session started (id {})", plan.session_id);
    let compositor_name = if backend == "glaze" { "glaze-compositor" } else { "sway" };
    while !shutdown_requested() && supervisor.any_running() {
        let events = supervisor.poll();
        let mut compositor_gone = false;
        for (name, status) in events {
            eprintln!("glaze-session: component '{name}' exited with {status:?}");
            if name == compositor_name {
                compositor_gone = true;
                if name == "glaze-compositor" && !status.success() && !shutdown_requested() {
                    let reason = format!("glaze-compositor exited with {status} during a session");
                    write_crash_marker(&reason);
                    eprintln!(
                        "glaze-session: {reason}; the next login within 10 minutes uses Sway \
                         (marker {})",
                        crash_marker().display()
                    );
                }
            }
        }
        if compositor_gone {
            eprintln!("glaze-session: the compositor is gone; ending the session");
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    supervisor.stop_all();
    println!("glaze-session: session stopped");
    ExitCode::SUCCESS
}

/// Install best-effort SIGINT/SIGTERM handling so `run` shuts components down
/// cleanly. On non-Unix platforms we simply rely on process termination.
#[cfg(unix)]
fn install_signal_handlers() {
    unsafe {
        signal(SIGINT, handle_signal);
        signal(SIGTERM, handle_signal);
    }
}

#[cfg(unix)]
extern "C" fn handle_signal(_sig: i32) {
    // Async-signal-safe: only an atomic store.
    SHUTDOWN.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

#[cfg(unix)]
const SIGINT: i32 = 2;
#[cfg(unix)]
const SIGTERM: i32 = 15;

#[cfg(unix)]
extern "C" {
    // True function-pointer type avoids an `as usize` cast (and its lint).
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}

#[cfg(not(unix))]
fn install_signal_handlers() {}

#[cfg(not(unix))]
fn shutdown_requested() -> bool {
    false
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || has_flag(&args, "-h") || has_flag(&args, "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    for dir in glaze_common::paths::migrate_legacy_dirs() {
        eprintln!("[glaze-session] migrated legacy FeDe settings into {}", dir.display());
    }
    match args[0].as_str() {
        "env" => cmd_env(&args[1..]),
        "check" => cmd_check(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "status" => {
            // Without a live supervisor in this process, report the environment.
            let plan = SessionPlan::build(std::env::var("XDG_SESSION_ID").ok(), "auto");
            let json = serde_json::json!({
                "session_id": plan.session_id,
                "vars": plan.vars,
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("glaze-session: unknown subcommand '{other}'\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
