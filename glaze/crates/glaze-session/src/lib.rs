//! `glaze-session` — session environment plus a portable component supervisor.
//!
//! Kept portable on purpose: the environment planning and the supervision loop
//! use only `std`, so they can be unit-tested anywhere. Only the *default*
//! component list and the signal handling are platform aware.

use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub mod restore;

/// The environment variables a Glaze session must export before launching apps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    pub vars: BTreeMap<String, String>,
    pub session_id: String,
}

impl SessionPlan {
    /// Build the standard Glaze session environment.
    ///
    /// `session_id` defaults to `glaze-<pid>` when not supplied by the display
    /// manager, which keeps `systemd --user` and portal lookups deterministic.
    pub fn build(session_id: Option<String>, backend: &str) -> Self {
        let mut vars = BTreeMap::new();
        let sid = session_id.unwrap_or_else(|| format!("glaze-{}", std::process::id()));

        // Session identity — consumed by portals, systemd and app toolkits.
        vars.insert("XDG_CURRENT_DESKTOP".into(), "Glaze".into());
        vars.insert("XDG_SESSION_DESKTOP".into(), "glaze".into());
        vars.insert("XDG_SESSION_TYPE".into(), "wayland".into());
        vars.insert("XDG_SESSION_CLASS".into(), "user".into());
        vars.insert("GLAZE_SESSION".into(), "1".into());
        vars.insert("GLAZE_COMPOSITOR_BACKEND".into(), backend.to_string());

        // Toolkit cooperation so GTK/Qt/Electron/Firefox prefer Wayland.
        vars.insert("QT_QPA_PLATFORM".into(), "wayland;xcb".into());
        vars.insert("GDK_BACKEND".into(), "wayland,x11".into());
        vars.insert("MOZ_ENABLE_WAYLAND".into(), "1".into());
        vars.insert("ELECTRON_OZONE_PLATFORM_HINT".into(), "auto".into());
        vars.insert("SDL_VIDEODRIVER".into(), "wayland,x11".into());
        vars.insert("CLUTTER_BACKEND".into(), "wayland".into());

        SessionPlan {
            vars,
            session_id: sid,
        }
    }

    /// Apply the environment to the current process.
    pub fn apply(&self) {
        std::env::set_var("XDG_SESSION_ID", &self.session_id);
        for (k, v) in &self.vars {
            std::env::set_var(k, v);
        }
    }

    /// A `KEY=VALUE KEY=VALUE` string suitable for a `.desktop` `Exec` or shell.
    pub fn to_export_shell(&self) -> String {
        let mut s = format!("export XDG_SESSION_ID={}\n", shell_quote(&self.session_id));
        for (k, v) in &self.vars {
            s.push_str(&format!("export {k}={}\n", shell_quote(v)));
        }
        s
    }
}

/// Quote a value for POSIX shell `export` (single-quote, escape embedded `'`).
pub fn shell_quote(v: &str) -> String {
    let escaped = v.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// A single supervised component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    /// Restart the component if it exits unexpectedly.
    pub respawn: bool,
    /// This component creates the Wayland display later components need:
    /// the supervisor waits for its socket and exports `WAYLAND_DISPLAY`.
    pub provides_wayland: bool,
    /// This component is sway: once it is up, the supervisor finds its IPC
    /// socket (`$XDG_RUNTIME_DIR/sway-ipc.<uid>.<pid>.sock`) and exports
    /// `SWAYSOCK`, so the shell and service can read the window tree.
    pub provides_swaysock: bool,
}

impl Component {
    pub fn new(name: &str, program: &str) -> Self {
        Component {
            name: name.into(),
            program: program.into(),
            args: Vec::new(),
            respawn: true,
            provides_wayland: false,
            provides_swaysock: false,
        }
    }

    /// Mark as the Wayland display provider (compositor).
    pub fn provides_wayland(mut self) -> Self {
        self.provides_wayland = true;
        self
    }

    /// Mark as sway, whose IPC socket later components need (`SWAYSOCK`).
    pub fn provides_swaysock(mut self) -> Self {
        self.provides_swaysock = true;
        self
    }

    pub fn with_args<I: IntoIterator<Item = String>>(mut self, args: I) -> Self {
        self.args = args.into_iter().collect();
        self
    }

    pub fn no_respawn(mut self) -> Self {
        self.respawn = false;
        self
    }
}

/// The default component set for a developer-mode nested session.
///
/// This does **not** touch the login manager or any system configuration; it is
/// only executed by the explicit `glaze-session run` command.
pub fn default_components(backend: &str) -> Vec<Component> {
    default_components_with(backend, None)
}

/// The default component set, starting sway with `sway_config` (`-c`) when
/// given. Glaze ships its own sway config (no `bar {}`, no swaybg, floating by
/// default) so the distribution's `/etc/sway/config` never leaks into the
/// session; see [`glaze_sway_config`].
pub fn default_components_with(
    backend: &str,
    sway_config: Option<&std::path::Path>,
) -> Vec<Component> {
    let mut comps = Vec::new();
    match backend {
        "sway" => {
            let mut args = vec!["--unsupported-gpu".to_string()];
            if let Some(path) = sway_config {
                args.push("-c".to_string());
                args.push(path.display().to_string());
            }
            comps.push(
                Component::new("sway", "sway")
                    .with_args(args)
                    .no_respawn()
                    .provides_wayland()
                    .provides_swaysock(),
            )
        }
        "glaze" => comps.push(
            Component::new("glaze-compositor", "glaze-compositor")
                .with_args(["--backend".to_string(), "udev".to_string()])
                .no_respawn()
                .provides_wayland(),
        ),
        _ => {}
    }
    comps.push(
        Component::new("glaze-service", "glaze-service")
            // Without `--serve` the binary only prints usage and exits.
            .with_args(["--serve".to_string()])
            .no_respawn(),
    );
    comps.push(Component::new("glaze-shell", "glaze-shell").no_respawn());
    comps
}

/// The Glaze-owned sway config, `$XDG_CONFIG_HOME/glaze/sway.conf`, when it
/// exists (installed by `scripts/install-session.sh`). `None` means sway would
/// fall back to the distribution config, which the caller should log.
pub fn glaze_sway_config() -> Option<std::path::PathBuf> {
    let path = glaze_common::paths::glaze_config_dir().join("sway.conf");
    path.is_file().then_some(path)
}

/// File name sway gives its IPC socket: `sway-ipc.<uid>.<pid>.sock`.
pub fn sway_socket_name(uid: u32, pid: u32) -> String {
    format!("sway-ipc.{uid}.{pid}.sock")
}

/// Find the IPC socket of the sway process `pid` in `dir`.
///
/// The exact `sway-ipc.<uid>.<pid>.sock` name is tried first. If the uid is
/// not the one sway used (for example `/proc/self/loginuid` is unset), any
/// `sway-ipc.<digits>.<pid>.sock` is accepted, so the match is still tied to
/// our own child's pid and never picks up another sway instance.
pub fn find_sway_socket(dir: &std::path::Path, uid: u32, pid: u32) -> Option<std::path::PathBuf> {
    let exact = dir.join(sway_socket_name(uid, pid));
    if exact.exists() {
        return Some(exact);
    }
    let suffix = format!(".{pid}.sock");
    let rd = std::fs::read_dir(dir).ok()?;
    let mut hits: Vec<std::path::PathBuf> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let middle = name.strip_prefix("sway-ipc.")?.strip_suffix(&suffix)?;
            (!middle.is_empty() && middle.bytes().all(|b| b.is_ascii_digit())).then(|| e.path())
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// Poll for [`find_sway_socket`] until it appears, `gone()` reports the
/// process died, or `timeout` elapses.
pub fn wait_for_sway_socket(
    dir: &std::path::Path,
    uid: u32,
    pid: u32,
    timeout: Duration,
    mut gone: impl FnMut() -> bool,
) -> Option<std::path::PathBuf> {
    let start = Instant::now();
    loop {
        if let Some(path) = find_sway_socket(dir, uid, pid) {
            return Some(path);
        }
        if gone() || start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ask sway (through `swaymsg exec`) for the `DISPLAY` its children get,
/// writing it to a file under the runtime dir; waits up to 2 s.
pub fn sway_x_display(swaysock: &std::path::Path) -> Option<String> {
    let file = paths_runtime_dir().join("glaze").join("sway-display");
    let _ = std::fs::create_dir_all(file.parent()?);
    let _ = std::fs::remove_file(&file);
    let cmd = format!("printf %s \"$DISPLAY\" > '{}'", file.display());
    let ok = Command::new("swaymsg")
        .arg("-s")
        .arg(swaysock)
        .arg("exec")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return None;
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok(d) = std::fs::read_to_string(&file) {
            let d = d.trim().to_string();
            if d.starts_with(':') {
                return Some(d);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Variables pushed into the systemd user manager and the D-Bus activation
/// environment once the compositor is up, so D-Bus-activated services
/// (portals, notification daemons) can reach the session.
pub const ACTIVATION_VARS: &[&str] = &[
    "WAYLAND_DISPLAY",
    "SWAYSOCK",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "XDG_SESSION_DESKTOP",
    "DISPLAY",
];

/// Arguments for `dbus-update-activation-environment`: `--systemd` plus an
/// explicit `KEY=VALUE` for every [`ACTIVATION_VARS`] entry known in `env`
/// (later entries win). Explicit values avoid depending on what the helper
/// inherits.
pub fn activation_env_args(env: &[(String, String)]) -> Vec<String> {
    let mut args = vec!["--systemd".to_string()];
    for key in ACTIVATION_VARS {
        if let Some((_, v)) = env.iter().rev().find(|(k, _)| k == key) {
            args.push(format!("{key}={v}"));
        }
    }
    args
}

/// Best effort: run `dbus-update-activation-environment` with
/// [`activation_env_args`], giving up after 5 s. Every outcome is logged.
fn update_activation_environment(env: &[(String, String)]) {
    let args = activation_env_args(env);
    let mut child = match Command::new("dbus-update-activation-environment")
        .args(&args)
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("[glaze-session] dbus-update-activation-environment not run: {e}");
            return;
        }
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                eprintln!(
                    "[glaze-session] activation environment updated: {}",
                    args[1..].join(" ")
                );
                return;
            }
            Ok(Some(status)) => {
                eprintln!("[glaze-session] dbus-update-activation-environment failed: {status}");
                return;
            }
            Ok(None) if start.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!("[glaze-session] dbus-update-activation-environment timed out; killed");
                return;
            }
        }
    }
}

/// Spawn one component's process (free function so callers can hold a mutable
/// borrow of the supervisor while spawning).
fn spawn_child(spec: &Component, env: &[(String, String)]) -> std::io::Result<Child> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    cmd.envs(env.iter().map(|(k, v)| (k, v)));
    cmd.stdin(Stdio::null());
    // Inherit stdout/stderr so logs land in the session log.
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.spawn()
}

fn paths_runtime_dir() -> std::path::PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(v) if !v.is_empty() => v.into(),
        _ => std::env::temp_dir(),
    }
}

/// Names of `wayland-*` sockets (not `.lock` files) present in `dir`.
pub fn wayland_sockets(dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("wayland-") && !name.ends_with(".lock") {
                out.insert(name);
            }
        }
    }
    out
}

/// Poll until a `wayland-*` socket not in `before` appears (returns its name),
/// `gone()` reports the provider died, or `timeout` elapses.
pub fn wait_for_new_socket(
    dir: &std::path::Path,
    before: &std::collections::BTreeSet<String>,
    timeout: Duration,
    mut gone: impl FnMut() -> bool,
) -> Option<String> {
    let start = Instant::now();
    // A crashed or killed compositor leaves its socket file behind, and the
    // next one reuses the name (`wayland-1` again), so "new" also means
    // "(re)created after we started waiting".
    let since = std::time::SystemTime::now();
    loop {
        if let Some(n) = wayland_sockets(dir).difference(before).next() {
            return Some(n.clone());
        }
        let recreated = wayland_sockets(dir).into_iter().find(|n| {
            before.contains(n)
                && std::fs::metadata(dir.join(n))
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t >= since)
        });
        if let Some(n) = recreated {
            return Some(n);
        }
        if gone() || start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Runtime state for one supervised child.
#[derive(Debug)]
struct Running {
    spec: Component,
    child: Option<Child>,
    started_at: Instant,
    restarts: u32,
    last_exit: Option<std::process::ExitStatus>,
}

/// A portable supervisor for the Glaze component graph.
pub struct Supervisor {
    running: Vec<Running>,
    /// Maximum automatic restarts per component.
    pub max_restarts: u32,
    /// Extra environment for every child (e.g. `WAYLAND_DISPLAY`).
    env: Vec<(String, String)>,
    /// How long to wait for a compositor's Wayland socket.
    pub socket_timeout: Duration,
    /// Push the session variables into systemd/D-Bus activation once the
    /// compositor is ready (off in tests).
    pub update_activation_env: bool,
}

impl Default for Supervisor {
    fn default() -> Self {
        Supervisor {
            running: Vec::new(),
            max_restarts: 3,
            env: Vec::new(),
            socket_timeout: Duration::from_secs(20),
            update_activation_env: false,
        }
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `KEY=VALUE` pairs every child gets (later values win).
    pub fn extend_env<I: IntoIterator<Item = (String, String)>>(&mut self, vars: I) {
        self.env.extend(vars);
    }

    /// The extra environment children receive.
    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    /// Start every component, in order. Returns names that failed to start.
    pub fn start(&mut self, components: Vec<Component>) -> Vec<String> {
        let mut failed = Vec::new();
        for spec in components {
            let before = if spec.provides_wayland {
                wayland_sockets(&paths_runtime_dir())
            } else {
                Default::default()
            };
            match spawn_child(&spec, &self.env) {
                Ok(mut child) => {
                    if spec.provides_wayland {
                        match wait_for_new_socket(
                            &paths_runtime_dir(),
                            &before,
                            self.socket_timeout,
                            || matches!(child.try_wait(), Ok(Some(_))),
                        ) {
                            Some(name) => {
                                eprintln!(
                                    "[glaze-session] {} is ready: WAYLAND_DISPLAY={name}",
                                    spec.name
                                );
                                self.env.push(("WAYLAND_DISPLAY".into(), name));
                            }
                            None => eprintln!(
                                "[glaze-session] {} produced no Wayland socket within {:?}; \
                                 later components will likely fail",
                                spec.name, self.socket_timeout
                            ),
                        }
                    }
                    if spec.provides_swaysock {
                        let pid = child.id();
                        match wait_for_sway_socket(
                            &paths_runtime_dir(),
                            glaze_common::paths::current_uid(),
                            pid,
                            self.socket_timeout,
                            || matches!(child.try_wait(), Ok(Some(_))),
                        ) {
                            Some(path) => {
                                eprintln!(
                                    "[glaze-session] {} IPC is ready: SWAYSOCK={}",
                                    spec.name,
                                    path.display()
                                );
                                self.env
                                    .push(("SWAYSOCK".into(), path.display().to_string()));
                                // Sway's (lazy) XWayland display, so X11-only
                                // apps launched by the shell (VLC 3's GUI)
                                // get a real, server-decorated window instead
                                // of falling back to a CLI or bare video window.
                                if let Some(d) = sway_x_display(&path) {
                                    eprintln!("[glaze-session] sway XWayland: DISPLAY={d}");
                                    self.env.push(("DISPLAY".into(), d));
                                }
                            }
                            None => eprintln!(
                                "[glaze-session] no sway-ipc.*.{pid}.sock within {:?}; the \
                                 taskbar will have no window list",
                                self.socket_timeout
                            ),
                        }
                    }
                    if spec.provides_wayland && self.update_activation_env {
                        update_activation_environment(&self.env);
                    }
                    self.running.push(Running {
                        spec,
                        child: Some(child),
                        started_at: Instant::now(),
                        restarts: 0,
                        last_exit: None,
                    });
                }
                Err(_) => failed.push(spec.name),
            }
        }
        failed
    }

    /// Start a compositor component and wait until it is really up: its
    /// Wayland socket must appear within `timeout` and the process must still
    /// be alive `settle` later. On failure the child is stopped and the
    /// reason returned (for the fallback decision and the session log); on
    /// success `WAYLAND_DISPLAY` is exported to later components.
    pub fn start_compositor(
        &mut self,
        spec: Component,
        timeout: Duration,
        settle: Duration,
    ) -> Result<String, String> {
        let before = wayland_sockets(&paths_runtime_dir());
        let mut child = spawn_child(&spec, &self.env)
            .map_err(|e| format!("cannot start {}: {e}", spec.program))?;
        let socket = wait_for_new_socket(&paths_runtime_dir(), &before, timeout, || {
            matches!(child.try_wait(), Ok(Some(_)))
        });
        let fail = |child: &mut Child, why: String| -> String {
            let status = match child.try_wait() {
                Ok(Some(s)) => format!("exited with {s}"),
                _ => {
                    let _ = child.kill();
                    let s = child.wait().map(|s| s.to_string()).unwrap_or_default();
                    format!("still running, stopped ({s})")
                }
            };
            format!("{why}; {} {status}", spec.name)
        };
        let Some(name) = socket else {
            let why = if matches!(child.try_wait(), Ok(Some(_))) {
                "it exited before creating its Wayland socket".to_string()
            } else {
                format!("no Wayland socket within {timeout:?}")
            };
            return Err(fail(&mut child, why));
        };
        let start = Instant::now();
        while start.elapsed() < settle {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return Err(fail(
                    &mut child,
                    format!("died within {settle:?} of creating {name}"),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!(
            "[glaze-session] {} is ready: WAYLAND_DISPLAY={name}",
            spec.name
        );
        self.env.push(("WAYLAND_DISPLAY".into(), name.clone()));
        if self.update_activation_env {
            update_activation_environment(&self.env);
        }
        self.running.push(Running {
            spec,
            child: Some(child),
            started_at: Instant::now(),
            restarts: 0,
            last_exit: None,
        });
        Ok(name)
    }

    /// Exit status of a finished component, if it ran and stopped.
    pub fn exit_status(&self, name: &str) -> Option<std::process::ExitStatus> {
        self.running
            .iter()
            .find(|r| r.spec.name == name)
            .and_then(|r| r.last_exit)
    }

    /// Poll children once: record exits and respawn where allowed.
    /// Returns the list of `(component, status)` events observed.
    pub fn poll(&mut self) -> Vec<(String, std::process::ExitStatus)> {
        let mut events = Vec::new();
        for r in self.running.iter_mut() {
            let Some(child) = r.child.as_mut() else {
                continue;
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    events.push((r.spec.name.clone(), status));
                    r.last_exit = Some(status);
                    r.child = None;
                    if r.spec.respawn && r.restarts < self.max_restarts {
                        if let Ok(new_child) = spawn_child(&r.spec, &self.env) {
                            r.child = Some(new_child);
                            r.restarts += 1;
                            r.started_at = Instant::now();
                        }
                    }
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
        events
    }

    /// True when at least one component is still running.
    pub fn any_running(&self) -> bool {
        self.running.iter().any(|r| r.child.is_some())
    }

    /// A snapshot of component health for logging / the `status` subcommand.
    pub fn status(&self) -> Vec<ComponentStatus> {
        self.running
            .iter()
            .map(|r| ComponentStatus {
                name: r.spec.name.clone(),
                running: r.child.is_some(),
                uptime: r.started_at.elapsed(),
                restarts: r.restarts,
                exit_code: r.last_exit.and_then(|s| s.code()),
            })
            .collect()
    }

    /// Terminate all children (used on session shutdown).
    pub fn stop_all(&mut self) {
        for r in self.running.iter_mut() {
            if let Some(child) = r.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            r.child = None;
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Serializable health snapshot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComponentStatus {
    pub name: String,
    pub running: bool,
    #[serde(skip)]
    pub uptime: Duration,
    pub restarts: u32,
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reused_socket_name_counts_as_new() {
        let dir = std::env::temp_dir().join(format!("glaze-sock-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let stale = dir.join("wayland-1");
        std::fs::write(&stale, b"").unwrap();
        let before = wayland_sockets(&dir);
        assert!(before.contains("wayland-1"));
        // Nothing recreated: times out.
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(wait_for_new_socket(&dir, &before, Duration::from_millis(150), || false), None);
        let path = stale.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let _ = std::fs::remove_file(&path);
            std::fs::write(&path, b"").unwrap();
        });
        let got = wait_for_new_socket(&dir, &before, Duration::from_secs(3), || false);
        t.join().unwrap();
        assert_eq!(got.as_deref(), Some("wayland-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_sets_wayland_session_identity() {
        let plan = SessionPlan::build(Some("glaze-test-1".into()), "sway");
        assert_eq!(plan.vars.get("XDG_CURRENT_DESKTOP").unwrap(), "Glaze");
        assert_eq!(plan.vars.get("XDG_SESSION_TYPE").unwrap(), "wayland");
        assert_eq!(plan.vars.get("XDG_SESSION_DESKTOP").unwrap(), "glaze");
        assert_eq!(plan.vars.get("MOZ_ENABLE_WAYLAND").unwrap(), "1");
        assert_eq!(plan.session_id, "glaze-test-1");
    }

    #[test]
    fn plan_generates_session_id_when_missing() {
        let plan = SessionPlan::build(None, "auto");
        assert!(plan.session_id.starts_with("glaze-"));
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn export_shell_contains_key_values() {
        let plan = SessionPlan::build(Some("s".to_string()), "sway");
        let s = plan.to_export_shell();
        assert!(s.contains("export XDG_CURRENT_DESKTOP='Glaze'"));
    }

    #[test]
    fn default_components_include_shell_and_service() {
        let comps = default_components("sway");
        let names: Vec<&str> = comps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"sway"));
        assert!(names.contains(&"glaze-service"));
        assert!(names.contains(&"glaze-shell"));
    }

    #[test]
    fn service_is_started_with_serve() {
        let comps = default_components("sway");
        let svc = comps.iter().find(|c| c.name == "glaze-service").unwrap();
        assert_eq!(svc.args, ["--serve"]);
    }

    #[test]
    fn compositor_provides_wayland_and_precedes_shell() {
        for backend in ["sway", "glaze"] {
            let comps = default_components(backend);
            assert!(comps[0].provides_wayland);
            assert!(!comps.last().unwrap().provides_wayland);
        }
    }

    #[test]
    fn waits_for_a_new_wayland_socket() {
        let dir = std::env::temp_dir().join(format!("glaze-sock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("wayland-0"), "").unwrap(); // pre-existing
        std::fs::write(dir.join("wayland-0.lock"), "").unwrap();
        let before = wayland_sockets(&dir);
        assert_eq!(before.len(), 1);
        // Times out when nothing new appears.
        assert_eq!(
            wait_for_new_socket(&dir, &before, Duration::from_millis(120), || false),
            None
        );
        // Provider death short-circuits.
        assert_eq!(
            wait_for_new_socket(&dir, &before, Duration::from_secs(5), || true),
            None
        );
        // New socket appears later.
        let d2 = dir.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            std::fs::write(d2.join("wayland-1"), "").unwrap();
        });
        let got = wait_for_new_socket(&dir, &before, Duration::from_secs(5), || false);
        t.join().unwrap();
        assert_eq!(got.as_deref(), Some("wayland-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("glaze-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sway_gets_glaze_config_and_swaysock_role() {
        let cfg = std::path::Path::new("/home/u/.config/glaze/sway.conf");
        let comps = default_components_with("sway", Some(cfg));
        let sway = &comps[0];
        assert_eq!(
            sway.args,
            ["--unsupported-gpu", "-c", "/home/u/.config/glaze/sway.conf"]
        );
        assert!(sway.provides_swaysock && sway.provides_wayland);
        // Without a config file sway keeps its own defaults (no bogus -c).
        assert_eq!(default_components("sway")[0].args, ["--unsupported-gpu"]);
        // Only sway advertises an IPC socket.
        assert!(default_components("glaze")
            .iter()
            .all(|c| !c.provides_swaysock));
    }

    #[test]
    fn sway_socket_name_matches_sway_format() {
        assert_eq!(sway_socket_name(1000, 691), "sway-ipc.1000.691.sock");
    }

    #[test]
    fn finds_sway_socket_for_our_pid_only() {
        let dir = temp_dir("swaysock");
        std::fs::write(dir.join("sway-ipc.1000.42.sock"), "").unwrap(); // other sway
        std::fs::write(dir.join("sway-ipc.1000.7.sockx"), "").unwrap(); // not a socket name
        assert_eq!(find_sway_socket(&dir, 1000, 7), None);
        std::fs::write(dir.join("sway-ipc.1000.7.sock"), "").unwrap();
        assert_eq!(
            find_sway_socket(&dir, 1000, 7),
            Some(dir.join("sway-ipc.1000.7.sock"))
        );
        // Wrong uid guess (e.g. unset loginuid) still finds our pid's socket.
        assert_eq!(
            find_sway_socket(&dir, u32::MAX, 42),
            Some(dir.join("sway-ipc.1000.42.sock"))
        );
        // A non-numeric uid part is rejected.
        std::fs::write(dir.join("sway-ipc.x.9.sock"), "").unwrap();
        assert_eq!(find_sway_socket(&dir, 1, 9), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waits_for_sway_socket() {
        let dir = temp_dir("swaywait");
        assert_eq!(
            wait_for_sway_socket(&dir, 1000, 5, Duration::from_millis(100), || false),
            None
        );
        assert_eq!(
            wait_for_sway_socket(&dir, 1000, 5, Duration::from_secs(5), || true),
            None
        );
        let d2 = dir.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            std::fs::write(d2.join("sway-ipc.1000.5.sock"), "").unwrap();
        });
        let got = wait_for_sway_socket(&dir, 1000, 5, Duration::from_secs(5), || false);
        t.join().unwrap();
        assert_eq!(got, Some(dir.join("sway-ipc.1000.5.sock")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn activation_args_are_explicit_and_latest_wins() {
        let env = vec![
            ("XDG_SESSION_TYPE".to_string(), "tty".to_string()),
            ("WAYLAND_DISPLAY".to_string(), "wayland-1".to_string()),
            ("XDG_SESSION_TYPE".to_string(), "wayland".to_string()),
            (
                "SWAYSOCK".to_string(),
                "/run/user/1000/sway-ipc.1000.5.sock".to_string(),
            ),
            ("UNRELATED".to_string(), "x".to_string()),
        ];
        assert_eq!(
            activation_env_args(&env),
            [
                "--systemd",
                "WAYLAND_DISPLAY=wayland-1",
                "SWAYSOCK=/run/user/1000/sway-ipc.1000.5.sock",
                "XDG_SESSION_TYPE=wayland",
            ]
        );
    }

    #[test]
    fn supervisor_env_is_passed_to_children() {
        let mut sup = Supervisor::new();
        sup.extend_env([("GLAZE_TEST_PROBE".to_string(), "wayland".to_string())]);
        assert_eq!(
            sup.env(),
            [("GLAZE_TEST_PROBE".to_string(), "wayland".to_string())]
        );
        #[cfg(unix)]
        {
            let probe = Component::new("probe", "sh")
                .with_args([
                    "-c".to_string(),
                    "test \"$GLAZE_TEST_PROBE\" = wayland".to_string(),
                ])
                .no_respawn();
            assert!(sup.start(vec![probe]).is_empty());
            let mut code = None;
            for _ in 0..100 {
                if let Some((_, status)) = sup.poll().into_iter().next() {
                    code = status.code();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(code, Some(0), "child must see the supervisor env");
        }
    }

    #[test]
    fn supervisor_detects_exit_and_respawns() {
        // Use a program guaranteed to exit immediately and portably.
        #[cfg(windows)]
        let (prog, args) = ("cmd", vec!["/C".to_string(), "exit 0".to_string()]);
        #[cfg(unix)]
        let (prog, args) = ("true", vec![]);

        let mut sup = Supervisor::new();
        let comp = Component::new("probe", prog).with_args(args);
        let failed = sup.start(vec![comp]);
        assert!(
            failed.is_empty(),
            "component should have started: {failed:?}"
        );

        // Give it a moment, then poll until we observe the exit/respawn.
        let mut observed = 0;
        for _ in 0..50 {
            observed += sup.poll().len();
            if observed >= 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(observed >= 1, "expected to observe the probe exiting");
        let status = sup.status();
        assert_eq!(status.len(), 1);
        assert!(status[0].restarts >= 1, "respawn should have happened");
        sup.stop_all();
    }
}
