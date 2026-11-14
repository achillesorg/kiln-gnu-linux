//! `glaze-shell` — the Glaze desktop shell for a Wayland session.
//!
//! GTK4 view layer. Behaviour lives in `glaze-shell-core` (taskbar model,
//! start-menu model, clock, status feed) and tokens/CSS in `glaze-design`;
//! this binary wires them to widgets, the compositor backend and the service.
//!
//! - [`shell`]: config + live reload, CSS, one taskbar per output, the
//!   compositor event stream.
//! - [`panel_ui`]: the taskbar (mockup taskbar-v2).
//! - [`launcher`]: the start menu.
//! - [`anim`]: the frame-clock spring animation engine.
//! - [`icons`] / [`ink`]: ink icons (agent C's SVG set, cairo fallback).
//!
//! Honest limitations: hover cards list window titles (no thumbnails: needs
//! screencopy); volume is shown only when `wpctl` works; battery only when
//! sysfs has one; the lock action needs a locker (`loginctl lock-session`).

mod anim;
mod backend;
mod classic_menu;
mod desktop_ui;
mod icons;
mod ink;
mod keybindings;
mod launcher;
mod menu;
mod mobile_ui;
mod panel_ui;
mod ghost;
mod shell;
mod slide;
mod start_pane;
mod toast;
mod wincontrols;

use std::process::ExitCode;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use glaze_common::config::GlazeConfig;
use glaze_common::model::WindowInfo;
use glaze_common::paths;
use glaze_common::theme::{CssOptions, ThemeTokens};
use glaze_common::wallpaper::{self, Orientation};
use glaze_service::registry::{AppRegistry, DiscoverOptions};
use glaze_shell_core::form_factor::{pref_from_env, DisplayInfo, FormFactor};
use glaze_wm::keybindings::Keymap;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::Edge;

/// Reverse-DNS application id (`docs/adr/ADR-0001-architecture.md`).
const APP_ID: &str = "org.glaze.Shell";

/// Parsed command-line options.
#[derive(Debug, Default)]
struct Args {
    /// Force a normal window instead of a layer-shell surface.
    windowed: bool,
    help: bool,
    version: bool,
}

fn parse_args<I: Iterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut parsed = Args::default();
    for arg in args {
        match arg.as_str() {
            "--windowed" => parsed.windowed = true,
            "-h" | "--help" => parsed.help = true,
            "-V" | "--version" => parsed.version = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(parsed)
}

fn print_help() {
    println!(
        "glaze-shell {version}\n\n\
         Usage: glaze-shell [OPTIONS]\n\n\
         Options:\n  \
         --windowed      Force a normal decorated window (skip gtk4-layer-shell)\n  \
         -h, --help      Print this help and exit\n  \
         -V, --version   Print the version and exit\n\n\
         Configuration is read from {config} and reloaded live.",
        version = glaze_shell_core::VERSION,
        config = paths::config_file().display(),
    );
}

/// Lock: ask glaze-compositor to start its screen locker (ext-session-lock)
/// over its control socket when it runs the session, else logind.
pub const LOCK_SH: &str = "if [ -n \"$GLAZE_COMPOSITOR_SOCKET\" ] && [ -S \"$GLAZE_COMPOSITOR_SOCKET\" ]; then python3 -c 'import os,socket; s=socket.socket(socket.AF_UNIX); s.connect(os.environ[\"GLAZE_COMPOSITOR_SOCKET\"]); s.sendall(b\"{\\\"kind\\\":\\\"lock\\\"}\\n\"); s.recv(64)'; else loginctl lock-session; fi";

/// Log out: ask glaze-compositor to exit over its control socket when it runs
/// the session, else `swaymsg exit` (Sway session).
pub const LOGOUT_SH: &str = "if [ -n \"$GLAZE_COMPOSITOR_SOCKET\" ] && [ -S \"$GLAZE_COMPOSITOR_SOCKET\" ]; then python3 -c 'import os,socket; s=socket.socket(socket.AF_UNIX); s.connect(os.environ[\"GLAZE_COMPOSITOR_SOCKET\"]); s.sendall(b\"{\\\"kind\\\":\\\"exit\\\"}\\n\"); s.recv(64)'; else swaymsg exit; fi";

fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("glaze-shell: {e}");
            eprintln!("Try 'glaze-shell --help'.");
            return ExitCode::from(2);
        }
    };
    if args.help {
        print_help();
        return ExitCode::SUCCESS;
    }
    if args.version {
        println!("glaze-shell {}", glaze_shell_core::VERSION);
        return ExitCode::SUCCESS;
    }
    if let Err(e) = gtk4::init() {
        eprintln!("glaze-shell: failed to initialise GTK4: {e}");
        eprintln!("glaze-shell: is a display available (WAYLAND_DISPLAY or DISPLAY)?");
        return ExitCode::FAILURE;
    }
    eprintln!(
        "[glaze-shell] app id {APP_ID}, version {}",
        glaze_shell_core::VERSION
    );
    glaze_i18n::install_from_env();
    eprintln!("[glaze-shell] locale: {}", glaze_i18n::active_locale());

    let loaded = shell::load_config();
    let cfg = shell::with_effective_scheme(loaded.config.clone());

    let pref = pref_from_env(std::env::var("GLAZE_FORM_FACTOR").ok().as_deref())
        .unwrap_or(cfg.shell.form_factor);
    let display_info = primary_display_info();
    let form = FormFactor::resolve(pref, &display_info);
    eprintln!(
        "[glaze-shell] form factor: {} (preference {}, output {}x{})",
        form.slug(),
        pref.slug(),
        display_info.width_px,
        display_info.height_px,
    );
    let wallpaper_path = wallpaper::resolve(
        &cfg,
        cfg.appearance.scheme,
        Orientation::for_size(display_info.width_px, display_info.height_px),
        &wallpaper::search_dirs(),
    );
    match &wallpaper_path {
        Some(p) => eprintln!("[glaze-shell] wallpaper: {}", p.display()),
        None => eprintln!("[glaze-shell] wallpaper: none (theme canvas colour)"),
    }

    let registry = Rc::new(AppRegistry::discover(DiscoverOptions::default()));
    eprintln!(
        "[glaze-shell] discovered {} installed application(s)",
        registry.len()
    );

    if form.is_mobile() {
        let tokens = shell::load_tokens(&cfg);
        install_basic_css(&cfg, &tokens);
        return run_mobile(
            &cfg,
            &tokens,
            registry,
            args.windowed,
            &display_info,
            wallpaper_path,
        );
    }

    let attached = backend::attach(&cfg);
    eprintln!(
        "[glaze-shell] compositor backend: {}",
        attached.summary().note
    );
    if cfg.session.restore_apps && std::env::var_os("GLAZE_LIVE_RESTART").is_none() {
        restore_session(&registry);
    }

    let shell = shell::Shell::new(
        loaded,
        registry.clone(),
        attached.backend.clone(),
        attached.windows.clone(),
        args.windowed,
    );
    shell.build_panels();
    shell.start_window_events();
    shell.start_controls_tracking();
    shell.watch_config();
    shell.watch_auto_scheme();
    spawn_status_poller(shell.clone());

    if let Some(window) = shell.first_window() {
        let keymap = Rc::new(Keymap::default_glaze());
        let n = keybindings::install(&window, keymap, || {
            eprintln!(
                "[glaze-shell] launcher keybinding pressed (global binding needs the compositor)"
            );
        });
        eprintln!("[glaze-shell] loaded {n} Glaze keybinding(s) on the panel");
    }

    if glaze_shell_core::panel::desktop_surface_requested(&cfg) {
        let (desktop_content, wall_stack) = desktop_ui::build(
            registry.clone(),
            &cfg,
            wallpaper_path.as_deref(),
            cfg.desktop.show_icons,
        );
        shell.set_desktop(
            wall_stack,
            wallpaper_path.clone(),
            Orientation::for_size(display_info.width_px, display_info.height_px),
        );
        let desktop_window = create_desktop_window(args.windowed, &desktop_content);
        desktop_window.present();
    }

    let main_loop = Rc::new(glib::MainLoop::new(None, false));
    install_signal_handlers(&main_loop);
    eprintln!(
        "[glaze-shell] shell presented ({}) in {} ms",
        shell.tokens().label(),
        started.elapsed().as_millis()
    );
    {
        // First frame actually on screen: the first tick of the bar's clock.
        let t = started;
        if let Some(w) = shell.first_window() {
            w.add_tick_callback(move |_, _| {
                eprintln!(
                    "[glaze-shell] first frame after {} ms",
                    t.elapsed().as_millis()
                );
                glib::ControlFlow::Break
            });
        }
    }
    main_loop.run();
    eprintln!("[glaze-shell] main loop exited");
    record_session(shell.windows().as_deref());
    ExitCode::SUCCESS
}

/// CSS for the mobile layout (no live reload there yet).
fn install_basic_css(cfg: &GlazeConfig, tokens: &ThemeTokens) {
    let css = tokens.to_gtk_css_with(CssOptions {
        reduced_motion: cfg.appearance.reduced_motion,
        ..CssOptions::default()
    });
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&css);
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
}

/// Poll the service for tray/notification state every 2 s.
#[cfg(unix)]
fn spawn_status_poller(shell: Rc<shell::Shell>) {
    use glaze_shell_core::socket_client::UnixSocketClient;
    use glaze_shell_core::status_feed::poll_status;
    let mut client = UnixSocketClient::new();
    let mut announced = false;
    let mut tick = move || {
        match poll_status(&mut client) {
            Ok(snapshot) => shell.apply_status(snapshot),
            Err(error) => {
                if !announced {
                    eprintln!(
                        "[glaze-shell] status feed unavailable ({}): {error}",
                        client.socket_path().display()
                    );
                    announced = true;
                }
            }
        }
        glib::ControlFlow::Continue
    };
    tick();
    glib::timeout_add_seconds_local(2, tick);
}

#[cfg(not(unix))]
fn spawn_status_poller(_shell: Rc<shell::Shell>) {}

/// Measure the primary output and input devices for form-factor detection.
fn primary_display_info() -> DisplayInfo {
    let mut info = DisplayInfo {
        width_px: 1920,
        height_px: 1080,
        scale: 1.0,
        has_touch: false,
        has_pointer: true,
    };
    if let Some(display) = gdk::Display::default() {
        // GDK monitor geometry is already in logical px; scale stays 1.0.
        if let Some(m) = display
            .monitors()
            .item(0)
            .and_then(|o| o.downcast::<gdk::Monitor>().ok())
        {
            let g = m.geometry();
            info.width_px = g.width();
            info.height_px = g.height();
        }
        if let Some(seat) = display.default_seat() {
            let caps = seat.capabilities();
            info.has_touch = caps.contains(gdk::SeatCapabilities::TOUCH);
            info.has_pointer = caps.contains(gdk::SeatCapabilities::POINTER);
        }
    }
    info
}

/// Run the mobile layout: wallpaper surface, bottom nav bar, app-grid launcher.
fn run_mobile(
    cfg: &GlazeConfig,
    tokens: &ThemeTokens,
    registry: Rc<AppRegistry>,
    windowed: bool,
    display: &DisplayInfo,
    wallpaper_path: Option<std::path::PathBuf>,
) -> ExitCode {
    // No desktop icons on the touch home screen: apps live in the grid.
    let (desktop_content, _wall) =
        desktop_ui::build(registry.clone(), cfg, wallpaper_path.as_deref(), false);
    let desktop_window = create_desktop_window(windowed, &desktop_content);
    desktop_window.present();

    let shell = mobile_ui::build(
        cfg,
        tokens,
        registry,
        windowed,
        (display.width_px, display.height_px),
    );
    shell.nav.present();
    // The grid starts hidden; the Apps button shows it.
    let _keep_grid_alive = &shell.grid;

    let main_loop = Rc::new(glib::MainLoop::new(None, false));
    install_signal_handlers(&main_loop);
    eprintln!(
        "[glaze-shell] mobile shell presented (theme {})",
        tokens.label()
    );
    main_loop.run();
    eprintln!("[glaze-shell] main loop exited");
    ExitCode::SUCCESS
}

/// Replay the applications the last session was running (`session.restore_apps`).
///
/// Only applications that are **still installed** are launched; a recorded id
/// that no longer resolves is reported and skipped (`glaze-session::restore`).
fn restore_session(registry: &AppRegistry) {
    let path = glaze_session::restore::RunningApps::default_path();
    let stored = glaze_session::restore::RunningApps::load_from(&path);
    if stored.is_empty() {
        eprintln!(
            "[glaze-shell] session restore: nothing recorded in {}",
            path.display()
        );
        return;
    }

    let plan = glaze_session::restore::restore_plan(
        &stored,
        registry,
        glaze_session::restore::DEFAULT_MAX_RESTORE,
    );
    eprintln!(
        "[glaze-shell] session restore: {} of {} recorded application(s) still installed",
        plan.len(),
        stored.len()
    );
    for id in plan {
        match registry.get(&id) {
            Some(app) => match glaze_service::launch::launch(app) {
                Ok(_child) => eprintln!("[glaze-shell] restored {id}"),
                Err(error) => eprintln!("[glaze-shell] could not restore {id}: {error}"),
            },
            None => eprintln!("[glaze-shell] restore skipped {id}: no longer installed"),
        }
    }
}

/// Record the applications the session was running, for the next login.
///
/// Written **only** when a compositor backend actually reported windows: with no
/// backend Glaze knows nothing about what was open, and must not invent a list to
/// restore later.
fn record_session(windows: Option<&[WindowInfo]>) {
    let Some(windows) = windows else {
        eprintln!(
            "[glaze-shell] session record: no compositor backend reported windows; nothing recorded"
        );
        return;
    };
    let running =
        glaze_session::restore::RunningApps::new(windows.iter().filter_map(|w| w.app_id.as_deref()));
    let path = glaze_session::restore::RunningApps::default_path();
    match running.save_to(&path) {
        Ok(()) => eprintln!(
            "[glaze-shell] session record: {} application(s) saved to {}",
            running.len(),
            path.display()
        ),
        Err(error) => eprintln!("[glaze-shell] session record: could not save: {error}"),
    }
}

/// Create the desktop surface window: a layer-shell **background** surface when
/// available (never covering windows), else a plain windowed fallback.
fn create_desktop_window(force_windowed: bool, content: &gtk4::Widget) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.set_title(Some("Glaze Desktop"));
    window.set_child(Some(content));
    window.set_decorated(false);

    if !force_windowed && gtk4_layer_shell::is_supported() {
        use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
        window.init_layer_shell();
        window.set_namespace(Some("glaze-desktop"));
        window.set_layer(Layer::Background);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        eprintln!("[glaze-shell] desktop surface: layer-shell background layer");
    } else {
        window.set_default_size(1280, 800);
        eprintln!(
            "[glaze-shell] desktop surface: windowed ({})",
            if force_windowed {
                "--windowed"
            } else {
                "layer-shell unavailable"
            }
        );
    }
    window
}

/// Quit the GLib main loop cleanly on SIGINT / SIGTERM.
///
/// glib 0.22 removed `source::unix_signal_add` (it existed in 0.20), so the
/// handlers set an atomic flag that a small main-loop timeout polls — the same
/// portable pattern `glaze-session` uses for its signal handling.
fn install_signal_handlers(main_loop: &Rc<glib::MainLoop>) {
    #[cfg(unix)]
    unsafe {
        signal(libc::SIGINT, on_signal);
        signal(libc::SIGTERM, on_signal);
    }
    let main_loop = main_loop.clone();
    glib::timeout_add_local(Duration::from_millis(200), move || {
        if SHUTDOWN.load(Ordering::Relaxed) {
            eprintln!("[glaze-shell] termination signal received; shutting down");
            main_loop.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Set by the raw signal handler; polled by the main loop.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_sig: i32) {
    // Async-signal-safe: only an atomic store.
    SHUTDOWN.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
extern "C" {
    // Declared with a real function-pointer type to avoid an `as usize` cast.
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}
