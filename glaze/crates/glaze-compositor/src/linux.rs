//! Linux entry point: arguments, logging, the Wayland display, the event loop,
//! the control socket, and backend selection.

use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::time::Duration;

use calloop::generic::Generic;
use calloop::signals::{Signal, Signals};
use calloop::{EventLoop, Interest, Mode, PostAction};
use glaze_common::model::{WindowInfo, WorkspaceInfo};
use glaze_wm::{LayoutMode, LayoutScope, ModeRequest};
#[allow(unused_imports)]
use serde_json;
use smithay::reexports::wayland_server::Display;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{info, warn};

use crate::control::{self, ControlTarget};
use crate::state::{ClientState, Options, State};

const USAGE: &str = "\
glaze-compositor — Glaze's Wayland compositor

USAGE: glaze-compositor [OPTIONS]

  --backend B          udev (a tty session: DRM/KMS, libinput, libseat),
                       winit (nested in a Wayland/X11 session) or auto
                       (default: winit when WAYLAND_DISPLAY/DISPLAY is set,
                       else udev)
  --spawn CMD          run CMD (via sh -c) inside the compositor; repeatable
  --size WxH           nested window size (default 1280x800); for udev use
                       GLAZE_OUTPUT_MODE=WxH[@Hz] to pick a connector mode
  --reduced-motion     disable animations (also: [appearance] reduced_motion)
  --anim-speed F       animation duration multiplier (default 1.0; env GLAZE_ANIM_SPEED)
  --dark               dark frame palette
  --tiling             start with the global layout set to tiling
  -h, --help           this help

Environment: RUST_LOG (default info), GLAZE_COMPOSITOR_SOCKET (control socket path),
XCURSOR_THEME / XCURSOR_SIZE (pointer on the udev backend).
Ctrl+Alt+Backspace exits; Ctrl+Alt+F1..F12 switch VTs (udev).";

/// Install a `tracing` subscriber driven by `RUST_LOG` (default `info`).
pub fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .try_init();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendKind {
    Auto,
    Winit,
    Udev,
}

struct Args {
    options: Options,
    tiling: bool,
    backend: BackendKind,
}

fn parse_args() -> Result<Args, String> {
    let mut options = Options::default();
    let config =
        glaze_common::config::GlazeConfig::load_from(&glaze_common::paths::config_file()).ok();
    if let Some(c) = &config {
        options.motion.reduced = c.appearance.reduced_motion;
        options.dark = matches!(c.appearance.scheme, glaze_common::config::ColorScheme::Dark);
    }
    if let Some(speed) = std::env::var("GLAZE_ANIM_SPEED")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        options.motion.speed = speed;
        options.motion_override = true;
    }
    let mut tiling = false;
    let mut backend = BackendKind::Auto;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--spawn" => options
                .spawn
                .push(args.next().ok_or("--spawn needs a command")?),
            "--size" => {
                let v = args.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size needs WxH")?;
                options.size = Some((
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                ));
            }
            "--reduced-motion" => {
                options.motion.reduced = true;
                options.motion_override = true;
            }
            "--anim-speed" => {
                options.motion_override = true;
                options.motion.speed = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or("--anim-speed needs a number")?;
            }
            "--dark" => options.dark = true,
            "--tiling" => tiling = true,
            "--backend" => {
                backend = match args.next().as_deref() {
                    Some("udev") | Some("drm") | Some("tty") => BackendKind::Udev,
                    Some("winit") | Some("nested") => BackendKind::Winit,
                    Some("auto") => BackendKind::Auto,
                    other => {
                        return Err(format!(
                            "--backend needs udev, winit or auto, not {other:?}"
                        ))
                    }
                }
            }
            "-h" | "--help" => return Err(USAGE.to_owned()),
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    Ok(Args {
        options,
        tiling,
        backend,
    })
}

/// Start the compositor and block until it exits.
pub fn run() -> io::Result<()> {
    let args = parse_args().map_err(io::Error::other)?;
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "XDG_RUNTIME_DIR is not set; glaze-compositor creates its Wayland socket there",
        ));
    }
    let has_parent =
        std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
    let backend = match args.backend {
        BackendKind::Auto if has_parent => BackendKind::Winit,
        BackendKind::Auto => BackendKind::Udev,
        b => b,
    };
    if backend == BackendKind::Winit && !has_parent {
        return Err(io::Error::other(
            "no WAYLAND_DISPLAY or DISPLAY: the nested (winit) backend needs a parent session; \
             use --backend udev on a tty",
        ));
    }
    info!(
        ?backend,
        motion_speed = args.options.motion.speed,
        reduced_motion = args.options.motion.reduced,
        "glaze-compositor starting"
    );

    let mut event_loop: EventLoop<'static, State> =
        EventLoop::try_new().map_err(io::Error::from)?;
    let display: Display<State> = Display::new().map_err(io::Error::other)?;
    let dh = display.handle();

    event_loop
        .handle()
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state: &mut State| {
                // SAFETY: the display is only touched here and lives as long
                // as the loop (anvil pattern).
                unsafe {
                    display.get_mut().dispatch_clients(state).unwrap();
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| io::Error::other(e.to_string()))?;
    let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM]).map_err(io::Error::from)?;
    event_loop
        .handle()
        .insert_source(signals, |event, _, state: &mut State| {
            info!(signal = ?event.signal(), "termination signal, shutting down");
            state.request_exit();
        })
        .map_err(|e| io::Error::other(e.to_string()))?;

    let mut state = State::new(
        dh,
        event_loop.handle(),
        event_loop.get_signal(),
        std::ffi::OsString::new(),
        &args.options,
    )?;
    if args.tiling {
        state
            .wm
            .set_layout(LayoutScope::Global, ModeRequest::Set(LayoutMode::Tiling));
    }

    // The winit backend reads the *parent's* WAYLAND_DISPLAY, so it must be
    // created before anything points WAYLAND_DISPLAY at us (children get our
    // socket through `State::spawn`).
    match backend {
        BackendKind::Udev => crate::backend::udev::init(&mut state)
            .map_err(|e| io::Error::other(format!("session (udev/DRM) backend failed: {e}")))?,
        _ => crate::backend::winit::init(&mut state, args.options.size.unwrap_or((1280, 800)))?,
    }
    // The Wayland socket appears only once the backend works, so a session
    // launcher waiting for it (glaze-session) never mistakes a compositor that
    // is about to fail for a ready one.
    let socket = ListeningSocketSource::new_auto().map_err(io::Error::other)?;
    let socket_name = socket.socket_name().to_os_string();
    event_loop
        .handle()
        .insert_source(socket, |stream, _, state: &mut State| {
            if let Err(e) = state
                .display_handle
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                warn!(%e, "rejected client");
            }
        })
        .map_err(|e| io::Error::other(e.to_string()))?;
    state.socket_name = socket_name.clone();
    crate::xwayland::start(&mut state);
    info!(socket = ?socket_name, "listening on the Wayland socket");

    match start_control_socket(&mut state) {
        Ok(path) => {
            info!(path = %path.display(), "control socket");
            state.control_socket = Some(path);
        }
        Err(e) => warn!(%e, "control socket unavailable"),
    }

    // Live frame colours and motion settings from the Glaze config and the
    // system colour scheme (~/.config/glaze/colour-scheme), polled once a
    // second (a stat of two files).
    state.reload_config(true);
    let _ = state.loop_handle.insert_source(
        calloop::timer::Timer::from_duration(Duration::from_secs(1)),
        |_, _, state| {
            state.reload_config(false);
            calloop::timer::TimeoutAction::ToDuration(Duration::from_secs(1))
        },
    );

    for cmd in args.options.spawn.clone() {
        state.spawn(&cmd);
    }

    // Idle-redraw guard: while something animates the loop ticks at most
    // every 16 ms (vblanks pace the frames themselves); otherwise it sleeps
    // until a client, input, a vblank or a timer wakes it, so an idle
    // desktop renders nothing and costs no wake-ups.
    while !state.exit_requested {
        let timeout = state.wants_tick().then_some(Duration::from_millis(16));
        event_loop
            .dispatch(timeout, &mut state)
            .map_err(io::Error::from)?;
        state.loop_wakeups += 1;
        crate::backend::post_dispatch(&mut state);
        state.check_locker();
        state.check_exit_fade();
        state.flush_ipc_events();
        let _ = state.display_handle.flush_clients();
    }

    if let Some(path) = &state.control_socket {
        let _ = std::fs::remove_file(path);
    }
    info!(frames = state.frames_total, "glaze-compositor stopped");
    Ok(())
}

// --- control socket -------------------------------------------------------------

impl ControlTarget for State {
    fn windows(&self) -> Vec<WindowInfo> {
        self.wm.window_infos()
    }
    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        self.wm.workspace_infos()
    }
    fn activate(&mut self, id: &str) -> bool {
        self.activate_window(id)
    }
    fn close(&mut self, id: &str) -> bool {
        self.close_window(id)
    }
    fn set_layout(&mut self, scope: LayoutScope, request: ModeRequest) -> LayoutMode {
        State::set_layout(self, scope, request);
        self.wm.active_mode()
    }
    fn workspace_count(&self) -> u32 {
        self.wm.workspace_count()
    }
    fn minimize(&mut self, id: &str) -> bool {
        self.minimize_window(id)
    }
    fn restore(&mut self, id: &str) -> bool {
        self.activate_window(id)
    }
    fn set_maximized(&mut self, id: &str, on: Option<bool>) -> bool {
        let Some(w) = self.wm.get(id) else {
            return false;
        };
        let target = on.unwrap_or(!w.maximized);
        State::set_maximized(self, id, target);
        true
    }
    fn set_fullscreen(&mut self, id: &str, on: Option<bool>) -> bool {
        let Some(w) = self.wm.get(id) else {
            return false;
        };
        let target = on.unwrap_or(!w.fullscreen);
        State::set_fullscreen(self, id, target);
        true
    }
    fn move_to_workspace(&mut self, id: &str, workspace: u32) -> bool {
        if self.wm.get(id).is_none() {
            return false;
        }
        self.wm.move_to_workspace(id, workspace);
        self.apply_layout(true);
        true
    }
    fn window_state(&self, id: &str) -> Option<(bool, bool, bool)> {
        self.wm
            .get(id)
            .map(|w| (w.maximized, w.fullscreen, w.minimized))
    }
    fn layout_mode(&self, workspace: Option<u32>) -> LayoutMode {
        self.wm
            .layout
            .mode_for(workspace.unwrap_or_else(|| self.wm.active_workspace()))
    }
    fn set_minimize_rects(&mut self, namespace: &str, rects: &[control::RectMsg]) -> bool {
        let Some(origin) = self.layer_origin(namespace) else {
            return false;
        };
        self.minimize_rects = rects
            .iter()
            .filter(|r| r.w > 0 && r.h > 0)
            .map(|r| {
                (
                    r.id.clone(),
                    glaze_wm::Rect::new(origin.0 + r.x, origin.1 + r.y, r.w, r.h),
                )
            })
            .collect();
        true
    }
    fn lock(&mut self) {
        self.start_locker();
    }
    fn is_locked(&self) -> bool {
        self.locked
    }
    fn exit(&mut self) {
        info!("log out requested over the control socket");
        self.request_exit_animated();
    }
    fn set_frame_colors(&mut self, c: &crate::frame::FrameColorsMsg) -> bool {
        match crate::frame::Palette::from_tokens(c) {
            Some(p) => {
                info!(focused_bg = %c.focused_bg, accent = %c.focused_border, "frame colours from the shell");
                self.set_palette(p);
                true
            }
            None => false,
        }
    }
}

fn start_control_socket(state: &mut State) -> io::Result<std::path::PathBuf> {
    let path = std::env::var_os(control::SOCKET_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(control::default_socket_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if path.exists() {
        // A live compositor would answer; a stale file would not.
        if UnixStream::connect(&path).is_ok() {
            return Err(io::Error::other(format!(
                "{} is in use by another compositor",
                path.display()
            )));
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    state
        .loop_handle
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            |_, listener, state: &mut State| {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => add_control_client(state, stream),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) => {
                            warn!(%e, "control accept failed");
                            break;
                        }
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(path)
}

fn add_control_client(state: &mut State, stream: UnixStream) {
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let mut buf: Vec<u8> = Vec::new();
    let res = state.loop_handle.insert_source(
        Generic::new(stream, Interest::READ, Mode::Level),
        move |_, stream, state: &mut State| {
            let s: &UnixStream = stream;
            let mut chunk = [0u8; 4096];
            loop {
                match (&*s).read(&mut chunk) {
                    Ok(0) => return Ok(PostAction::Remove),
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => return Ok(PostAction::Remove),
                }
            }
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                if line.trim().is_empty() {
                    continue;
                }
                let reply = match control::handle_ext_line(state, &line) {
                    Some(control::ExtReply::Subscribe) => {
                        // This connection becomes an event stream.
                        if let Ok(clone) = s.try_clone() {
                            info!("control: event subscriber");
                            state.subscribers.push(clone);
                        }
                        serde_json::json!({"kind": "ok"})
                    }
                    Some(control::ExtReply::Json(v)) => v,
                    None => {
                        serde_json::to_value(control::handle_line(state, &line)).unwrap_or_default()
                    }
                };
                let is_error = reply.get("kind").and_then(|k| k.as_str()) == Some("error");
                let quiet = line.contains("\"list-windows\"")
                    || line.contains("\"get-layout\"")
                    || line.contains("\"list-workspaces\"")
                    || line.contains("\"ping\"")
                    || line.contains("\"set-minimize-rects\"");
                if is_error || !quiet {
                    info!(request = %line.trim(), ok = !is_error, "control request");
                }
                let mut out = reply.to_string();
                out.push('\n');
                let _ = s.set_nonblocking(false);
                let _ = (&*s).write_all(out.as_bytes());
                let _ = s.set_nonblocking(true);
            }
            Ok(PostAction::Continue)
        },
    );
    if let Err(e) = res {
        warn!(%e, "control client source");
    }
}
