//! `glaze` — the command-line client for the Glaze desktop service.
//!
//! Exit codes: `0` success, `1` user error (bad input / unknown app / bad
//! value), `2` internal error (config I/O, IPC transport). Nothing here panics
//! on bad input; every failure is reported as a clear message.

use std::path::Path;
use std::process::ExitCode;

use glaze_cli::args::{self, Command, ConfigKey, ConfigValue};
use glaze_cli::output;
use glaze_common::config::GlazeConfig;
use glaze_common::paths;
use glaze_common::{PRODUCT_NAME, VERSION};
use glaze_service::registry::{AppRegistry, DiscoverOptions};
use glaze_service::search::search_apps;

#[cfg(unix)]
use glaze_common::ipc::{Request, Response};

const EXIT_OK: u8 = 0;
const EXIT_USER: u8 = 1;
const EXIT_INTERNAL: u8 = 2;

const HELP: &str = "\
Glaze CLI — inspect and drive the Glaze desktop service.

USAGE:
    glaze <command> [options]

COMMANDS:
    apps                          List discovered applications
    search <query> [--limit N]    Ranked application search
    launch <id>                   Build argv and launch an application
    exec-line <id>                Print the exact launch argv as JSON
    status                        Show version, registry size and paths
    tray                          List StatusNotifier tray items (Unix only)
    notifications                 Show notification history + unread (Unix only)
    config get [key]              Print the whole config or one value
    config set <key> <value>      Persist a configuration value
    ipc <request-json>            Send a raw IPC request (Unix only)

OPTIONS:
    -h, --help       Show this help
    -V, --version    Show the version

CONFIG KEYS:
    appearance.theme           clay | classic
    appearance.scheme          light | dark
    appearance.reduced_motion  true | false
    panel.position             top | bottom | left | right
    panel.height               <logical px, 16-512>
    panel.clock_24h            true | false
    panel.show_seconds         true | false
    launcher.pinned            comma-separated desktop ids (\"\" clears)

EXIT CODES:
    0 success   1 user error   2 internal error
";

/// A command failure carrying the process exit code to use.
struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    fn user(message: impl Into<String>) -> Self {
        Failure {
            code: EXIT_USER,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Failure {
            code: EXIT_INTERNAL,
            message: message.into(),
        }
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match args::parse(&argv) {
        Ok(cmd) => run(cmd),
        Err(e) => {
            eprintln!("glaze: {e}");
            eprintln!("Try 'glaze --help'.");
            ExitCode::from(EXIT_USER)
        }
    }
}

fn run(cmd: Command) -> ExitCode {
    let result = match cmd {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Version => {
            println!("{PRODUCT_NAME} {VERSION}");
            Ok(())
        }
        Command::Apps => cmd_apps(),
        Command::Search { query, limit } => cmd_search(&query, limit),
        Command::Launch { id } => cmd_launch(&id),
        Command::ExecLine { id } => cmd_exec_line(&id),
        Command::Status => cmd_status(),
        Command::Tray => cmd_tray(),
        Command::Notifications => cmd_notifications(),
        Command::ConfigGet { key } => cmd_config_get(key),
        Command::ConfigSet { key, value } => cmd_config_set(key, value),
        Command::Ipc { request } => cmd_ipc(&request),
    };
    match result {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(f) => {
            eprintln!("glaze: {}", f.message);
            ExitCode::from(f.code)
        }
    }
}

/// Discover applications from the process's real XDG environment.
fn discover() -> AppRegistry {
    AppRegistry::discover(DiscoverOptions::default())
}

fn cmd_apps() -> Result<(), Failure> {
    let reg = discover();
    if reg.is_empty() {
        println!("no applications discovered");
        return Ok(());
    }
    print!("{}", output::render_apps_table(reg.apps()));
    Ok(())
}

fn cmd_search(query: &str, limit: usize) -> Result<(), Failure> {
    let limit = if limit == 0 {
        args::DEFAULT_SEARCH_LIMIT
    } else {
        limit
    };
    let reg = discover();
    let hits = search_apps(reg.apps(), query, limit);
    if hits.is_empty() {
        println!("no matches for {query:?}");
        return Ok(());
    }
    print!("{}", output::render_hits_table(&hits));
    Ok(())
}

fn cmd_launch(id: &str) -> Result<(), Failure> {
    let reg = discover();
    let app = reg
        .get(id)
        .ok_or_else(|| Failure::user(format!("no application with id {id:?} (see `glaze apps`)")))?;
    let argv = glaze_service::launch::build_command(app)
        .map_err(|e| Failure::user(format!("cannot build command for {id:?}: {e}")))?;
    println!("argv: {}", output::to_line_json(&argv));
    let child = glaze_service::launch::launch(app)
        .map_err(|e| Failure::user(format!("failed to launch {id:?}: {e}")))?;
    println!("launched {} (pid {})", app.name, child.id());
    Ok(())
}

fn cmd_exec_line(id: &str) -> Result<(), Failure> {
    let reg = discover();
    let app = reg
        .get(id)
        .ok_or_else(|| Failure::user(format!("no application with id {id:?} (see `glaze apps`)")))?;
    let argv = glaze_service::launch::build_command(app)
        .map_err(|e| Failure::user(format!("cannot build command for {id:?}: {e}")))?;
    println!("{}", output::to_pretty_json(&argv));
    Ok(())
}

fn cmd_status() -> Result<(), Failure> {
    let reg = discover();
    let config_path = paths::config_file();
    let socket = paths::control_socket();
    let cfg = load_config(&config_path)?;

    println!("{PRODUCT_NAME} {VERSION}");
    println!("protocol      : {}", glaze_common::ipc::PROTOCOL_VERSION);
    println!("apps          : {}", reg.len());
    println!(
        "theme         : {} ({})",
        cfg.appearance.theme.slug(),
        cfg.appearance.scheme.slug()
    );
    println!("config file   : {}", config_path.display());
    println!("control socket: {}", socket.display());
    println!("socket exists : {}", socket_status(&socket));
    print_service_status();
    Ok(())
}

/// Ask the running service for its live view (compositor backend, windows).
/// Best effort: `glaze status` still succeeds without a running service.
#[cfg(unix)]
fn print_service_status() {
    match ipc_request(&Request::Status) {
        Ok(Response::Status { status }) => {
            println!("service       : running ({})", status.version);
            println!("backend       : {}", status.compositor_backend);
            println!("windows       : {}", status.window_count);
        }
        Ok(Response::Error { message, .. }) => println!("service       : error: {message}"),
        Ok(other) => println!("service       : unexpected reply: {other:?}"),
        Err(e) => println!("service       : not reachable ({})", e.message),
    }
}

#[cfg(not(unix))]
fn print_service_status() {
    println!("service       : n/a (unix only)");
}

#[cfg(unix)]
fn socket_status(path: &Path) -> &'static str {
    if path.exists() {
        "yes"
    } else {
        "no"
    }
}

#[cfg(not(unix))]
fn socket_status(_path: &Path) -> &'static str {
    "n/a (unix only)"
}

fn load_config(path: &Path) -> Result<GlazeConfig, Failure> {
    GlazeConfig::load_from(path)
        .map_err(|e| Failure::internal(format!("cannot read config {}: {e}", path.display())))
}

fn config_value_string(cfg: &GlazeConfig, key: ConfigKey) -> String {
    match key {
        ConfigKey::Theme => cfg.appearance.theme.slug().to_string(),
        ConfigKey::Scheme => cfg.appearance.scheme.slug().to_string(),
        ConfigKey::ReducedMotion => cfg.appearance.reduced_motion.to_string(),
        ConfigKey::PanelPosition => cfg.panel.position.slug().to_string(),
        ConfigKey::PanelHeight => cfg
            .panel
            .height
            .map(|h| h.to_string())
            .unwrap_or_else(|| "auto".to_string()),
        ConfigKey::Clock24h => cfg.panel.clock_24h.to_string(),
        ConfigKey::ShowSeconds => cfg.panel.show_seconds.to_string(),
        ConfigKey::Pinned => cfg.launcher.pinned.join(","),
    }
}

fn cmd_config_get(key: Option<ConfigKey>) -> Result<(), Failure> {
    let path = paths::config_file();
    let cfg = load_config(&path)?;
    match key {
        None => {
            let toml = cfg
                .to_toml_string()
                .map_err(|e| Failure::internal(format!("cannot serialise config: {e}")))?;
            print!("{toml}");
        }
        Some(k) => println!("{}", config_value_string(&cfg, k)),
    }
    eprintln!("config: {}", path.display());
    Ok(())
}

fn cmd_config_set(key: ConfigKey, value: ConfigValue) -> Result<(), Failure> {
    let path = paths::config_file();
    let mut cfg = load_config(&path)?;
    apply_config(&mut cfg, key, value);
    cfg.save_to(&path)
        .map_err(|e| Failure::internal(format!("cannot save config {}: {e}", path.display())))?;
    println!("{} = {}", key.as_str(), config_value_string(&cfg, key));
    println!("wrote {}", path.display());
    Ok(())
}

/// Apply a parsed value. The parser guarantees key/value pairs match.
fn apply_config(cfg: &mut GlazeConfig, key: ConfigKey, value: ConfigValue) {
    match (key, value) {
        (ConfigKey::Theme, ConfigValue::Theme(v)) => cfg.appearance.theme = v,
        (ConfigKey::Scheme, ConfigValue::Scheme(v)) => cfg.appearance.scheme = v,
        (ConfigKey::ReducedMotion, ConfigValue::Bool(v)) => cfg.appearance.reduced_motion = v,
        (ConfigKey::PanelPosition, ConfigValue::Position(v)) => cfg.panel.position = v,
        (ConfigKey::PanelHeight, ConfigValue::Height(v)) => cfg.panel.height = Some(v),
        (ConfigKey::Clock24h, ConfigValue::Bool(v)) => cfg.panel.clock_24h = v,
        (ConfigKey::ShowSeconds, ConfigValue::Bool(v)) => cfg.panel.show_seconds = v,
        (ConfigKey::Pinned, ConfigValue::Pinned(v)) => cfg.launcher.pinned = v,
        _ => {}
    }
}

#[cfg(unix)]
fn ipc_request(req: &Request) -> Result<Response, Failure> {
    let socket = paths::control_socket();
    glaze_service::ipc_server::request_once(&socket, req).map_err(|e| {
        Failure::user(format!(
            "IPC to {} failed: {e} (is glaze-service running?)",
            socket.display()
        ))
    })
}

/// Turn a structured [`Response::Error`] into a user-facing failure.
#[cfg(unix)]
fn check_service_error(resp: Response) -> Result<Response, Failure> {
    match resp {
        Response::Error { code, message } => Err(Failure::user(format!(
            "service error ({code:?}): {message}"
        ))),
        ok => Ok(ok),
    }
}

#[cfg(unix)]
fn cmd_tray() -> Result<(), Failure> {
    let resp = check_service_error(ipc_request(&Request::ListTray)?)?;
    match resp {
        Response::Tray { items } => {
            print!("{}", output::render_tray_table(&items));
            Ok(())
        }
        other => Err(Failure::internal(format!("unexpected reply: {other:?}"))),
    }
}

#[cfg(unix)]
fn cmd_notifications() -> Result<(), Failure> {
    let resp = check_service_error(ipc_request(&Request::ListNotifications)?)?;
    match resp {
        Response::Notifications {
            items,
            unread_ids,
            dnd,
        } => {
            print!(
                "{}",
                output::render_notifications_table(&items, &unread_ids, dnd)
            );
            Ok(())
        }
        other => Err(Failure::internal(format!("unexpected reply: {other:?}"))),
    }
}

#[cfg(not(unix))]
fn cmd_tray() -> Result<(), Failure> {
    Err(Failure::user(
        "the IPC control socket is Unix-only; `glaze tray` is not supported on this platform",
    ))
}

#[cfg(not(unix))]
fn cmd_notifications() -> Result<(), Failure> {
    Err(Failure::user(
        "the IPC control socket is Unix-only; `glaze notifications` is not supported on this platform",
    ))
}

#[cfg(unix)]
fn cmd_ipc(request: &str) -> Result<(), Failure> {
    let req: Request = serde_json::from_str(request)
        .map_err(|e| Failure::user(format!("invalid request JSON: {e}")))?;
    let resp = ipc_request(&req)?;
    println!("{}", output::to_pretty_json(&resp));
    Ok(())
}

#[cfg(not(unix))]
fn cmd_ipc(_request: &str) -> Result<(), Failure> {
    Err(Failure::user(
        "the IPC control socket is Unix-only; not supported on this platform",
    ))
}
