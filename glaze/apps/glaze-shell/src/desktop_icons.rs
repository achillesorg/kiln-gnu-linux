//! Desktop icons: Home, Trash, the files in `~/Desktop` and the user's
//! shortcuts, on a snapping grid over the wallpaper.
//!
//! Interaction (Windows conventions): click selects, Ctrl/Shift click
//! extends (the tested `glaze_desktop::selection` model), dragging on empty
//! desktop draws a marquee that selects what it touches, dragging an icon
//! moves the selection and drops it on the nearest free grid cell, double
//! click or Enter opens, F2 renames, Delete moves files to the trash, the
//! arrow keys move the selection. Right-click on an icon: Open, Rename,
//! Delete, Properties; on empty desktop the desktop menu. Positions are kept
//! in `$XDG_STATE_HOME/glaze/desktop-icons.json`; `~/Desktop` is watched.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use glaze_desktop::icons::{DesktopIcon, IconTarget};
use glaze_desktop::selection::Selection;
use glaze_service::registry::AppRegistry;
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

use crate::menu::Entry;

const CELL_W: i32 = 92;
const CELL_H: i32 = 96;
const MARGIN: i32 = 10;

/// Marquee rectangle `(x, y, w, h)` in desktop coordinates.
type Band = (f64, f64, f64, f64);
/// An icon being dragged and its start position.
type DragItem = (Rc<Item>, f64, f64);

struct Item {
    icon: DesktopIcon,
    widget: gtk4::Box,
    col: Cell<i32>,
    row: Cell<i32>,
}

pub struct DesktopIcons {
    pub fixed: gtk4::Fixed,
    marquee: gtk4::DrawingArea,
    items: RefCell<Vec<Rc<Item>>>,
    sel: RefCell<Selection>,
    registry: Rc<AppRegistry>,
    band: Rc<Cell<Option<Band>>>,
    drag_move: RefCell<Option<Vec<DragItem>>>,
    on_background_menu: Rc<dyn Fn(f64, f64)>,
    _monitor: RefCell<Option<gio::FileMonitor>>,
}

fn state_file() -> PathBuf {
    glaze_common::paths::state_home().join("glaze/desktop-icons.json")
}

fn desktop_dir() -> PathBuf {
    glaze_common::paths::home_dir()
        .unwrap_or_default()
        .join("Desktop")
}

/// Home, Trash, then `~/Desktop` entries (folders first, by name), then the
/// model's shortcut icons.
fn collect(extra: Vec<DesktopIcon>) -> Vec<DesktopIcon> {
    let mut v = vec![
        DesktopIcon {
            app_id: None,
            label: "Home".into(),
            icon: Some("user-home".into()),
            target: IconTarget::Home,
        },
        DesktopIcon {
            app_id: None,
            label: "Trash".into(),
            icon: Some("user-trash".into()),
            target: IconTarget::Trash,
        },
    ];
    let mut files: Vec<PathBuf> = std::fs::read_dir(desktop_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    !p.file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(true)
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|p| {
        (
            !p.is_dir(),
            p.file_name().map(|n| n.to_string_lossy().to_lowercase()),
        )
    });
    for p in files {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (label, icon) = if p.extension().and_then(|e| e.to_str()) == Some("desktop") {
            // Launcher file: show its Name= and Icon= (no execution here).
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let field = |k: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(k))
                    .map(|v| v.trim().to_string())
            };
            (
                field("Name=").unwrap_or_else(|| name.clone()),
                field("Icon="),
            )
        } else if p.is_dir() {
            (name.clone(), Some("folder".to_string()))
        } else {
            let (ct, _) = gio::content_type_guess(Some(&p), None);
            (
                name.clone(),
                Some(
                    gio::content_type_get_generic_icon_name(&ct)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "text-x-generic".into()),
                ),
            )
        };
        v.push(DesktopIcon {
            app_id: None,
            label,
            icon,
            target: IconTarget::File(p),
        });
    }
    v.extend(extra);
    v
}

impl DesktopIcons {
    pub fn new(
        registry: Rc<AppRegistry>,
        extra: Vec<DesktopIcon>,
        on_background_menu: Rc<dyn Fn(f64, f64)>,
    ) -> Rc<Self> {
        let fixed = gtk4::Fixed::new();
        fixed.set_hexpand(true);
        fixed.set_vexpand(true);
        fixed.set_focusable(true);
        let marquee = gtk4::DrawingArea::new();
        marquee.set_can_target(false);
        marquee.set_hexpand(true);
        marquee.set_vexpand(true);
        let band: Rc<Cell<Option<Band>>> = Rc::new(Cell::new(None));
        {
            let b = band.clone();
            marquee.set_draw_func(move |_, cr, _, _| {
                if let Some((x, y, w, h)) = b.get() {
                    cr.rectangle(x + 0.5, y + 0.5, w, h);
                    // Terracotta tint and hairline (theme accent default).
                    cr.set_source_rgba(0.851, 0.467, 0.341, 0.16);
                    let _ = cr.fill_preserve();
                    cr.set_source_rgba(0.851, 0.467, 0.341, 0.85);
                    cr.set_line_width(1.0);
                    let _ = cr.stroke();
                }
            });
        }
        let me = Rc::new(DesktopIcons {
            fixed,
            marquee,
            items: RefCell::new(Vec::new()),
            sel: RefCell::new(Selection::new()),
            registry,
            band,
            drag_move: RefCell::new(None),
            on_background_menu,
            _monitor: RefCell::new(None),
        });
        me.rebuild(extra.clone());
        me.install_gestures();
        // Follow ~/Desktop.
        let dir = gio::File::for_path(desktop_dir());
        let _ = std::fs::create_dir_all(desktop_dir());
        if let Ok(mon) = dir.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        {
            let w = Rc::downgrade(&me);
            let pending = Rc::new(Cell::new(false));
            mon.connect_changed(move |_, _, _, _| {
                if pending.replace(true) {
                    return;
                }
                let (w, p, extra) = (w.clone(), pending.clone(), extra.clone());
                glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
                    p.set(false);
                    if let Some(m) = w.upgrade() {
                        m.rebuild(extra);
                    }
                });
            });
            *me._monitor.borrow_mut() = Some(mon);
        }
        me
    }

    /// The marquee layer (stack it above the icons).
    pub fn marquee(&self) -> &gtk4::DrawingArea {
        &self.marquee
    }

    fn rebuild(self: &Rc<Self>, extra: Vec<DesktopIcon>) {
        for it in self.items.borrow_mut().drain(..) {
            self.fixed.remove(&it.widget);
        }
        let saved: HashMap<String, (i32, i32)> = std::fs::read_to_string(state_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let rows = self.rows();
        let mut taken: Vec<(i32, i32)> = saved.values().copied().collect();
        let mut next = (0, 0);
        for icon in collect(extra) {
            let key = icon.key();
            let (c, r) = match saved.get(&key) {
                Some(p) => *p,
                None => {
                    while taken.contains(&next) {
                        next = if next.1 + 1 >= rows {
                            (next.0 + 1, 0)
                        } else {
                            (next.0, next.1 + 1)
                        };
                    }
                    taken.push(next);
                    next
                }
            };
            let w = self.icon_widget(&icon);
            self.fixed.put(&w, 0.0, 0.0);
            let it = Rc::new(Item {
                icon,
                widget: w,
                col: Cell::new(c),
                row: Cell::new(r),
            });
            self.place(&it);
            self.items.borrow_mut().push(it);
        }
        self.sync_selection();
    }

    fn rows(&self) -> i32 {
        let h = self.fixed.height();
        let h = if h > 0 { h } else { 720 };
        ((h - 2 * MARGIN) / CELL_H).max(1)
    }

    fn place(&self, it: &Item) {
        let (x, y) = (
            MARGIN + it.col.get() * CELL_W,
            MARGIN + it.row.get() * CELL_H,
        );
        self.fixed.move_(&it.widget, x as f64, y as f64);
    }

    fn save_positions(&self) {
        let map: HashMap<String, (i32, i32)> = self
            .items
            .borrow()
            .iter()
            .map(|it| (it.icon.key(), (it.col.get(), it.row.get())))
            .collect();
        let p = state_file();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string(&map) {
            let _ = std::fs::write(p, s);
        }
    }

    fn icon_widget(&self, icon: &DesktopIcon) -> gtk4::Box {
        let col = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        col.add_css_class("glaze-desktop-icon");
        col.set_size_request(CELL_W - 8, CELL_H - 8);
        let name = icon
            .icon
            .clone()
            .unwrap_or_else(|| "application-x-executable".into());
        let image = if name.starts_with('/') {
            gtk4::Image::from_file(&name)
        } else {
            gtk4::Image::from_icon_name(&name)
        };
        image.set_pixel_size(44);
        col.append(&image);
        let label = gtk4::Label::new(Some(&icon.label));
        label.add_css_class("glaze-desktop-icon-label");
        label.set_wrap(true);
        label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        label.set_justify(gtk4::Justification::Center);
        label.set_lines(2);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(12);
        col.append(&label);
        col.set_tooltip_text(Some(&icon.label));
        col
    }

    fn item_at(&self, x: f64, y: f64) -> Option<Rc<Item>> {
        self.items
            .borrow()
            .iter()
            .find(|it| {
                let (ix, iy) = (
                    (MARGIN + it.col.get() * CELL_W) as f64,
                    (MARGIN + it.row.get() * CELL_H) as f64,
                );
                x >= ix && x < ix + (CELL_W - 8) as f64 && y >= iy && y < iy + (CELL_H - 8) as f64
            })
            .cloned()
    }

    fn order(&self) -> Vec<String> {
        let mut v: Vec<(i32, i32, String)> = self
            .items
            .borrow()
            .iter()
            .map(|it| (it.col.get(), it.row.get(), it.icon.key()))
            .collect();
        v.sort();
        v.into_iter().map(|(_, _, k)| k).collect()
    }

    fn sync_selection(&self) {
        let sel = self.sel.borrow();
        for it in self.items.borrow().iter() {
            if sel.is_selected(&it.icon.key()) {
                it.widget.add_css_class("glaze-selected");
            } else {
                it.widget.remove_css_class("glaze-selected");
            }
        }
    }

    fn selected(&self) -> Vec<Rc<Item>> {
        let sel = self.sel.borrow();
        self.items
            .borrow()
            .iter()
            .filter(|it| sel.is_selected(&it.icon.key()))
            .cloned()
            .collect()
    }

    fn install_gestures(self: &Rc<Self>) {
        // Click: select / extend / open.
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        {
            let m = Rc::downgrade(self);
            click.connect_pressed(move |g, n, x, y| {
                let Some(m) = m.upgrade() else { return };
                m.fixed.grab_focus();
                let state = g.current_event_state();
                let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
                let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
                match m.item_at(x, y) {
                    Some(it) => {
                        let key = it.icon.key();
                        let already = m.sel.borrow().is_selected(&key);
                        if !(already && !ctrl && !shift && n == 1) || n > 1 {
                            let order = m.order();
                            m.sel.borrow_mut().click(&key, ctrl, shift, &order);
                        }
                        m.sync_selection();
                        if n == 2 {
                            m.open(&it);
                        }
                    }
                    None => {
                        if !ctrl {
                            m.sel.borrow_mut().clear();
                            m.sync_selection();
                        }
                    }
                }
            });
        }
        self.fixed.add_controller(click);

        // Drag: move the selection, or draw a marquee.
        let drag = gtk4::GestureDrag::new();
        drag.set_button(1);
        {
            let m = Rc::downgrade(self);
            drag.connect_drag_begin(move |_, x, y| {
                let Some(m) = m.upgrade() else { return };
                if m.item_at(x, y).is_some() {
                    let starts = m
                        .selected()
                        .into_iter()
                        .map(|it| {
                            let (ix, iy) = (
                                (MARGIN + it.col.get() * CELL_W) as f64,
                                (MARGIN + it.row.get() * CELL_H) as f64,
                            );
                            (it, ix, iy)
                        })
                        .collect();
                    *m.drag_move.borrow_mut() = Some(starts);
                } else {
                    m.drag_move.borrow_mut().take();
                    m.band.set(Some((x, y, 0.0, 0.0)));
                    m.marquee.set_opacity(1.0);
                }
            });
        }
        {
            let m = Rc::downgrade(self);
            drag.connect_drag_update(move |g, dx, dy| {
                let Some(m) = m.upgrade() else { return };
                let moving = m.drag_move.borrow().clone();
                if let Some(starts) = moving {
                    for (it, ix, iy) in &starts {
                        m.fixed.move_(&it.widget, ix + dx, iy + dy);
                    }
                    return;
                }
                let Some((sx, sy)) = g.start_point() else {
                    return;
                };
                let (x, y, w, h) = (sx.min(sx + dx), sy.min(sy + dy), dx.abs(), dy.abs());
                m.band.set(Some((x, y, w, h)));
                m.marquee.queue_draw();
                // Select what the band touches.
                let mut sel = Selection::new();
                let order = m.order();
                for it in m.items.borrow().iter() {
                    let (ix, iy) = (
                        (MARGIN + it.col.get() * CELL_W) as f64,
                        (MARGIN + it.row.get() * CELL_H) as f64,
                    );
                    let hit = ix < x + w
                        && ix + (CELL_W - 8) as f64 > x
                        && iy < y + h
                        && iy + (CELL_H - 8) as f64 > y;
                    if hit {
                        sel.click(&it.icon.key(), true, false, &order);
                    }
                }
                *m.sel.borrow_mut() = sel;
                m.sync_selection();
            });
        }
        {
            let m = Rc::downgrade(self);
            drag.connect_drag_end(move |_, dx, dy| {
                let Some(m) = m.upgrade() else { return };
                let finished = m.drag_move.borrow_mut().take();
                if let Some(starts) = finished {
                    // Snap to the nearest free cells (moved icons first).
                    let rows = m.rows();
                    let moving: Vec<String> =
                        starts.iter().map(|(it, _, _)| it.icon.key()).collect();
                    let mut taken: Vec<(i32, i32)> = m
                        .items
                        .borrow()
                        .iter()
                        .filter(|it| !moving.contains(&it.icon.key()))
                        .map(|it| (it.col.get(), it.row.get()))
                        .collect();
                    for (it, ix, iy) in &starts {
                        let mut c = (((ix + dx) - MARGIN as f64) / CELL_W as f64)
                            .round()
                            .max(0.0) as i32;
                        let mut r = (((iy + dy) - MARGIN as f64) / CELL_H as f64)
                            .round()
                            .clamp(0.0, (rows - 1) as f64)
                            as i32;
                        while taken.contains(&(c, r)) {
                            r += 1;
                            if r >= rows {
                                r = 0;
                                c += 1;
                            }
                        }
                        taken.push((c, r));
                        it.col.set(c);
                        it.row.set(r);
                        m.place(it);
                    }
                    m.save_positions();
                } else if m.band.get().is_some() {
                    m.band.set(None);
                    m.marquee.queue_draw();
                }
            });
        }
        self.fixed.add_controller(drag);

        // Right-click.
        let rc = gtk4::GestureClick::new();
        rc.set_button(3);
        {
            let m = Rc::downgrade(self);
            rc.connect_pressed(move |_, _, x, y| {
                let Some(m) = m.upgrade() else { return };
                match m.item_at(x, y) {
                    Some(it) => {
                        let key = it.icon.key();
                        if !m.sel.borrow().is_selected(&key) {
                            let order = m.order();
                            m.sel.borrow_mut().click(&key, false, false, &order);
                            m.sync_selection();
                        }
                        m.icon_menu(&it, x, y);
                    }
                    None => (m.on_background_menu)(x, y),
                }
            });
        }
        self.fixed.add_controller(rc);

        // Keyboard.
        let keys = gtk4::EventControllerKey::new();
        {
            let m = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, k, _, state| {
                let Some(m) = m.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                m.on_key(k, state)
            });
        }
        self.fixed.add_controller(keys);
    }

    fn on_key(self: &Rc<Self>, k: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        let sel = self.selected();
        match k {
            gdk::Key::Return | gdk::Key::KP_Enter => {
                for it in &sel {
                    self.open(it);
                }
            }
            gdk::Key::F2 => {
                if let Some(it) = sel.first() {
                    self.rename(it);
                }
            }
            gdk::Key::Delete => self.trash(&sel),
            gdk::Key::a if state.contains(gdk::ModifierType::CONTROL_MASK) => {
                let order = self.order();
                self.sel.borrow_mut().all(&order);
                self.sync_selection();
            }
            gdk::Key::Up | gdk::Key::Down | gdk::Key::Left | gdk::Key::Right => {
                let (dc, dr) = match k {
                    gdk::Key::Up => (0, -1),
                    gdk::Key::Down => (0, 1),
                    gdk::Key::Left => (-1, 0),
                    _ => (1, 0),
                };
                let from = sel
                    .first()
                    .map(|it| (it.col.get(), it.row.get()))
                    .unwrap_or((0, -1));
                let target = (from.0 + dc, from.1 + dr);
                let hit = self
                    .items
                    .borrow()
                    .iter()
                    .find(|it| (it.col.get(), it.row.get()) == target)
                    .cloned()
                    .or_else(|| self.items.borrow().first().cloned());
                if let Some(it) = hit {
                    let order = self.order();
                    self.sel
                        .borrow_mut()
                        .click(&it.icon.key(), false, false, &order);
                    self.sync_selection();
                }
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    /// Line every icon up again, column by column, sorted by name (Home and
    /// Trash first).
    pub fn arrange_by_name(&self) {
        let rows = self.rows();
        let mut items = self.items.borrow().clone();
        items.sort_by_key(|it| {
            let fixed = matches!(it.icon.target, IconTarget::Home | IconTarget::Trash);
            (!fixed, it.icon.label.to_lowercase())
        });
        for (i, it) in items.iter().enumerate() {
            it.col.set(i as i32 / rows);
            it.row.set(i as i32 % rows);
            self.place(it);
        }
        self.save_positions();
    }

    fn open(&self, it: &Item) {
        match &it.icon.target {
            IconTarget::File(p) if p.extension().and_then(|e| e.to_str()) == Some("desktop") => {
                let path = p.display().to_string();
                crate::classic_menu::run_argv(&["gio", "launch", &path]);
            }
            IconTarget::File(p) => crate::classic_menu::open_path(&p.display().to_string()),
            IconTarget::Home => {
                let h = glaze_common::paths::home_dir().unwrap_or_default();
                crate::classic_menu::open_path(&h.display().to_string());
            }
            IconTarget::Trash => crate::classic_menu::open_path("trash:///"),
            _ => {
                if let Some(argv) =
                    glaze_desktop::icons::resolve_target(&it.icon, self.registry.apps())
                {
                    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                    crate::classic_menu::run_argv(&refs);
                }
            }
        }
    }

    fn file_of(it: &Item) -> Option<&Path> {
        match &it.icon.target {
            IconTarget::File(p) => Some(p.as_path()),
            _ => None,
        }
    }

    fn rename(self: &Rc<Self>, it: &Rc<Item>) {
        let Some(path) = Self::file_of(it).map(Path::to_path_buf) else {
            crate::toast::show(
                "Cannot rename",
                &format!(
                    "\u{201c}{}\u{201d} is not a file on the desktop.",
                    it.icon.label
                ),
            );
            return;
        };
        let pop = gtk4::Popover::new();
        pop.add_css_class("glaze-popover");
        pop.set_has_arrow(false);
        pop.set_parent(&it.widget);
        pop.set_position(gtk4::PositionType::Bottom);
        let entry = gtk4::Entry::new();
        entry.add_css_class("glaze-run-entry");
        entry.set_text(
            &path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        pop.set_child(Some(&entry));
        let p2 = pop.clone();
        entry.connect_activate(move |e| {
            let new = e.text().to_string();
            if !new.trim().is_empty() && !new.contains('/') {
                let to = path.with_file_name(new.trim());
                if let Err(err) = std::fs::rename(&path, &to) {
                    crate::toast::show("Rename failed", &err.to_string());
                }
            }
            p2.popdown();
        });
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    fn trash(&self, items: &[Rc<Item>]) {
        let mut n = 0;
        for it in items {
            if let Some(p) = Self::file_of(it) {
                match gio::File::for_path(p).trash(gio::Cancellable::NONE) {
                    Ok(()) => n += 1,
                    Err(e) => crate::toast::show("Could not move to the trash", &e.to_string()),
                }
            }
        }
        if n > 0 {
            crate::toast::show("Moved to the trash", &format!("{n} item(s)"));
        }
    }

    fn properties(&self, it: &Item) {
        let body = match Self::file_of(it) {
            Some(p) => {
                let meta = std::fs::metadata(p).ok();
                let kind = if p.is_dir() { "Folder" } else { "File" };
                let size = meta.map(|m| m.len()).unwrap_or(0);
                format!("{kind} \u{00b7} {} bytes \u{00b7} {}", size, p.display())
            }
            None => match &it.icon.target {
                IconTarget::Home => "Your home folder".into(),
                IconTarget::Trash => "Deleted files, until the trash is emptied".into(),
                _ => it.icon.app_id.clone().unwrap_or_default(),
            },
        };
        crate::toast::show(&it.icon.label, &body);
    }

    fn icon_menu(self: &Rc<Self>, it: &Rc<Item>, x: f64, y: f64) {
        let is_file = Self::file_of(it).is_some();
        let n = self.selected().len();
        let (m1, i1) = (self.clone(), it.clone());
        let (m2, i2) = (self.clone(), it.clone());
        let m3 = self.clone();
        let (m4, i4) = (self.clone(), it.clone());
        let v = vec![
            Entry::Header(if n > 1 {
                format!("{n} items")
            } else {
                it.icon.label.clone()
            }),
            Entry::item(Some("folder"), "Open", move || {
                for s in m1.selected() {
                    m1.open(&s);
                }
                let _ = &i1;
            }),
            Entry::item(None, "Rename", move || m2.rename(&i2))
                .accel("F2")
                .enabled_if(is_file && n == 1),
            Entry::item(Some("window-close"), "Delete", move || {
                let sel = m3.selected();
                m3.trash(&sel)
            })
            .accel("Delete")
            .danger()
            .enabled_if(is_file),
            Entry::Separator,
            Entry::item(Some("settings"), "Properties", move || m4.properties(&i4)),
        ];
        crate::menu::popup_default(
            &self.fixed,
            Some(gdk::Rectangle::new(x as i32, y as i32, 1, 1)),
            gtk4::PositionType::Bottom,
            v,
        );
    }
}
