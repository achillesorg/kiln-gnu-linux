//! Live state of the running Glaze compositor (Linux only).
//!
//! [`State`] owns every Smithay protocol state object plus the portable
//! [`WindowManager`] (the policy brain) and the per-window render data
//! (frames, animations). The `impl <Protocol>Handler for State` blocks live in
//! [`crate::handlers`], one file per protocol, following anvil's layout.
//!
//! The one rule that keeps this file manageable: **policy changes go through
//! `self.wm`, then [`State::apply_layout`] mirrors the result onto surfaces**
//! (map/unmap, configure sizes and states, keyboard focus, animations,
//! foreign-toplevel state). Nothing else moves windows.

use smithay::wayland::seat::WaylandFocus;
use std::ffi::OsString;
use std::time::{Duration, Instant};

use calloop::{LoopHandle, LoopSignal};
use glaze_wm::{LayoutScope, ModeRequest, Rect};
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::ContextId;
use smithay::desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::{CursorImageStatus, PointerHandle};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Logical, Point, Size, SERIAL_COUNTER};
use smithay::wayland::compositor::{with_states, CompositorClientState, CompositorState};
use smithay::wayland::foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelListState};
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgShellState, XdgToplevelSurfaceData};
use smithay::wayland::shm::ShmState;
use smithay::wayland::viewporter::ViewporterState;
use tracing::{debug, info, warn};

use crate::anim::{AnimKind, FrameStats, MotionConfig, RectF, WindowAnim};
use crate::backend::BackendData;
use crate::foreign_toplevel::WlrForeignToplevelState;
use crate::frame::{Button, Insets, Palette, TitleState};
use crate::render::FrameParts;
use crate::wm::WindowManager;

/// Logical seat name.
pub const SEAT_NAME: &str = "seat0";
const KEY_REPEAT_DELAY_MS: i32 = 300;
const KEY_REPEAT_RATE_HZ: i32 = 30;

/// Per-client data Smithay requires.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        debug!(?client_id, "client initialised");
    }
    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        match reason {
            DisconnectReason::ProtocolError(ref e) => {
                warn!(?client_id, error = %e, "client killed for a protocol error")
            }
            _ => debug!(?client_id, ?reason, "client disconnected"),
        }
    }
}

/// One toplevel and everything Glaze draws for it.
pub struct WindowEntry {
    /// `None` until the first buffer commit maps it into the window manager.
    pub id: Option<String>,
    pub window: Window,
    /// Glaze draws the frame (xdg-decoration negotiated server-side).
    pub ssd: bool,
    /// The outer rect last applied (what the window rests at).
    pub rest: Option<Rect>,
    pub anim: Option<WindowAnim>,
    /// In the render list. A minimising window stays shown until its
    /// animation ends.
    pub shown: bool,
    pub title_state: Option<TitleState>,
    pub title_buffer: Option<MemoryRenderBuffer>,
    pub hovered: Option<Button>,
    pub pressed: Option<Button>,
    pub ext_handle: Option<ForeignToplevelHandle>,
    pub parts: FrameParts,
    /// How often the client asked for client-side decorations.
    pub csd_requests: u8,
    /// Snapshot (old outer rect + textures) shown during a size morph.
    pub morph: Option<(Rect, Vec<GhostSurface>)>,
    /// Previous title bar and when the crossfade to the new one started.
    pub prev_title: Option<(MemoryRenderBuffer, Duration)>,
}

impl WindowEntry {
    pub fn new(window: Window) -> Self {
        Self {
            id: None,
            window,
            ssd: false,
            rest: None,
            anim: None,
            shown: false,
            title_state: None,
            title_buffer: None,
            hovered: None,
            pressed: None,
            ext_handle: None,
            parts: FrameParts::default(),
            csd_requests: 0,
            morph: None,
            prev_title: None,
        }
    }

    /// The xdg toplevel (`None` for an X11 window).
    pub fn toplevel(&self) -> Option<&ToplevelSurface> {
        self.window.toplevel()
    }

    /// The X11 window (XWayland), if this is one.
    pub fn x11(&self) -> Option<&smithay::xwayland::X11Surface> {
        self.window.x11_surface()
    }

    /// The root wl_surface (an X11 window has none until XWayland
    /// associates it).
    pub fn wl_surface(&self) -> Option<WlSurface> {
        self.window.wl_surface().map(|s| s.into_owned())
    }

    pub fn insets(&self, wm: &WindowManager) -> Insets {
        let fullscreen = self
            .id
            .as_deref()
            .and_then(|id| wm.get(id))
            .is_some_and(|w| w.fullscreen);
        if self.ssd && !fullscreen {
            Insets::FRAMED
        } else {
            Insets::NONE
        }
    }
}

/// A textured snapshot of one surface of a closing window.
pub struct GhostSurface {
    pub id: smithay::backend::renderer::element::Id,
    pub texture: GlesTexture,
    pub context: ContextId<GlesTexture>,
    /// Offset of the surface from the client geometry origin.
    pub offset: Point<i32, Logical>,
    pub size: Size<i32, Logical>,
    pub scale: i32,
    pub transform: smithay::utils::Transform,
}

/// A window that is gone but still animating out.
pub struct Ghost {
    pub rest: Rect,
    pub insets: Insets,
    pub surfaces: Vec<GhostSurface>,
    pub title_buffer: Option<MemoryRenderBuffer>,
    pub parts: FrameParts,
    pub anim: WindowAnim,
}

/// Startup options.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub spawn: Vec<String>,
    pub motion: MotionConfig,
    /// `--anim-speed`, `--reduced-motion` or `GLAZE_ANIM_SPEED` given: the
    /// config's `[motion]` no longer applies.
    pub motion_override: bool,
    pub dark: bool,
    pub size: Option<(i32, i32)>,
}

pub struct State {
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, State>,
    pub loop_signal: LoopSignal,
    pub socket_name: OsString,
    pub control_socket: Option<std::path::PathBuf>,

    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub kde_decoration_state: smithay::wayland::shell::kde::decoration::KdeDecorationState,
    /// Surfaces that agreed to server-side frames over the KDE protocol.
    pub kde_ssd: Vec<WlSurface>,
    pub layer_shell_state: WlrLayerShellState,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    /// `zwlr_data_control_manager_v1` (wl-copy/wl-paste, cliphist).
    pub data_control_state: smithay::wayland::selection::wlr_data_control::DataControlState,
    pub output_manager_state: OutputManagerState,
    pub viewporter_state: ViewporterState,
    pub fractional_scale_state: FractionalScaleManagerState,
    pub foreign_toplevel_list_state: ForeignToplevelListState,
    pub wlr_foreign_toplevel: WlrForeignToplevelState,
    pub cursor_shape_state: smithay::wayland::cursor_shape::CursorShapeManagerState,
    pub session_lock_state: smithay::wayland::session_lock::SessionLockManagerState,
    pub xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState,
    /// The X11 window manager once XWayland is ready.
    pub xwm: Option<smithay::xwayland::X11Wm>,
    /// XWayland display number (`DISPLAY=:N`).
    pub xdisplay: Option<u32>,
    /// Mapped override-redirect X11 windows (menus, tooltips).
    pub x11_override: Vec<smithay::xwayland::X11Surface>,
    /// ext-session-lock is active: only lock surfaces are drawn and get input.
    pub locked: bool,
    pub lock_surfaces: Vec<(Output, smithay::wayland::session_lock::LockSurface)>,
    pub last_locker_spawn: Option<Instant>,
    pub popups: PopupManager,

    pub seat: Seat<Self>,
    pub pointer: PointerHandle<Self>,
    pub cursor_status: CursorImageStatus,
    pub pointer_location: Point<f64, Logical>,

    pub space: Space<Window>,
    /// The primary output: the window manager's work area lives here.
    pub output: Option<Output>,
    /// Every output, primary first (DRM backend: one per connector).
    pub outputs: Vec<Output>,
    pub backend: Option<BackendData>,
    /// Draw the pointer ourselves (DRM); nested, the parent draws it.
    pub software_cursor: bool,
    pub cursors: Option<crate::cursor::CursorCache>,
    /// Cursor the compositor wants over its own frames and grabs (resize
    /// edges, move); `None` = whatever the client under the pointer set.
    pub frame_cursor: Option<smithay::input::pointer::CursorIcon>,

    pub wm: WindowManager,
    pub windows: Vec<WindowEntry>,
    pub ghosts: Vec<Ghost>,
    pub palette: Palette,
    pub title_font: Option<ab_glyph::FontVec>,
    pub motion: MotionConfig,

    pub start_time: Instant,
    pub needs_redraw: bool,
    pub frame_stats: FrameStats,
    pub last_stats_log: Instant,
    pub frames_total: u64,
    pub exit_requested: bool,
    /// Last left click on a title bar, for double-click maximise.
    pub last_title_click: Option<(String, Instant)>,
    /// Keys whose press triggered a shortcut; their release is swallowed.
    pub suppressed_keys: Vec<smithay::input::keyboard::Keycode>,

    /// Control-socket event subscribers (the Glaze shell's taskbar).
    pub subscribers: Vec<std::os::unix::net::UnixStream>,
    /// Window/workspace state may have changed since the last event.
    pub ipc_dirty: bool,
    /// What subscribers last saw (window infos, workspaces, layout).
    pub ipc_last: String,
    /// New window ids not announced yet.
    pub ipc_new: Vec<String>,
    /// mtimes of the config file and the colour-scheme file.
    pub config_stamp: Option<(Option<std::time::SystemTime>, Option<std::time::SystemTime>)>,
    /// (dark, accent) last derived from config, to apply only real changes.
    pub config_look: Option<(bool, Option<String>)>,
    /// Motion from the command line / environment (wins over config).
    pub motion_override: bool,

    /// Active workspace at the last layout (for slide direction).
    pub last_workspace: u32,
    /// When the Alt+Tab card appeared (fade/scale in).
    pub switcher_since: Duration,
    /// When the session got locked (lock surface fade-in).
    pub locked_since: Duration,
    /// Log-out fade: when it started (the compositor exits at its end).
    pub exit_fade: Option<Duration>,
    pub fade_buf: smithay::backend::renderer::element::solid::SolidColorBuffer,
    /// First time each layer surface was drawn (popup/menu fade+offset).
    pub layer_seen: std::collections::HashMap<smithay::reexports::wayland_server::backend::ObjectId, Duration>,
    /// Window being resized interactively and the dragged edges.
    pub resizing: Option<(String, crate::frame::Edges)>,
    /// Drag-to-edge snap target shown while moving a window.
    pub snap_preview: Option<SnapPreview>,
    pub preview_parts: FrameParts,
    /// Task-button rectangles (global) reported by the shell: minimise and
    /// restore animate to/from them.
    pub minimize_rects: std::collections::HashMap<String, Rect>,
    /// Alt+Tab switcher card (painted when its content changes).
    pub switcher: Option<(Vec<crate::frame::SwitcherRow>, usize, MemoryRenderBuffer, (i32, i32))>,
}

/// The paper sheet that shows where a dragged window will snap.
#[derive(Debug, Clone)]
pub struct SnapPreview {
    pub window: String,
    pub zone: glaze_wm::SnapZone,
    pub target: Rect,
    pub from: RectF,
    pub start: Duration,
    pub duration: Duration,
}

impl SnapPreview {
    /// Current rect and opacity (grows out of the pointer's window).
    pub fn sample(&self, now: Duration) -> (RectF, f32) {
        let t = if self.duration.is_zero() {
            1.0
        } else {
            (now.saturating_sub(self.start).as_secs_f64() / self.duration.as_secs_f64())
                .clamp(0.0, 1.0)
        };
        let e = crate::anim::Easing::OutCubic.apply(t);
        (
            RectF::lerp(self.from, RectF::from(self.target), e),
            (0.35 + 0.65 * e) as f32,
        )
    }

    pub fn is_settling(&self, now: Duration) -> bool {
        now.saturating_sub(self.start) < self.duration
    }
}

impl State {
    pub fn new(
        display_handle: DisplayHandle,
        loop_handle: LoopHandle<'static, State>,
        loop_signal: LoopSignal,
        socket_name: OsString,
        options: &Options,
    ) -> std::io::Result<Self> {
        let dh = &display_handle;
        let compositor_state = CompositorState::new::<Self>(dh);
        let shm_state = ShmState::new::<Self>(dh, vec![]);
        let xdg_shell_state = XdgShellState::new::<Self>(dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(dh);
        let kde_decoration_state =
            smithay::wayland::shell::kde::decoration::KdeDecorationState::new::<Self>(
                dh,
                smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode::Server,
            );
        let layer_shell_state = WlrLayerShellState::new::<Self>(dh);
        let data_device_state = DataDeviceState::new::<Self>(dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(dh);
        let data_control_state =
            smithay::wayland::selection::wlr_data_control::DataControlState::new::<Self, _>(
                dh,
                Some(&primary_selection_state),
                |_| true,
            );
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
        let viewporter_state = ViewporterState::new::<Self>(dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(dh);
        let foreign_toplevel_list_state = ForeignToplevelListState::new::<Self>(dh);
        let wlr_foreign_toplevel = WlrForeignToplevelState::new(dh);
        let cursor_shape_state =
            smithay::wayland::cursor_shape::CursorShapeManagerState::new::<Self>(dh);
        let xwayland_shell_state =
            smithay::wayland::xwayland_shell::XWaylandShellState::new::<Self>(dh);
        let session_lock_state =
            smithay::wayland::session_lock::SessionLockManagerState::new::<Self, _>(dh, |_| true);

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(dh, SEAT_NAME);
        let pointer = seat.add_pointer();
        seat.add_keyboard(
            XkbConfig::default(),
            KEY_REPEAT_DELAY_MS,
            KEY_REPEAT_RATE_HZ,
        )
        .map_err(|e| std::io::Error::other(format!("keyboard (libxkbcommon?): {e}")))?;

        let title_font = match crate::frame::load_title_font() {
            Some((font, path)) => {
                info!(font = %path, "title font loaded");
                Some(font)
            }
            None => {
                warn!("no title font found: frames are drawn without title text");
                None
            }
        };

        info!(
            "bound globals: wl_compositor, wl_subcompositor, wl_shm, wl_seat, wl_data_device_manager, \
             zwp_primary_selection_device_manager_v1, wl_output, zxdg_output_manager_v1, xdg_wm_base, \
             zxdg_decoration_manager_v1, zwlr_layer_shell_v1, wp_viewporter, \
             wp_fractional_scale_manager_v1, ext_foreign_toplevel_list_v1, \
             zwlr_foreign_toplevel_manager_v1, wp_cursor_shape_manager_v1, ext_session_lock_manager_v1, \
             org_kde_kwin_server_decoration_manager, zwlr_data_control_manager_v1"
        );

        Ok(Self {
            display_handle,
            loop_handle,
            loop_signal,
            socket_name,
            control_socket: None,
            compositor_state,
            shm_state,
            xdg_shell_state,
            xdg_decoration_state,
            kde_decoration_state,
            kde_ssd: Vec::new(),
            layer_shell_state,
            seat_state,
            data_device_state,
            primary_selection_state,
            data_control_state,
            output_manager_state,
            viewporter_state,
            fractional_scale_state,
            foreign_toplevel_list_state,
            wlr_foreign_toplevel,
            cursor_shape_state,
            session_lock_state,
            xwayland_shell_state,
            xwm: None,
            xdisplay: None,
            x11_override: Vec::new(),
            locked: false,
            lock_surfaces: Vec::new(),
            last_locker_spawn: None,
            popups: PopupManager::default(),
            seat,
            pointer,
            cursor_status: CursorImageStatus::default_named(),
            pointer_location: Point::from((0.0, 0.0)),
            space: Space::default(),
            output: None,
            outputs: Vec::new(),
            backend: None,
            software_cursor: false,
            cursors: None,
            frame_cursor: None,
            wm: WindowManager::new(Rect::new(0, 0, 1280, 800)),
            windows: Vec::new(),
            ghosts: Vec::new(),
            palette: if options.dark {
                Palette::clay_dark()
            } else {
                Palette::clay_light()
            },
            title_font,
            motion: options.motion,
            start_time: Instant::now(),
            needs_redraw: true,
            frame_stats: FrameStats::default(),
            last_stats_log: Instant::now(),
            frames_total: 0,
            exit_requested: false,
            last_title_click: None,
            suppressed_keys: Vec::new(),
            subscribers: Vec::new(),
            ipc_dirty: true,
            ipc_last: String::new(),
            ipc_new: Vec::new(),
            config_stamp: None,
            config_look: None,
            motion_override: options.motion_override,
            snap_preview: None,
            resizing: None,
            last_workspace: 0,
            switcher_since: Duration::ZERO,
            locked_since: Duration::ZERO,
            exit_fade: None,
            fade_buf: Default::default(),
            layer_seen: Default::default(),
            preview_parts: FrameParts::default(),
            switcher: None,
            minimize_rects: Default::default(),
        })
    }

    /// Global position of the layer surface with `namespace` (any output).
    pub fn layer_origin(&self, namespace: &str) -> Option<(i32, i32)> {
        for output in &self.outputs {
            let out_loc = self.space.output_geometry(output)?.loc;
            let found = {
                let map = layer_map_for_output(output);
                let layer = map.layers().find(|l| l.namespace() == namespace).cloned();
                layer.and_then(|l| map.layer_geometry(&l))
            };
            if let Some(g) = found {
                return Some((out_loc.x + g.loc.x, out_loc.y + g.loc.y));
            }
        }
        None
    }

    /// Replace the frame palette and repaint every title bar.
    pub fn set_palette(&mut self, palette: Palette) {
        if self.palette == palette {
            return;
        }
        self.palette = palette;
        for entry in &mut self.windows {
            entry.title_state = None;
        }
        self.refresh_titles();
        self.needs_redraw = true;
    }

    /// Re-read `~/.config/glaze/config.toml` and `~/.config/glaze/colour-scheme`
    /// when they changed: frame scheme/accent and motion settings follow live.
    pub fn reload_config(&mut self, force: bool) {
        let dir = glaze_common::paths::glaze_config_dir();
        let cfg_path = dir.join("config.toml");
        let scheme_path = dir.join("colour-scheme");
        let mtime = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let stamp = (mtime(&cfg_path), mtime(&scheme_path));
        if !force && self.config_stamp == Some(stamp) {
            return;
        }
        self.config_stamp = Some(stamp);
        let cfg = glaze_common::config::GlazeConfig::load_from(&cfg_path).ok();
        let file_scheme = std::fs::read_to_string(&scheme_path)
            .ok()
            .map(|s| s.trim().to_owned());
        let dark = match file_scheme.as_deref() {
            Some("dark") => true,
            Some("light") => false,
            _ => cfg.as_ref().is_some_and(config_is_dark),
        };
        let accent = cfg.as_ref().and_then(|c| c.appearance.accent.clone());
        let look = (dark, accent.clone());
        if force || self.config_look.as_ref() != Some(&look) {
            info!(dark, accent = ?accent, "frame scheme from config");
            self.config_look = Some(look);
            let accent = accent.as_deref().and_then(crate::frame::Rgba::parse);
            self.set_palette(Palette::for_scheme(dark, accent));
        }
        if let (Some(c), false) = (&cfg, self.motion_override) {
            use glaze_common::config::MotionStyle;
            let base = 1.0 / c.motion.speed.clamp(0.25, 4.0);
            let motion = MotionConfig {
                speed: match c.motion.style {
                    MotionStyle::Snappy => base * 0.65,
                    _ => base,
                },
                reduced: c.appearance.reduced_motion || c.motion.style == MotionStyle::Off,
                intensity: read_intensity(&cfg_path),
            };
            if motion != self.motion {
                info!(speed = motion.speed, reduced = motion.reduced, intensity = ?motion.intensity, "motion from config");
                self.motion = motion;
            }
        }
    }

    /// Tell control-socket subscribers about changes (once per dispatch).
    pub fn flush_ipc_events(&mut self) {
        if self.subscribers.is_empty() {
            self.ipc_new.clear();
            return;
        }
        let mut lines: Vec<String> = self
            .ipc_new
            .drain(..)
            .map(|id| {
                serde_json::json!({"kind":"event","scope":"window","change":"new","id":id})
                    .to_string()
            })
            .collect();
        if self.ipc_dirty {
            self.ipc_dirty = false;
            let sig = serde_json::json!([
                self.wm.window_infos(),
                self.wm.workspace_infos(),
                self.wm.active_mode().as_str()
            ])
            .to_string();
            if sig != self.ipc_last {
                self.ipc_last = sig;
                lines.push(
                    serde_json::json!({"kind":"event","scope":"window","change":"change"})
                        .to_string(),
                );
            }
        }
        if lines.is_empty() {
            return;
        }
        let mut payload = lines.join("\n");
        payload.push('\n');
        self.subscribers.retain(|s| {
            use std::io::Write;
            let _ = s.set_nonblocking(true);
            match (&*s).write(payload.as_bytes()) {
                Ok(n) if n == payload.len() => true,
                Ok(_) => true, // short write: the next event resyncs anyway
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => true,
                Err(_) => {
                    info!("control: event subscriber gone");
                    false
                }
            }
        });
    }

    /// Housekeeping after a frame was rendered for `output`.
    pub fn after_render(&mut self, output: &Output) {
        self.space.refresh();
        self.popups.cleanup();
        layer_map_for_output(output).cleanup();
        let _ = self.display_handle.flush_clients();
    }

    /// Monotonic animation clock.
    pub fn now(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Log out with a short fade to ink; the loop stops when it ends.
    pub fn request_exit_animated(&mut self) {
        if self.motion.duration(crate::anim::durations::LOGOUT).is_zero() {
            self.request_exit();
            return;
        }
        if self.exit_fade.is_none() {
            self.exit_fade = Some(self.now());
            self.needs_redraw = true;
        }
    }

    /// Stop the loop once the log-out fade has run.
    pub fn check_exit_fade(&mut self) {
        if let Some(t) = self.exit_fade {
            if self.now().saturating_sub(t) >= self.motion.duration(crate::anim::durations::LOGOUT) {
                self.request_exit();
            }
        }
    }

    pub fn request_exit(&mut self) {
        self.exit_requested = true;
        self.loop_signal.stop();
    }

    /// Anything animating right now (forces a redraw every frame).
    pub fn animating(&self) -> bool {
        let now = self.now();
        self.windows
            .iter()
            .any(|w| w.anim.is_some_and(|a| !a.is_done(now)))
            || !self.ghosts.is_empty()
            || self.snap_preview.as_ref().is_some_and(|p| p.is_settling(now))
            || self.windows.iter().any(|w| {
                w.prev_title
                    .as_ref()
                    .is_some_and(|(_, t)| now.saturating_sub(*t) < self.motion.duration(crate::anim::durations::FOCUS))
            })
            || (self.switcher.is_some()
                && now.saturating_sub(self.switcher_since) < self.motion.duration(crate::anim::durations::SWITCHER))
            || (self.locked
                && now.saturating_sub(self.locked_since) < self.motion.duration(crate::anim::durations::LOCK))
            || self.exit_fade.is_some()
            || self
                .layer_seen
                .values()
                .any(|t| now.saturating_sub(*t) < self.motion.duration(crate::anim::durations::POPUP))
    }

    // --- lookup ---------------------------------------------------------------

    pub fn entry_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.window.wl_surface().is_some_and(|s| *s == *surface))
    }

    pub fn entry_for_id(&self, id: &str) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.id.as_deref() == Some(id))
    }

    // --- output -----------------------------------------------------------------

    /// Output size or layer exclusive zones changed: recompute the work area
    /// and re-apply the layout.
    pub fn update_work_area(&mut self) {
        let Some(output) = self.output.clone() else {
            return;
        };
        let Some(geo) = self.space.output_geometry(&output) else {
            return;
        };
        let zone = {
            let mut map = layer_map_for_output(&output);
            map.arrange();
            map.non_exclusive_zone()
        };
        let out = Rect::new(geo.loc.x, geo.loc.y, geo.size.w, geo.size.h);
        let area = Rect::new(
            geo.loc.x + zone.loc.x,
            geo.loc.y + zone.loc.y,
            zone.size.w,
            zone.size.h,
        );
        if out != self.wm.output() || area != self.wm.work_area() {
            info!(?out, ?area, "work area");
            self.wm.set_output(out, area);
            self.apply_layout(true);
        }
    }

    // --- windows ----------------------------------------------------------------

    /// A client committed its first buffer: hand the window to the manager.
    pub fn manage_window(&mut self, index: usize) {
        let (title, app_id) = window_metadata(&self.windows[index]);
        let insets = self.windows[index].insets(&self.wm);
        let geo = self.windows[index].window.geometry();
        let outer = insets.outer(Rect::new(0, 0, geo.size.w.max(1), geo.size.h.max(1)));
        let pinned = match (self.windows[index].toplevel(), self.windows[index].x11()) {
            (Some(t), _) => t.parent().is_some(),
            (None, Some(x)) => x.is_transient_for().is_some() || x.is_popup(),
            _ => false,
        };
        let id = self.wm.add_window(
            app_id.clone(),
            title.clone().unwrap_or_default(),
            (outer.w, outer.h),
            pinned,
        );
        info!(id = %id, title = ?title, app_id = ?app_id, ssd = self.windows[index].ssd, "toplevel mapped");
        let handle = self.foreign_toplevel_list_state.new_toplevel::<Self>(
            title.clone().unwrap_or_default(),
            app_id.clone().unwrap_or_default(),
        );
        handle.send_done();
        let entry = &mut self.windows[index];
        entry.id = Some(id.clone());
        entry.ext_handle = Some(handle);
        self.ipc_new.push(id.clone());
        self.wlr_foreign_toplevel_new(&id);
        self.apply_layout(true);
    }

    /// The toplevel role was destroyed: animate it out and forget it.
    pub fn unmanage_window(&mut self, surface: &ToplevelSurface) {
        let Some(index) = self.entry_for_surface(surface.wl_surface()) else {
            return;
        };
        let mut entry = self.windows.remove(index);
        self.space.unmap_elem(&entry.window);
        if let Some(handle) = &entry.ext_handle {
            self.foreign_toplevel_list_state.remove_toplevel(handle);
        }
        if let Some(id) = entry.id.as_deref() {
            info!(id = %id, "toplevel unmapped");
            self.wlr_foreign_toplevel_closed(id);
            if let (Some(rest), true) = (entry.rest, entry.shown) {
                let surfaces = self.capture_surfaces(&entry.window);
                let now = self.now();
                let anim = WindowAnim::close(rest, now, &self.motion);
                if !surfaces.is_empty() && !anim.is_done(now) {
                    self.ghosts.push(Ghost {
                        rest,
                        insets: entry.insets(&self.wm),
                        surfaces,
                        title_buffer: entry.title_buffer.clone(),
                        parts: std::mem::take(&mut entry.parts),
                        anim,
                    });
                }
            }
            self.wm.remove_window(id);
        }
        self.apply_layout(true);
    }

    fn capture_surfaces(&mut self, window: &Window) -> Vec<GhostSurface> {
        match self.backend.as_mut() {
            Some(backend) => backend.capture_window(window),
            None => Vec::new(),
        }
    }

    /// Mirror the window manager onto surfaces. `animate` = play transitions.
    pub fn apply_layout(&mut self, animate: bool) {
        let ws_now = self.wm.active_workspace();
        let ws_dir: i32 = match ws_now.cmp(&self.last_workspace) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
        };
        self.last_workspace = ws_now;
        let mut morph_wanted: Vec<(usize, Rect)> = Vec::new();
        let now = self.now();
        let targets = self.wm.targets();
        let focused = self.wm.focused().map(str::to_owned);
        let motion = self.motion;
        let output_rect = self.wm.output();

        for i in 0..self.windows.len() {
            let Some(id) = self.windows[i].id.clone() else {
                continue;
            };
            let target = targets.iter().find(|(t, _)| *t == id).map(|(_, r)| *r);
            let insets = self.windows[i].insets(&self.wm);
            let managed = self.wm.get(&id).cloned();
            let minimized = managed.as_ref().is_some_and(|w| w.minimized);
            let tiled = self.wm.is_tiled(&id);
            let taskbar_rect = self
                .minimize_rects
                .get(&id)
                .copied()
                .or_else(|| self.wlr_foreign_toplevel.rectangle(&id));
            let entry = &mut self.windows[i];
            match target {
                Some(rect) => {
                    let client = insets.client(rect);
                    let maximized = managed.as_ref().is_some_and(|w| w.maximized);
                    let fullscreen = managed.as_ref().is_some_and(|w| w.fullscreen);
                    let activated = focused.as_deref() == Some(id.as_str());
                    if let Some(toplevel) = entry.toplevel() {
                        toplevel.with_pending_state(|s| {
                            s.size = Some((client.w, client.h).into());
                            set_state(s, xdg_toplevel::State::Maximized, maximized);
                            set_state(s, xdg_toplevel::State::Fullscreen, fullscreen);
                            set_state(s, xdg_toplevel::State::Activated, activated);
                            for t in [
                                xdg_toplevel::State::TiledLeft,
                                xdg_toplevel::State::TiledRight,
                                xdg_toplevel::State::TiledTop,
                                xdg_toplevel::State::TiledBottom,
                            ] {
                                set_state(s, t, tiled || maximized);
                            }
                        });
                        toplevel.send_pending_configure();
                    } else if let Some(x) = entry.x11() {
                        // X11: the WM places the window (global logical coords).
                        let _ = x.configure(smithay::utils::Rectangle::new(
                            (client.x, client.y).into(),
                            (client.w, client.h).into(),
                        ));
                        let _ = x.set_maximized(maximized);
                        let _ = x.set_fullscreen(fullscreen);
                        let _ = x.set_activated(activated);
                    }
                    entry.window.set_activated(activated);

                    if animate {
                        let running = entry.anim.filter(|a| !a.is_done(now));
                        if entry.rest.is_none() {
                            entry.anim = Some(WindowAnim::open(rect, now, &motion));
                        } else if let Some(a) = running.filter(|a| a.kind == AnimKind::Minimize) {
                            // Interrupted minimise: fly back from where it is.
                            entry.anim = Some(a.reversed_to_restore(rect, now, &motion));
                        } else if !entry.shown && ws_dir != 0 {
                            // Workspace switch: slide in from the side.
                            let from = RectF::from(rect).shifted(f64::from(ws_dir * output_rect.w), 0.0);
                            entry.anim = Some(WindowAnim::slide_in(from, rect, now, &motion));
                        } else if !entry.shown {
                            let from = taskbar_rect
                                .unwrap_or_else(|| default_minimize_target(rect, output_rect));
                            info!(id = %id, from = ?from, to = ?rect, button = taskbar_rect.is_some(), "restore animation");
                            entry.anim = Some(WindowAnim::restore(rect, from, now, &motion));
                        } else if entry.rest != Some(rect) {
                            let from = match running {
                                Some(a) => a.sample(now).rect,
                                None => RectF::from(entry.rest.unwrap_or(rect)),
                            };
                            let old = entry.rest.unwrap_or(rect);
                            if (old.w, old.h) != (rect.w, rect.h) && running.is_none() {
                                // Size changes: morph a frozen snapshot.
                                morph_wanted.push((i, old));
                            }
                            entry.anim = Some(WindowAnim::moving(from, rect, now, &motion));
                        }
                    }
                    entry.rest = Some(rect);
                    entry.shown = true;
                    let geo_loc = entry.window.geometry().loc;
                    let loc = Point::from((client.x - geo_loc.x, client.y - geo_loc.y));
                    self.space.map_element(entry.window.clone(), loc, activated);
                }
                None => {
                    if entry.shown {
                        self.space.unmap_elem(&entry.window);
                        let still_minimizing = entry
                            .anim
                            .is_some_and(|a| {
                                matches!(a.kind, AnimKind::Minimize | AnimKind::SlideOut) && !a.is_done(now)
                            });
                        if !minimized && animate && ws_dir != 0 {
                            // Workspace switch: slide out to the other side.
                            if let Some(rest) = entry.rest {
                                let to = RectF::from(rest).shifted(f64::from(-ws_dir * output_rect.w), 0.0);
                                entry.anim = Some(WindowAnim::slide_out(rest, to, now, &motion));
                                continue;
                            }
                        }
                        if minimized && animate && !still_minimizing {
                            if let Some(rest) = entry.rest {
                                let to = taskbar_rect
                                    .unwrap_or_else(|| default_minimize_target(rest, output_rect));
                                let from = match entry.anim.filter(|a| !a.is_done(now)) {
                                    Some(a) => {
                                        let r = a.sample(now).rect;
                                        Rect::new(r.x as i32, r.y as i32, r.w as i32, r.h as i32)
                                    }
                                    None => rest,
                                };
                                info!(id = %id, from = ?from, to = ?to, button = taskbar_rect.is_some(), "minimise animation");
                                entry.anim = Some(WindowAnim::minimize(from, to, now, &motion));
                                // Stay shown until the animation ends.
                                continue;
                            }
                        }
                        if !still_minimizing {
                            entry.shown = false;
                            entry.anim = None;
                        }
                    }
                }
            }
        }

        // Frozen snapshots for size-changing moves (maximise, snap, tiling,
        // fullscreen): the old content scales into the new rect instead of
        // jumping while the client re-renders at its new size.
        for (i, old) in morph_wanted {
            let window = self.windows[i].window.clone();
            let surfaces = self.capture_surfaces(&window);
            if !surfaces.is_empty() {
                let e = &mut self.windows[i];
                e.morph = Some((old, surfaces));
            }
        }

        // Stacking order: raise in WM order (bottom first) and keep
        // `self.windows` in the same order for rendering and hit-testing.
        let order: Vec<String> = self.wm.windows().iter().map(|w| w.id.clone()).collect();
        self.windows.sort_by_key(|e| {
            e.id.as_deref()
                .and_then(|id| order.iter().position(|o| o == id))
                .unwrap_or(usize::MAX)
        });
        let mapped: Vec<Window> = self.space.elements().cloned().collect();
        for entry in &self.windows {
            if mapped.contains(&entry.window) {
                self.space.raise_element(&entry.window, false);
            }
        }

        // Keyboard focus follows the manager (unless a layer surface holds it,
        // or the session is locked).
        let keyboard = self.seat.get_keyboard().expect("keyboard");
        let layer_focus = keyboard
            .current_focus()
            .is_some_and(|s| self.is_layer_surface(&s));
        if !layer_focus && !self.locked {
            let surface = focused
                .as_deref()
                .and_then(|id| self.entry_for_id(id))
                .and_then(|i| self.windows[i].wl_surface());
            if keyboard.current_focus() != surface {
                keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
            }
        }

        self.refresh_titles();
        self.refresh_switcher();
        self.wlr_foreign_toplevel_refresh_all();
        self.needs_redraw = true;
        self.ipc_dirty = true;
    }

    /// Show, update or hide the Alt+Tab card to match the window manager.
    pub fn refresh_switcher(&mut self) {
        if !self.wm.is_cycling() {
            if self.switcher.take().is_some() {
                self.needs_redraw = true;
            }
            return;
        }
        let (order, candidate) = self.wm.cycle_list();
        let rows: Vec<crate::frame::SwitcherRow> = order
            .iter()
            .filter_map(|id| self.wm.get(id))
            .map(|w| crate::frame::SwitcherRow {
                title: if w.title.is_empty() {
                    w.app_id.clone().unwrap_or_default()
                } else {
                    w.title.clone()
                },
                app: w.app_id.clone().unwrap_or_default(),
                minimized: w.minimized,
            })
            .collect();
        let selected = candidate
            .and_then(|c| order.iter().position(|o| *o == c))
            .unwrap_or(0);
        if self
            .switcher
            .as_ref()
            .is_some_and(|(r, s, _, _)| *r == rows && *s == selected)
        {
            return;
        }
        let canvas =
            crate::frame::paint_switcher(&rows, selected, &self.palette, self.title_font.as_ref());
        let buf = MemoryRenderBuffer::from_slice(
            &canvas.data,
            smithay::backend::allocator::Fourcc::Argb8888,
            (canvas.width, canvas.height),
            1,
            smithay::utils::Transform::Normal,
            None,
        );
        if self.switcher.is_none() {
            self.switcher_since = self.now();
        }
        self.switcher = Some((rows, selected, buf, (canvas.width, canvas.height)));
        self.needs_redraw = true;
    }

    /// Finish animations that ended: minimised windows stop being drawn.
    pub fn retire_animations(&mut self) {
        let now = self.now();
        for entry in &mut self.windows {
            if let Some(a) = entry.anim {
                if a.is_done(now) {
                    if matches!(a.kind, AnimKind::Minimize | AnimKind::SlideOut) {
                        entry.shown = false;
                    }
                    entry.morph = None;
                    entry.anim = None;
                    self.needs_redraw = true;
                }
            }
        }
        let before = self.ghosts.len();
        self.ghosts.retain(|g| !g.anim.is_done(now));
        if self.ghosts.len() != before {
            self.needs_redraw = true;
        }
    }

    /// Repaint title bars whose state changed (title, focus, hover, width).
    pub fn refresh_titles(&mut self) {
        let focused = self.wm.focused().map(str::to_owned);
        for entry in &mut self.windows {
            if !entry.ssd {
                continue;
            }
            let (Some(id), Some(rest)) = (entry.id.clone(), entry.rest) else {
                continue;
            };
            let managed = self.wm.get(&id);
            let state = TitleState {
                width: rest.w,
                title: managed.map(|w| w.title.clone()).unwrap_or_default(),
                focused: focused.as_deref() == Some(id.as_str()),
                hovered: entry.hovered,
                pressed: entry.pressed,
                maximized: managed.is_some_and(|w| w.maximized),
            };
            if entry.title_state.as_ref() != Some(&state) {
                // Focus changes crossfade the old title bar into the new.
                let focus_changed = entry
                    .title_state
                    .as_ref()
                    .is_some_and(|old| old.focused != state.focused && old.width == state.width);
                if focus_changed {
                    if let Some(old) = entry.title_buffer.clone() {
                        entry.prev_title = Some((old, self.start_time.elapsed()));
                    }
                }
                let canvas =
                    crate::frame::paint_title_bar(&state, &self.palette, self.title_font.as_ref());
                // Opaque below the rounded corners: lets the damage tracker
                // skip what is underneath the title bar.
                let opaque = smithay::utils::Rectangle::new(
                    (0, 5).into(),
                    (canvas.width, (canvas.height - 5).max(0)).into(),
                );
                entry.title_buffer = Some(MemoryRenderBuffer::from_slice(
                    &canvas.data,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (canvas.width, canvas.height),
                    1,
                    smithay::utils::Transform::Normal,
                    Some(vec![opaque]),
                ));
                entry.title_state = Some(state);
                self.needs_redraw = true;
            }
        }
    }

    /// Client changed title or app_id.
    pub fn refresh_metadata(&mut self, surface: &ToplevelSurface) {
        let Some(index) = self.entry_for_surface(surface.wl_surface()) else {
            return;
        };
        let Some(id) = self.windows[index].id.clone() else {
            return;
        };
        let (title, app_id) = toplevel_metadata(surface);
        let title = title.unwrap_or_default();
        self.wm.set_title(&id, title.clone());
        self.wm.set_app_id(&id, app_id.clone());
        if let Some(handle) = &self.windows[index].ext_handle {
            handle.send_title(&title);
            handle.send_app_id(app_id.as_deref().unwrap_or_default());
            handle.send_done();
        }
        self.wlr_foreign_toplevel_refresh(&id);
        self.refresh_titles();
        self.ipc_dirty = true;
    }

    // --- actions used by input, the control socket and foreign-toplevel -------

    pub fn activate_window(&mut self, id: &str) -> bool {
        if !self.wm.focus(id) {
            return false;
        }
        self.apply_layout(true);
        true
    }

    pub fn minimize_window(&mut self, id: &str) -> bool {
        if !self.wm.minimize(id) {
            return false;
        }
        info!(id = %id, "minimised");
        self.apply_layout(true);
        true
    }

    pub fn close_window(&mut self, id: &str) -> bool {
        match self.entry_for_id(id) {
            Some(i) => {
                if let Some(t) = self.windows[i].toplevel() {
                    t.send_close();
                } else if let Some(x) = self.windows[i].x11() {
                    let _ = x.close();
                }
                true
            }
            None => false,
        }
    }

    pub fn set_maximized(&mut self, id: &str, maximized: bool) {
        if self.wm.set_maximized(id, maximized) {
            self.apply_layout(true);
        }
    }

    pub fn set_fullscreen(&mut self, id: &str, fullscreen: bool) {
        if self.wm.set_fullscreen(id, fullscreen) {
            self.apply_layout(true);
        }
    }

    pub fn set_layout(&mut self, scope: LayoutScope, request: ModeRequest) {
        let changed = self.wm.set_layout(scope, request);
        info!(
            ?scope,
            ?request,
            ?changed,
            active_mode = self.wm.active_mode().as_str(),
            "layout"
        );
        self.apply_layout(true);
        for (id, r) in self.wm.targets() {
            info!(id = %id, x = r.x, y = r.y, w = r.w, h = r.h, tiled = self.wm.is_tiled(&id), "layout target");
        }
    }

    pub fn apply_action(&mut self, action: glaze_wm::WmAction) {
        match self.wm.apply(action) {
            crate::wm::Applied::Relayout => {
                info!(?action, "wm action");
                self.apply_layout(true);
            }
            crate::wm::Applied::Close(id) => {
                self.close_window(&id);
            }
            crate::wm::Applied::Launcher => {
                info!("launcher requested (the shell owns the launcher)");
            }
            crate::wm::Applied::Ignored(reason) => debug!(?action, reason, "action ignored"),
        }
    }

    pub fn is_layer_surface(&self, surface: &WlSurface) -> bool {
        self.outputs.iter().any(|o| {
            layer_map_for_output(o)
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .is_some()
        })
    }

    /// Spawn a client inside this compositor.
    pub fn spawn(&self, command: &str) {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd.env("WAYLAND_DISPLAY", &self.socket_name);
        match self.xdisplay {
            Some(n) => {
                cmd.env("DISPLAY", format!(":{n}"));
            }
            None => {
                cmd.env_remove("DISPLAY");
            }
        }
        cmd.env_remove("SWAYSOCK");
        cmd.env("XDG_SESSION_TYPE", "wayland");
        cmd.env("XDG_CURRENT_DESKTOP", "Glaze");
        if let Some(path) = &self.control_socket {
            cmd.env(crate::control::SOCKET_ENV, path);
        }
        match cmd.spawn() {
            Ok(child) => info!(command, pid = child.id(), "spawned client"),
            Err(e) => warn!(command, %e, "failed to spawn client"),
        }
    }
}

fn set_state(
    s: &mut smithay::wayland::shell::xdg::ToplevelState,
    state: xdg_toplevel::State,
    on: bool,
) {
    if on {
        s.states.set(state);
    } else {
        s.states.unset(state);
    }
}

/// `[motion] intensity = "off" | "low" | "normal" | "high"` (raw TOML read,
/// so the shared config schema does not need to know the key yet).
fn read_intensity(path: &std::path::Path) -> crate::anim::Intensity {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| {
            t.get("motion")?
                .get("intensity")?
                .as_str()
                .and_then(crate::anim::Intensity::parse)
        })
        .unwrap_or_default()
}

/// Effective scheme from `[appearance]`: `scheme_mode = "auto"` is light
/// from 07:00 to 19:00 local time (the shell's rule), else `scheme`.
fn config_is_dark(c: &glaze_common::config::GlazeConfig) -> bool {
    use glaze_common::config::ColorScheme;
    let hour = local_hour().clamp(0, 23) as u32;
    matches!(
        c.appearance.scheme_mode.resolve(c.appearance.scheme, hour),
        ColorScheme::Dark
    )
}

fn local_hour() -> i32 {
    // SAFETY: localtime_r writes into the provided struct only.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_hour
    }
}

/// Without a taskbar rectangle, minimise towards the bottom centre.
pub fn default_minimize_target(rest: Rect, output: Rect) -> Rect {
    let w = (rest.w / 5).max(64);
    let h = (rest.h / 5).max(24);
    Rect::new(rest.x + (rest.w - w) / 2, output.bottom() - h, w, h)
}

/// Title and app id of any window (X11: title and WM_CLASS class).
pub fn window_metadata(entry: &WindowEntry) -> (Option<String>, Option<String>) {
    if let Some(t) = entry.toplevel() {
        return toplevel_metadata(t);
    }
    match entry.x11() {
        Some(x) => {
            let class = x.class();
            (
                Some(x.title()).filter(|t| !t.is_empty()),
                Some(if class.is_empty() { x.instance() } else { class }).filter(|c| !c.is_empty()),
            )
        }
        None => (None, None),
    }
}

/// Title and app_id the client reported.
pub fn toplevel_metadata(surface: &ToplevelSurface) -> (Option<String>, Option<String>) {
    with_states(surface.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .map(|data| {
                let data = data.lock().unwrap();
                (data.title.clone(), data.app_id.clone())
            })
            .unwrap_or((None, None))
    })
}

// Delegates.
smithay::delegate_compositor!(State);
smithay::delegate_shm!(State);
smithay::delegate_output!(State);
smithay::delegate_seat!(State);
smithay::delegate_data_device!(State);
smithay::delegate_primary_selection!(State);
smithay::delegate_xdg_shell!(State);
smithay::delegate_xdg_decoration!(State);
smithay::delegate_layer_shell!(State);
smithay::delegate_viewporter!(State);
smithay::delegate_fractional_scale!(State);
smithay::delegate_foreign_toplevel_list!(State);
smithay::delegate_cursor_shape!(State);

impl smithay::wayland::tablet_manager::TabletSeatHandler for State {}
