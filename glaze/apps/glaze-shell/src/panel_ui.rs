//! Taskbar v2 (approved mockup `taskbar-v2`): start button, pinned launchers,
//! animated running-window buttons with grouping/count pill/active underline,
//! hover card, context menu, float/tile toggle, tray, serif clock and the
//! show-desktop sliver. Every size/colour comes from the design tokens and
//! every behaviour switch from `[panel]` in the user config.
//!
//! Decisions (what exists, labels, widths, positions, click results) come
//! from the tested `glaze_shell_core::bar` model; this module draws them and
//! animates the transitions with [`crate::anim::Animator`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use glaze_backend::{Backend, LayoutMode, LayoutScope};
use glaze_common::color::Color;
use glaze_common::config::{
    ButtonStyle, GlazeConfig, GroupingMode, IconStyle, LabelMode, LayoutScope as CfgLayoutScope,
    PanelConfig, PanelPosition, PanelSection, TrayItem,
};
use glaze_common::model::{AppEntry, WindowInfo};
use glaze_common::theme::ThemeTokens;
use glaze_design::motion::MotionSettings;
use glaze_service::registry::AppRegistry;
use glaze_shell_core::bar::{self, BarMetrics, ClickAction, TaskButton};
use glaze_shell_core::clock::{format_date, format_time, ClockParts, Weekday};
use glib::ControlFlow;
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::anim::Animator;
use crate::icons;
use crate::ink::Glyph;
use crate::launcher;

/// One backend action applied to a window id (context-menu items).
type WindowOp = Rc<dyn Fn(&dyn Backend, &str) -> Result<(), glaze_backend::BackendError>>;

/// Shared, per-shell context handed to every panel.
#[derive(Clone)]
pub struct Ctx {
    pub cfg: Rc<GlazeConfig>,
    pub tokens: Rc<ThemeTokens>,
    pub registry: Rc<AppRegistry>,
    pub backend: Option<Rc<dyn Backend>>,
    pub motion: MotionSettings,
    /// Persist a new explicit pin list (writes the user config).
    pub set_pins: Rc<dyn Fn(Vec<String>)>,
    pub on_mark_all_read: Rc<dyn Fn()>,
    /// Ask the shell to refresh window state now.
    pub refresh: Rc<dyn Fn()>,
}

impl Ctx {
    pub fn ink(&self) -> Color {
        self.tokens.palette.ink
    }
    pub fn stroke(&self) -> f64 {
        self.tokens.metrics.icon_stroke
    }
    fn icon_px(&self) -> i32 {
        self.tokens.metrics.icon_size
    }
    pub fn pins(&self) -> Vec<String> {
        bar::effective_pins(
            &self.cfg.launcher.pinned,
            self.cfg.panel.default_pins,
            self.registry.apps(),
        )
    }
}

/// One taskbar on one output.
pub struct Panel {
    pub window: gtk4::Window,
    /// Output connector name (e.g. `Virtual-1`).
    #[allow(dead_code)]
    pub connector: String,
    position: PanelPosition,
    pins: Vec<(String, gtk4::Button)>,
    tasks: Option<TaskStrip>,
    toggle: Option<LayoutToggle>,
    tray: Option<Tray>,
}

impl Panel {
    /// Feed a fresh window list (from the backend event stream).
    pub fn update_windows(&self, windows: &[WindowInfo], apps: &[AppEntry]) {
        if let Some(t) = &self.tasks {
            t.update(windows);
        }
        // Pinned apps with an open window get the running mark.
        for (id, button) in &self.pins {
            let running = windows.iter().any(|w| {
                w.app_id
                    .as_deref()
                    .and_then(|a| bar::find_app(a, apps))
                    .map(|a| &a.id == id)
                    .unwrap_or(false)
            });
            if running {
                button.add_css_class("glaze-running");
            } else {
                button.remove_css_class("glaze-running");
            }
        }
    }

    /// Feed the focused workspace's float/tile mode.
    pub fn update_layout(&self, mode: Option<LayoutMode>) {
        if let Some(t) = &self.toggle {
            t.set_mode(mode);
        }
    }

    pub fn apply_status(&self, snapshot: &glaze_shell_core::status_feed::StatusSnapshot) {
        if let Some(t) = &self.tray {
            t.apply_status(snapshot);
        }
    }

    /// Screen rectangle of the task button holding window `id`.
    pub fn button_screen_rect(&self, id: &str) -> Option<(f64, f64, f64, f64)> {
        let t = self.tasks.as_ref()?;
        let (x, y, w, h) = t.window_button_rect(id)?;
        let (mw, mh) = crate::ghost::monitor_size(&self.window)?;
        let (ww, wh) = (self.window.width() as f64, self.window.height() as f64);
        let (ox, oy) = match self.position {
            PanelPosition::Bottom => (0.0, mh as f64 - wh),
            PanelPosition::Right => (mw as f64 - ww, 0.0),
            PanelPosition::Top | PanelPosition::Left => (0.0, 0.0),
        };
        Some((x + ox, y + oy, w, h))
    }

    pub fn destroy(&self) {
        if let Some(t) = &self.tasks {
            t.card.unparent();
            t.menu.unparent();
        }
        self.window.destroy();
    }
}

/// The live sub-widgets of a built panel.
#[derive(Default)]
pub struct PanelParts {
    /// Pinned launchers by app id (running indicator).
    pins: Vec<(String, gtk4::Button)>,
    tasks: Option<TaskStrip>,
    toggle: Option<LayoutToggle>,
    tray: Option<Tray>,
}

impl PanelParts {
    pub fn into_panel(
        self,
        window: gtk4::Window,
        connector: String,
        position: PanelPosition,
    ) -> Panel {
        Panel {
            window,
            connector,
            position,
            pins: self.pins,
            tasks: self.tasks,
            toggle: self.toggle,
            tray: self.tray,
        }
    }
}

/// Build the taskbar content for `pcfg` (the panel config for this output).
pub fn build(ctx: &Ctx, pcfg: &PanelConfig) -> (gtk4::Widget, PanelParts) {
    let m = &ctx.tokens.metrics;
    let vertical = pcfg.position.is_vertical();
    let orient = if vertical {
        gtk4::Orientation::Vertical
    } else {
        gtk4::Orientation::Horizontal
    };
    let row = gtk4::Box::new(orient, 0);
    row.add_css_class("glaze-panel");
    row.add_css_class(match pcfg.position {
        PanelPosition::Bottom => "glaze-edge-bottom",
        PanelPosition::Top => "glaze-edge-top",
        PanelPosition::Left => "glaze-edge-left",
        PanelPosition::Right => "glaze-edge-right",
    });
    row.add_css_class(match pcfg.style {
        glaze_common::config::PanelStyle::ClassicBar => "glaze-style-bar",
        glaze_common::config::PanelStyle::Floating => "glaze-style-floating",
    });
    row.add_css_class(match pcfg.button_style {
        ButtonStyle::Paper => "glaze-style-paper",
        ButtonStyle::Flat => "glaze-style-flat",
        ButtonStyle::Outline => "glaze-style-outline",
    });
    let pad = m.panel_padding.max(0);
    let thickness = panel_thickness(ctx);
    if vertical {
        row.set_width_request(thickness);
    } else {
        row.set_height_request(thickness);
    }

    let mut parts = PanelParts::default();
    let has_tasks = pcfg.sections.contains(&PanelSection::Tasks);
    let sections: Vec<PanelSection> = pcfg
        .sections
        .iter()
        .copied()
        .filter(|s| *s != PanelSection::LayoutToggle || pcfg.show_layout_toggle)
        .collect();
    let mut prev: Option<PanelSection> = None;
    let mut well: Option<gtk4::Box> = None;
    for section in sections {
        // Quick Launch grip after Start, hairline separator before tasks.
        if matches!(
            (prev, section),
            (Some(PanelSection::Start), PanelSection::Pinned)
        ) {
            let grip = gtk4::Box::new(orient, 0);
            grip.add_css_class("glaze-grip");
            row.append(&grip);
        } else if matches!(
            (prev, section),
            (Some(PanelSection::Pinned), PanelSection::Tasks)
                | (Some(PanelSection::Start), PanelSection::Tasks)
        ) {
            let sep = gtk4::Separator::new(if vertical {
                gtk4::Orientation::Horizontal
            } else {
                gtk4::Orientation::Vertical
            });
            sep.add_css_class("glaze-section-sep");
            row.append(&sep);
        }
        // Tray and clock share one inset well (Windows 2000 notification area).
        let in_well = matches!(section, PanelSection::Tray | PanelSection::Clock);
        if in_well && !matches!(prev, Some(PanelSection::Tray | PanelSection::Clock)) {
            let w = gtk4::Box::new(orient, 2);
            w.add_css_class("glaze-tray-well");
            w.set_valign(gtk4::Align::Center);
            w.set_halign(gtk4::Align::Center);
            row.append(&w);
            well = Some(w);
        }
        let target: gtk4::Box = if in_well {
            well.clone().expect("well created above")
        } else {
            row.clone()
        };
        match section {
            PanelSection::Start => {
                let b = start_button(ctx, pcfg, vertical);
                if vertical {
                    b.set_margin_top(pad + 2);
                    b.set_halign(gtk4::Align::Center);
                } else {
                    // Classic bar: Start sits flush in the corner.
                    let flush = pcfg.style == glaze_common::config::PanelStyle::ClassicBar;
                    b.set_margin_start(if flush { 0 } else { pad + 2 });
                    b.set_margin_end(if flush { 0 } else { 4 });
                }
                row.append(&b);
                let bar = pcfg.style == glaze_common::config::PanelStyle::ClassicBar;
                if bar && pcfg.show_search && !vertical {
                    let sb = bar_icon_button(ctx, "search", Glyph::Search, "Search apps");
                    let start = b.clone();
                    sb.connect_clicked(move |_| {
                        // Opens Start; typing goes straight to its search.
                        start.emit_clicked();
                    });
                    row.append(&sb);
                }
                if bar && pcfg.show_task_view && !vertical {
                    let tv = bar_icon_button(ctx, "layout-float", Glyph::Float, "Task view");
                    let c2 = ctx.clone();
                    tv.connect_clicked(move |btn| task_view(btn, &c2));
                    row.append(&tv);
                }
            }
            // Pinned launchers live in the task strip (merged buttons);
            // the separate box only appears without a task section.
            PanelSection::Pinned if has_tasks => {}
            PanelSection::Pinned => {
                let (b, pins) = pins_box(ctx, orient);
                row.append(&b);
                parts.pins = pins;
            }
            PanelSection::Tasks => {
                let strip = TaskStrip::new(ctx, pcfg, vertical);
                row.append(&strip.root);
                parts.tasks = Some(strip);
            }
            PanelSection::Spacer => {
                let s = gtk4::Box::new(orient, 0);
                if !has_tasks {
                    s.set_hexpand(!vertical);
                    s.set_vexpand(vertical);
                }
                row.append(&s);
            }
            PanelSection::LayoutToggle => {
                let t = LayoutToggle::new(ctx, orient);
                t.root.set_margin_start(6);
                t.root.set_margin_end(12);
                row.append(&t.root);
                parts.toggle = Some(t);
            }
            PanelSection::Tray => {
                let t = Tray::new(ctx, pcfg, orient);
                target.append(&t.root);
                parts.tray = Some(t);
            }
            PanelSection::Clock => {
                let c = clock(pcfg, vertical);
                c.set_margin_start(2);
                target.append(&c);
            }
            PanelSection::ShowDesktop => {
                let b = show_desktop(ctx);
                b.set_margin_start(6);
                row.append(&b);
            }
        }
        prev = Some(section);
    }
    (row.upcast(), parts)
}

/// Bar thickness from tokens and density/height config.
pub fn panel_thickness(ctx: &Ctx) -> i32 {
    let m = &ctx.tokens.metrics;
    m.panel_height.max(m.control_height + 4)
}

// ---------------------------------------------------------------------------
// Start button and pinned launchers
// ---------------------------------------------------------------------------

/// Original Start mark for the classic bar: a terracotta ring with a cream
/// five-point star, pixel-snapped for 16-20 px.
pub fn start_mark(ctx: &Ctx, px: i32) -> gtk4::Widget {
    let ring = ctx.tokens.palette.accent.to_rgb().to_hex();
    let star = ctx.tokens.palette.surface_raised.to_rgb().to_hex();
    // Star points on a radius-5.4 circle around (10,10), inner radius 2.3.
    let mut pts = String::new();
    for i in 0..10 {
        let r = if i % 2 == 0 { 5.4 } else { 2.35 };
        let a = std::f64::consts::PI * (i as f64) / 5.0 - std::f64::consts::FRAC_PI_2;
        pts.push_str(&format!(
            "{:.2},{:.2} ",
            10.0 + r * a.cos(),
            10.2 + r * a.sin()
        ));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
<circle cx="10" cy="10" r="9" fill="{ring}"/>
<polygon points="{pts}" fill="{star}" stroke="{star}" stroke-width="0.6" stroke-linejoin="round"/>
</svg>"##
    );
    match icons::render_svg_size(&svg, px, px) {
        Some(tex) => {
            let img = gtk4::Image::from_paintable(Some(&tex));
            img.set_pixel_size(px);
            img.set_valign(gtk4::Align::Center);
            img.add_css_class("glaze-start-mark");
            img.upcast()
        }
        None => icons::icon("start", Glyph::StartDots, px, ctx.ink(), ctx.stroke()),
    }
}

/// Task view: every open window as a card (icon, title, workspace); click
/// focuses or restores it. Sway has no live thumbnails, so cards show the
/// app icon rather than fake previews.
fn task_view(anchor: &gtk4::Button, ctx: &Ctx) {
    use crate::menu::Entry;
    let Some(be) = ctx.backend.clone() else {
        return;
    };
    let Ok(ws) = be.windows() else { return };
    let mut v = vec![Entry::Header(match ws.len() {
        0 => "No open windows".to_string(),
        1 => "1 window".to_string(),
        n => format!("{n} windows"),
    })];
    for w in ws {
        let (be, id, min) = (be.clone(), w.id.clone(), w.minimized);
        let r = ctx.refresh.clone();
        let mut label = w.title.clone();
        if w.minimized {
            label.push_str("  (minimised)");
        }
        v.push(
            Entry::item(Some("window-restore"), label, move || {
                let _ = if min {
                    be.restore_window(&id)
                } else {
                    be.activate_window(&id)
                };
                r();
            })
            .accel(match w.workspace {
                Some(n) => format!("Desktop {}", n + 1),
                None => String::new(),
            }),
        );
    }
    crate::menu::popup(
        anchor,
        None,
        popover_position(ctx.cfg.panel.position),
        ctx,
        v,
    );
}

/// A full-height icon button of the classic bar (search, task view).
fn bar_icon_button(ctx: &Ctx, name: &str, glyph: Glyph, tip: &str) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.add_css_class("glaze-bar-button");
    b.set_tooltip_text(Some(tip));
    b.update_property(&[gtk4::accessible::Property::Label(tip)]);
    b.set_child(Some(&icons::icon(name, glyph, 18, ctx.ink(), ctx.stroke())));
    b
}

fn start_button(ctx: &Ctx, pcfg: &PanelConfig, vertical: bool) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.add_css_class("glaze-start-button");
    button.set_valign(gtk4::Align::Center);
    button.set_tooltip_text(Some("Start"));
    button.update_property(&[gtk4::accessible::Property::Label("Start")]);
    if pcfg.style == glaze_common::config::PanelStyle::ClassicBar {
        // Classic bar: icon only, full height.
        button.set_valign(gtk4::Align::Fill);
        button.set_child(Some(&start_mark(ctx, 20)));
        launcher::attach(&button, ctx, pcfg.position);
        return button;
    }
    let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    // Dark Clay uses a tinted Start pill (CSS), so the mark follows its text.
    let mark_fg = if ctx.tokens.scheme == glaze_design::kinds::ColorScheme::Dark
        && ctx.tokens.kind == glaze_design::kinds::ThemeKind::Clay
    {
        ctx.tokens.palette.accent_soft_text
    } else {
        ctx.tokens.palette.accent_text
    };
    let brand = icons::brand(&ctx.cfg.branding);
    let mark = icons::start_icon(&brand, 16, mark_fg, ctx.stroke() + 0.25);
    mark.set_valign(gtk4::Align::Center);
    inner.append(&mark);
    let text = pcfg.start_label.trim();
    if !vertical && !text.is_empty() {
        let label = gtk4::Label::new(Some(text));
        label.add_css_class("glaze-start-label");
        label.set_valign(gtk4::Align::Center);
        inner.append(&label);
    }
    button.set_child(Some(&inner));
    launcher::attach(&button, ctx, pcfg.position);
    button
}

fn pins_box(ctx: &Ctx, orient: gtk4::Orientation) -> (gtk4::Box, Vec<(String, gtk4::Button)>) {
    let mut pins = Vec::new();
    let b = gtk4::Box::new(orient, 2);
    b.add_css_class("glaze-pins");
    b.set_valign(gtk4::Align::Center);
    b.set_halign(gtk4::Align::Center);
    b.set_margin_start(4);
    b.set_margin_end(4);
    for id in ctx.pins() {
        let Some(app) = ctx.registry.get(&id).cloned() else {
            eprintln!("[glaze-shell] pinned app {id} is not installed; skipped");
            continue;
        };
        let button = gtk4::Button::new();
        button.add_css_class("glaze-pin-button");
        button.set_valign(gtk4::Align::Center);
        button.set_tooltip_text(Some(&app.name));
        button.set_child(Some(&app_icon(ctx, &app.id, Some(&app))));
        {
            let app = app.clone();
            button.connect_clicked(move |_| launch(&app));
        }
        pins.push((app.id.clone(), button.clone()));
        let menu_ctx = ctx.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);
        let btn = button.clone();
        let position = popover_position(ctx.cfg.panel.position);
        gesture.connect_released(move |_, _, _, _| {
            let app2 = app.clone();
            let ctx2 = menu_ctx.clone();
            let id = app.id.clone();
            crate::menu::popup(
                &btn,
                None,
                position,
                &menu_ctx,
                vec![
                    crate::menu::Entry::Header(app.name.clone()),
                    crate::menu::Entry::item(Some("start"), "Open", move || launch(&app2)),
                    crate::menu::Entry::Separator,
                    crate::menu::Entry::item(Some("unpin"), "Unpin from taskbar", move || {
                        (ctx2.set_pins)(bar::toggle_pin(&ctx2.pins(), &id))
                    }),
                ],
            );
        });
        button.add_controller(gesture);
        b.append(&button);
    }
    (b, pins)
}

/// The icon for an app: ink glyph when Glaze has one (icon_style = ink), else
/// the theme icon from the .desktop entry, else a generic window glyph.
pub fn app_icon(ctx: &Ctx, app_id: &str, app: Option<&AppEntry>) -> gtk4::Widget {
    app_icon_sized(ctx, app_id, app, ctx.icon_px())
}

pub fn app_icon_sized(ctx: &Ctx, app_id: &str, app: Option<&AppEntry>, px: i32) -> gtk4::Widget {
    let cats = app.map(|a| a.categories.clone()).unwrap_or_default();
    if ctx.cfg.appearance.icon_style == IconStyle::Ink {
        if let Some(name) = icons::app_icon_name(app_id, &cats) {
            return icons::icon(name, icons::fallback_for(name), px, ctx.ink(), ctx.stroke());
        }
    }
    if let Some(icon) = app.and_then(|a| a.icon.clone()) {
        let theme = gtk4::IconTheme::for_display(&gdk::Display::default().expect("display"));
        if theme.has_icon(&icon) || icon.starts_with('/') {
            let img = if icon.starts_with('/') {
                gtk4::Image::from_file(&icon)
            } else {
                gtk4::Image::from_icon_name(&icon)
            };
            img.set_pixel_size(px);
            return img.upcast();
        }
    }
    icons::icon("window-restore", Glyph::Window, px, ctx.ink(), ctx.stroke())
}

/// Snap/maximise window `id` with `glaze-snap` (same work-area logic as the
/// Super+arrow keys; remembers the old geometry for restore). Returns
/// false when the helper is not installed.
pub fn snap(id: &str, mode: &str) -> bool {
    match std::process::Command::new("glaze-snap")
        .args([mode, id])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut c) => {
            std::thread::spawn(move || {
                let _ = c.wait();
            });
            true
        }
        Err(e) => {
            eprintln!("[glaze-shell] glaze-snap {mode} {id}: {e}");
            false
        }
    }
}

/// Launch an app. If it fails to start, or dies with an error within 4 s,
/// a toast names the app and the reason (its stderr is kept in
/// `~/.cache/glaze/apps/<id>.log`).
pub fn launch(app: &AppEntry) {
    let name = app.name.clone();
    let res = glaze_service::launch::launch_watched(app, Duration::from_secs(4), move |exit| {
        let reason = exit.reason();
        let log = exit.log_path.display().to_string();
        glib::MainContext::default().invoke(move || {
            crate::toast::show(
                &format!("{name} closed right after starting"),
                &format!("It {reason}. Details: {log}"),
            );
        });
    });
    match res {
        Ok(p) => eprintln!("[glaze-shell] launched {} (pid {})", app.id, p.id()),
        Err(e) => {
            eprintln!("[glaze-shell] failed to launch {}: {e}", app.id);
            crate::toast::show(&format!("Could not open {}", app.name), &e.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Running-window strip
// ---------------------------------------------------------------------------

struct Slot {
    button: gtk4::Button,
    label: gtk4::Label,
    pill: gtk4::Label,
    underline: gtk4::Box,
    model: RefCell<TaskButton>,
    x: Cell<f64>,
    w: Cell<f64>,
    /// Current running/active bar width (animated).
    ul_w: Cell<f64>,
    removing: Cell<bool>,
}

#[derive(Clone)]
struct TaskStrip {
    root: gtk4::Overlay,
    fixed: gtk4::Fixed,
    slots: Rc<RefCell<HashMap<String, Rc<Slot>>>>,
    order: Rc<RefCell<Vec<String>>>,
    windows: Rc<RefCell<Vec<WindowInfo>>>,
    available: Rc<Cell<i32>>,
    anim: Animator,
    card: gtk4::Popover,
    card_for: Rc<RefCell<Option<String>>>,
    card_timer: Rc<RefCell<Option<glib::SourceId>>>,
    menu: gtk4::Popover,
    menu_open: Rc<Cell<bool>>,
    /// The card was opened by a click (chooser), not by hovering.
    card_pinned: Rc<Cell<bool>>,
    ctx: Ctx,
    pcfg: Rc<PanelConfig>,
    vertical: bool,
    metrics: BarMetrics,
    height: i32,
}

impl TaskStrip {
    fn new(ctx: &Ctx, pcfg: &PanelConfig, vertical: bool) -> Self {
        let m = &ctx.tokens.metrics;
        let root = gtk4::Overlay::new();
        root.add_css_class("glaze-task-area");
        let base = gtk4::DrawingArea::new();
        if vertical {
            base.set_vexpand(true);
        } else {
            base.set_hexpand(true);
        }
        root.set_child(Some(&base));
        let fixed = gtk4::Fixed::new();
        fixed.set_halign(gtk4::Align::Start);
        fixed.set_valign(if vertical {
            gtk4::Align::Start
        } else {
            gtk4::Align::Center
        });
        if vertical {
            fixed.set_halign(gtk4::Align::Center);
            fixed.set_margin_top(6);
        } else {
            fixed.set_margin_start(
                if pcfg.style == glaze_common::config::PanelStyle::ClassicBar {
                    2
                } else {
                    8
                },
            );
        }
        root.add_overlay(&fixed);
        root.set_clip_overlay(&fixed, true);

        let card = gtk4::Popover::new();
        card.add_css_class("glaze-popover");
        card.add_css_class("glaze-hover-card");
        card.set_has_arrow(false);
        card.set_autohide(false);
        card.set_can_focus(false);
        card.set_parent(&root);
        card.set_position(popover_position(pcfg.position));

        let menu = gtk4::Popover::new();
        menu.add_css_class("glaze-popover");
        menu.set_has_arrow(false);
        menu.set_parent(&root);
        menu.set_position(popover_position(pcfg.position));

        let strip = TaskStrip {
            root,
            fixed,
            slots: Rc::new(RefCell::new(HashMap::new())),
            order: Rc::new(RefCell::new(Vec::new())),
            windows: Rc::new(RefCell::new(Vec::new())),
            available: Rc::new(Cell::new(0)),
            anim: Animator::new("taskbar", &base, ctx.motion, ctx.cfg.motion.log_frame_times),
            card,
            card_for: Rc::new(RefCell::new(None)),
            card_timer: Rc::new(RefCell::new(None)),
            menu,
            menu_open: Rc::new(Cell::new(false)),
            card_pinned: Rc::new(Cell::new(false)),
            ctx: ctx.clone(),
            pcfg: Rc::new(pcfg.clone()),
            vertical,
            metrics: if pcfg.style == glaze_common::config::PanelStyle::ClassicBar {
                // Full-height icon buttons, edge to edge (Windows 10 bar).
                BarMetrics {
                    task_width: (m.task_button_width + 32).min(200),
                    icon_width: 46,
                    spacing: 0,
                }
            } else {
                BarMetrics {
                    task_width: m.task_button_width,
                    icon_width: m.task_icon_width,
                    spacing: 8,
                }
            },
            height: if pcfg.style == glaze_common::config::PanelStyle::ClassicBar {
                panel_thickness(ctx) - 1
            } else {
                m.control_height
            },
        };
        {
            let s = strip.clone();
            base.connect_resize(move |_, w, h| {
                let avail = if s.vertical { h - 12 } else { w - 16 };
                if s.available.get() != avail {
                    s.available.set(avail);
                    let ws = s.windows.borrow().clone();
                    s.update(&ws);
                }
            });
        }
        {
            let s = strip.clone();
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter(move |_, _, _| s.cancel_card_timer());
            let s = strip.clone();
            motion.connect_leave(move |_| s.schedule_card_hide());
            strip.card.add_controller(motion);
        }
        strip
    }

    /// Send `(window id, rect relative to the panel surface)` for every
    /// task button to the compositor (Glaze compositor only; others ignore).
    fn report_button_rects(&self) {
        let Some(be) = self.ctx.backend.clone() else {
            return;
        };
        let Some(root) = self.root.root() else {
            return;
        };
        let mut rects: Vec<(String, i32, i32, i32, i32)> = Vec::new();
        for slot in self.slots.borrow().values() {
            if slot.removing.get() {
                continue;
            }
            let Some(b) = slot.button.compute_bounds(&root) else {
                continue;
            };
            let r = (
                b.x().round() as i32,
                b.y().round() as i32,
                b.width().round() as i32,
                b.height().round() as i32,
            );
            for w in &slot.model.borrow().windows {
                rects.push((w.id.clone(), r.0, r.1, r.2, r.3));
            }
        }
        if let Err(e) = be.set_minimize_rects(&rects) {
            if !e.is_unsupported() {
                eprintln!("[glaze-shell] task button rects: {e}");
            }
        }
    }

    /// The button holding window `id`, relative to the panel surface.
    fn window_button_rect(&self, id: &str) -> Option<(f64, f64, f64, f64)> {
        let root = self.root.root()?;
        let slots = self.slots.borrow();
        let slot = slots
            .values()
            .find(|s| !s.removing.get() && s.model.borrow().windows.iter().any(|w| w.id == id))?;
        let b = slot.button.compute_bounds(&root)?;
        Some((
            b.x() as f64,
            b.y() as f64,
            b.width() as f64,
            b.height() as f64,
        ))
    }

    fn label_mode(&self) -> LabelMode {
        let bar = self.pcfg.style == glaze_common::config::PanelStyle::ClassicBar;
        if self.vertical || (bar && !self.pcfg.bar_labels) {
            LabelMode::IconOnly
        } else {
            self.pcfg.label_mode()
        }
    }

    fn update(&self, windows: &[WindowInfo]) {
        *self.windows.borrow_mut() = windows.to_vec();
        // One button per app: pinned launchers and their running windows
        // are the same button (Windows 10 taskbar).
        let buttons = bar::merged_buttons(
            windows,
            self.ctx.registry.apps(),
            &self.ctx.pins(),
            self.pcfg.grouping,
            self.label_mode(),
            &self.metrics,
            self.available.get(),
        );
        let positions = bar::positions(&buttons, &self.metrics);
        let ms = self.ctx.tokens.metrics.motion_menu_ms as f64 + 80.0;
        let new_keys: Vec<String> = buttons.iter().map(|b| b.key.clone()).collect();
        let old_keys: Vec<String> = self.order.borrow().clone();

        // Removed buttons: shrink + fade, then destroy.
        for key in old_keys.iter().filter(|k| !new_keys.contains(k)) {
            let Some(slot) = self.slots.borrow().get(key).cloned() else {
                continue;
            };
            if slot.removing.replace(true) {
                continue;
            }
            let s = self.clone();
            let k = key.clone();
            let gone = slot.clone();
            let sl = slot.clone();
            let s2 = self.clone();
            self.anim.animate(
                &format!("{key}/w"),
                slot.w.get(),
                self.metrics.icon_width as f64 * 0.5,
                ms,
                move |v| {
                    sl.w.set(v);
                    s2.place(&sl);
                },
                Some(Box::new(move || {
                    if gone.removing.get() {
                        s.fixed.remove(&gone.button);
                        s.slots.borrow_mut().remove(&k);
                        s.order.borrow_mut().retain(|o| o != &k);
                    }
                })),
            );
            let fade = slot.clone();
            self.anim.animate(
                &format!("{key}/o"),
                slot.button.opacity(),
                0.0,
                ms * 0.6,
                move |v| fade.button.set_opacity(v),
                None,
            );
        }

        for (b, (key, x, w)) in buttons.iter().zip(positions.iter()) {
            let existing = self.slots.borrow().get(key).cloned();
            let slot = match existing {
                Some(slot) => {
                    if slot.removing.replace(false) {
                        let f = slot.clone();
                        self.anim.animate(
                            &format!("{key}/o"),
                            slot.button.opacity(),
                            1.0,
                            ms,
                            move |v| f.button.set_opacity(v),
                            None,
                        );
                    }
                    self.refresh_slot(&slot, b);
                    slot
                }
                None => {
                    let slot = self.make_slot(b);
                    slot.x.set(*x as f64);
                    slot.w.set(self.metrics.icon_width as f64 * 0.5);
                    slot.button.set_opacity(0.0);
                    self.fixed.put(&slot.button, 0.0, 0.0);
                    self.place(&slot);
                    self.slots.borrow_mut().insert(key.clone(), slot.clone());
                    let f = slot.clone();
                    self.anim.animate(
                        &format!("{key}/o"),
                        0.0,
                        1.0,
                        ms,
                        move |v| f.button.set_opacity(v),
                        None,
                    );
                    slot
                }
            };
            let (s1, sl1) = (self.clone(), slot.clone());
            self.anim.animate(
                &format!("{key}/x"),
                slot.x.get(),
                *x as f64,
                ms,
                move |v| {
                    sl1.x.set(v);
                    s1.place(&sl1);
                },
                None,
            );
            let (s2, sl2) = (self.clone(), slot.clone());
            self.anim.animate(
                &format!("{key}/w"),
                slot.w.get(),
                *w as f64,
                ms,
                move |v| {
                    sl2.w.set(v);
                    s2.place(&sl2);
                },
                None,
            );
        }
        let mut order = new_keys;
        for k in old_keys {
            if !order.contains(&k) && self.slots.borrow().contains_key(&k) {
                order.push(k);
            }
        }
        *self.order.borrow_mut() = order;

        // Tell the compositor where each window's button is, so minimise and
        // restore animate to and from it (after the reorder animation).
        let me = self.clone();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(ms as u64 + 60),
            move || me.report_button_rects(),
        );

        // Keep an open hover card in sync.
        let open = self.card_for.borrow().clone();
        if let Some(k) = open {
            match buttons.iter().find(|b| b.key == k) {
                Some(b) if self.card.is_visible() => self.fill_card(b),
                None => self.hide_card_now(),
                _ => {}
            }
        }
    }

    fn place(&self, slot: &Slot) {
        let w = slot.w.get().max(1.0).round() as i32;
        if self.vertical {
            slot.button
                .set_size_request(self.metrics.icon_width, self.height);
            self.fixed.move_(&slot.button, 0.0, slot.x.get().round());
        } else {
            slot.button.set_size_request(w, self.height);
            self.fixed.move_(&slot.button, slot.x.get().round(), 0.0);
        }
        slot.underline
            .set_size_request(slot.ul_w.get().round().max(0.0) as i32, -1);
    }

    fn make_slot(&self, b: &TaskButton) -> Rc<Slot> {
        let button = gtk4::Button::new();
        button.add_css_class("glaze-task-button");
        button.set_focus_on_click(false);
        let overlay = gtk4::Overlay::new();
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
        content.set_valign(gtk4::Align::Center);
        let app = bar::find_app(&b.app_id, self.ctx.registry.apps()).cloned();
        let px = if self.pcfg.style == glaze_common::config::PanelStyle::ClassicBar {
            22
        } else {
            16
        };
        content.append(&app_icon_sized(&self.ctx, &b.app_id, app.as_ref(), px));
        content.set_halign(gtk4::Align::Center);
        let label = gtk4::Label::new(None);
        label.add_css_class("glaze-task-label");
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_width_chars(1);
        content.append(&label);
        let pill = gtk4::Label::new(None);
        pill.add_css_class("glaze-count-pill");
        pill.set_valign(gtk4::Align::Center);
        content.append(&pill);
        overlay.set_child(Some(&content));
        let underline = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        underline.add_css_class("glaze-underline");
        underline.set_valign(gtk4::Align::End);
        underline.set_halign(gtk4::Align::Center);
        underline.set_can_target(false);
        underline.set_visible(false);
        overlay.add_overlay(&underline);
        button.set_child(Some(&overlay));

        let slot = Rc::new(Slot {
            button: button.clone(),
            label,
            pill,
            underline,
            model: RefCell::new(b.clone()),
            x: Cell::new(0.0),
            w: Cell::new(0.0),
            ul_w: Cell::new(0.0),
            removing: Cell::new(false),
        });
        slot.model.borrow_mut().active = false;
        self.refresh_slot(&slot, b);

        {
            let s = self.clone();
            let slot = slot.clone();
            button.connect_clicked(move |_| {
                let model = slot.model.borrow().clone();
                if model.is_group() {
                    // Windows 2000/XP grouping: a group button opens its
                    // window chooser; pick one there.
                    s.toggle_chooser(&slot);
                    return;
                }
                s.hide_card_now();
                s.primary_click(&model);
            });
        }
        {
            let s = self.clone();
            let slot = slot.clone();
            let g = gtk4::GestureClick::new();
            g.set_button(2);
            g.connect_released(move |_, _, _, _| {
                let id = slot.model.borrow().app_id.clone();
                match bar::find_app(&id, s.ctx.registry.apps()) {
                    Some(app) => launch(app),
                    None => eprintln!(
                        "[glaze-shell] no .desktop entry for {id}; cannot open a new window"
                    ),
                }
            });
            button.add_controller(g);
        }
        {
            let s = self.clone();
            let slot = slot.clone();
            let g = gtk4::GestureClick::new();
            g.set_button(3);
            g.connect_released(move |_, _, _, _| {
                s.hide_card_now();
                s.context_menu(&slot);
            });
            button.add_controller(g);
        }
        {
            let s = self.clone();
            let slot2 = slot.clone();
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter(move |_, _, _| s.schedule_card(&slot2));
            let s = self.clone();
            motion.connect_leave(move |_| s.schedule_card_hide());
            button.add_controller(motion);
        }
        slot
    }

    fn refresh_slot(&self, slot: &Rc<Slot>, b: &TaskButton) {
        let was_active = slot.model.borrow().active;
        *slot.model.borrow_mut() = b.clone();
        slot.label.set_text(&b.label);
        slot.label.set_visible(b.show_label);
        slot.pill.set_text(&b.count().to_string());
        slot.pill.set_visible(b.is_group() && b.show_label);
        for (cls, on) in [
            ("glaze-active", b.active),
            ("glaze-minimized", b.minimized),
            ("glaze-group", b.is_group()),
        ] {
            if on {
                slot.button.add_css_class(cls);
            } else {
                slot.button.remove_css_class(cls);
            }
        }
        let tip = if b.is_group() {
            format!("{} — {} windows", b.label, b.count())
        } else if b.minimized {
            format!("{} (minimised)", b.label)
        } else {
            b.label.clone()
        };
        slot.button
            .update_property(&[gtk4::accessible::Property::Label(&tip)]);
        if !b.show_label {
            slot.button.set_tooltip_text(Some(&tip));
        } else {
            slot.button.set_tooltip_text(None);
        }
        // Running bar (classic bar: short bar for every running app, long
        // for the active one; floating: active only). Width springs between
        // states; no allocation per frame.
        let has_underline = self.ctx.tokens.metrics.underline_height > 0;
        let bar = self.pcfg.style == glaze_common::config::PanelStyle::ClassicBar;
        let w = slot.w.get().max(b.width as f64);
        let target = if !has_underline || b.windows.is_empty() {
            0.0
        } else if b.active {
            (w - if bar { 10.0 } else { 8.0 }).max(0.0)
        } else if bar {
            18.0
        } else {
            0.0
        };
        if target > 0.0 || slot.ul_w.get() > 0.5 {
            slot.underline.set_visible(true);
            let (ul, sl) = (slot.underline.clone(), Rc::downgrade(slot));
            let _ = was_active;
            self.anim.animate(
                &format!("{}/u", b.key),
                slot.ul_w.get(),
                target,
                self.ctx.tokens.metrics.motion_micro_ms as f64 + 80.0,
                move |v| {
                    if let Some(sl) = sl.upgrade() {
                        sl.ul_w.set(v);
                        ul.set_size_request(v.round().max(0.0) as i32, -1);
                        ul.set_visible(v > 0.5);
                    }
                },
                None,
            );
        }
    }

    fn primary_click(&self, b: &TaskButton) {
        let Some(backend) = self.ctx.backend.clone() else {
            eprintln!(
                "[glaze-shell] no compositor backend: cannot act on {}",
                b.key
            );
            return;
        };
        let action = bar::click_action(b);
        let result = match &action {
            ClickAction::Launch(id) => {
                if let Some(app) = self.ctx.registry.get(id) {
                    launch(app);
                }
                Ok(())
            }
            ClickAction::Focus(id) => backend.activate_window(id),
            ClickAction::Minimize(ids) => ids.iter().try_for_each(|id| backend.minimize_window(id)),
            ClickAction::Restore(ids) => ids
                .iter()
                .rev()
                .try_for_each(|id| backend.restore_window(id)),
        };
        match result {
            Ok(()) => eprintln!("[glaze-shell] task {}: {:?}", b.key, action),
            Err(e) => eprintln!("[glaze-shell] task {}: {:?} failed: {e}", b.key, action),
        }
        (self.ctx.refresh)();
    }

    fn context_menu(&self, slot: &Slot) {
        use crate::menu::Entry;
        let b = slot.model.borrow().clone();
        let app = bar::find_app(&b.app_id, self.ctx.registry.apps()).cloned();
        if b.windows.is_empty() {
            // A pinned launcher: open it or unpin it.
            let Some(app) = app else { return };
            let (a2, ctx, id) = (app.clone(), self.ctx.clone(), app.id.clone());
            let v = vec![
                Entry::Header(app.name.clone()),
                Entry::item(Some("start"), "Open", move || launch(&a2)),
                Entry::Separator,
                Entry::item(Some("unpin"), "Unpin from taskbar", move || {
                    (ctx.set_pins)(bar::toggle_pin(&ctx.pins(), &id))
                }),
            ];
            let rect = self.slot_rect(slot);
            let pop = crate::menu::popup(
                &self.root,
                Some(rect),
                popover_position(self.pcfg.position),
                &self.ctx,
                v,
            );
            self.menu_open.set(true);
            let open = self.menu_open.clone();
            pop.connect_closed(move |_| open.set(false));
            return;
        }
        let ids: Vec<String> = b.windows.iter().map(|w| w.id.clone()).collect();
        let group = ids.len() > 1;
        let refresh = self.ctx.refresh.clone();
        let be = self.ctx.backend.clone();
        // Run `f` on every window of the button, then refresh.
        let each = |f: WindowOp| {
            let (be, ids, r) = (be.clone(), ids.clone(), refresh.clone());
            move || {
                if let Some(be) = &be {
                    for id in &ids {
                        if let Err(e) = f(be.as_ref(), id) {
                            eprintln!("[glaze-shell] window {id}: {e}");
                        }
                    }
                }
                r();
            }
        };
        let mut v = vec![Entry::Header(if group {
            format!(
                "{} \u{2014} {} windows",
                bar::app_name(&b.app_id, self.ctx.registry.apps()),
                ids.len()
            )
        } else {
            b.label.clone()
        })];
        let has_be = be.is_some();
        if b.minimized {
            v.push(
                Entry::item(
                    Some("window-restore"),
                    if group { "Restore all" } else { "Restore" },
                    each(Rc::new(|be, id| be.restore_window(id))),
                )
                .enabled_if(has_be),
            );
        } else {
            v.push(
                Entry::item(
                    Some("window-minimise"),
                    if group { "Minimise all" } else { "Minimise" },
                    each(Rc::new(|be, id| be.minimize_window(id))),
                )
                .accel("Super+H")
                .enabled_if(has_be),
            );
        }
        if !group {
            let id = ids[0].clone();
            let r = refresh.clone();
            let be2 = be.clone();
            v.push(
                Entry::item(Some("window-maximise"), "Maximise / restore", move || {
                    if !snap(&id, "toggle-maximise") {
                        if let Some(be) = &be2 {
                            let _ = be.toggle_maximize(&id);
                        }
                    }
                    r();
                })
                .accel("Super+Up")
                .enabled_if(has_be && !b.minimized),
            );
            let snaps: [(&str, &str, &str); 10] = [
                ("Left half", "left", "Super+Left"),
                ("Right half", "right", "Super+Right"),
                ("Top half", "top", "Super+Shift+Up"),
                ("Bottom half", "bottom", "Super+Shift+Down"),
                ("Top left quarter", "topleft", "Super+Shift+Left"),
                ("Top right quarter", "topright", "Super+Shift+Right"),
                ("Bottom left quarter", "bottomleft", "Super+Ctrl+Left"),
                ("Bottom right quarter", "bottomright", "Super+Ctrl+Right"),
                ("Centre", "center", "Super+C"),
                ("Restore size", "restore", "Super+Down"),
            ];
            let items: Vec<Entry> = snaps
                .iter()
                .map(|(label, mode, key)| {
                    let (id, mode, r) = (ids[0].clone(), mode.to_string(), refresh.clone());
                    Entry::item(None, *label, move || {
                        snap(&id, &mode);
                        r();
                    })
                    .accel(*key)
                })
                .collect();
            if !b.minimized && has_be {
                v.push(Entry::sub(Some("layout-tile"), "Snap", items));
            }
        }
        let ws: Vec<Entry> = (1..=4u32)
            .map(|n| {
                Entry::item(
                    None,
                    format!("Workspace {n}"),
                    each(Rc::new(move |be, id| be.move_to_workspace(id, n))),
                )
                .accel(format!("Super+Shift+{n}"))
            })
            .collect();
        v.push(Entry::sub(Some("layout-tile"), "Move to workspace", ws));
        v.push(Entry::Separator);
        if let Some(app) = app {
            let a2 = app.clone();
            v.push(Entry::item(Some("start"), "New window", move || {
                launch(&a2)
            }));
            let ctx = self.ctx.clone();
            let id = app.id.clone();
            let pinned = self.ctx.pins().contains(&id);
            v.push(Entry::item(
                Some(if pinned { "unpin" } else { "pin" }),
                if pinned {
                    "Unpin from taskbar"
                } else {
                    "Pin to taskbar"
                },
                move || (ctx.set_pins)(bar::toggle_pin(&ctx.pins(), &id)),
            ));
            v.push(Entry::Separator);
        }
        v.push(
            Entry::item(
                Some("window-close"),
                if group {
                    format!("Close all {} windows", ids.len())
                } else {
                    "Close window".to_string()
                },
                each(Rc::new(|be, id| be.close_window(id))),
            )
            .accel(if group { "" } else { "Super+Shift+Q" })
            .danger()
            .enabled_if(has_be),
        );
        let rect = self.slot_rect(slot);
        let pop = crate::menu::popup(
            &self.root,
            Some(rect),
            popover_position(self.pcfg.position),
            &self.ctx,
            v,
        );
        self.menu_open.set(true);
        let open = self.menu_open.clone();
        pop.connect_closed(move |_| open.set(false));
    }

    fn slot_rect(&self, slot: &Slot) -> gdk::Rectangle {
        // The button's real bounds in the strip (robust against margins and
        // in-flight slide animations).
        let bounds = if slot.button.parent().is_some() && self.root.is_mapped() {
            slot.button.compute_bounds(&self.root)
        } else {
            None
        };
        if let Some(b) = bounds {
            return gdk::Rectangle::new(
                b.x().round() as i32,
                b.y().round() as i32,
                (b.width().round() as i32).max(1),
                (b.height().round() as i32).max(1),
            );
        }
        let (x, w) = (slot.x.get() as i32, slot.w.get() as i32);
        let off = self.fixed.margin_start();
        let y = (self.root.height() - self.height) / 2;
        gdk::Rectangle::new(x + off, y.max(0), w.max(1), self.height)
    }

    fn point_at(&self, pop: &gtk4::Popover, slot: &Slot) {
        pop.set_pointing_to(Some(&self.slot_rect(slot)));
    }

    /// Open (or close) the group's window chooser right away.
    fn toggle_chooser(&self, slot: &Rc<Slot>) {
        self.cancel_card_timer();
        let b = slot.model.borrow().clone();
        let open_for = self.card_for.borrow().clone();
        let same = self.card.is_visible() && open_for.as_deref() == Some(b.key.as_str());
        if same && self.card_pinned.get() {
            self.hide_card_now();
            return;
        }
        self.card_pinned.set(true);
        eprintln!(
            "[glaze-shell] window chooser: {} ({} windows)",
            b.key,
            b.count()
        );
        if same {
            return;
        }
        self.fill_card(&b);
        *self.card_for.borrow_mut() = Some(b.key.clone());
        self.point_at(&self.card, slot);
        // Pop up after the click has fully finished (a popup mapped while
        // the button still holds the implicit pointer grab was dropped).
        let s = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(30), move || {
            if !s.card.is_visible() {
                animate_popup(&s.card, &s.ctx);
            }
        });
    }

    fn cancel_card_timer(&self) {
        if let Some(id) = self.card_timer.borrow_mut().take() {
            id.remove();
        }
    }

    fn schedule_card(&self, slot: &Rc<Slot>) {
        self.cancel_card_timer();
        // The window list only helps for several windows; one window or a
        // launcher just shows its tooltip.
        if self.menu_open.get() || slot.model.borrow().count() < 2 {
            return;
        }
        let delay = if self.card.is_visible() { 80 } else { 450 };
        let s = self.clone();
        let slot = slot.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            s.card_timer.borrow_mut().take();
            if slot.removing.get() {
                return;
            }
            let b = slot.model.borrow().clone();
            s.card_pinned.set(false);
            s.fill_card(&b);
            *s.card_for.borrow_mut() = Some(b.key.clone());
            s.point_at(&s.card, &slot);
            if !s.card.is_visible() {
                animate_popup(&s.card, &s.ctx);
            }
        });
        *self.card_timer.borrow_mut() = Some(id);
    }

    fn schedule_card_hide(&self) {
        self.cancel_card_timer();
        let s = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
            s.card_timer.borrow_mut().take();
            s.hide_card_now();
        });
        *self.card_timer.borrow_mut() = Some(id);
    }

    fn hide_card_now(&self) {
        self.cancel_card_timer();
        if self.card.is_visible() {
            self.card.popdown();
        }
        self.card_for.borrow_mut().take();
    }

    /// Hover card: app name, window count and one paper row per window
    /// (title, minimised state, close). Titles only: live thumbnails need
    /// screencopy and are not faked.
    fn fill_card(&self, b: &TaskButton) {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let head = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let name = gtk4::Label::new(Some(&bar::app_name(&b.app_id, self.ctx.registry.apps())));
        name.add_css_class("glaze-card-title");
        head.append(&name);
        let sub = gtk4::Label::new(Some(&if b.count() == 1 {
            "1 window".to_string()
        } else {
            format!("{} windows", b.count())
        }));
        sub.add_css_class("glaze-card-sub");
        head.append(&sub);
        root.append(&head);
        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        for w in &b.windows {
            let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            let row = gtk4::Button::new();
            row.add_css_class("glaze-card-row");
            row.set_hexpand(true);
            if w.focused {
                row.add_css_class("glaze-active");
            }
            let text = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
            let t = gtk4::Label::new(Some(&w.title));
            t.set_xalign(0.0);
            t.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            t.set_max_width_chars(36);
            text.append(&t);
            if w.minimized {
                let m = gtk4::Label::new(Some("minimised"));
                m.add_css_class("glaze-card-sub");
                m.set_xalign(0.0);
                text.append(&m);
            }
            row.set_child(Some(&text));
            {
                let s = self.clone();
                let id = w.id.clone();
                let minimized = w.minimized;
                row.connect_clicked(move |_| {
                    s.hide_card_now();
                    if let Some(be) = &s.ctx.backend {
                        let r = if minimized {
                            be.restore_window(&id)
                        } else {
                            be.activate_window(&id)
                        };
                        if let Err(e) = r {
                            eprintln!("[glaze-shell] hover card {id}: {e}");
                        }
                    }
                    (s.ctx.refresh)();
                });
            }
            line.append(&row);
            // Minimise / restore per window: GTK apps' own title-bar
            // minimise does nothing on Sway (it ignores set_minimized).
            let mini = gtk4::Button::new();
            mini.add_css_class("glaze-card-close");
            mini.set_valign(gtk4::Align::Center);
            mini.set_tooltip_text(Some(if w.minimized {
                "Restore window"
            } else {
                "Minimise window"
            }));
            mini.set_child(Some(&icons::icon(
                if w.minimized {
                    "window-restore"
                } else {
                    "window-minimise"
                },
                Glyph::Window,
                14,
                self.ctx.ink(),
                self.ctx.stroke(),
            )));
            {
                let s = self.clone();
                let id = w.id.clone();
                let minimized = w.minimized;
                mini.connect_clicked(move |_| {
                    if let Some(be) = &s.ctx.backend {
                        let r = if minimized {
                            be.restore_window(&id)
                        } else {
                            be.minimize_window(&id)
                        };
                        if let Err(e) = r {
                            eprintln!("[glaze-shell] hover card {id}: {e}");
                        }
                    }
                    (s.ctx.refresh)();
                });
            }
            line.append(&mini);
            let close = gtk4::Button::new();
            close.add_css_class("glaze-card-close");
            close.set_valign(gtk4::Align::Center);
            close.set_tooltip_text(Some("Close window"));
            close.set_child(Some(&icons::icon(
                "window-close",
                Glyph::Close,
                14,
                self.ctx.ink(),
                self.ctx.stroke(),
            )));
            {
                let s = self.clone();
                let id = w.id.clone();
                close.connect_clicked(move |_| {
                    if let Some(be) = &s.ctx.backend {
                        if let Err(e) = be.close_window(&id) {
                            eprintln!("[glaze-shell] close {id}: {e}");
                        }
                    }
                    (s.ctx.refresh)();
                });
            }
            line.append(&close);
            rows.append(&line);
        }
        root.append(&rows);
        if b.is_group() {
            let foot = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            foot.set_halign(gtk4::Align::End);
            let ids: Vec<String> = b.windows.iter().map(|w| w.id.clone()).collect();
            let all_min = b.minimized;
            {
                let (s, ids) = (self.clone(), ids.clone());
                foot.append(&menu_item(
                    if all_min {
                        "Restore all"
                    } else {
                        "Minimise all"
                    },
                    move || {
                        s.hide_card_now();
                        if let Some(be) = &s.ctx.backend {
                            for id in &ids {
                                let r = if all_min {
                                    be.restore_window(id)
                                } else {
                                    be.minimize_window(id)
                                };
                                if let Err(e) = r {
                                    eprintln!("[glaze-shell] group {id}: {e}");
                                }
                            }
                        }
                        (s.ctx.refresh)();
                    },
                ));
            }
            {
                let s = self.clone();
                let close = menu_item("Close all", move || {
                    s.hide_card_now();
                    if let Some(be) = &s.ctx.backend {
                        for id in &ids {
                            if let Err(e) = be.close_window(id) {
                                eprintln!("[glaze-shell] close {id}: {e}");
                            }
                        }
                    }
                    (s.ctx.refresh)();
                });
                close.add_css_class("glaze-danger");
                foot.append(&close);
            }
            root.append(&foot);
        }
        self.card.set_child(Some(&root));
    }
}

// ---------------------------------------------------------------------------
// Popover helpers
// ---------------------------------------------------------------------------

pub fn popover_position(position: PanelPosition) -> gtk4::PositionType {
    match position {
        PanelPosition::Bottom => gtk4::PositionType::Top,
        PanelPosition::Top => gtk4::PositionType::Bottom,
        PanelPosition::Left => gtk4::PositionType::Right,
        PanelPosition::Right => gtk4::PositionType::Left,
    }
}

pub fn menu_item(label: &str, on: impl Fn() + 'static) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.add_css_class("glaze-menu-item");
    let l = gtk4::Label::new(Some(label));
    l.set_xalign(0.0);
    b.set_child(Some(&l));
    b.connect_clicked(move |_| on());
    b
}

/// Pop up with a short fade (frame-clock spring; instant under reduced motion).
pub fn animate_popup(pop: &gtk4::Popover, ctx: &Ctx) {
    let Some(child) = pop.child() else {
        pop.popup();
        return;
    };
    child.set_opacity(0.0);
    pop.popup();
    let host = pop.parent().unwrap_or_else(|| child.clone());
    let anim = Animator::new("popover", &host, ctx.motion, ctx.cfg.motion.log_frame_times);
    let c = child.clone();
    anim.animate(
        "o",
        0.0,
        1.0,
        ctx.tokens.metrics.motion_menu_ms as f64,
        move |v| c.set_opacity(v),
        None,
    );
}

// ---------------------------------------------------------------------------
// Float/tile toggle
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct LayoutToggle {
    root: gtk4::Box,
    float_b: gtk4::Button,
    tile_b: gtk4::Button,
}

impl LayoutToggle {
    fn new(ctx: &Ctx, orient: gtk4::Orientation) -> Self {
        let root = gtk4::Box::new(orient, 0);
        root.add_css_class("glaze-segmented");
        root.set_valign(gtk4::Align::Center);
        root.set_halign(gtk4::Align::Center);
        let mk = |name: &str, glyph: Glyph, tip: &str| {
            let b = gtk4::Button::new();
            b.add_css_class("glaze-segment");
            b.set_tooltip_text(Some(tip));
            b.set_child(Some(&icons::icon(name, glyph, 16, ctx.ink(), ctx.stroke())));
            b
        };
        let float_b = mk("layout-float", Glyph::Float, "Floating windows (stacking)");
        let tile_b = mk("layout-tile", Glyph::Tile, "Tiling");
        root.append(&float_b);
        root.append(&tile_b);
        let scope = match ctx.cfg.panel.layout_scope {
            CfgLayoutScope::Workspace => LayoutScope::FocusedWorkspace,
            CfgLayoutScope::Global => LayoutScope::AllWorkspaces,
        };
        let t = LayoutToggle {
            root,
            float_b: float_b.clone(),
            tile_b: tile_b.clone(),
        };
        if ctx.backend.is_none() {
            t.root.set_sensitive(false);
            t.root
                .set_tooltip_text(Some("No compositor backend: float/tile unavailable"));
        }
        for (b, mode) in [(float_b, LayoutMode::Float), (tile_b, LayoutMode::Tile)] {
            let ctx = ctx.clone();
            let t2 = t.clone();
            b.connect_clicked(move |_| {
                if let Some(be) = &ctx.backend {
                    match be.set_layout_mode(mode, scope) {
                        Ok(()) => {
                            eprintln!("[glaze-shell] layout: {} ({scope:?})", mode.slug());
                            t2.set_mode(Some(mode));
                        }
                        Err(e) => eprintln!("[glaze-shell] layout {} failed: {e}", mode.slug()),
                    }
                }
                (ctx.refresh)();
            });
        }
        t
    }

    fn set_mode(&self, mode: Option<LayoutMode>) {
        for (b, m) in [
            (&self.float_b, LayoutMode::Float),
            (&self.tile_b, LayoutMode::Tile),
        ] {
            if mode == Some(m) {
                b.add_css_class("glaze-selected");
            } else {
                b.remove_css_class("glaze-selected");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tray
// ---------------------------------------------------------------------------

struct Tray {
    root: gtk4::Box,
    sni: gtk4::Box,
    bell: Option<gtk4::MenuButton>,
    bell_list: gtk4::Box,
    ctx: Ctx,
    unread: Cell<usize>,
}

impl Tray {
    fn new(ctx: &Ctx, pcfg: &PanelConfig, orient: gtk4::Orientation) -> Self {
        let root = gtk4::Box::new(orient, 1);
        root.add_css_class("glaze-tray");
        root.set_valign(gtk4::Align::Center);
        root.set_halign(gtk4::Align::Center);
        let sni = gtk4::Box::new(orient, 2);
        let bell_list = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let mut bell = None;
        for item in &pcfg.tray_items {
            match item {
                TrayItem::StatusNotifier => root.append(&sni),
                TrayItem::Network => {
                    let (name, tip) = network_state();
                    root.append(&tray_button(ctx, name, &tip));
                }
                TrayItem::Volume => {
                    if let Some((name, tip)) = volume_state() {
                        root.append(&tray_button(ctx, name, &tip));
                    }
                }
                TrayItem::Battery => {
                    if let Some((name, tip)) = battery_state() {
                        root.append(&tray_button(ctx, name, &tip));
                    }
                }
                TrayItem::Notifications => {
                    let mb = gtk4::MenuButton::new();
                    mb.add_css_class("glaze-tray-button");
                    mb.set_has_frame(false);
                    mb.set_always_show_arrow(false);
                    mb.set_tooltip_text(Some("Notifications"));
                    mb.set_child(Some(&icons::icon(
                        "bell",
                        Glyph::Bell,
                        18,
                        ctx.ink(),
                        ctx.stroke(),
                    )));
                    let pop = gtk4::Popover::new();
                    pop.add_css_class("glaze-popover");
                    pop.set_has_arrow(false);
                    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
                    let title = gtk4::Label::new(Some("Notifications"));
                    title.add_css_class("glaze-card-title");
                    title.set_xalign(0.0);
                    content.append(&title);
                    let sc = gtk4::ScrolledWindow::new();
                    sc.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
                    sc.set_min_content_height(120);
                    sc.set_min_content_width(280);
                    sc.set_child(Some(&bell_list));
                    content.append(&sc);
                    let f = ctx.on_mark_all_read.clone();
                    content.append(&menu_item("Mark all read", move || f()));
                    pop.set_child(Some(&content));
                    mb.set_popover(Some(&pop));
                    root.append(&mb);
                    bell = Some(mb);
                }
            }
        }
        let empty = gtk4::Label::new(Some("No notifications"));
        empty.add_css_class("glaze-card-sub");
        bell_list.append(&empty);
        Tray {
            root,
            sni,
            bell,
            bell_list,
            ctx: ctx.clone(),
            unread: Cell::new(0),
        }
    }

    fn apply_status(&self, snapshot: &glaze_shell_core::status_feed::StatusSnapshot) {
        while let Some(c) = self.sni.first_child() {
            self.sni.remove(&c);
        }
        for item in snapshot.tray.iter() {
            let img = gtk4::Image::from_icon_name(item.icon.as_deref().unwrap_or("image-missing"));
            img.set_pixel_size(16);
            img.set_tooltip_text(Some(&item.title));
            self.sni.append(&img);
        }
        let unread = snapshot.badge.unread_count();
        if let Some(mb) = &self.bell {
            if unread != self.unread.get() {
                self.unread.set(unread);
                let icon = if unread > 0 {
                    icons::icon_with_accent(
                        "bell-dot",
                        Glyph::Bell,
                        18,
                        self.ctx.ink(),
                        self.ctx.tokens.palette.accent,
                        self.ctx.stroke(),
                    )
                } else {
                    icons::icon("bell", Glyph::Bell, 18, self.ctx.ink(), self.ctx.stroke())
                };
                mb.set_child(Some(&icon));
                mb.set_tooltip_text(Some(&if unread > 0 {
                    format!("{unread} unread notification(s)")
                } else {
                    "Notifications".into()
                }));
            }
        }
        while let Some(c) = self.bell_list.first_child() {
            self.bell_list.remove(&c);
        }
        if snapshot.notifications.is_empty() {
            let empty = gtk4::Label::new(Some("No notifications"));
            empty.add_css_class("glaze-card-sub");
            self.bell_list.append(&empty);
        }
        for n in snapshot.notifications.iter().rev() {
            let row = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
            let s = gtk4::Label::new(Some(&n.summary));
            s.set_xalign(0.0);
            s.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let a = gtk4::Label::new(Some(&n.app_name));
            a.add_css_class("glaze-card-sub");
            a.set_xalign(0.0);
            row.append(&s);
            row.append(&a);
            self.bell_list.append(&row);
        }
    }
}

fn tray_button(ctx: &Ctx, name: &str, tip: &str) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.add_css_class("glaze-tray-button");
    b.set_tooltip_text(Some(tip));
    b.set_child(Some(&icons::icon(
        name,
        icons::fallback_for(name),
        18,
        ctx.ink(),
        ctx.stroke(),
    )));
    b
}

/// Real network state from sysfs (no NetworkManager dependency).
fn network_state() -> (&'static str, String) {
    let mut up = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/net") {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "lo" {
                continue;
            }
            let state = std::fs::read_to_string(e.path().join("operstate")).unwrap_or_default();
            if state.trim() == "up" {
                up.push((name, e.path().join("wireless").exists()));
            }
        }
    }
    match up.first() {
        Some((n, true)) => ("network-wifi", format!("Connected ({n})")),
        Some((n, false)) => ("network-wired", format!("Connected ({n}, wired)")),
        None => ("network-off", "Not connected".into()),
    }
}

/// Battery from sysfs; `None` when the machine has no battery (the VM).
fn battery_state() -> Option<(&'static str, String)> {
    let rd = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for e in rd.flatten() {
        let ty = std::fs::read_to_string(e.path().join("type")).unwrap_or_default();
        if ty.trim() != "Battery" {
            continue;
        }
        let cap: u32 = std::fs::read_to_string(e.path().join("capacity"))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let status = std::fs::read_to_string(e.path().join("status")).unwrap_or_default();
        let name = if status.trim() == "Charging" {
            "battery-charging"
        } else if cap > 80 {
            "battery-full"
        } else if cap > 40 {
            "battery-medium"
        } else if cap > 10 {
            "battery-low"
        } else {
            "battery-empty"
        };
        return Some((name, format!("Battery {cap}% ({})", status.trim())));
    }
    None
}

/// Volume via `wpctl` (PipeWire); `None` when there is no audio stack.
fn volume_state() -> Option<(&'static str, String)> {
    let out = std::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let v: f64 = s.split_whitespace().nth(1)?.parse().ok()?;
    let muted = s.contains("MUTED");
    let name = if muted || v <= 0.0 {
        "volume-mute"
    } else if v < 0.34 {
        "volume-low"
    } else if v < 0.67 {
        "volume-medium"
    } else {
        "volume-high"
    };
    Some((
        name,
        format!(
            "Volume {:.0}%{}",
            v * 100.0,
            if muted { " (muted)" } else { "" }
        ),
    ))
}

// ---------------------------------------------------------------------------
// Clock and show desktop
// ---------------------------------------------------------------------------

/// The current local time as [`ClockParts`].
pub fn now_parts() -> ClockParts {
    match glib::DateTime::now_local() {
        Ok(now) => ClockParts {
            year: now.year(),
            month: now.month(),
            day: now.day_of_month(),
            hour: now.hour(),
            minute: now.minute(),
            second: now.second(),
            weekday: Weekday::from_iso(now.day_of_week()).unwrap_or(Weekday::Monday),
        },
        Err(_) => ClockParts::new(1970, 1, 1, 0, 0, 0),
    }
}

fn clock(pcfg: &PanelConfig, vertical: bool) -> gtk4::MenuButton {
    let button = gtk4::MenuButton::new();
    button.add_css_class("glaze-clock");
    button.set_has_frame(false);
    button.set_always_show_arrow(false);
    button.set_valign(gtk4::Align::Center);
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    col.set_valign(gtk4::Align::Center);
    let time = gtk4::Label::new(None);
    time.add_css_class("glaze-clock-time");
    time.set_xalign(1.0);
    let date = gtk4::Label::new(None);
    date.add_css_class("glaze-clock-date");
    date.set_xalign(1.0);
    col.append(&time);
    if pcfg.show_date && !vertical {
        col.append(&date);
    }
    button.set_child(Some(&col));
    let cal = gtk4::Calendar::new();
    let pop = gtk4::Popover::new();
    pop.add_css_class("glaze-popover");
    pop.set_has_arrow(false);
    pop.set_child(Some(&cal));
    button.set_popover(Some(&pop));
    let pcfg = pcfg.clone();
    let tip_target = button.clone();
    let last = RefCell::new(String::new());
    let tick = move || {
        let p = now_parts();
        let t = format_time(&p, pcfg.clock_24h, pcfg.show_seconds);
        // Only touch the widgets when the text changes (once a minute
        // without seconds): no per-second relayout/redraw of the bar.
        if *last.borrow() == t {
            return ControlFlow::Continue;
        }
        *last.borrow_mut() = t.clone();
        tip_target.set_tooltip_text(Some(&format_date(&p, "%A %d %b %Y")));
        time.set_text(&t);
        let fmt = pcfg
            .date_format
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "%a %d %b".into());
        date.set_text(&format_date(&p, &fmt));
        ControlFlow::Continue
    };
    tick();
    glib::timeout_add_seconds_local(1, tick);
    button
}

fn show_desktop(ctx: &Ctx) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.add_css_class("glaze-show-desktop");
    b.set_tooltip_text(Some("Show desktop"));
    let hidden: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let ctx = ctx.clone();
    b.connect_clicked(move |_| {
        let Some(be) = &ctx.backend else { return };
        let mut h = hidden.borrow_mut();
        if h.is_empty() {
            if let Ok(ws) = be.windows() {
                for w in ws.iter().filter(|w| !w.minimized && w.workspace.is_some()) {
                    if be.minimize_window(&w.id).is_ok() {
                        h.push(w.id.clone());
                    }
                }
            }
        } else {
            for id in h.drain(..).rev() {
                let _ = be.restore_window(&id);
            }
        }
        (ctx.refresh)();
    });
    b
}

/// Grouping mode in use (for logs).
pub fn grouping_slug(g: GroupingMode) -> &'static str {
    match g {
        GroupingMode::Always => "always",
        GroupingMode::Auto => "auto",
        GroupingMode::Never => "never",
    }
}
