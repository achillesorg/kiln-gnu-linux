//! Turning a registry entry into a real process, without ever involving a shell.

use std::process::{Command, Stdio};

use glaze_common::model::AppEntry;

use crate::desktop::{tokenize_exec, ExecContext, ExecError};

/// Errors from building or spawning a launch command.
#[derive(Debug)]
pub enum LaunchError {
    /// The entry has no usable `Exec`.
    EmptyExec,
    /// The `Exec` line could not be tokenised.
    Exec(ExecError),
    /// The process could not be spawned.
    Spawn(std::io::Error),
    /// A `Terminal=true` app was requested but no terminal emulator was found.
    NoTerminal,
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::EmptyExec => write!(f, "application has no Exec command"),
            LaunchError::Exec(e) => write!(f, "invalid Exec: {e}"),
            LaunchError::Spawn(e) => write!(f, "failed to spawn process: {e}"),
            LaunchError::NoTerminal => {
                write!(f, "no terminal emulator found for a Terminal=true app")
            }
        }
    }
}

impl std::error::Error for LaunchError {}

/// Terminal emulators tried, in order, for `Terminal=true` entries.
const TERMINAL_CANDIDATES: [&str; 8] = [
    "glaze-terminal",
    "kgx",
    "gnome-terminal",
    "konsole",
    "alacritty",
    "kitty",
    "foot",
    "xterm",
];

/// Build the argv for launching `app`, expanding field codes and wrapping
/// terminal apps in an available emulator.
pub fn build_command(app: &AppEntry) -> Result<Vec<String>, LaunchError> {
    let exec = app.exec.trim();
    if exec.is_empty() {
        return Err(LaunchError::EmptyExec);
    }
    let ctx = ExecContext {
        name: app.name.clone(),
        icon: app.icon.clone(),
        desktop_file: Some(app.source_path.clone()),
        files: Vec::new(),
        urls: Vec::new(),
    };
    let argv = tokenize_exec(exec, &ctx).map_err(LaunchError::Exec)?;

    if app.terminal {
        let term = std::env::var("GLAZE_TERMINAL")
            .ok()
            .filter(|s| !s.is_empty() && crate::registry::program_exists(s))
            .or_else(|| {
                TERMINAL_CANDIDATES
                    .iter()
                    .find(|t| crate::registry::program_exists(t))
                    .map(|s| s.to_string())
            })
            .ok_or(LaunchError::NoTerminal)?;
        let mut wrapped = vec![term, "-e".to_string()];
        wrapped.extend(argv);
        return Ok(wrapped);
    }
    Ok(argv)
}

/// A launched application process.
///
/// The `Child` handle is owned by a small reaper thread that `wait()`s for it,
/// so an app that exits never lingers as a zombie of the shell/service (seen
/// in M0: a restored `glaze-settings` stayed `<defunct>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launched {
    pid: u32,
}

impl Launched {
    /// Process id of the launched app.
    pub fn id(&self) -> u32 {
        self.pid
    }
}

/// Launch `app`, detaching it from the Glaze process and reaping it on exit.
///
/// The child is started in its own session where supported so that a crash
/// in a launched app never takes down the shell.
pub fn launch(app: &AppEntry) -> Result<Launched, LaunchError> {
    launch_inner(app, None)
}

/// How a watched launch ended (see [`launch_watched`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlyExit {
    pub app_id: String,
    pub pid: u32,
    /// Exit code, or `None` when killed by a signal.
    pub code: Option<i32>,
    /// Signal number when killed by one.
    pub signal: Option<i32>,
    pub after_ms: u128,
    /// Last lines the process wrote to stderr (from its log file).
    pub stderr_tail: String,
    pub log_path: std::path::PathBuf,
}

impl EarlyExit {
    /// One-line human reason, e.g. `exited with status 101: thread 'main' panicked ...`.
    pub fn reason(&self) -> String {
        let status = match (self.code, self.signal) {
            (Some(c), _) => format!("exited with status {c}"),
            (None, Some(s)) => format!("was killed by signal {s}"),
            _ => "exited".into(),
        };
        let last = self
            .stderr_tail
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("note:"))
            .unwrap_or("");
        if last.is_empty() {
            status
        } else {
            format!("{status}: {last}")
        }
    }
}

/// Launch like [`launch`], but keep the app's stderr in
/// `$XDG_CACHE_HOME/glaze/apps/<id>.log` and, if the process fails (non-zero
/// status or a signal) within `window`, call `on_fail` from the reaper
/// thread with the reason. A clean or late exit calls nothing.
pub fn launch_watched(
    app: &AppEntry,
    window: std::time::Duration,
    on_fail: impl FnOnce(EarlyExit) + Send + 'static,
) -> Result<Launched, LaunchError> {
    launch_inner(app, Some((window, Box::new(on_fail))))
}

type FailHook = (std::time::Duration, Box<dyn FnOnce(EarlyExit) + Send>);

fn app_log_path(app_id: &str) -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let safe: String = app_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    base.join("glaze/apps").join(format!("{safe}.log"))
}

fn tail_of(path: &std::path::Path, max: usize) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(max);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

fn launch_inner(app: &AppEntry, hook: Option<FailHook>) -> Result<Launched, LaunchError> {
    let argv = build_command(app)?;
    let (program, args) = argv.split_first().ok_or(LaunchError::EmptyExec)?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = app.working_dir.as_deref().filter(|d| !d.is_empty()) {
        let path = std::path::Path::new(dir);
        if path.is_dir() {
            cmd.current_dir(path);
        }
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    let log_path = app_log_path(&app.id);
    let log_file = hook.as_ref().and_then(|_| {
        let _ = std::fs::create_dir_all(log_path.parent()?);
        std::fs::File::create(&log_path).ok()
    });
    match log_file {
        Some(f) => cmd.stderr(Stdio::from(f)),
        None => cmd.stderr(Stdio::null()),
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New session: detach from the shell's controlling terminal.
        unsafe {
            cmd.pre_exec(|| {
                // setsid() — safe to ignore failure (already a session leader).
                libc_setsid();
                Ok(())
            });
        }
    }

    let started = std::time::Instant::now();
    let child = cmd.spawn().map_err(LaunchError::Spawn)?;
    let Some((window, on_fail)) = hook else {
        return Ok(reap_in_background(child));
    };
    let pid = child.id();
    let app_id = app.id.clone();
    let mut child = child;
    let spawned = std::thread::Builder::new()
        .name(format!("glaze-reap-{pid}"))
        .spawn(move || {
            let Ok(status) = child.wait() else { return };
            let after = started.elapsed();
            if status.success() || after > window {
                return;
            }
            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt;
                status.signal()
            };
            #[cfg(not(unix))]
            let signal = None;
            on_fail(EarlyExit {
                app_id,
                pid,
                code: status.code(),
                signal,
                after_ms: after.as_millis(),
                stderr_tail: tail_of(&log_path, 2048),
                log_path,
            });
        });
    if spawned.is_err() {
        eprintln!("[glaze-service] warning: no reaper thread for pid {pid}");
    }
    Ok(Launched { pid })
}

/// Hand `child` to a detached thread that waits for it.
fn reap_in_background(mut child: std::process::Child) -> Launched {
    let pid = child.id();
    let spawned = std::thread::Builder::new()
        .name(format!("glaze-reap-{pid}"))
        .spawn(move || {
            let _ = child.wait();
        });
    if spawned.is_err() {
        // Could not start a thread: fall back to a non-blocking poll so at
        // least an already-exited child is collected.
        eprintln!("[glaze-service] warning: no reaper thread for pid {pid}");
    }
    Launched { pid }
}

#[cfg(unix)]
extern "C" {
    fn setsid() -> i32;
}

#[cfg(unix)]
fn libc_setsid() {
    // Ignore the result: if it fails we are already a session leader.
    unsafe {
        setsid();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::model::AppEntry;

    #[test]
    fn early_exit_reason_uses_last_meaningful_stderr_line() {
        let e = EarlyExit {
            app_id: "glaze-settings".into(),
            pid: 1,
            code: Some(101),
            signal: None,
            after_ms: 40,
            stderr_tail: "\nthread 'main' panicked at src/main.rs:3:\nGTK has not been initialized\nnote: run with RUST_BACKTRACE=1\n".into(),
            log_path: "/tmp/x.log".into(),
        };
        assert_eq!(
            e.reason(),
            "exited with status 101: GTK has not been initialized"
        );
        let s = EarlyExit {
            code: None,
            signal: Some(11),
            stderr_tail: String::new(),
            ..e
        };
        assert_eq!(s.reason(), "was killed by signal 11");
    }

    fn entry(exec: &str, terminal: bool) -> AppEntry {
        AppEntry {
            id: "test".into(),
            name: "Test App".into(),
            comment: None,
            generic_name: None,
            exec: exec.into(),
            try_exec: None,
            icon: Some("test-icon".into()),
            categories: vec![],
            keywords: vec![],
            terminal,
            no_display: false,
            working_dir: None,
            source_path: "/usr/share/applications/test.desktop".into(),
        }
    }

    #[test]
    fn builds_plain_command() {
        let argv = build_command(&entry("/usr/bin/app --flag %U", false)).unwrap();
        assert_eq!(argv, vec!["/usr/bin/app", "--flag"]);
    }

    #[test]
    fn expands_desktop_file_code() {
        let argv = build_command(&entry("app %k", false)).unwrap();
        assert_eq!(argv, vec!["app", "/usr/share/applications/test.desktop"]);
    }

    #[test]
    fn empty_exec_is_rejected() {
        assert!(matches!(
            build_command(&entry("   ", false)).unwrap_err(),
            LaunchError::EmptyExec
        ));
    }

    /// M0 bug: launched apps that exit stayed zombies. The reaper must
    /// collect them.
    #[cfg(unix)]
    #[test]
    fn launched_process_is_reaped_not_left_a_zombie() {
        let launched = launch(&entry("true", false)).expect("spawn true");
        let pid = launched.id() as i32;
        // Give the child time to exit and the reaper to wait() for it.
        let mut reaped = false;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            // kill(pid, 0) fails with ESRCH once the process entry is gone; a
            // zombie would still accept signal 0.
            if unsafe { kill(pid, 0) } != 0 {
                reaped = true;
                break;
            }
        }
        assert!(reaped, "pid {pid} still exists (zombie?)");
    }

    #[cfg(unix)]
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    #[test]
    fn no_shell_interpretation_of_metacharacters() {
        let argv = build_command(&entry("app ; rm -rf /", false)).unwrap();
        assert_eq!(argv, vec!["app", ";", "rm", "-rf", "/"]);
        // Critically, the program is `app`, not `sh`.
        assert_eq!(argv[0], "app");
    }
}
