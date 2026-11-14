//! Command line: window options and headless commands (used by scripts and
//! by the VM verification, and handy for users).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::catalog::{self, Page};
use crate::store::Store;
use crate::{search, themeio, validate};

pub const USAGE: &str = "\
glaze-settings: Glaze settings (Kiln GNU/Linux)

USAGE:
    glaze-settings [--page PAGE] [--search TEXT] [--focus SETTING]
    glaze-settings COMMAND

WINDOW OPTIONS:
    --page PAGE            Open on a page (appearance, wallpaper, animation,
                           taskbar, start-menu, windows, displays, sound,
                           network, power, default-apps, about)
    --search TEXT          Open with TEXT in the search box
    --focus SETTING        Open the page of SETTING (an id from --list) and
                           move keyboard focus to its control

COMMANDS (no window):
    --list                 List pages and setting ids
    --find TEXT            Print the settings that match TEXT
    --check                Validate config.toml and print every issue
    --export-theme FILE [--name NAME]
                           Write the current look as a theme file
    --import-theme FILE    Validate a theme file, install it into
                           ~/.config/glaze/themes/ and select it
    --reset-section PAGE --yes
                           Reset one page to defaults (a backup is kept)
    --get KEY              Print a config.toml value (dotted key, e.g.
                           desktop.wallpaper_set; settings:KEY = sidecar)
    --set KEY VALUE        Set a value the way the window does (validated,
                           backed up, atomic write; the shell reloads live).
                           VALUE is TOML (8, true, \"dark\") or a bare string
    --unset KEY            Remove a value (back to its default)
    -h, --help             Print this help
    -V, --version          Print the version

Settings are saved as soon as they change to $XDG_CONFIG_HOME/glaze/config.toml
(read live by the Glaze shell) and, for keys the shell does not read yet,
$XDG_CONFIG_HOME/glaze/settings.toml. Backups: $XDG_STATE_HOME/glaze/settings-backups/.
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run,
    Help,
    Version,
    List,
    Find(String),
    Check,
    ExportTheme(PathBuf),
    ImportTheme(PathBuf),
    ResetSection(Page),
    Get(String),
    Set(String, String),
    Unset(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub command: Command,
    pub page: Option<Page>,
    pub search: Option<String>,
    pub focus: Option<String>,
    pub name: Option<String>,
    pub yes: bool,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Options, String> {
    let mut o = Options {
        command: Command::Run,
        page: None,
        search: None,
        focus: None,
        name: None,
        yes: false,
    };
    let mut it = args.into_iter();
    let mut cmd: Option<Command> = None;
    let set = |c: Command, cmd: &mut Option<Command>| -> Result<(), String> {
        if cmd.is_some() {
            return Err("only one command at a time".into());
        }
        *cmd = Some(c);
        Ok(())
    };
    while let Some(a) = it.next() {
        let mut val = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match a.as_str() {
            "-h" | "--help" => set(Command::Help, &mut cmd)?,
            "-V" | "--version" => set(Command::Version, &mut cmd)?,
            "--list" => set(Command::List, &mut cmd)?,
            "--check" => set(Command::Check, &mut cmd)?,
            "--find" => set(Command::Find(val("--find")?), &mut cmd)?,
            "--export-theme" => set(
                Command::ExportTheme(val("--export-theme")?.into()),
                &mut cmd,
            )?,
            "--import-theme" => set(
                Command::ImportTheme(val("--import-theme")?.into()),
                &mut cmd,
            )?,
            "--reset-section" => {
                let p = val("--reset-section")?;
                let page = Page::from_slug(&p).ok_or_else(|| format!("unknown page `{p}`"))?;
                set(Command::ResetSection(page), &mut cmd)?
            }
            "--get" => set(Command::Get(val("--get")?), &mut cmd)?,
            "--unset" => set(Command::Unset(val("--unset")?), &mut cmd)?,
            "--set" => {
                let k = val("--set")?;
                let v = val("--set")?;
                set(Command::Set(k, v), &mut cmd)?
            }
            "--page" => {
                let p = val("--page")?;
                o.page = Some(Page::from_slug(&p).ok_or_else(|| format!("unknown page `{p}`"))?);
            }
            "--search" => o.search = Some(val("--search")?),
            "--focus" => {
                let id = val("--focus")?;
                if catalog::find(&id).is_none() {
                    return Err(format!("unknown setting `{id}` (see --list)"));
                }
                o.focus = Some(id);
            }
            "--name" => o.name = Some(val("--name")?),
            "--yes" => o.yes = true,
            // GApplication/GTK may pass these when launched by a desktop.
            "--gapplication-service" => {}
            other => return Err(format!("unrecognised argument `{other}`")),
        }
    }
    if let Some(c) = cmd {
        o.command = c;
    }
    if matches!(o.command, Command::ResetSection(_)) && !o.yes {
        return Err("--reset-section needs --yes (it replaces settings; a backup is kept)".into());
    }
    Ok(o)
}

fn print_issues(issues: &[glaze_design::Issue]) {
    for i in issues {
        let level = if i.is_error() { "error" } else { "warning" };
        let path = if i.path.is_empty() { "(file)" } else { &i.path };
        println!("{level}: {path}: {}", i.message);
    }
}

/// Run a headless command.
pub fn run_headless(o: &Options) -> ExitCode {
    match &o.command {
        Command::List => {
            for p in Page::ALL {
                println!("{} ({}): {}", p.slug(), p.title(), p.blurb());
                for e in catalog::all().iter().filter(|e| e.page == p) {
                    println!("    {:<34} {} / {}", e.id, e.group, e.title);
                }
            }
            ExitCode::SUCCESS
        }
        Command::Find(q) => {
            let all = catalog::all();
            for h in search::search(&all, q) {
                let e = &all[h.index];
                println!(
                    "{:<34} {} > {} > {}",
                    e.id,
                    e.page.title(),
                    e.group,
                    e.title
                );
            }
            ExitCode::SUCCESS
        }
        Command::Check => {
            let s = Store::open_default();
            if let Some(r) = s.read_only() {
                println!("error: {}: does not parse: {r}", s.main_path().display());
                return ExitCode::FAILURE;
            }
            let l = s.loaded();
            print_issues(&l.issues);
            println!("{}: {} issue(s)", s.main_path().display(), l.issues.len());
            if glaze_design::issues::has_errors(&l.issues) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::ExportTheme(path) => {
            let s = Store::open_default();
            let cfg = s.config();
            let name = match validate::theme_name(o.name.as_deref().unwrap_or("My Glaze theme")) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("glaze-settings: --name: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let src = active_theme_src(&cfg);
            let text = themeio::export(&cfg, &name, src.as_deref());
            match crate::store::atomic_write(path, &text) {
                Ok(()) => {
                    println!("exported \"{name}\" to {}", path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("glaze-settings: {}: {e}", path.display());
                    ExitCode::FAILURE
                }
            }
        }
        Command::ImportTheme(path) => {
            let mut s = Store::open_default();
            match import_theme(&mut s, path) {
                Ok((name, dest, issues)) => {
                    print_issues(&issues);
                    println!("imported \"{name}\" as {} and selected it", dest.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("glaze-settings: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::ResetSection(page) => {
            let mut s = Store::open_default();
            let ch = page.reset_changes();
            if ch.is_empty() {
                eprintln!("glaze-settings: {} has nothing to reset", page.title());
                return ExitCode::FAILURE;
            }
            match s.apply(ch, &format!("Reset {}", page.title()), true) {
                Ok(()) => {
                    println!(
                        "reset {}; backup: {}",
                        page.title(),
                        s.last_backup()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "(no previous file)".into())
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("glaze-settings: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Get(key) => {
            let s = Store::open_default();
            let (doc, k) = split_doc(key);
            match s.get(doc, k) {
                Some(v) => {
                    println!("{v}");
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("glaze-settings: {key} is not set (the default applies)");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Set(key, raw) => {
            let mut s = Store::open_default();
            let (doc, k) = split_doc(key);
            let value = parse_value(raw);
            match s.set(doc, k, value.clone(), &format!("Set {k}")) {
                Ok(()) => {
                    println!("{key} = {value}");
                    sync_other_apps(&s, k);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("glaze-settings: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Unset(key) => {
            let mut s = Store::open_default();
            let (doc, k) = split_doc(key);
            match s.remove(doc, k, &format!("Reset {k}")) {
                Ok(()) => {
                    println!("{key} removed (default applies)");
                    sync_other_apps(&s, k);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("glaze-settings: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Run | Command::Help | Command::Version => ExitCode::SUCCESS,
    }
}

/// After a scheme change, switch GTK/Qt/portal apps like the window does.
fn sync_other_apps(s: &Store, key: &str) {
    if !key.starts_with("appearance.scheme") || crate::colourscheme::tool().is_none() {
        return;
    }
    let cfg = s.config();
    let hour = std::process::Command::new("date")
        .arg("+%H")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(12);
    let want = cfg
        .appearance
        .scheme_mode
        .resolve(cfg.appearance.scheme, hour);
    match crate::colourscheme::apply(want) {
        Ok(()) => println!("other apps: {}", crate::colourscheme::arg(want)),
        Err(e) => eprintln!("glaze-settings: {e}"),
    }
}

/// `settings:KEY` addresses the sidecar, anything else config.toml.
fn split_doc(key: &str) -> (crate::store::Doc, &str) {
    match key.strip_prefix("settings:") {
        Some(k) => (crate::store::Doc::Extra, k),
        None => (crate::store::Doc::Main, key),
    }
}

/// A TOML value (`8`, `true`, `"x"`, `[1, 2]`) or, failing that, a bare string.
pub fn parse_value(raw: &str) -> toml::Value {
    format!("v = {raw}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(raw.to_string()))
}

/// Text of the active theme file, if one is configured and readable.
pub fn active_theme_src(cfg: &glaze_common::config::GlazeConfig) -> Option<String> {
    let spec = cfg.appearance.theme_file.as_deref()?;
    glaze_common::config_check::read_theme_file(spec, &glaze_common::config_check::theme_dirs())
        .ok()
        .map(|(_, src)| src)
}

/// Validate, install and select a theme file. Refuses files with errors only
/// when nothing at all would apply (not TOML).
pub fn import_theme(
    s: &mut Store,
    path: &std::path::Path,
) -> Result<(String, PathBuf, Vec<glaze_design::Issue>), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let stem = path
        .file_stem()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| "theme".into());
    let checked = themeio::check(&src, &stem);
    if !checked.parses {
        let why = checked
            .issues
            .first()
            .map(|i| i.message.clone())
            .unwrap_or_else(|| "not a TOML file".into());
        return Err(format!("{} is not a theme file: {why}", path.display()));
    }
    let dir = themeio::user_theme_dir();
    let dest = themeio::destination(&dir, &checked.name, &src);
    crate::store::atomic_write(&dest, &src).map_err(|e| format!("{}: {e}", dest.display()))?;
    let spec = dest
        .file_stem()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_default();
    s.apply(
        vec![crate::store::Change::Set(
            crate::store::Doc::Main,
            "appearance.theme_file".into(),
            spec.into(),
        )],
        &format!("Import theme \"{}\"", checked.name),
        true,
    )?;
    Ok((checked.name, dest, checked.issues))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Options, String> {
        parse(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn window_options() {
        let o = p(&["--page", "taskbar", "--search", "clock"]).unwrap();
        assert_eq!(o.command, Command::Run);
        assert_eq!(o.page, Some(Page::Taskbar));
        assert_eq!(o.search.as_deref(), Some("clock"));
        assert!(p(&["--page", "nope"]).is_err());
        assert!(p(&["--focus", "panel.position"]).is_ok());
        assert!(p(&["--focus", "panel.nope"]).is_err());
    }

    #[test]
    fn commands() {
        assert_eq!(p(&["-V"]).unwrap().command, Command::Version);
        assert_eq!(p(&["--help"]).unwrap().command, Command::Help);
        assert!(p(&["--list", "--check"]).is_err());
        assert!(p(&["--reset-section", "taskbar"])
            .unwrap_err()
            .contains("--yes"));
        assert_eq!(
            p(&["--reset-section", "taskbar", "--yes"]).unwrap().command,
            Command::ResetSection(Page::Taskbar)
        );
        assert!(p(&["--export-theme"]).is_err());
        assert!(p(&["--wat"]).unwrap_err().contains("--wat"));
        assert_eq!(
            p(&["--set", "panel.height", "40"]).unwrap().command,
            Command::Set("panel.height".into(), "40".into())
        );
        assert!(p(&["--set", "panel.height"]).is_err());
    }

    #[test]
    fn set_values_parse_as_toml_or_string() {
        assert_eq!(parse_value("8"), toml::Value::Integer(8));
        assert_eq!(parse_value("true"), toml::Value::Boolean(true));
        assert_eq!(parse_value("\"dark\""), toml::Value::String("dark".into()));
        assert_eq!(parse_value("kiln"), toml::Value::String("kiln".into()));
        assert_eq!(parse_value("1.5"), toml::Value::Float(1.5));
    }

    #[test]
    fn usage_mentions_every_flag() {
        for f in [
            "--page",
            "--search",
            "--focus",
            "--list",
            "--find",
            "--check",
            "--export-theme",
            "--import-theme",
            "--reset-section",
            "--get",
            "--set",
            "--unset",
        ] {
            assert!(USAGE.contains(f), "{f}");
        }
    }
}
