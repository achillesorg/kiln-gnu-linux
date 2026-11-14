//! Desktop shell orchestration: config + tokens + CSS (with live reload),
//! one taskbar per output, the compositor event stream, and the status feed.
//!
//! - Config is loaded with `glaze_common::config_check` (validation issues are
//!   logged with key paths). The config directory and theme directories are
//!   watched with GIO file monitors (inotify); a change reloads the config and
//!   theme, re-emits the CSS and rebuilds the taskbars. A config that does not
//!   parse is reported and the running one is kept.
//! - Window state comes from the backend's event stream (Sway `SUBSCRIBE`
//!   window/workspace events, polled via its fd on the GLib main loop), not
//!   from a timer. If the stream is unavailable, a 750 ms poll is the
//!   fallback, and the log says so.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use glaze_backend::{Backend, BackendEvent};
use glaze_common::config::{Autohide, GlazeConfig, MotionStyle, PanelPosition};
use glaze_common::config_check::{self, Loaded};
use glaze_common::model::WindowInfo;
use glaze_common::paths;
use glaze_common::theme::{CssOptions, ThemeTokens};
use glaze_design::motion::MotionSettings;
use glaze_design::texture;
use glaze_service::registry::AppRegistry;
use gtk4::prelude::*;
use gtk4::{gdk, gdk_pixbuf, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::anim::Animator;
use crate::panel_ui::{self, Ctx, Panel};

pub struct Shell {
    cfg: RefCell<Rc<GlazeConfig>>,
    tokens: RefCell<Rc<ThemeTokens>>,
    provider: gtk4::CssProvider,
    registry: Rc<AppRegistry>,
    backend: Option<Rc<dyn Backend>>,
    panels: RefCell<Vec<Panel>>,
    controls: RefCell<Option<Rc<crate::wincontrols::WinControls>>>,
    /// Minimise/restore zoom animation (Sway session only).
    ghost: RefCell<Option<Rc<crate::ghost::Ghost>>>,
    windows: RefCell<Option<Vec<WindowInfo>>>,
    refresh_pending: Cell<bool>,
    new_windows: RefCell<Vec<String>>,
    windowed: bool,
    reload_timer: RefCell<Option<glib::SourceId>>,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    last_status: RefCell<Option<glaze_shell_core::status_feed::StatusSnapshot>>,
    /// Desktop wallpaper stack, the shown path and the output orientation.
    desktop: RefCell<Option<DesktopWall>>,
    /// Scheme last handed to `glaze-colour-scheme`.
    system_scheme: Cell<Option<glaze_design::kinds::ColorScheme>>,
}

struct DesktopWall {
    stack: gtk4::Stack,
    shown: Option<PathBuf>,
    orientation: glaze_common::wallpaper::Orientation,
}

/// Resolve `appearance.scheme_mode = "auto"` into a concrete scheme for now.
pub fn with_effective_scheme(mut cfg: GlazeConfig) -> GlazeConfig {
    let hour = glib::DateTime::now_local()
        .map(|d| d.hour() as u32)
        .unwrap_or(12);
    cfg.appearance.scheme = cfg
        .appearance
        .scheme_mode
        .resolve(cfg.appearance.scheme, hour);
    cfg
}

/// Load and log the config.
pub fn load_config() -> Loaded {
    let path = paths::config_file();
    let loaded = config_check::load_checked(&path);
    eprintln!(
        "[glaze-shell] config: {} ({} issue(s){})",
        path.display(),
        loaded.issues.len(),
        if loaded.usable { "" } else { ", NOT usable" }
    );
    for i in &loaded.issues {
        eprintln!("[glaze-shell] config {i}");
    }
    loaded
}

/// Resolve tokens (theme file + overrides) and log issues.
pub fn load_tokens(cfg: &GlazeConfig) -> ThemeTokens {
    let (tokens, issues, file) = config_check::load_tokens(cfg);
    eprintln!(
        "[glaze-shell] tokens: {} (theme file: {}, {} issue(s))",
        tokens.label(),
        file.map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".into()),
        issues.len()
    );
    for i in &issues {
        eprintln!("[glaze-shell] theme {i}");
    }
    tokens
}

/// Motion settings from config and the toolkit's animation setting.
pub fn motion_settings(cfg: &GlazeConfig) -> MotionSettings {
    let gtk_anim = gtk4::Settings::default()
        .map(|s| s.is_gtk_enable_animations())
        .unwrap_or(true);
    let reduced = cfg.appearance.reduced_motion || !gtk_anim;
    if cfg.motion.style == MotionStyle::StopMotion {
        eprintln!("[glaze-shell] motion.style = stop-motion is not implemented yet; using smooth");
    }
    MotionSettings::new(
        cfg.motion.speed,
        cfg.motion.style == MotionStyle::Snappy,
        cfg.motion.style == MotionStyle::Off,
        reduced,
    )
}

fn texture_dirs() -> Vec<PathBuf> {
    let mut v = vec![paths::data_home().join("glaze/assets/textures")];
    for d in paths::data_dirs() {
        v.push(d.join("glaze/assets/textures"));
    }
    if let Ok(exe) = std::env::current_exe() {
        for up in exe.ancestors().skip(1).take(6) {
            v.push(up.join("assets/textures"));
        }
    }
    v
}

/// Bake the configured texture at the configured intensity into the cache
/// (alpha scaled per pixel) and return its `file://` URL.
fn baked_texture_url(cfg: &GlazeConfig, tokens: &ThemeTokens) -> Option<String> {
    baked_texture(cfg, tokens, 1.0).map(|(u, _)| u)
}

/// The taskbar's tile: the same paper at 45 % of the surface intensity and
/// drawn at half size (finer grain), so the bar reads calm all day.
fn baked_panel_texture(cfg: &GlazeConfig, tokens: &ThemeTokens) -> Option<(String, u32)> {
    baked_texture(cfg, tokens, 0.45)
}

/// Bake the configured texture at `intensity * factor`; returns its URL
/// and tile width.
fn baked_texture(cfg: &GlazeConfig, tokens: &ThemeTokens, factor: f64) -> Option<(String, u32)> {
    if cfg.appearance.reduced_effects {
        return None;
    }
    let set = cfg.appearance.texture.as_str();
    let intensity = (cfg.appearance.texture_intensity * factor).clamp(0.0, 1.0);
    let tile = texture::tile_file(set, tokens.scheme)?;
    let baked = texture::baked_file(set, tokens.scheme, intensity)?;
    let out = paths::cache_home().join("glaze/textures").join(&baked);
    if out.is_file() {
        let w = gdk_pixbuf::Pixbuf::file_info(&out)
            .map(|(_, w, _)| w as u32)
            .unwrap_or(256);
        return Some((format!("file://{}", out.display()), w));
    }
    let src = texture_dirs()
        .into_iter()
        .map(|d| d.join(&tile))
        .find(|p| p.is_file());
    let Some(src) = src else {
        eprintln!("[glaze-shell] texture {tile} not installed; plain fills");
        return None;
    };
    let pb = match gdk_pixbuf::Pixbuf::from_file(&src) {
        Ok(pb) => pb,
        Err(e) => {
            eprintln!("[glaze-shell] texture {}: {e}", src.display());
            return None;
        }
    };
    if !pb.has_alpha() || pb.n_channels() != 4 {
        return None;
    }
    let mut bytes = pb.read_pixel_bytes().to_vec();
    for px in bytes.chunks_mut(4) {
        px[3] = texture::scale_alpha(px[3], intensity);
    }
    let scaled = gdk_pixbuf::Pixbuf::from_bytes(
        &glib::Bytes::from_owned(bytes),
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        pb.width(),
        pb.height(),
        pb.rowstride(),
    );
    if let Some(dir) = out.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match scaled.savev(&out, "png", &[]) {
        Ok(()) => {
            eprintln!(
                "[glaze-shell] texture: baked {} at {:.0}%",
                baked,
                intensity * 100.0
            );
            Some((format!("file://{}", out.display()), pb.width() as u32))
        }
        Err(e) => {
            eprintln!("[glaze-shell] texture bake failed: {e}");
            None
        }
    }
}

impl Shell {
    pub fn new(
        loaded: Loaded,
        registry: Rc<AppRegistry>,
        backend: Option<Rc<dyn Backend>>,
        initial_windows: Option<Vec<WindowInfo>>,
        windowed: bool,
    ) -> Rc<Self> {
        let cfg = with_effective_scheme(loaded.config);
        let tokens = load_tokens(&cfg);
        let provider = gtk4::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        let shell = Rc::new(Shell {
            cfg: RefCell::new(Rc::new(cfg)),
            tokens: RefCell::new(Rc::new(tokens)),
            provider,
            registry,
            backend,
            panels: RefCell::new(Vec::new()),
            controls: RefCell::new(None),
            ghost: RefCell::new(None),
            windows: RefCell::new(initial_windows),
            refresh_pending: Cell::new(false),
            new_windows: RefCell::new(Vec::new()),
            windowed,
            reload_timer: RefCell::new(None),
            monitors: RefCell::new(Vec::new()),
            last_status: RefCell::new(None),
            desktop: RefCell::new(None),
            system_scheme: Cell::new(None),
        });
        shell.install_css();
        shell
    }

    pub fn cfg(&self) -> Rc<GlazeConfig> {
        self.cfg.borrow().clone()
    }

    pub fn tokens(&self) -> Rc<ThemeTokens> {
        self.tokens.borrow().clone()
    }

    pub fn windows(&self) -> Option<Vec<WindowInfo>> {
        self.windows.borrow().clone()
    }

    fn install_css(&self) {
        let cfg = self.cfg();
        let tokens = self.tokens();
        let motion = motion_settings(&cfg);
        let css = tokens.to_gtk_css_full(CssOptions {
            reduced_motion: motion.instant(),
            focus_rings: true,
            duration_scale: if motion.instant() {
                0.0
            } else {
                motion.duration_scale * if motion.snappy { 0.65 } else { 1.0 }
            },
            reduced_effects: cfg.appearance.reduced_effects,
            texture_url: baked_texture_url(&cfg, &tokens),
        }, baked_panel_texture(&cfg, &tokens));
        self.provider.load_from_string(&css);
        eprintln!(
            "[glaze-shell] installed {} bytes of theme CSS ({})",
            css.len(),
            tokens.label()
        );
    }

    fn ctx(self: &Rc<Self>) -> Ctx {
        let cfg = self.cfg();
        let me = Rc::downgrade(self);
        let me2 = Rc::downgrade(self);
        Ctx {
            motion: motion_settings(&cfg),
            cfg,
            tokens: self.tokens(),
            registry: self.registry.clone(),
            backend: self.backend.clone(),
            set_pins: Rc::new(move |pins| {
                if let Some(s) = me.upgrade() {
                    s.save_pins(pins);
                }
            }),
            on_mark_all_read: Rc::new(mark_all_read),
            refresh: Rc::new(move || {
                if let Some(s) = me2.upgrade() {
                    s.schedule_refresh();
                }
            }),
        }
    }

    /// Build one taskbar per output (honouring per-monitor overrides).
    pub fn build_panels(self: &Rc<Self>) {
        for p in self.panels.borrow_mut().drain(..) {
            p.destroy();
        }
        let ctx = self.ctx();
        crate::menu::set_context(&ctx);
        crate::toast::set_context(
            &ctx,
            ctx.cfg.panel.position,
            panel_ui::panel_thickness(&ctx),
        );
        if let Some(c) = self.controls.borrow_mut().take() {
            c.destroy();
        }
        if !self.windowed {
            *self.controls.borrow_mut() = crate::wincontrols::WinControls::new(&ctx);
            if let Some(g) = self.ghost.borrow_mut().take() {
                g.destroy();
            }
            // The Glaze compositor animates minimise itself.
            if self.backend.as_ref().map(|b| b.name()) == Some("sway-ipc") {
                *self.ghost.borrow_mut() = crate::ghost::Ghost::new(&ctx);
            }
        }
        self.apply_frame_colors();
        self.apply_system_scheme();
        let monitors: Vec<(Option<gdk::Monitor>, String)> = match gdk::Display::default() {
            Some(d) => {
                let list = d.monitors();
                let mut v = Vec::new();
                for i in 0..list.n_items() {
                    if let Some(m) = list.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
                        let name = m.connector().map(|s| s.to_string()).unwrap_or_default();
                        v.push((Some(m), name));
                    }
                }
                if v.is_empty() {
                    v.push((None, String::new()));
                }
                v
            }
            None => vec![(None, String::new())],
        };
        let mut panels = Vec::new();
        for (monitor, connector) in monitors {
            if !ctx.cfg.panel.enabled_on(&connector) {
                eprintln!("[glaze-shell] taskbar disabled on {connector} (panel.monitors)");
                continue;
            }
            let pcfg = ctx.cfg.panel.for_monitor(&connector);
            let (content, parts) = panel_ui::build(&ctx, &pcfg);
            let window = self.create_panel_window(&ctx, &pcfg, monitor.as_ref(), &content);
            let panel = parts.into_panel(window, connector.clone(), pcfg.position);
            if let Some(ws) = self.windows.borrow().as_ref() {
                panel.update_windows(ws, self.registry.apps());
            }
            panel.update_layout(self.backend.as_ref().and_then(|b| b.layout_mode().ok()));
            if let Some(s) = self.last_status.borrow().as_ref() {
                panel.apply_status(s);
            }
            panel.window.present();
            eprintln!(
                "[glaze-shell] taskbar on {}: edge={} height={}px labels={:?} grouping={}",
                if connector.is_empty() {
                    "?"
                } else {
                    &connector
                },
                pcfg.position.slug(),
                panel_ui::panel_thickness(&ctx),
                pcfg.label_mode(),
                panel_ui::grouping_slug(pcfg.grouping),
            );
            panels.push(panel);
        }
        *self.panels.borrow_mut() = panels;
    }

    fn create_panel_window(
        &self,
        ctx: &Ctx,
        pcfg: &glaze_common::config::PanelConfig,
        monitor: Option<&gdk::Monitor>,
        content: &gtk4::Widget,
    ) -> gtk4::Window {
        let window = gtk4::Window::new();
        window.set_title(Some("Glaze Panel"));
        window.add_css_class("glaze-panel-window");
        window.set_child(Some(content));
        let thickness = panel_ui::panel_thickness(ctx);
        if !self.windowed && gtk4_layer_shell::is_supported() {
            window.init_layer_shell();
            window.set_namespace(Some("glaze-panel"));
            window.set_layer(Layer::Top);
            if let Some(m) = monitor {
                window.set_monitor(Some(m));
            }
            for edge in anchor_edges(pcfg.position) {
                window.set_anchor(*edge, true);
            }
            window.set_keyboard_mode(KeyboardMode::OnDemand);
            window.set_decorated(false);
            if pcfg.autohide == Autohide::Never {
                window.set_exclusive_zone(thickness);
            } else {
                setup_autohide(&window, ctx, pcfg.position, thickness);
            }
        } else {
            window.set_decorated(true);
            window.set_default_size(1100, thickness);
            eprintln!("[glaze-shell] panel: windowed (no layer-shell)");
        }
        window
    }

    /// Persist an explicit pin list into the user config (the file monitor
    /// then reloads everything).
    fn save_pins(&self, pins: Vec<String>) {
        let path = paths::config_file();
        let loaded = config_check::load_checked(&path);
        if !loaded.usable {
            eprintln!("[glaze-shell] not saving pins: the config file does not parse");
            return;
        }
        let mut cfg = loaded.config;
        // An explicit list (even empty) now replaces the default pins.
        cfg.panel.default_pins = false;
        cfg.launcher.pinned = pins.clone();
        match cfg.save_to(&path) {
            Ok(()) => eprintln!("[glaze-shell] pins saved: {pins:?}"),
            Err(e) => eprintln!("[glaze-shell] could not save pins: {e}"),
        }
    }

    /// Coalesce refresh requests into one tree read per main-loop turn.
    pub fn schedule_refresh(self: &Rc<Self>) {
        if self.refresh_pending.replace(true) {
            return;
        }
        let me = self.clone();
        glib::idle_add_local_once(move || {
            me.refresh_pending.set(false);
            me.refresh_now();
        });
    }

    fn refresh_now(self: &Rc<Self>) {
        let Some(be) = &self.backend else { return };
        for id in self.new_windows.borrow_mut().drain(..) {
            if let Err(e) = be.window_opened(&id) {
                eprintln!("[glaze-shell] layout for new window {id}: {e}");
            }
        }
        match be.windows() {
            Ok(ws) => {
                let changed = self.windows.borrow().as_ref() != Some(&ws);
                if changed {
                    self.play_minimize_transitions(&ws);
                    eprintln!("[glaze-shell] taskbar: {} live window(s)", ws.len());
                    for p in self.panels.borrow().iter() {
                        p.update_windows(&ws, self.registry.apps());
                    }
                    *self.windows.borrow_mut() = Some(ws);
                }
                let mode = be.layout_mode().ok();
                for p in self.panels.borrow().iter() {
                    p.update_layout(mode);
                }
                self.update_controls();
            }
            Err(e) => eprintln!("[glaze-shell] window refresh failed: {e}"),
        }
    }

    /// Windows that just became minimised (or restored) get the zoom
    /// animation between their frame and their task button. Covers every
    /// path: bar clicks, menus, Super+M, the title-bar strip, glaze-snap.
    fn play_minimize_transitions(&self, new: &[WindowInfo]) {
        let Some(ghost) = self.ghost.borrow().clone() else {
            return;
        };
        let old = self.windows.borrow();
        let Some(old) = old.as_ref() else { return };
        let mut moves: Vec<(String, bool)> = Vec::new();
        for w in new {
            if let Some(o) = old.iter().find(|o| o.id == w.id) {
                if o.minimized != w.minimized {
                    moves.push((w.id.clone(), w.minimized));
                }
            }
        }
        let Some((id, minimized)) = moves.into_iter().next() else {
            return;
        };
        let Some(be) = &self.backend else { return };
        let frame = be
            .frames()
            .ok()
            .and_then(|fs| fs.into_iter().find(|f| f.id == id));
        let Some(f) = frame else { return };
        let win = (f.x as f64, f.y as f64, f.width as f64, f.height as f64);
        let button = self
            .panels
            .borrow()
            .iter()
            .find_map(|p| p.button_screen_rect(&id))
            .unwrap_or((win.0 + win.2 / 2.0 - 40.0, win.1 + win.3, 80.0, 28.0));
        if minimized {
            ghost.play(win, button);
        } else {
            ghost.play(button, win);
        }
    }

    /// Compare the taskbar's window list with the compositor's; apply and
    /// log any drift (events are the primary path, this is the safety net).
    fn resync(self: &Rc<Self>) {
        let Some(be) = &self.backend else { return };
        let Ok(ws) = be.windows() else { return };
        let ids = |v: &[WindowInfo]| {
            let mut i: Vec<String> = v.iter().map(|w| w.id.clone()).collect();
            i.sort();
            i
        };
        let known = self
            .windows
            .borrow()
            .as_deref()
            .map(ids)
            .unwrap_or_default();
        if known != ids(&ws) {
            eprintln!(
                "[glaze-shell] resync: event stream missed a change ({} -> {} windows); fixed",
                known.len(),
                ws.len()
            );
            self.refresh_now();
        }
    }

    /// Keep the title-bar control strip on the focused window.
    fn update_controls(&self) {
        let Some(controls) = self.controls.borrow().clone() else {
            return;
        };
        let Some(be) = &self.backend else { return };
        let focused = self.windows.borrow().as_ref().and_then(|ws| {
            ws.iter()
                .find(|w| w.focused && !w.minimized)
                .map(|w| w.id.clone())
        });
        let frame = match (&focused, be.frames()) {
            (Some(id), Ok(frames)) => frames.into_iter().find(|f| &f.id == id),
            _ => None,
        };
        controls.follow(frame.as_ref());
    }

    /// Floating windows can be dragged without any IPC event, so the
    /// control strip re-reads the focused window's geometry while one is
    /// focused (cheap GET_TREE over the local socket).
    pub fn start_controls_tracking(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        let ticks = Cell::new(0u32);
        glib::timeout_add_local(Duration::from_millis(300), move || {
            let Some(s) = me.upgrade() else {
                return glib::ControlFlow::Break;
            };
            ticks.set(ticks.get().wrapping_add(1));
            // Follow a dragged window only while the title-bar strip is on
            // screen (one GET_TREE per tick); otherwise the bar is idle and
            // only the ~3 s safety resync talks to Sway.
            let strip = s
                .controls
                .borrow()
                .as_ref()
                .map(|c| c.is_shown())
                .unwrap_or(false);
            if strip {
                s.update_controls();
            }
            if ticks.get().is_multiple_of(10) {
                s.resync();
            }
            glib::ControlFlow::Continue
        });
    }

    /// Recolour Sway's title bars and window frames from the current tokens
    /// (same mapping as `glaze-colour-scheme`, which uses the default
    /// tokens): paper title (`surface_raised`), ink text, terracotta title
    /// outline on the focused window, and the 6 px resize band in a faint
    /// terracotta-tinted paper (focused) or the bar colour (others), so the
    /// band reads as part of the frame rather than a thick coloured line.
    fn apply_frame_colors(&self) {
        let Some(be) = &self.backend else { return };
        let p = self.tokens().palette;
        let hex = |c: glaze_common::color::Color| c.to_rgb().to_hex();
        let colors = glaze_backend::FrameColors {
            focused_bg: hex(p.surface_raised),
            focused_text: hex(p.text_primary),
            focused_border: hex(p.accent),
            focused_frame: hex(p.surface_raised.mix(p.accent, 0.22)),
            other_bg: hex(p.panel),
            other_text: hex(p.text_secondary),
            other_border: hex(p.border),
            other_frame: hex(p.panel),
            urgent_bg: hex(p.danger),
        };
        match be.set_frame_colors(&colors) {
            Ok(()) => eprintln!(
                "[glaze-shell] window frames recoloured from {}",
                self.tokens().label()
            ),
            Err(e) => eprintln!("[glaze-shell] window frame colours: {e}"),
        }
    }

    /// Tell GTK 3/4, libadwaita, portal apps and Qt about the scheme via
    /// `glaze-colour-scheme` (session start and every scheme change). The
    /// shell keeps painting Sway frames itself from the live tokens, so the
    /// script is asked not to touch them (`GLAZE_SWAY_FRAMES=skip`).
    fn apply_system_scheme(&self) {
        let scheme = self.tokens().scheme;
        if self.system_scheme.get() == Some(scheme) {
            return;
        }
        self.system_scheme.set(Some(scheme));
        let arg = if scheme == glaze_design::kinds::ColorScheme::Dark {
            "dark"
        } else {
            "light"
        };
        match std::process::Command::new("glaze-colour-scheme")
            .arg(arg)
            .env("GLAZE_SWAY_FRAMES", "skip")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                eprintln!("[glaze-shell] system colour scheme -> {arg} (glaze-colour-scheme)");
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(e) => {
                eprintln!("[glaze-shell] glaze-colour-scheme {arg}: {e} (apps keep their scheme)")
            }
        }
    }

    /// Subscribe to compositor events; fall back to polling.
    pub fn start_window_events(self: &Rc<Self>) {
        let Some(be) = self.backend.clone() else {
            eprintln!("[glaze-shell] taskbar: no backend, so no live window updates");
            return;
        };
        match be.subscribe() {
            Ok(stream) => {
                let fd = stream.raw_fd();
                eprintln!(
                    "[glaze-shell] window updates: {} event stream (fd {fd}), no polling",
                    be.name()
                );
                // glib 0.22 has no fd source, so a reader thread blocks in
                // poll(2) and hands batches to the main loop (`invoke`).
                SHELL.with(|c| *c.borrow_mut() = Rc::downgrade(self));
                let spawned = std::thread::Builder::new()
                    .name("glaze-events".into())
                    .spawn(move || event_thread(stream));
                if let Err(e) = spawned {
                    eprintln!("[glaze-shell] cannot start event thread ({e}); polling");
                    self.start_polling();
                }
            }
            Err(e) => {
                eprintln!("[glaze-shell] no event stream ({e}); polling every 750 ms");
                self.start_polling();
            }
        }
    }

    fn start_polling(self: &Rc<Self>) {
        let me = self.clone();
        glib::timeout_add_local(Duration::from_millis(750), move || {
            me.refresh_now();
            glib::ControlFlow::Continue
        });
    }

    /// Watch the config dir and theme dirs; reload on change (debounced).
    pub fn watch_config(self: &Rc<Self>) {
        let mut dirs = vec![paths::glaze_config_dir()];
        dirs.extend(config_check::theme_dirs());
        for dir in dirs {
            if !dir.is_dir() {
                continue;
            }
            let file = gio::File::for_path(&dir);
            match file.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
                Ok(mon) => {
                    let me = Rc::downgrade(self);
                    mon.connect_changed(move |_, f, _, ev| {
                        let relevant = f.path().map(|p| is_config_like(&p)).unwrap_or(false)
                            && matches!(
                                ev,
                                gio::FileMonitorEvent::ChangesDoneHint
                                    | gio::FileMonitorEvent::Created
                                    | gio::FileMonitorEvent::Deleted
                                    | gio::FileMonitorEvent::MovedIn
                                    | gio::FileMonitorEvent::Renamed
                            );
                        if relevant {
                            if let Some(s) = me.upgrade() {
                                s.schedule_reload();
                            }
                        }
                    });
                    eprintln!("[glaze-shell] live reload: watching {}", dir.display());
                    self.monitors.borrow_mut().push(mon);
                }
                Err(e) => eprintln!("[glaze-shell] cannot watch {}: {e}", dir.display()),
            }
        }
    }

    fn schedule_reload(self: &Rc<Self>) {
        if let Some(id) = self.reload_timer.borrow_mut().take() {
            id.remove();
        }
        let me = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(200), move || {
            me.reload_timer.borrow_mut().take();
            me.reload();
        });
        *self.reload_timer.borrow_mut() = Some(id);
    }

    fn reload(self: &Rc<Self>) {
        let started = std::time::Instant::now();
        let loaded = load_config();
        if !loaded.usable {
            eprintln!("[glaze-shell] live reload: keeping the running configuration");
            return;
        }
        let cfg = with_effective_scheme(loaded.config);
        let tokens = load_tokens(&cfg);
        *self.cfg.borrow_mut() = Rc::new(cfg);
        *self.tokens.borrow_mut() = Rc::new(tokens);
        self.install_css();
        self.build_panels();
        self.update_wallpaper();
        eprintln!(
            "[glaze-shell] live reload applied in {} ms",
            started.elapsed().as_millis()
        );
    }

    /// Hand the desktop's wallpaper stack to the shell (live swaps).
    pub fn set_desktop(
        &self,
        stack: gtk4::Stack,
        shown: Option<PathBuf>,
        orientation: glaze_common::wallpaper::Orientation,
    ) {
        *self.desktop.borrow_mut() = Some(DesktopWall {
            stack,
            shown,
            orientation,
        });
    }

    /// Re-resolve the wallpaper for the current config/scheme; cross-fade
    /// to it when it changed.
    fn update_wallpaper(&self) {
        let cfg = self.cfg();
        let mut d = self.desktop.borrow_mut();
        let Some(d) = d.as_mut() else { return };
        let path = glaze_common::wallpaper::resolve(
            &cfg,
            cfg.appearance.scheme,
            d.orientation,
            &glaze_common::wallpaper::search_dirs(),
        );
        if path == d.shown {
            return;
        }
        let fade = if motion_settings(&cfg).instant() {
            0
        } else {
            450
        };
        eprintln!(
            "[glaze-shell] wallpaper (live): {}",
            path.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none (canvas colour)".into())
        );
        crate::desktop_ui::set_wallpaper(&d.stack, path.as_deref(), fade);
        d.shown = path;
    }

    /// `scheme_mode = "auto"`: re-check the time every minute and switch
    /// light/dark live.
    pub fn watch_auto_scheme(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        glib::timeout_add_seconds_local(60, move || {
            let Some(s) = me.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let cfg = s.cfg();
            if cfg.appearance.scheme_mode == glaze_common::config::SchemeMode::Auto {
                let want = with_effective_scheme((*cfg).clone()).appearance.scheme;
                if want != s.tokens().scheme {
                    eprintln!("[glaze-shell] auto scheme: switching to {want:?}");
                    s.reload();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn apply_status(&self, snapshot: glaze_shell_core::status_feed::StatusSnapshot) {
        // The feed is polled every 2 s; rebuilding the tray and the
        // notification list each time cost redraws for nothing.
        let same = self
            .last_status
            .borrow()
            .as_ref()
            .map(|old| format!("{old:?}") == format!("{snapshot:?}"))
            .unwrap_or(false);
        if same {
            return;
        }
        for p in self.panels.borrow().iter() {
            p.apply_status(&snapshot);
        }
        *self.last_status.borrow_mut() = Some(snapshot);
    }

    /// The first panel window (for panel-local key bindings).
    pub fn first_window(&self) -> Option<gtk4::Window> {
        self.panels.borrow().first().map(|p| p.window.clone())
    }
}

thread_local! {
    /// The shell, for callbacks the event thread queues onto the main loop.
    static SHELL: RefCell<std::rc::Weak<Shell>> = const { RefCell::new(std::rc::Weak::new()) };
}

/// `swaymsg reload` re-reads sway.conf, which resets the frame colours to
/// the light defaults written there: repaint them from the live tokens.
fn on_sway_reload() {
    SHELL.with(|c| {
        if let Some(me) = c.borrow().upgrade() {
            eprintln!("[glaze-shell] sway reloaded its config; repainting window frames");
            me.apply_frame_colors();
        }
    });
}

/// Main-loop side of the event thread.
fn on_events(new_ids: Vec<String>, ended: Option<String>) {
    SHELL.with(|c| {
        let Some(me) = c.borrow().upgrade() else {
            return;
        };
        me.new_windows.borrow_mut().extend(new_ids);
        me.schedule_refresh();
        if let Some(e) = ended {
            eprintln!("[glaze-shell] event stream ended ({e}); re-subscribing");
            let me2 = me.clone();
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                me2.start_window_events()
            });
        }
    });
}

/// Block on the event socket; forward each batch to the main loop.
fn event_thread(mut stream: Box<dyn glaze_backend::EventSource>) {
    let fd = stream.raw_fd();
    loop {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd for the duration of the call.
        let r = unsafe { libc::poll(&mut pfd, 1, -1) };
        if r < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            let e = std::io::Error::last_os_error().to_string();
            glib::MainContext::default().invoke(move || on_events(Vec::new(), Some(e)));
            return;
        }
        match stream.read_events() {
            Ok(events) => {
                if events.is_empty() {
                    continue;
                }
                let reloaded = events.iter().any(
                    |ev| matches!(ev, BackendEvent::Workspace { change } if change == "reload"),
                );
                if reloaded {
                    glib::MainContext::default().invoke(on_sway_reload);
                }
                let ids: Vec<String> = events
                    .into_iter()
                    .filter_map(|ev| match ev {
                        BackendEvent::Window {
                            change,
                            id: Some(id),
                        } if change == "new" => Some(id),
                        _ => None,
                    })
                    .collect();
                glib::MainContext::default().invoke(move || on_events(ids, None));
            }
            Err(e) => {
                let e = e.to_string();
                glib::MainContext::default().invoke(move || on_events(Vec::new(), Some(e)));
                return;
            }
        }
    }
}

fn is_config_like(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("toml")
}

fn mark_all_read() {
    #[cfg(unix)]
    {
        use glaze_shell_core::socket_client::UnixSocketClient;
        let mut client = UnixSocketClient::new();
        match glaze_shell_core::status_feed::mark_all_read(&mut client) {
            Ok(()) => eprintln!("[glaze-shell] notifications marked read"),
            Err(e) => eprintln!("[glaze-shell] could not mark notifications read: {e}"),
        }
    }
}

/// The screen edges a panel is anchored to for each configured position.
pub fn anchor_edges(position: PanelPosition) -> &'static [Edge] {
    match position {
        PanelPosition::Bottom => &[Edge::Left, Edge::Right, Edge::Bottom],
        PanelPosition::Top => &[Edge::Left, Edge::Right, Edge::Top],
        PanelPosition::Left => &[Edge::Left, Edge::Top, Edge::Bottom],
        PanelPosition::Right => &[Edge::Right, Edge::Top, Edge::Bottom],
    }
}

/// Auto-hide: the bar slides off its edge leaving a 3 px strip, and slides
/// back while the pointer is over it (frame-clock animated margin). `smart`
/// currently behaves like `always` (overlap detection is not implemented).
fn setup_autohide(window: &gtk4::Window, ctx: &Ctx, position: PanelPosition, thickness: i32) {
    let edge = match position {
        PanelPosition::Bottom => Edge::Bottom,
        PanelPosition::Top => Edge::Top,
        PanelPosition::Left => Edge::Left,
        PanelPosition::Right => Edge::Right,
    };
    window.set_exclusive_zone(0);
    let hidden = -(thickness - 3) as f64;
    window.set_margin(edge, hidden as i32);
    let anim = Animator::new(
        "autohide",
        window,
        ctx.motion,
        ctx.cfg.motion.log_frame_times,
    );
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let current = Rc::new(Cell::new(hidden));
    let motion = gtk4::EventControllerMotion::new();
    {
        let (w, a, t, cur) = (window.clone(), anim.clone(), timer.clone(), current.clone());
        let ms = ctx.tokens.metrics.motion_menu_ms as f64;
        motion.connect_enter(move |_, _, _| {
            if let Some(id) = t.borrow_mut().take() {
                id.remove();
            }
            let (w2, c2) = (w.clone(), cur.clone());
            a.animate(
                "m",
                cur.get(),
                0.0,
                ms,
                move |v| {
                    c2.set(v);
                    w2.set_margin(edge, v.round() as i32);
                },
                None,
            );
        });
    }
    {
        let (w, a, t, cur) = (window.clone(), anim, timer, current);
        let ms = ctx.tokens.metrics.motion_menu_ms as f64;
        motion.connect_leave(move |_| {
            let (w2, a2, c2) = (w.clone(), a.clone(), cur.clone());
            let id = glib::timeout_add_local_once(Duration::from_millis(700), move || {
                let (w3, c3) = (w2.clone(), c2.clone());
                a2.animate(
                    "m",
                    c2.get(),
                    hidden,
                    ms,
                    move |v| {
                        c3.set(v);
                        w3.set_margin(edge, v.round() as i32);
                    },
                    None,
                );
            });
            if let Some(old) = t.borrow_mut().replace(id) {
                old.remove();
            }
        });
    }
    window.add_controller(motion);
}
