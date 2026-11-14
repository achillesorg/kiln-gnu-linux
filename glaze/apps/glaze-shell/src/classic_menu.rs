//! Classic start menu: the Windows 2000 structure, modernised in the Kiln
//! paper style (`launcher.style = "classic"`, the default).
//!
//! ```text
//! +--+-----------------------------+
//! |K | [search apps.............]  |
//! |i | Programs                 >  |  -> categories > apps (cascading)
//! |l | Documents                >  |  -> recently used files
//! |n | Settings                 >  |
//! |  | Search                      |
//! |G | Help and About              |
//! |N | Run...                      |
//! |U | --------------------------- |
//! |  | Log Off glaze...             |
//! |^^| Shut Down...                |
//! +--+-----------------------------+
//! ```
//!
//! The vertical strip carries an original arches mark and a "Kiln GNU/Linux"
//! wordmark. Submenus cascade on hover after a short delay and fade in.
//! Keyboard: typing filters apps in the search field (type-ahead), Up/Down
//! move, Right/Enter open a submenu, Left closes it, Enter activates, Esc
//! closes the deepest level (then clears the search, then the menu). No web
//! search, no ads, no telemetry: everything listed is local.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use glaze_common::config::PanelPosition;
use glaze_common::model::AppEntry;
use glaze_service::search::search_apps;
use glaze_shell_core::startmenu::{self, RecentApps};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::anim::Animator;
use crate::icons;
use crate::ink::Glyph;
use crate::panel_ui::{self, Ctx};

/// Builds a submenu's rows when it opens.
type SubFn = Rc<dyn Fn(&Rc<Classic>) -> Vec<RowSpec>>;

/// What a row does.
#[derive(Clone)]
enum Act {
    Launch(String),
    Open(String),
    Sub(SubFn),
    Panel(&'static str),
    FocusSearch,
    Disabled,
}

#[derive(Clone)]
struct RowSpec {
    label: String,
    icon: RowIcon,
    act: Act,
    separator_before: bool,
}

#[derive(Clone)]
enum RowIcon {
    Ink(&'static str),
    App(String),
    None,
}

struct Level {
    rows: Vec<(gtk4::Button, RowSpec)>,
    sel: Cell<usize>,
    /// The cascade column of this level (None for the root).
    col: Option<gtk4::Widget>,
}

pub struct Classic {
    pop: gtk4::Popover,
    /// The root paper card (strip + column).
    card: gtk4::Box,
    /// Holds the root card and the cascade columns side by side, all in one
    /// popover surface (no nested popups: Wayland grabs made those close
    /// the parent).
    outer: gtk4::Box,
    entry: gtk4::Entry,
    stack: gtk4::Stack,
    menu_box: gtk4::Box,
    results_box: gtk4::Box,
    levels: RefCell<Vec<Level>>,
    ctx: Ctx,
    anim: Animator,
    hover_timer: RefCell<Option<glib::SourceId>>,
    closing: Cell<bool>,
    recent: RefCell<RecentApps>,
    position: PanelPosition,
    /// (depth, row) whose submenu is currently open.
    open_sub_index: Cell<Option<(usize, usize)>>,
    /// Restores the panel's keyboard mode when the menu closes.
    kbd_restore: RefCell<Option<Box<dyn FnOnce()>>>,
    /// Draws the root card shifted for the open/close slide.
    slide: crate::slide::SlideBin,
    /// The card's item column (fades a beat behind the panel).
    column: gtk4::Box,
    /// Slide distance of the running motion (28 px open, 12 px close).
    travel: Cell<f64>,
    /// When the popover last closed (Start-click toggle).
    closed_at: Cell<Option<std::time::Instant>>,
}

/// Wire the classic menu to the Start button.
pub fn attach(button: &gtk4::Button, ctx: &Ctx, position: PanelPosition) {
    let pop = gtk4::Popover::new();
    pop.add_css_class("glaze-popover");
    pop.add_css_class("glaze-classic-menu");
    pop.set_has_arrow(false);
    pop.set_autohide(true);
    pop.set_parent(button);
    pop.set_position(panel_ui::popover_position(position));
    pop.set_halign(gtk4::Align::Start);
    pop.set_offset(
        0,
        if position == PanelPosition::Bottom {
            -4
        } else {
            4
        },
    );

    let outer = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    outer.set_valign(gtk4::Align::End);
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("glaze-menu-card");
    card.set_valign(gtk4::Align::End);
    if ctx.cfg.launcher.show_strip {
        card.append(&brand_strip(ctx));
    }
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    col.add_css_class("glaze-menu-column");
    col.set_size_request(250, -1);

    let search_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    search_row.add_css_class("glaze-search");
    let glyph = icons::icon(
        "search",
        Glyph::Search,
        16,
        ctx.tokens.palette.text_secondary,
        ctx.stroke(),
    );
    glyph.set_margin_start(8);
    search_row.append(&glyph);
    let entry = gtk4::Entry::new();
    entry.add_css_class("glaze-search-entry");
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("Search apps"));
    search_row.append(&entry);
    col.append(&search_row);

    let stack = gtk4::Stack::new();
    // Same height for menu, results and panels: the card never jumps.
    stack.set_vhomogeneous(true);
    stack.set_hhomogeneous(true);
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_transition_duration(if ctx.motion.instant() { 0 } else { 120 });
    let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    stack.add_named(&menu_box, Some("menu"));
    let results_scroll = gtk4::ScrolledWindow::new();
    results_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    results_scroll.set_vexpand(true);
    let results_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    results_scroll.set_child(Some(&results_box));
    stack.add_named(&results_scroll, Some("results"));
    col.append(&stack);
    card.append(&col);
    // The card sits in a SlideBin: open/close slide it without resizing the
    // popover surface (a margin animation made the popup jump on open).
    let slide = crate::slide::SlideBin::new(&card);
    slide.set_valign(gtk4::Align::End);
    outer.append(&slide);
    pop.set_child(Some(&outer));

    let me = Rc::new(Classic {
        pop: pop.clone(),
        card,
        outer,
        entry: entry.clone(),
        stack,
        menu_box,
        results_box,
        levels: RefCell::new(Vec::new()),
        ctx: ctx.clone(),
        // Driven by the panel surface's frame clock (the popover's own
        // clock stalled on the VM).
        anim: Animator::new(
            "start-menu",
            button,
            ctx.motion,
            ctx.cfg.motion.log_frame_times,
        ),
        hover_timer: RefCell::new(None),
        closing: Cell::new(false),
        recent: RefCell::new(RecentApps::load(&RecentApps::default_path(), 10)),
        position,
        open_sub_index: Cell::new(None),
        kbd_restore: RefCell::new(None),
        closed_at: Cell::new(None),
        slide: slide.clone(),
        column: col.clone(),
        travel: Cell::new(28.0),
    });

    {
        let m = me.clone();
        entry.connect_changed(move |e| m.on_query(&e.text()));
    }
    {
        let key = gtk4::EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let m = me.clone();
        key.connect_key_pressed(move |_, k, _, _| m.on_key(k));
        pop.add_controller(key);
    }
    {
        let m = me.clone();
        let b = button.clone();
        button.connect_clicked(move |_| {
            // A click on Start while the menu is open first dismisses the
            // popup (autohide grab), then arrives here: do not reopen it.
            let just_closed = m
                .closed_at
                .get()
                .is_some_and(|t| t.elapsed() < Duration::from_millis(250));
            if m.pop.is_visible() {
                m.close();
            } else if just_closed {
                m.closed_at.set(None);
            } else {
                *m.kbd_restore.borrow_mut() = Some(crate::menu::hold_keyboard(b.upcast_ref()));
                m.open();
                b.add_css_class("glaze-open");
            }
        });
    }
    {
        let b = button.clone();
        let m = me.clone();
        pop.connect_closed(move |_| {
            m.closed_at.set(Some(std::time::Instant::now()));
            if let Some(r) = m.kbd_restore.borrow_mut().take() {
                r();
            }
            b.remove_css_class("glaze-open");
            m.close_submenus_from(1);
        });
    }
}

/// Vertical brand strip: arches mark and a rotated "Kiln GNU/Linux"
/// wordmark, rendered from an SVG with the token colours.
fn brand_strip(ctx: &Ctx) -> gtk4::Widget {
    let strip = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    strip.add_css_class("glaze-menu-strip");
    strip.set_size_request(30, -1);
    strip.set_vexpand(true);
    let dark_clay = ctx.tokens.scheme == glaze_design::kinds::ColorScheme::Dark
        && ctx.tokens.kind == glaze_design::kinds::ThemeKind::Clay;
    let fg = if dark_clay {
        ctx.tokens.palette.accent_soft_text
    } else {
        ctx.tokens.palette.accent_text
    }
    .to_rgb()
    .to_hex();
    let serif = "Source Serif 4, Noto Serif, DejaVu Serif, serif";
    let h = 230;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="{h}" viewBox="0 0 30 {h}">
<g fill="none" stroke="{fg}" stroke-width="1.6" stroke-linecap="round">
<path d="M8 {b0}V{a0}A7 7 0 0 1 22 {a0}V{b0}"/>
<path d="M12 {b0}V{a1}A3 3 0 0 1 18 {a1}V{b0}"/></g>
<text transform="translate(20.5 {t}) rotate(-90)" font-family="{serif}" fill="{fg}">
<tspan font-size="15" font-weight="700">Kiln</tspan><tspan font-size="11.5" dx="5" opacity="0.85">GNU/Linux</tspan></text>
</svg>"##,
        b0 = h - 10,
        a0 = h - 19,
        a1 = h - 17,
        t = h - 36,
    );
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&strip));
    if let Some(tex) = icons::render_svg_size(&svg, 30, h) {
        let pic = gtk4::Image::from_paintable(Some(&tex));
        pic.set_pixel_size(h);
        pic.set_size_request(30, h);
        pic.set_valign(gtk4::Align::End);
        pic.set_halign(gtk4::Align::Center);
        pic.set_can_target(false);
        overlay.add_overlay(&pic);
        overlay.set_clip_overlay(&pic, true);
    }
    overlay.upcast()
}

impl Classic {
    fn root_rows(&self) -> Vec<RowSpec> {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let mut v = vec![
            RowSpec {
                label: "Programs".into(),
                icon: RowIcon::Ink("apps"),
                act: Act::Sub(Rc::new(|m| m.programs_rows())),
                separator_before: false,
            },
            RowSpec {
                label: "Documents".into(),
                icon: RowIcon::Ink("folder"),
                act: Act::Sub(Rc::new(|_| documents_rows())),
                separator_before: false,
            },
            RowSpec {
                label: "Settings".into(),
                icon: RowIcon::Ink("settings"),
                act: Act::Sub(Rc::new(|m| m.settings_rows())),
                separator_before: false,
            },
            RowSpec {
                label: "Search".into(),
                icon: RowIcon::Ink("search"),
                act: Act::FocusSearch,
                separator_before: false,
            },
            RowSpec {
                label: "Help and About".into(),
                icon: RowIcon::Ink("start"),
                act: Act::Panel("about"),
                separator_before: false,
            },
            RowSpec {
                label: "Run\u{2026}".into(),
                icon: RowIcon::Ink("terminal"),
                act: Act::Panel("run"),
                separator_before: false,
            },
        ];
        v.push(RowSpec {
            label: match display_name(&user) {
                Some(n) => format!("Log Off {n}\u{2026}"),
                None => "Log Off\u{2026}".into(),
            },
            icon: RowIcon::Ink("logout"),
            act: Act::Panel("logoff"),
            separator_before: true,
        });
        v.push(RowSpec {
            label: "Shut Down\u{2026}".into(),
            icon: RowIcon::Ink("power"),
            act: Act::Panel("shutdown"),
            separator_before: false,
        });
        v
    }

    fn programs_rows(&self) -> Vec<RowSpec> {
        let apps = self.ctx.registry.apps();
        let mut v: Vec<RowSpec> = Vec::new();
        for (section, list) in startmenu::grouped(apps) {
            let ids: Vec<String> = list.iter().map(|a| a.id.clone()).collect();
            v.push(RowSpec {
                label: section.to_string(),
                icon: RowIcon::Ink("folder"),
                act: Act::Sub(Rc::new(move |m: &Rc<Classic>| {
                    ids.iter()
                        .filter_map(|id| m.ctx.registry.get(id))
                        .map(app_row)
                        .collect()
                })),
                separator_before: false,
            });
        }
        let pins: Vec<RowSpec> = self
            .ctx
            .pins()
            .iter()
            .filter_map(|id| self.ctx.registry.get(id))
            .map(app_row)
            .collect();
        let recent: Vec<RowSpec> = self
            .recent
            .borrow()
            .installed(apps)
            .into_iter()
            .take(4)
            .map(app_row)
            .collect();
        let mut out = pins;
        for r in recent {
            if !out.iter().any(|p| p.label == r.label) {
                out.push(r);
            }
        }
        if let Some(first) = v.first_mut() {
            first.separator_before = !out.is_empty();
        }
        out.extend(v);
        if out.is_empty() {
            out.push(empty_row());
        }
        out
    }

    fn settings_rows(&self) -> Vec<RowSpec> {
        let mut v = Vec::new();
        if let Some(app) = self.ctx.registry.get("glaze-settings") {
            v.push(RowSpec {
                label: "Glaze Settings".into(),
                icon: RowIcon::Ink("settings"),
                act: Act::Launch(app.id.clone()),
                separator_before: false,
            });
            v.push(RowSpec {
                label: "Taskbar and Start Menu".into(),
                icon: RowIcon::Ink("layout-float"),
                act: Act::Launch(app.id.clone()),
                separator_before: false,
            });
        }
        v.push(RowSpec {
            label: "Configuration folder".into(),
            icon: RowIcon::Ink("folder"),
            act: Act::Open(glaze_common::paths::glaze_config_dir().display().to_string()),
            separator_before: !v.is_empty(),
        });
        v
    }

    fn open(self: &Rc<Self>) {
        let t0 = std::time::Instant::now();
        self.closing.set(false);
        self.entry.set_text("");
        self.show_menu();
        // Start fully "down and transparent", then pop up: the surface has
        // its final size from the first frame.
        self.travel.set(28.0);
        self.apply_progress(0.0);
        self.pop.popup();
        eprintln!(
            "[glaze-shell] start menu: open handled in {:.1} ms",
            t0.elapsed().as_secs_f64() * 1000.0
        );
        self.entry.grab_focus();
        // Windows 11 style: slide up 28 px with a critically damped spring
        // (decelerates, no overshoot) over ~220 ms. One progress value.
        let me = Rc::downgrade(self);
        self.anim.animate(
            "p",
            0.0,
            1.0,
            self.ctx.tokens.metrics.motion_menu_ms as f64 + 40.0,
            move |v| {
                if let Some(m) = me.upgrade() {
                    m.apply_progress(v);
                }
            },
            None,
        );
    }

    /// Open progress 0..1. The panel slides `travel` px from the taskbar
    /// edge and is opaque by 40 %; the items fade in a beat behind it
    /// (staggered), so it never reads as a plain crossfade.
    fn apply_progress(&self, p: f64) {
        let p = p.clamp(0.0, 1.0);
        self.card.set_opacity((p / 0.4).min(1.0));
        self.column.set_opacity(((p - 0.25) / 0.75).clamp(0.0, 1.0));
        let dir = if self.position == PanelPosition::Top { -1.0 } else { 1.0 };
        self.slide.set_offset(dir * self.travel.get() * (1.0 - p));
    }

    fn close(self: &Rc<Self>) {
        if self.closing.replace(true) {
            return;
        }
        self.close_submenus_from(1);
        let pop = self.pop.clone();
        let me = Rc::downgrade(self);
        let me2 = self.clone();
        // Closing: slide down 12 px and fade, faster (~140 ms).
        self.travel.set(12.0);
        self.anim.animate(
            "p",
            1.0,
            0.0,
            self.ctx.tokens.metrics.motion_micro_ms as f64,
            move |v| {
                if let Some(m) = me.upgrade() {
                    m.apply_progress(v);
                }
            },
            Some(Box::new(move || {
                if me2.closing.get() {
                    pop.popdown();
                }
            })),
        );
    }

    fn show_menu(self: &Rc<Self>) {
        self.close_submenus_from(0);
        clear(&self.menu_box);
        let rows = self.root_rows();
        let level = self.build_level(&self.menu_box, rows, 0, self.ctx.cfg.launcher.icon_size);
        *self.levels.borrow_mut() = vec![level];
        self.stack.set_visible_child_name("menu");
    }

    fn build_level(
        self: &Rc<Self>,
        into: &gtk4::Box,
        specs: Vec<RowSpec>,
        depth: usize,
        icon_px: i32,
    ) -> Level {
        let mut rows = Vec::new();
        for spec in specs {
            if spec.separator_before {
                let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
                sep.add_css_class("glaze-menu-sep");
                into.append(&sep);
            }
            let b = self.row_widget(&spec, icon_px);
            let idx = rows.len();
            {
                let m = self.clone();
                b.connect_clicked(move |_| m.activate(depth, idx));
            }
            {
                let m = self.clone();
                let motion = gtk4::EventControllerMotion::new();
                motion.connect_enter(move |_, _, _| m.hover(depth, idx));
                b.add_controller(motion);
            }
            into.append(&b);
            rows.push((b, spec));
        }
        Level {
            rows,
            sel: Cell::new(usize::MAX),
            col: None,
        }
    }

    fn row_widget(&self, spec: &RowSpec, icon_px: i32) -> gtk4::Button {
        let b = gtk4::Button::new();
        b.add_css_class("glaze-menu-row");
        b.set_focus_on_click(false);
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        match &spec.icon {
            RowIcon::Ink(name) => line.append(&icons::icon(
                name,
                icons::fallback_for(name),
                icon_px,
                self.ctx.ink(),
                self.ctx.stroke(),
            )),
            RowIcon::App(id) => {
                let app = self.ctx.registry.get(id).cloned();
                line.append(&panel_ui::app_icon_sized(
                    &self.ctx,
                    id,
                    app.as_ref(),
                    icon_px,
                ));
            }
            RowIcon::None => {
                let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                spacer.set_size_request(icon_px, icon_px);
                line.append(&spacer);
            }
        }
        let l = gtk4::Label::new(Some(&spec.label));
        l.set_xalign(0.0);
        l.set_hexpand(true);
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        l.set_max_width_chars(30);
        line.append(&l);
        if matches!(spec.act, Act::Sub(_)) {
            line.append(&icons::icon(
                "chevron-right",
                Glyph::Window,
                14,
                self.ctx.tokens.palette.text_secondary,
                self.ctx.stroke(),
            ));
        }
        if matches!(spec.act, Act::Disabled) {
            b.set_sensitive(false);
        }
        b.set_child(Some(&line));
        b
    }

    fn select(&self, depth: usize, idx: usize) {
        let levels = self.levels.borrow();
        let Some(level) = levels.get(depth) else {
            return;
        };
        for (i, (b, _)) in level.rows.iter().enumerate() {
            if i == idx {
                b.add_css_class("glaze-selected");
            } else {
                b.remove_css_class("glaze-selected");
            }
        }
        level.sel.set(idx);
    }

    fn cancel_hover(&self) {
        if let Some(id) = self.hover_timer.borrow_mut().take() {
            id.remove();
        }
    }

    fn hover(self: &Rc<Self>, depth: usize, idx: usize) {
        if self.levels.borrow().len() <= depth {
            return;
        }
        self.select(depth, idx);
        self.cancel_hover();
        let m = self.clone();
        // Win2K-style cascade delay: open (or close siblings) after a beat.
        let id = glib::timeout_add_local_once(Duration::from_millis(220), move || {
            m.hover_timer.borrow_mut().take();
            let is_sub = {
                let levels = m.levels.borrow();
                levels
                    .get(depth)
                    .and_then(|l| l.rows.get(idx))
                    .map(|(_, s)| matches!(s.act, Act::Sub(_)))
                    .unwrap_or(false)
            };
            let already = m.open_sub_index.get() == Some((depth, idx));
            if is_sub && !already {
                m.open_sub(depth, idx, false);
            } else if !is_sub {
                m.close_submenus_from(depth + 1);
            }
        });
        *self.hover_timer.borrow_mut() = Some(id);
    }

    fn open_sub(self: &Rc<Self>, depth: usize, idx: usize, select_first: bool) {
        self.close_submenus_from(depth + 1);
        let (anchor, act) = {
            let levels = self.levels.borrow();
            let Some((b, spec)) = levels.get(depth).and_then(|l| l.rows.get(idx)) else {
                return;
            };
            (b.clone(), spec.act.clone())
        };
        let Act::Sub(make) = act else { return };
        let specs = make(self);
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.add_css_class("glaze-menu-card");
        card.add_css_class("glaze-submenu-card");
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_propagate_natural_width(true);
        scroll.set_max_content_height(560);
        scroll.set_child(Some(&body));
        card.append(&scroll);
        let mut level = self.build_level(&body, specs, depth + 1, 18);
        // Align the column's top with the hovered row (Windows 2000 cascade),
        // keeping it on the floor when it would not fit above.
        let outer_h = self.card.height().max(1);
        let row_top = anchor
            .compute_point(&self.outer, &gtk4::graphene::Point::new(0.0, 0.0))
            .map(|p| p.y() as i32)
            .unwrap_or(0);
        let (_, nat_h, _, _) = card.measure(gtk4::Orientation::Vertical, -1);
        let col_h = nat_h.min(560 + 10);
        let outer_now = self.outer.height().max(outer_h);
        let bottom_gap = (outer_now - row_top - col_h + 4).max(0);
        card.set_valign(gtk4::Align::End);
        card.set_margin_bottom(bottom_gap);
        card.set_opacity(0.0);
        self.outer.append(&card);
        level.col = Some(card.clone().upcast());
        self.levels.borrow_mut().push(level);
        self.open_sub_index.set(Some((depth, idx)));
        let c = card.clone();
        let ms = self.ctx.tokens.metrics.motion_menu_ms as f64;
        self.anim.animate(
            &format!("sub{depth}o"),
            0.0,
            1.0,
            ms,
            move |v| c.set_opacity(v),
            None,
        );
        if select_first {
            self.select(depth + 1, 0);
        }
    }

    fn close_submenus_from(&self, depth: usize) {
        let mut levels = self.levels.borrow_mut();
        while levels.len() > depth.max(1) {
            if let Some(l) = levels.pop() {
                if let Some(c) = l.col {
                    self.outer.remove(&c);
                }
            }
        }
        if depth == 0 {
            levels.clear();
        }
        if levels.len() <= 1 {
            self.open_sub_index.set(None);
        }
    }

    fn activate(self: &Rc<Self>, depth: usize, idx: usize) {
        let act = {
            let levels = self.levels.borrow();
            match levels.get(depth).and_then(|l| l.rows.get(idx)) {
                Some((_, s)) => s.act.clone(),
                None => return,
            }
        };
        match act {
            Act::Launch(id) => self.launch(&id),
            Act::Open(path) => {
                open_path(&path);
                self.close();
            }
            Act::Sub(_) => {
                self.cancel_hover();
                self.open_sub(depth, idx, true);
            }
            Act::Panel(name) => {
                self.close_submenus_from(1);
                self.show_panel(name);
            }
            Act::FocusSearch => {
                self.close_submenus_from(1);
                self.entry.grab_focus();
            }
            Act::Disabled => {}
        }
    }

    fn launch(self: &Rc<Self>, id: &str) {
        let Some(app) = self.ctx.registry.get(id).cloned() else {
            return;
        };
        panel_ui::launch(&app);
        let mut r = self.recent.borrow_mut();
        r.push(id);
        if let Err(e) = r.save(&RecentApps::default_path()) {
            eprintln!("[glaze-shell] could not save recent apps: {e}");
        }
        drop(r);
        self.close();
    }

    fn on_query(self: &Rc<Self>, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            self.show_menu();
            return;
        }
        self.close_submenus_from(0);
        clear(&self.results_box);
        let hits = search_apps(self.ctx.registry.apps(), q, 12);
        let specs: Vec<RowSpec> = if hits.is_empty() {
            vec![RowSpec {
                label: format!("No apps match \u{201c}{q}\u{201d}"),
                icon: RowIcon::None,
                act: Act::Disabled,
                separator_before: false,
            }]
        } else {
            hits.iter()
                .filter_map(|h| self.ctx.registry.get(&h.app_id))
                .map(app_row)
                .collect()
        };
        let level = self.build_level(&self.results_box, specs, 0, 20);
        *self.levels.borrow_mut() = vec![level];
        self.select(0, 0);
        self.stack.set_visible_child_name("results");
    }

    fn on_key(self: &Rc<Self>, k: gdk::Key) -> glib::Propagation {
        let depth = self.levels.borrow().len().saturating_sub(1);
        let sel = self
            .levels
            .borrow()
            .get(depth)
            .map(|l| l.sel.get())
            .unwrap_or(usize::MAX);
        let len = self
            .levels
            .borrow()
            .get(depth)
            .map(|l| l.rows.len())
            .unwrap_or(0);
        let enabled = |i: usize| {
            self.levels
                .borrow()
                .get(depth)
                .and_then(|l| l.rows.get(i))
                .map(|(_, s)| !matches!(s.act, Act::Disabled))
                .unwrap_or(false)
        };
        match k {
            gdk::Key::Down | gdk::Key::Up if len > 0 => {
                let down = k == gdk::Key::Down;
                let mut i = if sel == usize::MAX {
                    if down {
                        len - 1
                    } else {
                        0
                    }
                } else {
                    sel
                };
                for _ in 0..len {
                    i = if down {
                        (i + 1) % len
                    } else {
                        (i + len - 1) % len
                    };
                    if enabled(i) {
                        break;
                    }
                }
                self.cancel_hover();
                self.select(depth, i);
                glib::Propagation::Stop
            }
            gdk::Key::Right if sel != usize::MAX => {
                let is_sub = self
                    .levels
                    .borrow()
                    .get(depth)
                    .and_then(|l| l.rows.get(sel))
                    .map(|(_, s)| matches!(s.act, Act::Sub(_)))
                    .unwrap_or(false);
                if is_sub {
                    self.open_sub(depth, sel, true);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
            gdk::Key::Left if depth > 0 => {
                self.close_submenus_from(depth);
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if sel != usize::MAX {
                    self.activate(depth, sel);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                if depth > 0 {
                    self.close_submenus_from(depth);
                } else if self.stack.visible_child_name().as_deref() == Some("panel") {
                    self.show_menu();
                    self.entry.grab_focus();
                } else if !self.entry.text().is_empty() {
                    self.entry.set_text("");
                } else {
                    self.close();
                }
                glib::Propagation::Stop
            }
            _ => {
                // Type-ahead: any printable key goes to the search field.
                if !self.entry.has_focus() {
                    if let Some(c) = k.to_unicode().filter(|c| !c.is_control()) {
                        self.entry.grab_focus();
                        let mut pos = self.entry.text().chars().count() as i32;
                        self.entry.insert_text(&c.to_string(), &mut pos);
                        self.entry.set_position(-1);
                        return glib::Propagation::Stop;
                    }
                }
                glib::Propagation::Proceed
            }
        }
    }

    /// Inline panels (Run, Shut Down, Log Off, About) replace the item list
    /// inside the same card, like a dialog sheet.
    fn show_panel(self: &Rc<Self>, name: &'static str) {
        if let Some(old) = self.stack.child_by_name("panel") {
            self.stack.remove(&old);
        }
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        panel.add_css_class("glaze-menu-panel");
        let title = gtk4::Label::new(None);
        title.add_css_class("glaze-card-title");
        title.set_xalign(0.0);
        panel.append(&title);
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        buttons.set_halign(gtk4::Align::End);
        let back = {
            let m = self.clone();
            panel_ui::menu_item("Cancel", move || {
                m.show_menu();
                m.entry.grab_focus();
            })
        };
        match name {
            "run" => {
                title.set_text("Run");
                let hint = gtk4::Label::new(Some(
                    "Type the name of a program to open it. No shell is involved.",
                ));
                hint.add_css_class("glaze-card-sub");
                hint.set_wrap(true);
                hint.set_xalign(0.0);
                panel.append(&hint);
                let e = gtk4::Entry::new();
                e.add_css_class("glaze-run-entry");
                e.set_placeholder_text(Some("e.g. foot"));
                panel.append(&e);
                let m = self.clone();
                let e2 = e.clone();
                let go = panel_ui::menu_item("OK", move || {
                    if run_command(&e2.text()) {
                        m.close();
                    }
                });
                go.add_css_class("glaze-confirm");
                {
                    let m = self.clone();
                    e.connect_activate(move |e| {
                        if run_command(&e.text()) {
                            m.close();
                        }
                    });
                }
                buttons.append(&back);
                buttons.append(&go);
                panel.append(&buttons);
                self.attach_panel(&panel);
                e.grab_focus();
            }
            "logoff" => {
                title.set_text("Log off");
                let q = gtk4::Label::new(Some(
                    "End this session? Unsaved work in open apps may be lost.",
                ));
                q.set_wrap(true);
                q.set_xalign(0.0);
                panel.append(&q);
                let go = panel_ui::menu_item("Log Off", {
                    let m = self.clone();
                    move || {
                        m.close();
                        run_argv(&["sh", "-c", crate::LOGOUT_SH]);
                    }
                });
                go.add_css_class("glaze-confirm");
                buttons.append(&back);
                buttons.append(&go);
                panel.append(&buttons);
                self.attach_panel(&panel);
                go.grab_focus();
            }
            "shutdown" => {
                title.set_text("Shut Down");
                let q = gtk4::Label::new(Some("What do you want the computer to do?"));
                q.set_xalign(0.0);
                panel.append(&q);
                let choices: [(&str, &str, &[&str]); 4] = [
                    ("power", "Shut down", &["systemctl", "poweroff"]),
                    ("restart", "Restart", &["systemctl", "reboot"]),
                    ("sleep", "Sleep", &["systemctl", "suspend"]),
                    ("lock", "Lock", &["sh", "-c", crate::LOCK_SH]),
                ];
                let chosen: Rc<Cell<usize>> = Rc::new(Cell::new(0));
                let mut group: Option<gtk4::CheckButton> = None;
                for (i, (icon, label, _)) in choices.iter().enumerate() {
                    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                    let radio = gtk4::CheckButton::new();
                    radio.add_css_class("glaze-radio");
                    if let Some(g) = &group {
                        radio.set_group(Some(g));
                    } else {
                        group = Some(radio.clone());
                        radio.set_active(true);
                    }
                    let c = chosen.clone();
                    radio.connect_toggled(move |r| {
                        if r.is_active() {
                            c.set(i);
                        }
                    });
                    line.append(&radio);
                    line.append(&icons::icon(
                        icon,
                        icons::fallback_for(icon),
                        18,
                        self.ctx.ink(),
                        self.ctx.stroke(),
                    ));
                    let l = gtk4::Label::new(Some(label));
                    line.append(&l);
                    panel.append(&line);
                }
                let go = panel_ui::menu_item("OK", {
                    let m = self.clone();
                    move || {
                        let (_, _, argv) = choices[chosen.get()];
                        m.close();
                        run_argv(argv);
                    }
                });
                go.add_css_class("glaze-confirm");
                buttons.append(&back);
                buttons.append(&go);
                panel.append(&buttons);
                self.attach_panel(&panel);
                go.grab_focus();
            }
            _ => {
                title.set_text("Kiln GNU/Linux");
                let os = os_pretty_name();
                for line in [
                    os.as_str(),
                    &format!("Glaze desktop {}", glaze_shell_core::VERSION),
                    "Super+Return  terminal",
                    "Type in this menu to search apps",
                    "Settings: ~/.config/glaze/config.toml (live reload)",
                    "No telemetry. Nothing in this menu leaves your computer.",
                ] {
                    let l = gtk4::Label::new(Some(line));
                    l.set_xalign(0.0);
                    l.set_wrap(true);
                    l.add_css_class("glaze-card-sub");
                    panel.append(&l);
                }
                let ok = {
                    let m = self.clone();
                    panel_ui::menu_item("Back", move || {
                        m.show_menu();
                        m.entry.grab_focus();
                    })
                };
                buttons.append(&ok);
                panel.append(&buttons);
                self.attach_panel(&panel);
                ok.grab_focus();
            }
        }
    }

    fn attach_panel(&self, panel: &gtk4::Box) {
        self.stack.add_named(panel, Some("panel"));
        self.stack.set_visible_child_name("panel");
        self.levels.borrow_mut().clear();
        self.levels.borrow_mut().push(Level {
            rows: Vec::new(),
            sel: Cell::new(usize::MAX),
            col: None,
        });
    }
}

fn app_row(app: &AppEntry) -> RowSpec {
    RowSpec {
        label: app.name.clone(),
        icon: RowIcon::App(app.id.clone()),
        act: Act::Launch(app.id.clone()),
        separator_before: false,
    }
}

/// The user's full name from GECOS, when one is set (the bare login name is
/// not shown: Windows 2000 shows "Log Off <name>", but a login like the dev
/// VM's is noise).
fn display_name(login: &str) -> Option<String> {
    let gecos = std::fs::read_to_string("/etc/passwd").ok().and_then(|p| {
        p.lines()
            .find(|l| l.split(':').next() == Some(login))
            .and_then(|l| l.split(':').nth(4))
            .map(|g| g.split(',').next().unwrap_or("").trim().to_string())
    });
    gecos.filter(|g| !g.is_empty() && !g.eq_ignore_ascii_case(login))
}

fn empty_row() -> RowSpec {
    RowSpec {
        label: "Nothing here yet".into(),
        icon: RowIcon::None,
        act: Act::Disabled,
        separator_before: false,
    }
}

fn clear(b: &gtk4::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// Recently used files from `recently-used.xbel` (newest last in the file).
fn documents_rows() -> Vec<RowSpec> {
    let path = glaze_common::paths::data_home().join("recently-used.xbel");
    let Ok(src) = std::fs::read_to_string(path) else {
        return vec![empty_row()];
    };
    let mut files: Vec<String> = Vec::new();
    for part in src.split("href=\"").skip(1) {
        if let Some(end) = part.find('"') {
            let href = &part[..end];
            if let Some(p) = href.strip_prefix("file://") {
                let p = percent_decode(p);
                if !files.contains(&p) && std::path::Path::new(&p).exists() {
                    files.push(p);
                }
            }
        }
    }
    files.reverse();
    let rows: Vec<RowSpec> = files
        .into_iter()
        .take(12)
        .map(|p| RowSpec {
            label: std::path::Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone()),
            icon: RowIcon::Ink("folder"),
            act: Act::Open(p),
            separator_before: false,
        })
        .collect();
    if rows.is_empty() {
        vec![empty_row()]
    } else {
        rows
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "Kiln GNU/Linux".into())
}

pub(crate) fn open_path(path: &str) {
    for opener in ["xdg-open", "gio"] {
        let mut cmd = std::process::Command::new(opener);
        if opener == "gio" {
            cmd.arg("open");
        }
        if let Ok(mut c) = cmd.arg(path).spawn() {
            std::thread::spawn(move || {
                let _ = c.wait();
            });
            return;
        }
    }
    eprintln!("[glaze-shell] cannot open {path}: no xdg-open or gio");
}

/// Run a typed command without a shell (whitespace-split argv).
fn run_command(text: &str) -> bool {
    let argv: Vec<&str> = text.split_whitespace().collect();
    if argv.is_empty() {
        return false;
    }
    run_argv(&argv)
}

pub(crate) fn run_argv(argv: &[&str]) -> bool {
    let Some((prog, args)) = argv.split_first() else {
        return false;
    };
    eprintln!("[glaze-shell] run: {}", argv.join(" "));
    match std::process::Command::new(prog).args(args).spawn() {
        Ok(mut c) => {
            std::thread::spawn(move || {
                let _ = c.wait();
            });
            true
        }
        Err(e) => {
            eprintln!("[glaze-shell] run {prog} failed: {e}");
            false
        }
    }
}
