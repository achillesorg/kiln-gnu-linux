//! `glaze-service` — the real Glaze service binary.
//!
//! Subcommands:
//! - `--list-apps`      list discovered `.desktop` applications (id + name)
//! - `--search <query>` rank registry entries for a launcher query
//! - `--serve`          bind the control socket and serve the shell (Unix only)
//!
//! The registry/query paths are portable; `--serve` needs a Unix-domain socket,
//! so on other hosts it prints an honest unsupported message and exits non-zero
//! instead of faking a listener (`GLAZE.md` §4.3, §7.2).

use std::process::ExitCode;

use glaze_service::registry::{AppRegistry, DiscoverOptions};
use glaze_service::search::search_apps;
// Needed for the `Backend::name()` trait method used when attaching a
// compositor backend. The attach path is Unix-only, so the import is too.
#[cfg(unix)]
use glaze_backend::Backend;

/// Parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    ListApps,
    Search(String),
    Serve,
    Invalid(String),
}

const USAGE: &str = "\
glaze-service — Glaze application registry and IPC service

USAGE:
    glaze-service <command>

COMMANDS:
    --list-apps            list discovered applications (.desktop entries)
    --search <query>       rank applications for a launcher query
    --serve                bind $XDG_RUNTIME_DIR/glaze/control.sock and serve
    --version, -V          print the version
    --help, -h             print this help
";

/// Parse process arguments (without the program name). Pure and unit-tested.
fn parse_args(args: &[String]) -> Command {
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        None | Some("--help") | Some("-h") | Some("help") => Command::Help,
        Some("--version") | Some("-V") => Command::Version,
        Some("--list-apps") | Some("list-apps") => Command::ListApps,
        Some("--serve") | Some("serve") => Command::Serve,
        Some("--search") | Some("search") => match iter.next() {
            Some(q) => Command::Search(q.clone()),
            None => Command::Invalid("--search requires a query argument".into()),
        },
        Some(other) => Command::Invalid(format!("unknown argument: {other}")),
    }
}

fn discover() -> AppRegistry {
    AppRegistry::discover(DiscoverOptions::default())
}

fn list_apps() -> ExitCode {
    let registry = discover();
    for app in registry.apps() {
        println!("{}\t{}", app.id, app.name);
    }
    eprintln!("glaze-service: {} application(s) discovered", registry.len());
    ExitCode::SUCCESS
}

fn search(query: &str) -> ExitCode {
    let registry = discover();
    let hits = search_apps(registry.apps(), query, 20);
    for hit in &hits {
        println!("{}\t{}\t{}", hit.score, hit.name, hit.app_id);
    }
    if hits.is_empty() {
        eprintln!("glaze-service: no matches for {query:?}");
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    run(std::env::args().skip(1).collect())
}

fn run(args: Vec<String>) -> ExitCode {
    match parse_args(&args) {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("glaze-service {}", glaze_common::VERSION);
            ExitCode::SUCCESS
        }
        Command::ListApps => list_apps(),
        Command::Search(q) => search(&q),
        Command::Serve => serve(),
        Command::Invalid(msg) => {
            eprintln!("glaze-service: {msg}");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(unix)]
fn attach_backend() -> Option<Box<dyn glaze_backend::Backend>> {
    // Prefer the standard wlr-foreign-toplevel protocol; fall back to Sway IPC.
    if let Ok(backend) = glaze_backend::ForeignToplevelBackend::connect() {
        eprintln!("glaze-service: compositor backend = {}", backend.name());
        return Some(Box::new(backend));
    }
    if let Ok(backend) = glaze_backend::SwayBackend::connect_from_env() {
        eprintln!("glaze-service: compositor backend = {}", backend.name());
        return Some(Box::new(backend));
    }
    eprintln!("glaze-service: no compositor backend available (window list will be empty)");
    None
}

/// Claim and serve the two session-bus names, feeding the service's own state.
///
/// The returned connections must stay alive for the exported objects to remain
/// on the bus, so the caller keeps them for the lifetime of the serve loop. A
/// name already owned by another daemon is reported, not fought over.
#[cfg(all(unix, feature = "dbus"))]
fn start_bus_adapters(server: &glaze_service::IpcServer) -> Vec<zbus::blocking::Connection> {
    let state = server.state();
    let mut connections = Vec::new();

    match glaze_service::dbus::start_notification_server(std::sync::Arc::clone(&state)) {
        Ok(connection) => {
            eprintln!("glaze-service: serving org.freedesktop.Notifications");
            connections.push(connection);
        }
        Err(error) => eprintln!("glaze-service: notification server not started: {error}"),
    }
    match glaze_service::dbus::start_status_notifier_watcher(state) {
        Ok(connection) => {
            eprintln!("glaze-service: serving org.kde.StatusNotifierWatcher");
            connections.push(connection);
        }
        Err(error) => eprintln!("glaze-service: StatusNotifier watcher not started: {error}"),
    }

    connections
}

#[cfg(unix)]
fn serve() -> ExitCode {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use glaze_common::paths;
    use glaze_service::IpcServer;

    let registry = discover();
    eprintln!("glaze-service: {} application(s) discovered", registry.len());

    let server = match attach_backend() {
        Some(backend) => IpcServer::with_backend(registry, backend),
        None => IpcServer::new(registry),
    };

    // With the (off-by-default) `dbus` feature, also serve the session bus.
    // Both exported objects share *this* server's state, so what arrives on the
    // bus is exactly what the shell reads through `ListTray`/`ListNotifications`.
    #[cfg(feature = "dbus")]
    let _bus = start_bus_adapters(&server);

    let shutdown = Arc::new(AtomicBool::new(false));
    signals::install(Arc::clone(&shutdown));

    let socket = paths::control_socket();
    eprintln!("glaze-service: listening on {}", socket.display());
    match server.run(shutdown) {
        Ok(()) => {
            eprintln!("glaze-service: shut down cleanly");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("glaze-service: server error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(unix))]
fn serve() -> ExitCode {
    eprintln!(
        "glaze-service: --serve needs a Unix host (the control socket is a \
         Unix-domain socket). Run the service on Linux/Fedora."
    );
    ExitCode::FAILURE
}

#[cfg(unix)]
mod signals {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;

    static TERMINATED: AtomicBool = AtomicBool::new(false);

    extern "C" {
        // Declare the handler parameter with its true function-pointer type
        // rather than `usize`, so no `as usize` cast is needed (and no
        // `fn_to_numeric_cast` lint fires on Linux).
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    }

    extern "C" fn on_signal(_signum: i32) {
        TERMINATED.store(true, Ordering::SeqCst);
    }

    /// Install SIGTERM/SIGINT handlers that flip `shutdown`, so the accept loop
    /// can exit and remove its socket. A watcher thread bridges the async-signal
    /// flag to the loop's `AtomicBool`.
    pub fn install(shutdown: Arc<AtomicBool>) {
        // SAFETY: `on_signal` has the exact `extern "C" fn(c_int)` signature
        // `signal(2)` expects; the previous handler is intentionally discarded.
        unsafe {
            signal(SIGTERM, on_signal);
            signal(SIGINT, on_signal);
        }
        std::thread::spawn(move || loop {
            if TERMINATED.load(Ordering::SeqCst) {
                shutdown.store(true, Ordering::SeqCst);
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(parse_args(&[]), Command::Help);
        assert_eq!(parse_args(&args(&["--help"])), Command::Help);
        assert_eq!(parse_args(&args(&["-V"])), Command::Version);
        assert_eq!(parse_args(&args(&["--list-apps"])), Command::ListApps);
        assert_eq!(parse_args(&args(&["serve"])), Command::Serve);
        assert_eq!(
            parse_args(&args(&["--search", "fire"])),
            Command::Search("fire".into())
        );
    }

    #[test]
    fn rejects_missing_query_and_unknown_flags() {
        assert_eq!(
            parse_args(&args(&["--search"])),
            Command::Invalid("--search requires a query argument".into())
        );
        assert!(matches!(
            parse_args(&args(&["--nope"])),
            Command::Invalid(_)
        ));
    }
}
