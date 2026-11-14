//! The one Glaze context menu, used by the desktop, the taskbar (task and
//! pinned buttons), the tray and the start-menu items.
//!
//! Flat paper card (tokens: hairline border, small radius, faint hard
//! shadow, paper texture), 32 px rows with an ink icon column, label and a
//! right-aligned shortcut or chevron, hairline separators, optional header,
//! tinted-accent highlight for hover and keyboard selection (no focus-ring
//! boxes), dimmed disabled rows. Submenus cascade on hover (short delay) as
//! extra columns inside the same popover surface (nested Wayland popups
//! close their parents). Keyboard: Up/Down, Right/Enter open a submenu,
//! Left/Esc close one level, Enter activates, typing jumps to the next row
//! starting with the typed letters. Open is a short fade + slide driven by
//! the frame clock; reduced motion makes it instant.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::anim::Animator;
use crate::icons;
use crate::panel_ui::Ctx;

/// One menu entry.
#[derive(Clone)]
pub enum Entry {
    Item {
        icon: Option<&'static str>,
        label: String,
        accel: Option<String>,
        enabled: bool,
        danger: bool,
        action: Rc<dyn Fn()>,
    },
    Sub {
        icon: Option<&'static str>,
        label: String,
        items: Rc<Vec<Entry>>,
    },
    Separator,
    Header(String),
}

impl Entry {
    pub fn item(
        icon: Option<&'static str>,
        label: impl Into<String>,
        f: impl Fn() + 'static,
    ) -> Self {
        Entry::Item {
            icon,
            label: label.into(),
            accel: None,
            enabled: true,
            danger: false,
            action: Rc::new(f),
        }
    }

    pub fn sub(icon: Option<&'static str>, label: impl Into<String>, items: Vec<Entry>) -> Self {
        Entry::Sub {
            icon,
            label: label.into(),
            items: Rc::new(items),
        }
    }

    /// Mark an item disabled (dimmed, skipped by keyboard navigation).
    pub fn disabled(mut self) -> Self {
        if let Entry::Item { enabled, .. } = &mut self {
            *enabled = false;
        }
        self
    }

    /// Mark an item destructive (danger colour on hover).
    pub fn danger(mut self) -> Self {
        if let Entry::Item { danger, .. } = &mut self {
            *danger = true;
        }
        self
    }

    /// Right-aligned shortcut hint, e.g. `Super+H`.
    pub fn accel(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        if let Entry::Item { accel, .. } = &mut self {
            *accel = (!text.is_empty()).then_some(text);
        }
        self
    }

    pub fn enabled_if(self, on: bool) -> Self {
        if on {
            self
        } else {
            self.disabled()
        }
    }
}

#[derive(Clone)]
enum Kind {
    Act(Rc<dyn Fn()>),
    Sub(Rc<Vec<Entry>>),
    Off,
}

struct Col {
    card: gtk4::Box,
    /// (row, lowercase label for type-ahead, action, display label)
    rows: Vec<(gtk4::Button, String, Kind, String)>,
    sel: Cell<Option<usize>>,
}

struct Menu {
    pop: gtk4::Popover,
    outer: gtk4::Box,
    cols: RefCell<Vec<Col>>,
    ctx: Ctx,
    anim: Animator,
    hover: RefCell<Option<glib::SourceId>>,
    typed: RefCell<(String, Instant)>,
    /// (depth, row) whose submenu is open.
    open_sub: Cell<Option<(usize, usize)>>,
    up: bool,
    /// Screen x of the menu's left edge and the screen width (both in the
    /// parent surface's coordinates, which start at the screen's left for
    /// the full-width taskbar and desktop); `None` when unknown.
    room: Cell<Option<(f64, f64)>>,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// Remember the shell context for surfaces that have none (the desktop).
pub fn set_context(ctx: &Ctx) {
    CTX.with(|c| *c.borrow_mut() = Some(ctx.clone()));
}

/// [`popup`] with the remembered shell context.
pub fn popup_default(
    parent: &impl IsA<gtk4::Widget>,
    at: Option<gdk::Rectangle>,
    position: gtk4::PositionType,
    entries: Vec<Entry>,
) {
    match CTX.with(|c| c.borrow().clone()) {
        Some(ctx) => {
            popup(parent, at, position, &ctx, entries);
        }
        None => eprintln!("[glaze-shell] menu: no shell context yet"),
    }
}

/// Give the layer surface holding `w` exclusive keyboard focus while a menu
/// is open (Sway does not always move keyboard focus to an on-demand layer
/// surface's popup: with foot focused, keys kept going to foot). Returns
/// the closure that restores the previous mode.
pub fn hold_keyboard(w: &gtk4::Widget) -> Box<dyn FnOnce()> {
    use gtk4_layer_shell::{KeyboardMode, LayerShell};
    let Some(win) = w.root().and_then(|r| r.downcast::<gtk4::Window>().ok()) else {
        return Box::new(|| {});
    };
    if !win.is_layer_window() {
        return Box::new(|| {});
    }
    let prev = win.keyboard_mode();
    if prev == KeyboardMode::Exclusive {
        return Box::new(|| {});
    }
    win.set_keyboard_mode(KeyboardMode::Exclusive);
    Box::new(move || {
        // Drop the focus first so Sway hands it back to the window that
        // had it, then allow on-demand focus again.
        win.set_keyboard_mode(KeyboardMode::None);
        if prev != KeyboardMode::None {
            glib::timeout_add_local_once(Duration::from_millis(60), move || {
                win.set_keyboard_mode(prev)
            });
        }
    })
}

/// Pop up a menu on `parent`, pointing at `at` (in `parent` coordinates;
/// `None` = the whole parent), opening towards `position`.
pub fn popup(
    parent: &impl IsA<gtk4::Widget>,
    at: Option<gdk::Rectangle>,
    mut position: gtk4::PositionType,
    ctx: &Ctx,
    entries: Vec<Entry>,
) -> gtk4::Popover {
    let pop = gtk4::Popover::new();
    pop.add_css_class("glaze-popover");
    pop.add_css_class("glaze-flat-menu");
    pop.set_has_arrow(false);
    pop.set_autohide(true);
    pop.set_position(position);
    pop.set_parent(parent);
    if let Some(r) = at {
        pop.set_pointing_to(Some(&r));
    }
    // Vertical room: a menu opened downward near the bottom (desktop) would
    // run under the taskbar; open it upward instead.
    if position == gtk4::PositionType::Bottom {
        let native_y = at.and_then(|r| {
            let native = parent.as_ref().native()?;
            parent
                .as_ref()
                .compute_point(&native, &gtk4::graphene::Point::new(0.0, r.y() as f32))
                .map(|p| p.y() as f64)
        });
        let screen_h = parent
            .as_ref()
            .native()
            .and_then(|n| n.surface())
            .and_then(|surf| gdk::Display::default().and_then(|d| d.monitor_at_surface(&surf)))
            .map(|m| m.geometry().height() as f64);
        let bar = if ctx.cfg.panel.position == glaze_common::config::PanelPosition::Bottom {
            crate::panel_ui::panel_thickness(ctx) as f64
        } else {
            0.0
        };
        let est: i32 = entries
            .iter()
            .map(|e| match e {
                Entry::Separator => 9,
                Entry::Header(_) => 26,
                _ => 32,
            })
            .sum::<i32>()
            + 12;
        if let (Some(y), Some(sh)) = (native_y, screen_h) {
            if y + est as f64 + 6.0 > sh - bar {
                position = gtk4::PositionType::Top;
                pop.set_position(position);
            }
        }
    }
    let up = position == gtk4::PositionType::Top;
    let outer = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    outer.set_valign(if up {
        gtk4::Align::End
    } else {
        gtk4::Align::Start
    });
    pop.set_child(Some(&outer));
    // Where the menu will sit on screen, to decide cascade vs drill-down.
    let screen_w = parent
        .as_ref()
        .native()
        .and_then(|n| n.surface())
        .and_then(|surf| gdk::Display::default().and_then(|d| d.monitor_at_surface(&surf)))
        .map(|m| m.geometry().width() as f64);
    let anchor_x = at.and_then(|r| {
        let native = parent.as_ref().native()?;
        parent
            .as_ref()
            .compute_point(
                &native,
                &gtk4::graphene::Point::new(r.x() as f32, r.y() as f32),
            )
            .map(|p| (p.x() as f64, r.width() as f64))
    });
    let anim = Animator::new("menu", parent, ctx.motion, ctx.cfg.motion.log_frame_times);
    let m = Rc::new(Menu {
        pop: pop.clone(),
        outer,
        cols: RefCell::new(Vec::new()),
        ctx: ctx.clone(),
        anim,
        hover: RefCell::new(None),
        typed: RefCell::new((String::new(), Instant::now())),
        open_sub: Cell::new(None),
        up,
        room: Cell::new(None),
    });
    m.push_col(&entries, None);
    // Sway does not move or grow a mapped popup when its size changes, so a
    // cascade taller than the root column would be squeezed. Reserve the
    // tallest cascade's height up front (the extra area is transparent).
    let (_, root_h, _, _) = m.outer.measure(gtk4::Orientation::Vertical, -1);
    let need = tallest_sub(&entries);
    if need > root_h {
        m.outer.set_size_request(-1, need);
    }
    let (_, nat_w, _, _) = m.outer.measure(gtk4::Orientation::Horizontal, -1);
    let nat_w = nat_w as f64;
    let sw = screen_w.unwrap_or(f64::MAX);
    // Start-aligned popover: GTK then keeps the menu's left edge fixed when
    // a cascade column widens it (a centred popover jumped sideways). The
    // offset places that left edge: at the pointer for pointer menus, centred
    // under a button otherwise, clamped on screen (Sway does not slide
    // popups that would overflow).
    pop.set_halign(gtk4::Align::Start);
    let left = match anchor_x {
        Some((x, w)) => {
            let want = if w <= 1.0 {
                x
            } else {
                x + w / 2.0 - nat_w / 2.0
            };
            let l = want.min(sw - nat_w - 4.0).max(4.0f64.min(x));
            let off = (l - x).round() as i32;
            if off != 0 {
                pop.set_offset(off, 0);
            }
            Some(l)
        }
        None => None,
    };
    let room = match (left, screen_w) {
        (Some(l), Some(sw)) => Some((l.max(0.0).min((sw - nat_w).max(0.0)), sw)),
        _ => None,
    };
    m.room.set(room);
    {
        let key = gtk4::EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let mm = m.clone();
        key.connect_key_pressed(move |_, k, _, _| mm.on_key(k));
        pop.add_controller(key);
    }
    {
        let mm = Rc::downgrade(&m);
        // Keep the menu alive while shown; drop and unparent when closed.
        let keep = RefCell::new(Some(m.clone()));
        let restore = RefCell::new(Some(hold_keyboard(parent.as_ref())));
        pop.connect_closed(move |p| {
            if let Some(r) = restore.borrow_mut().take() {
                r();
            }
            if let Some(m) = mm.upgrade() {
                m.cancel_hover();
            }
            keep.borrow_mut().take();
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
    }
    // Open: fade + 6 px slide away from the anchor.
    let card = m.cols.borrow()[0].card.clone();
    card.set_opacity(0.0);
    pop.popup();
    let ms = ctx.tokens.metrics.motion_menu_ms as f64;
    {
        let c = card.clone();
        m.anim
            .animate("o", 0.0, 1.0, ms, move |v| c.set_opacity(v), None);
    }
    let first = m.next_enabled(0, None, true);
    if let Some(i) = first {
        // Keyboard users start on the first row; the pointer simply
        // overrides it on hover. No focus ring is drawn.
        m.select(0, i);
    }
    pop.grab_focus();
    pop
}

/// Estimated pixel height of the tallest submenu column under `entries`
/// (rows 32 px, separators 9 px, headers 26 px, card padding 10 px).
fn tallest_sub(entries: &[Entry]) -> i32 {
    entries
        .iter()
        .filter_map(|e| match e {
            Entry::Sub { items, .. } => {
                let own: i32 = items
                    .iter()
                    .map(|i| match i {
                        Entry::Separator => 9,
                        Entry::Header(_) => 26,
                        _ => 32,
                    })
                    .sum::<i32>()
                    + 10;
                Some(own.max(tallest_sub(items)))
            }
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

impl Menu {
    fn push_col(self: &Rc<Self>, entries: &[Entry], align_to: Option<&gtk4::Widget>) {
        let depth = self.cols.borrow().len();
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.add_css_class("glaze-menu-card");
        card.add_css_class("glaze-flat-menu-card");
        let mut rows = Vec::new();
        for e in entries {
            match e {
                Entry::Separator => {
                    let s = gtk4::Separator::new(gtk4::Orientation::Horizontal);
                    s.add_css_class("glaze-menu-sep");
                    card.append(&s);
                }
                Entry::Header(t) => {
                    let l = gtk4::Label::new(Some(t));
                    l.add_css_class("glaze-menu-header");
                    l.set_xalign(0.0);
                    l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    l.set_max_width_chars(34);
                    card.append(&l);
                }
                Entry::Item {
                    icon,
                    label,
                    accel,
                    enabled,
                    danger,
                    action,
                } => {
                    let b = self.row(*icon, label, accel.as_deref(), false);
                    if *danger {
                        b.add_css_class("glaze-danger");
                    }
                    let kind = if *enabled {
                        Kind::Act(action.clone())
                    } else {
                        b.add_css_class("glaze-disabled");
                        Kind::Off
                    };
                    card.append(&b);
                    rows.push((b, label.to_lowercase(), kind, label.clone()));
                }
                Entry::Sub { icon, label, items } => {
                    let b = self.row(*icon, label, None, true);
                    card.append(&b);
                    rows.push((
                        b,
                        label.to_lowercase(),
                        Kind::Sub(items.clone()),
                        label.clone(),
                    ));
                }
            }
        }
        for (i, (b, _, _, _)) in rows.iter().enumerate() {
            let m = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(m) = m.upgrade() {
                    m.activate(depth, i);
                }
            });
            let motion = gtk4::EventControllerMotion::new();
            let m = Rc::downgrade(self);
            motion.connect_enter(move |_, _, _| {
                if let Some(m) = m.upgrade() {
                    m.hover(depth, i);
                }
            });
            b.add_controller(motion);
        }
        if let Some(anchor) = align_to {
            // Cascade: line the column up with its row (Windows 2000).
            let y = anchor
                .compute_point(&self.outer, &gtk4::graphene::Point::new(0.0, 0.0))
                .map(|p| p.y() as i32)
                .unwrap_or(0);
            let (_, nat_h, _, _) = card.measure(gtk4::Orientation::Vertical, -1);
            let outer_h = self.outer.height().max(1);
            if self.up {
                card.set_valign(gtk4::Align::End);
                card.set_margin_bottom((outer_h - y - nat_h + 6).max(0));
            } else {
                card.set_valign(gtk4::Align::Start);
                card.set_margin_top((y - 5).max(0));
            }
            card.set_opacity(0.0);
            let ms = self.ctx.tokens.metrics.motion_menu_ms as f64;
            let c = card.clone();
            self.anim.animate(
                &format!("c{depth}o"),
                0.0,
                1.0,
                ms,
                move |v| c.set_opacity(v),
                None,
            );
        } else {
            card.set_valign(if self.up {
                gtk4::Align::End
            } else {
                gtk4::Align::Start
            });
        }
        self.outer.append(&card);
        self.cols.borrow_mut().push(Col {
            card,
            rows,
            sel: Cell::new(None),
        });
    }

    fn row(
        &self,
        icon: Option<&'static str>,
        label: &str,
        accel: Option<&str>,
        sub: bool,
    ) -> gtk4::Button {
        let b = gtk4::Button::new();
        b.add_css_class("glaze-mrow");
        b.set_focus_on_click(false);
        b.set_can_focus(false);
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        slot.set_size_request(16, 16);
        slot.set_valign(gtk4::Align::Center);
        if let Some(name) = icon {
            slot.append(&icons::icon(
                name,
                icons::fallback_for(name),
                16,
                self.ctx.ink(),
                self.ctx.stroke(),
            ));
        }
        line.append(&slot);
        let l = gtk4::Label::new(Some(label));
        l.set_xalign(0.0);
        l.set_hexpand(true);
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        l.set_max_width_chars(36);
        l.add_css_class("glaze-mrow-label");
        line.append(&l);
        if let Some(a) = accel {
            let al = gtk4::Label::new(Some(a));
            al.add_css_class("glaze-mrow-accel");
            line.append(&al);
        }
        if sub {
            line.append(&icons::icon(
                "chevron-right",
                icons::fallback_for("chevron-right"),
                14,
                self.ctx.tokens.palette.text_secondary,
                self.ctx.stroke(),
            ));
        }
        b.set_child(Some(&line));
        b
    }

    fn select(&self, depth: usize, idx: usize) {
        let cols = self.cols.borrow();
        let Some(col) = cols.get(depth) else { return };
        for (i, (b, _, _, _)) in col.rows.iter().enumerate() {
            if i == idx {
                b.add_css_class("glaze-selected");
            } else {
                b.remove_css_class("glaze-selected");
            }
        }
        col.sel.set(Some(idx));
    }

    fn cancel_hover(&self) {
        if let Some(id) = self.hover.borrow_mut().take() {
            id.remove();
        }
    }

    fn hover(self: &Rc<Self>, depth: usize, idx: usize) {
        if self.cols.borrow().len() <= depth {
            return;
        }
        self.select(depth, idx);
        self.cancel_hover();
        let m = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(200), move || {
            let Some(m) = m.upgrade() else { return };
            m.hover.borrow_mut().take();
            let is_sub = m
                .kind(depth, idx)
                .map(|k| matches!(k, Kind::Sub(_)))
                .unwrap_or(false);
            if is_sub && m.open_sub.get() != Some((depth, idx)) {
                m.open_sub(depth, idx, false);
            } else if !is_sub {
                m.close_from(depth + 1);
            }
        });
        *self.hover.borrow_mut() = Some(id);
    }

    fn kind(&self, depth: usize, idx: usize) -> Option<Kind> {
        self.cols
            .borrow()
            .get(depth)
            .and_then(|c| c.rows.get(idx))
            .map(|(_, _, k, _)| k.clone())
    }

    fn open_sub(self: &Rc<Self>, depth: usize, idx: usize, select_first: bool) {
        self.close_from(depth + 1);
        let Some(Kind::Sub(items)) = self.kind(depth, idx) else {
            return;
        };
        let anchor: gtk4::Widget = self.cols.borrow()[depth].rows[idx].0.clone().upcast();
        // Would the cascade leave the screen? Then drill down in place
        // (the parent column is hidden, Left/Esc brings it back).
        let drill = self.room.get().is_some_and(|(left, sw)| {
            let shown: f64 = self
                .cols
                .borrow()
                .iter()
                .filter(|c| c.card.is_visible())
                .map(|c| c.card.width() as f64 + 2.0)
                .sum();
            left + shown + 230.0 > sw
        });
        if drill {
            let title = self.cols.borrow()[depth].rows[idx].3.clone();
            if let Some(c) = self.cols.borrow().last() {
                c.card.set_visible(false);
            }
            let mut v = vec![Entry::Header(format!("\u{2039}  {title}"))];
            v.extend(items.iter().cloned());
            self.push_col(&v, None);
        } else {
            self.push_col(&items, Some(&anchor));
        }
        self.open_sub.set(Some((depth, idx)));
        if select_first {
            if let Some(i) = self.next_enabled(depth + 1, None, true) {
                self.select(depth + 1, i);
            }
        }
    }

    fn close_from(&self, depth: usize) {
        let mut cols = self.cols.borrow_mut();
        while cols.len() > depth.max(1) {
            if let Some(c) = cols.pop() {
                self.outer.remove(&c.card);
            }
        }
        if let Some(c) = cols.last() {
            c.card.set_visible(true);
        }
        if cols.len() <= 1
            || self
                .open_sub
                .get()
                .map(|(d, _)| d + 1 >= cols.len())
                .unwrap_or(false)
        {
            self.open_sub.set(None);
        }
    }

    fn activate(self: &Rc<Self>, depth: usize, idx: usize) {
        match self.kind(depth, idx) {
            Some(Kind::Act(f)) => {
                self.cancel_hover();
                self.pop.popdown();
                f();
            }
            Some(Kind::Sub(_)) => {
                self.cancel_hover();
                self.open_sub(depth, idx, true);
            }
            _ => {}
        }
    }

    fn next_enabled(&self, depth: usize, from: Option<usize>, down: bool) -> Option<usize> {
        let cols = self.cols.borrow();
        let col = cols.get(depth)?;
        let n = col.rows.len();
        if n == 0 {
            return None;
        }
        let mut i = match (from, down) {
            (Some(f), _) => f,
            (None, true) => n - 1,
            (None, false) => 0,
        };
        for _ in 0..n {
            i = if down { (i + 1) % n } else { (i + n - 1) % n };
            if !matches!(col.rows[i].2, Kind::Off) {
                return Some(i);
            }
        }
        None
    }

    fn on_key(self: &Rc<Self>, k: gdk::Key) -> glib::Propagation {
        let depth = self.cols.borrow().len().saturating_sub(1);
        let sel = self.cols.borrow().get(depth).and_then(|c| c.sel.get());
        match k {
            gdk::Key::Down | gdk::Key::Up => {
                if let Some(i) = self.next_enabled(depth, sel, k == gdk::Key::Down) {
                    self.cancel_hover();
                    self.select(depth, i);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Right => {
                if let Some(i) = sel {
                    if matches!(self.kind(depth, i), Some(Kind::Sub(_))) {
                        self.open_sub(depth, i, true);
                    }
                }
                glib::Propagation::Stop
            }
            gdk::Key::Left => {
                if depth > 0 {
                    self.close_from(depth);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                if let Some(i) = sel {
                    self.activate(depth, i);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                if depth > 0 {
                    self.close_from(depth);
                } else {
                    self.pop.popdown();
                }
                glib::Propagation::Stop
            }
            _ => {
                let Some(c) = k.to_unicode().filter(|c| !c.is_control()) else {
                    return glib::Propagation::Proceed;
                };
                let prefix = {
                    let mut t = self.typed.borrow_mut();
                    if t.1.elapsed() > Duration::from_millis(900) {
                        t.0.clear();
                    }
                    t.0.extend(c.to_lowercase());
                    t.1 = Instant::now();
                    t.0.clone()
                };
                let hit = {
                    let cols = self.cols.borrow();
                    let col = &cols[depth];
                    let n = col.rows.len();
                    let start = if prefix.chars().count() == 1 {
                        sel.map(|s| s + 1).unwrap_or(0)
                    } else {
                        sel.unwrap_or(0)
                    };
                    (0..n).map(|o| (start + o) % n.max(1)).find(|&i| {
                        !matches!(col.rows[i].2, Kind::Off) && col.rows[i].1.starts_with(&prefix)
                    })
                };
                if let Some(i) = hit {
                    self.select(depth, i);
                }
                glib::Propagation::Stop
            }
        }
    }
}
