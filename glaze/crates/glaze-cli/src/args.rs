//! Pure, portable argument parsing for the `glaze` command.
//!
//! Everything here is a plain function over `&[String]` with no I/O, so the
//! whole command surface is unit-testable on any host (including Windows).
//! [`parse`] takes the arguments **after** the program name
//! (i.e. `std::env::args().skip(1)`).

use std::fmt;

use glaze_common::config::{ColorScheme, PanelPosition, ThemeKind};

/// Default number of ranked results when `--limit` is not supplied.
pub const DEFAULT_SEARCH_LIMIT: usize = 20;

/// Minimum accepted explicit panel height (logical px).
pub const MIN_PANEL_HEIGHT: i32 = 16;
/// Maximum accepted explicit panel height (logical px).
pub const MAX_PANEL_HEIGHT: i32 = 512;

/// A parsed `glaze` subcommand.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `glaze apps` — list discovered applications.
    Apps,
    /// `glaze search <query> [--limit N]`.
    Search { query: String, limit: usize },
    /// `glaze launch <id>` — build argv and spawn the app.
    Launch { id: String },
    /// `glaze exec-line <id>` — print the exact launch argv as JSON.
    ExecLine { id: String },
    /// `glaze status` — version, registry size and paths.
    Status,
    /// `glaze tray` — the service's StatusNotifier tray items.
    Tray,
    /// `glaze notifications` — the notification history and unread count.
    Notifications,
    /// `glaze config get [key]`.
    ConfigGet { key: Option<ConfigKey> },
    /// `glaze config set <key> <value>`.
    ConfigSet { key: ConfigKey, value: ConfigValue },
    /// `glaze ipc <request-json>` — raw request over the control socket.
    Ipc { request: String },
    /// `glaze --help` / no arguments.
    Help,
    /// `glaze --version`.
    Version,
}

/// A settable/gettable configuration key, addressed by its dotted path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    Theme,
    Scheme,
    ReducedMotion,
    PanelPosition,
    PanelHeight,
    Clock24h,
    ShowSeconds,
    Pinned,
}

impl ConfigKey {
    /// Every supported key, in help order.
    pub const ALL: [ConfigKey; 8] = [
        ConfigKey::Theme,
        ConfigKey::Scheme,
        ConfigKey::ReducedMotion,
        ConfigKey::PanelPosition,
        ConfigKey::PanelHeight,
        ConfigKey::Clock24h,
        ConfigKey::ShowSeconds,
        ConfigKey::Pinned,
    ];

    /// The dotted path used on the command line.
    pub const fn as_str(self) -> &'static str {
        match self {
            ConfigKey::Theme => "appearance.theme",
            ConfigKey::Scheme => "appearance.scheme",
            ConfigKey::ReducedMotion => "appearance.reduced_motion",
            ConfigKey::PanelPosition => "panel.position",
            ConfigKey::PanelHeight => "panel.height",
            ConfigKey::Clock24h => "panel.clock_24h",
            ConfigKey::ShowSeconds => "panel.show_seconds",
            ConfigKey::Pinned => "launcher.pinned",
        }
    }

    /// Parse a dotted key, rejecting anything unknown with a clear error.
    pub fn parse(s: &str) -> Result<Self, CliError> {
        match s.trim() {
            "appearance.theme" => Ok(ConfigKey::Theme),
            "appearance.scheme" => Ok(ConfigKey::Scheme),
            "appearance.reduced_motion" => Ok(ConfigKey::ReducedMotion),
            "panel.position" => Ok(ConfigKey::PanelPosition),
            "panel.height" => Ok(ConfigKey::PanelHeight),
            "panel.clock_24h" => Ok(ConfigKey::Clock24h),
            "panel.show_seconds" => Ok(ConfigKey::ShowSeconds),
            "launcher.pinned" => Ok(ConfigKey::Pinned),
            other => Err(CliError::UnknownConfigKey(other.to_string())),
        }
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A validated configuration value, already converted to its typed form.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Theme(ThemeKind),
    Scheme(ColorScheme),
    Bool(bool),
    Position(PanelPosition),
    Height(i32),
    Pinned(Vec<String>),
}

/// Errors produced by argument parsing. All of them are user errors.
#[derive(Debug, Clone, PartialEq)]
pub enum CliError {
    /// The subcommand is not recognised.
    UnknownCommand(String),
    /// A flag that is not defined for the current subcommand.
    UnknownFlag(String),
    /// A required argument was not supplied.
    MissingArgument(&'static str),
    /// A positional/extra argument that does not belong.
    UnexpectedArgument(String),
    /// A value failed validation.
    InvalidValue {
        key: String,
        value: String,
        detail: String,
    },
    /// `config get/set` was given a key Glaze does not know.
    UnknownConfigKey(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::UnknownCommand(c) => write!(f, "unknown command {c:?}"),
            CliError::UnknownFlag(fl) => write!(f, "unknown flag {fl:?}"),
            CliError::MissingArgument(what) => write!(f, "missing argument: {what}"),
            CliError::UnexpectedArgument(a) => write!(f, "unexpected argument {a:?}"),
            CliError::InvalidValue { key, value, detail } => {
                write!(f, "invalid value {value:?} for {key}: {detail}")
            }
            CliError::UnknownConfigKey(k) => write!(f, "unknown config key {k:?}"),
        }
    }
}

impl std::error::Error for CliError {}

/// Parse the command line (excluding the program name).
pub fn parse(argv: &[String]) -> Result<Command, CliError> {
    let Some(first) = argv.first() else {
        // No subcommand: show help rather than erroring. Friendly and matches
        // `glaze --help`; the exit status is still success.
        return Ok(Command::Help);
    };
    let rest = &argv[1..];
    match first.as_str() {
        "--help" | "-h" | "help" => {
            expect_no_extra(rest)?;
            Ok(Command::Help)
        }
        "--version" | "-V" | "version" => {
            expect_no_extra(rest)?;
            Ok(Command::Version)
        }
        "apps" => {
            expect_no_extra(rest)?;
            Ok(Command::Apps)
        }
        "search" => parse_search(rest),
        "launch" => parse_single_id(rest).map(|id| Command::Launch { id }),
        "exec-line" => parse_single_id(rest).map(|id| Command::ExecLine { id }),
        "status" => {
            expect_no_extra(rest)?;
            Ok(Command::Status)
        }
        "tray" => {
            expect_no_extra(rest)?;
            Ok(Command::Tray)
        }
        "notifications" | "notify" => {
            expect_no_extra(rest)?;
            Ok(Command::Notifications)
        }
        "config" => parse_config(rest),
        "ipc" => parse_ipc(rest),
        other if other.starts_with('-') => Err(CliError::UnknownFlag(other.to_string())),
        other => Err(CliError::UnknownCommand(other.to_string())),
    }
}

/// Reject any trailing arguments (flag-aware message).
fn expect_no_extra(args: &[String]) -> Result<(), CliError> {
    match args.first() {
        None => Ok(()),
        Some(a) if a.starts_with('-') => Err(CliError::UnknownFlag(a.clone())),
        Some(a) => Err(CliError::UnexpectedArgument(a.clone())),
    }
}

/// Parse exactly one `<id>` positional.
fn parse_single_id(args: &[String]) -> Result<String, CliError> {
    match args {
        [] => Err(CliError::MissingArgument("id")),
        [id] if id.starts_with('-') => Err(CliError::UnknownFlag(id.clone())),
        [id] => Ok(id.clone()),
        [_, extra, ..] => Err(CliError::UnexpectedArgument(extra.clone())),
    }
}

fn parse_search(args: &[String]) -> Result<Command, CliError> {
    let mut query: Option<String> = None;
    let mut limit = DEFAULT_SEARCH_LIMIT;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--limit" {
            let v = args
                .get(i + 1)
                .ok_or(CliError::MissingArgument("--limit value"))?;
            limit = parse_limit(v)?;
            i += 2;
        } else if let Some(v) = a.strip_prefix("--limit=") {
            limit = parse_limit(v)?;
            i += 1;
        } else if a.starts_with('-') && a != "-" {
            return Err(CliError::UnknownFlag(a.clone()));
        } else if query.is_none() {
            query = Some(a.clone());
            i += 1;
        } else {
            return Err(CliError::UnexpectedArgument(a.clone()));
        }
    }
    let query = query.ok_or(CliError::MissingArgument("query"))?;
    Ok(Command::Search { query, limit })
}

fn parse_limit(v: &str) -> Result<usize, CliError> {
    v.parse::<usize>().map_err(|_| CliError::InvalidValue {
        key: "--limit".to_string(),
        value: v.to_string(),
        detail: "expected a non-negative integer".to_string(),
    })
}

fn parse_config(args: &[String]) -> Result<Command, CliError> {
    let Some(sub) = args.first() else {
        return Err(CliError::MissingArgument("config subcommand (get|set)"));
    };
    let rest = &args[1..];
    match sub.as_str() {
        "get" => match rest {
            [] => Ok(Command::ConfigGet { key: None }),
            [k] => {
                if k.starts_with('-') {
                    Err(CliError::UnknownFlag(k.clone()))
                } else {
                    Ok(Command::ConfigGet {
                        key: Some(ConfigKey::parse(k)?),
                    })
                }
            }
            [_, extra, ..] => Err(CliError::UnexpectedArgument(extra.clone())),
        },
        "set" => match rest {
            [k, v] => {
                let (key, value) = parse_config_set(k, v)?;
                Ok(Command::ConfigSet { key, value })
            }
            [] | [_] => Err(CliError::MissingArgument("config set <key> <value>")),
            [_, _, extra, ..] => Err(CliError::UnexpectedArgument(extra.clone())),
        },
        other if other.starts_with('-') => Err(CliError::UnknownFlag(other.to_string())),
        other => Err(CliError::UnknownCommand(format!("config {other}"))),
    }
}

fn parse_ipc(args: &[String]) -> Result<Command, CliError> {
    match args {
        [] => Err(CliError::MissingArgument("request-json")),
        [json] => Ok(Command::Ipc {
            request: json.clone(),
        }),
        [_, extra, ..] => Err(CliError::UnexpectedArgument(extra.clone())),
    }
}

/// Validate and convert a `<key> <value>` pair for `config set`.
pub fn parse_config_set(key: &str, value: &str) -> Result<(ConfigKey, ConfigValue), CliError> {
    let k = ConfigKey::parse(key)?;
    let v = match k {
        ConfigKey::Theme => ConfigValue::Theme(
            ThemeKind::parse(value)
                .ok_or_else(|| invalid(key, value, "expected 'clay' or 'classic'"))?,
        ),
        ConfigKey::Scheme => ConfigValue::Scheme(
            ColorScheme::parse(value)
                .ok_or_else(|| invalid(key, value, "expected 'light' or 'dark'"))?,
        ),
        ConfigKey::ReducedMotion | ConfigKey::Clock24h | ConfigKey::ShowSeconds => {
            ConfigValue::Bool(parse_bool(value).ok_or_else(|| {
                invalid(
                    key,
                    value,
                    "expected a boolean (true/false/yes/no/on/off/1/0)",
                )
            })?)
        }
        ConfigKey::PanelPosition => ConfigValue::Position(
            parse_position(value)
                .ok_or_else(|| invalid(key, value, "expected top/bottom/left/right"))?,
        ),
        ConfigKey::PanelHeight => {
            let n: i32 = value
                .trim()
                .parse()
                .map_err(|_| invalid(key, value, "expected an integer height in logical px"))?;
            if !(MIN_PANEL_HEIGHT..=MAX_PANEL_HEIGHT).contains(&n) {
                return Err(invalid(
                    key,
                    value,
                    &format!("height must be between {MIN_PANEL_HEIGHT} and {MAX_PANEL_HEIGHT}"),
                ));
            }
            ConfigValue::Height(n)
        }
        ConfigKey::Pinned => ConfigValue::Pinned(parse_pinned(value)),
    };
    Ok((k, v))
}

fn invalid(key: &str, value: &str, detail: &str) -> CliError {
    CliError::InvalidValue {
        key: key.to_string(),
        value: value.to_string(),
        detail: detail.to_string(),
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn parse_position(s: &str) -> Option<PanelPosition> {
    match s.trim().to_ascii_lowercase().as_str() {
        "top" => Some(PanelPosition::Top),
        "bottom" => Some(PanelPosition::Bottom),
        "left" => Some(PanelPosition::Left),
        "right" => Some(PanelPosition::Right),
        _ => None,
    }
}

/// Split `launcher.pinned` on commas/semicolons; an empty string clears it.
fn parse_pinned(s: &str) -> Vec<String> {
    s.split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, CliError> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse(&owned)
    }

    #[test]
    fn no_args_and_help_show_help() {
        assert_eq!(p(&[]).unwrap(), Command::Help);
        assert_eq!(p(&["--help"]).unwrap(), Command::Help);
        assert_eq!(p(&["-h"]).unwrap(), Command::Help);
        assert_eq!(p(&["help"]).unwrap(), Command::Help);
        assert_eq!(
            p(&["help", "extra"]),
            Err(CliError::UnexpectedArgument("extra".into()))
        );
    }

    #[test]
    fn version_flag() {
        assert_eq!(p(&["--version"]).unwrap(), Command::Version);
        assert_eq!(p(&["-V"]).unwrap(), Command::Version);
        assert_eq!(p(&["version"]).unwrap(), Command::Version);
    }

    #[test]
    fn apps_command() {
        assert_eq!(p(&["apps"]).unwrap(), Command::Apps);
        assert_eq!(
            p(&["apps", "junk"]),
            Err(CliError::UnexpectedArgument("junk".into()))
        );
        assert_eq!(
            p(&["apps", "--x"]),
            Err(CliError::UnknownFlag("--x".into()))
        );
    }

    #[test]
    fn search_defaults_and_limit_forms() {
        assert_eq!(
            p(&["search", "fire"]).unwrap(),
            Command::Search {
                query: "fire".into(),
                limit: DEFAULT_SEARCH_LIMIT
            }
        );
        assert_eq!(
            p(&["search", "fire", "--limit", "3"]).unwrap(),
            Command::Search {
                query: "fire".into(),
                limit: 3
            }
        );
        assert_eq!(
            p(&["search", "--limit=5", "fire"]).unwrap(),
            Command::Search {
                query: "fire".into(),
                limit: 5
            }
        );
    }

    #[test]
    fn search_errors() {
        assert_eq!(p(&["search"]), Err(CliError::MissingArgument("query")));
        assert_eq!(
            p(&["search", "a", "b"]),
            Err(CliError::UnexpectedArgument("b".into()))
        );
        assert_eq!(
            p(&["search", "--bogus"]),
            Err(CliError::UnknownFlag("--bogus".into()))
        );
        assert_eq!(
            p(&["search", "fire", "--limit", "x"]),
            Err(CliError::InvalidValue {
                key: "--limit".into(),
                value: "x".into(),
                detail: "expected a non-negative integer".into()
            })
        );
        assert_eq!(
            p(&["search", "fire", "--limit"]),
            Err(CliError::MissingArgument("--limit value"))
        );
        assert_eq!(
            p(&["search", "fire", "--limit", "-1"]),
            Err(CliError::InvalidValue {
                key: "--limit".into(),
                value: "-1".into(),
                detail: "expected a non-negative integer".into()
            })
        );
    }

    #[test]
    fn launch_and_exec_line() {
        assert_eq!(
            p(&["launch", "org.mozilla.firefox"]).unwrap(),
            Command::Launch {
                id: "org.mozilla.firefox".into()
            }
        );
        assert_eq!(
            p(&["exec-line", "glaze-settings"]).unwrap(),
            Command::ExecLine {
                id: "glaze-settings".into()
            }
        );
        assert_eq!(p(&["launch"]), Err(CliError::MissingArgument("id")));
        assert_eq!(
            p(&["launch", "a", "b"]),
            Err(CliError::UnexpectedArgument("b".into()))
        );
        assert_eq!(
            p(&["exec-line", "--nope"]),
            Err(CliError::UnknownFlag("--nope".into()))
        );
    }

    #[test]
    fn status_command() {
        assert_eq!(p(&["status"]).unwrap(), Command::Status);
        assert_eq!(
            p(&["status", "x"]),
            Err(CliError::UnexpectedArgument("x".into()))
        );
    }

    #[test]
    fn tray_and_notifications_commands() {
        assert_eq!(p(&["tray"]).unwrap(), Command::Tray);
        assert_eq!(p(&["notifications"]).unwrap(), Command::Notifications);
        assert_eq!(p(&["notify"]).unwrap(), Command::Notifications);
        assert_eq!(
            p(&["tray", "x"]),
            Err(CliError::UnexpectedArgument("x".into()))
        );
        assert_eq!(
            p(&["notifications", "--x"]),
            Err(CliError::UnknownFlag("--x".into()))
        );
    }

    #[test]
    fn config_get() {
        assert_eq!(
            p(&["config", "get"]).unwrap(),
            Command::ConfigGet { key: None }
        );
        assert_eq!(
            p(&["config", "get", "panel.height"]).unwrap(),
            Command::ConfigGet {
                key: Some(ConfigKey::PanelHeight)
            }
        );
        assert_eq!(
            p(&["config", "get", "nope.key"]),
            Err(CliError::UnknownConfigKey("nope.key".into()))
        );
        assert_eq!(
            p(&["config"]),
            Err(CliError::MissingArgument("config subcommand (get|set)"))
        );
        assert_eq!(
            p(&["config", "frobnicate"]),
            Err(CliError::UnknownCommand("config frobnicate".into()))
        );
    }

    #[test]
    fn config_set_all_keys() {
        assert_eq!(
            p(&["config", "set", "appearance.theme", "classic"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::Theme,
                value: ConfigValue::Theme(ThemeKind::Classic)
            }
        );
        assert_eq!(
            p(&["config", "set", "appearance.scheme", "dark"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::Scheme,
                value: ConfigValue::Scheme(ColorScheme::Dark)
            }
        );
        assert_eq!(
            p(&["config", "set", "appearance.reduced_motion", "yes"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::ReducedMotion,
                value: ConfigValue::Bool(true)
            }
        );
        assert_eq!(
            p(&["config", "set", "panel.position", "left"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::PanelPosition,
                value: ConfigValue::Position(PanelPosition::Left)
            }
        );
        assert_eq!(
            p(&["config", "set", "panel.height", "40"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::PanelHeight,
                value: ConfigValue::Height(40)
            }
        );
        assert_eq!(
            p(&["config", "set", "panel.clock_24h", "false"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::Clock24h,
                value: ConfigValue::Bool(false)
            }
        );
        assert_eq!(
            p(&["config", "set", "panel.show_seconds", "on"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::ShowSeconds,
                value: ConfigValue::Bool(true)
            }
        );
        assert_eq!(
            p(&["config", "set", "launcher.pinned", "a.desktop, b.desktop"]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::Pinned,
                value: ConfigValue::Pinned(vec!["a.desktop".into(), "b.desktop".into()])
            }
        );
        // Empty clears the pin list.
        assert_eq!(
            p(&["config", "set", "launcher.pinned", ""]).unwrap(),
            Command::ConfigSet {
                key: ConfigKey::Pinned,
                value: ConfigValue::Pinned(vec![])
            }
        );
    }

    #[test]
    fn config_set_bad_values() {
        assert!(matches!(
            p(&["config", "set", "appearance.theme", "neon"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(matches!(
            p(&["config", "set", "appearance.scheme", "purple"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(matches!(
            p(&["config", "set", "panel.position", "diagonal"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(matches!(
            p(&["config", "set", "panel.height", "tall"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(matches!(
            p(&["config", "set", "panel.height", "4"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(matches!(
            p(&["config", "set", "panel.height", "9999"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(matches!(
            p(&["config", "set", "panel.clock_24h", "maybe"]),
            Err(CliError::InvalidValue { .. })
        ));
        assert_eq!(
            p(&["config", "set", "bogus.key", "1"]),
            Err(CliError::UnknownConfigKey("bogus.key".into()))
        );
    }

    #[test]
    fn config_set_missing_and_extra() {
        assert_eq!(
            p(&["config", "set"]),
            Err(CliError::MissingArgument("config set <key> <value>"))
        );
        assert_eq!(
            p(&["config", "set", "panel.height"]),
            Err(CliError::MissingArgument("config set <key> <value>"))
        );
        assert_eq!(
            p(&["config", "set", "panel.height", "40", "extra"]),
            Err(CliError::UnexpectedArgument("extra".into()))
        );
    }

    #[test]
    fn ipc_command() {
        assert_eq!(
            p(&["ipc", "{\"kind\":\"ping\"}"]).unwrap(),
            Command::Ipc {
                request: "{\"kind\":\"ping\"}".into()
            }
        );
        assert_eq!(p(&["ipc"]), Err(CliError::MissingArgument("request-json")));
        assert_eq!(
            p(&["ipc", "a", "b"]),
            Err(CliError::UnexpectedArgument("b".into()))
        );
    }

    #[test]
    fn unknown_command_and_flag() {
        assert_eq!(
            p(&["nonsense"]),
            Err(CliError::UnknownCommand("nonsense".into()))
        );
        assert_eq!(
            p(&["--frobnicate"]),
            Err(CliError::UnknownFlag("--frobnicate".into()))
        );
    }

    #[test]
    fn bool_parsing_variants() {
        for t in ["true", "TRUE", "yes", "on", "1"] {
            assert_eq!(parse_bool(t), Some(true), "{t}");
        }
        for f in ["false", "No", "off", "0"] {
            assert_eq!(parse_bool(f), Some(false), "{f}");
        }
        assert_eq!(parse_bool("banana"), None);
    }

    #[test]
    fn config_key_round_trips() {
        for k in ConfigKey::ALL {
            assert_eq!(ConfigKey::parse(k.as_str()).unwrap(), k);
        }
    }
}
