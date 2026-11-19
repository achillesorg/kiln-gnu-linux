//! Two-pane Start menu (`launcher.style = "two-pane"`, the default).
//!
//! Layout (Windows 10 shape, Glaze paper look): a tall card flush to the
//! taskbar's top-left corner with
//! - a 48 px icon rail (menu toggle on top; account, documents, pictures,
//!   settings and power at the bottom). The menu toggle expands the rail
//!   over the list with labels (width spring), without resizing the popup;
//! - an A-Z app list with letter headers; a header opens a letter grid to
//!   jump; typing filters the list (local search only);
//! - a tile area with named groups of flat square tiles in three sizes
//!   (small 1x1, wide 2x1, large 2x2 on a 100 px grid) in paper, ink and
//!   terracotta fills. Tiles can be dragged to reorder, and right-click
//!   unpins or resizes them; the layout is kept in
//!   `$XDG_STATE_HOME/glaze/start-tiles.json`.
//!
//! Motion: the card slides up 28 px out of the taskbar edge with the items
//! fading in a beat later; closing slides down 12 px and fades faster. The
//! popover surface keeps one size the whole time (the slide is drawn by a
//! `SlideBin`), so there is no first-frame jump.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glaze_common::config::PanelPosition;
use glaze_common::model::AppEntry;
use gtk4::prelude::*;
use gtk4::{gdk, glib};
use serde::{Deserialize, Serialize};

use crate::anim::Animator;
use crate::icons;
use crate::ink::Glyph;
use crate::menu::{self, Entry};
use crate::panel_ui::{self, Ctx};
use crate::slide::SlideBin;

const RAIL: i32 = 48;
const RAIL_OPEN: i32 = 208;
const LIST_W: i32 = 272;
const CELL: i32 = 100;
const GAP: i32 = 4;
const COLS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TileSize {
    Small,
    Wide,
    Large,
}

impl TileSize {
    fn span(self) -> (usize, usize) {
        match self {
            TileSize::Small => (1, 1),
            TileSize::Wide => (2, 1),
            TileSize::Large => (2, 2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Tile {
    app: String,
    size: TileSize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Group {
    name: String,
    tiles: Vec<Tile>,
}

fn tiles_path() -> std::path::PathBuf {
    glaze_common::paths::state_home().join("glaze/start-tiles.json")
}

/// First installed desktop id among `ids`, else an app in `category`.
fn pick(apps: &[AppEntry], ids: &[&str], category: &str) -> Option<String> {
    ids.iter()
        .find_map(|id| apps.iter().find(|a| a.id == *id && !a.no_display))
        .or_else(|| {
            apps.iter()
                .find(|a| !a.no_display && a.categories.iter().any(|c| c == category))
        })
        .map(|a| a.id.clone())
}

fn default_groups(apps: &[AppEntry]) -> Vec<Group> {
    let t = |ids: &[&str], cat: &str, size: TileSize| {
        pick(apps, ids, cat).map(|app| Tile { app, size })
    };
    let productivity = [
        t(
            &["kiln-files", "thunar", "org.gnome.Nautilus"],
            "FileManager",
            TileSize::Large,
        ),
        t(&["firefox", "firefox-esr"], "WebBrowser", TileSize::Wide),
        t(
            &["kiln-terminal", "foot"],
            "TerminalEmulator",
            TileSize::Small,
        ),
        t(
            &["kiln-text", "org.gnome.TextEditor"],
            "TextEditor",
            TileSize::Small,
        ),
        t(
            &["kiln-calculator", "org.gnome.Calculator"],
            "Calculator",
            TileSize::Small,
        ),
    ];
    let explore = [
        t(&["kiln-store"], "PackageManager", TileSize::Wide),
        t(&["glaze-settings"], "Settings", TileSize::Small),
        t(
            &["org.gnome.Loupe", "org.gnome.Evince"],
            "Viewer",
            TileSize::Small,
        ),
        t(
            &["kiln-monitor", "org.gnome.SystemMonitor"],
            "Monitor",
            TileSize::Small,
        ),
    ];
    let mut dedupe = Vec::new();
    let mut mk = |name: &str, list: &[Option<Tile>]| Group {
        name: name.into(),
        tiles: list
            .iter()
            .flatten()
            .filter(|t| {
                let fresh = !dedupe.contains(&t.app);
                dedupe.push(t.app.clone());
                fresh
            })
            .cloned()
            .collect(),
    };
    vec![mk("Productivity", &productivity), mk("Explore", &explore)]
}

fn load_groups(apps: &[AppEntry]) -> Vec<Group> {
    std::fs::read_to_string(tiles_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Group>>(&s).ok())
        .unwrap_or_else(|| default_groups(apps))
}

fn save_groups(groups: &[Group]) {
    let p = tiles_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(s) = serde_json::to_string_pretty(groups) {
        let _ = std::fs::write(p, s);
    }
}

/// Row-major packing of tiles into a `COLS`-wide grid.
fn pack(tiles: &[Tile]) -> Vec<(usize, usize)> {
    let mut used: Vec<[bool; COLS]> = Vec::new();
    let mut out = Vec::new();
    for t in tiles {
        let (w, h) = t.size.span();
        let mut placed = None;
        let mut row = 0;
        while placed.is_none() {
            while used.len() < row + h {
                used.push([false; COLS]);
            }
            for col in 0..=(COLS - w) {
                let free = (row..row + h).all(|r| (col..col + w).all(|c| !used[r][c]));
                if free {
                    placed = Some((col, row));
                    break;
                }
            }
            if placed.is_none() {
                row += 1;
            }
        }
        let (col, row) = placed.unwrap_or((0, 0));
        for line in used.iter_mut().skip(row).take(h) {
            for cell in line.iter_mut().skip(col).take(w) {
                *cell = true;
            }
        }
        out.push((col, row));
    }
    out
}

struct Pane {
    pop: gtk4::Popover,
    card: gtk4::Box,
    slide: SlideBin,
    content: gtk4::Box,
    list_box: gtk4::Box,
    list_scroll: gtk4::ScrolledWindow,
    tiles_box: gtk4::Box,
    rail: gtk4::Box,
    rail_labels: RefCell<Vec<gtk4::Label>>,
    rail_open: Cell<bool>,
    jump: gtk4::Box,
    ctx: Ctx,
    anim: Animator,
    query: RefCell<String>,
    groups: RefCell<Vec<Group>>,
    headers: RefCell<Vec<(char, gtk4::Widget)>>,
    travel: Cell<f64>,
    closing: Cell<bool>,
    /// Last applied open progress (0..1), so a toggle mid-animation reverses from here.
    prog: Cell<f64>,
    closed_at: Cell<Option<std::time::Instant>>,
    kbd: RefCell<Option<Box<dyn FnOnce()>>>,
    position: PanelPosition,
}

/// Wire the two-pane Start menu to the Start button.
pub fn attach(button: &gtk4::Button, ctx: &Ctx, position: PanelPosition) {
    let pop = gtk4::Popover::new();
    pop.add_css_class("glaze-popover");
    pop.add_css_class("glaze-start-pane");
    pop.set_has_arrow(false);
    pop.set_autohide(true);
    pop.set_parent(button);
    pop.set_position(panel_ui::popover_position(position));
    pop.set_halign(gtk4::Align::Start);

    // Height: tall, flush to the bar, leaving a little air at the top.
    let avail = gdk::Display::default()
        .and_then(|d| d.monitors().item(0))
        .and_then(|o| o.downcast::<gdk::Monitor>().ok())
        .map(|m| m.geometry().height() - panel_ui::panel_thickness(ctx))
        .unwrap_or(760);
    let height = (avail - 40).clamp(420, 720);
    let tiles_w = COLS as i32 * CELL + (COLS as i32 - 1) * GAP + 32;
    let width = RAIL + LIST_W + tiles_w;

    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("glaze-start-pane-card");
    card.set_size_request(width, height);

    let overlay = gtk4::Overlay::new();
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let rail_space = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rail_space.set_size_request(RAIL, -1);
    content.append(&rail_space);

    let list_scroll = gtk4::ScrolledWindow::new();
    list_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    list_scroll.set_size_request(LIST_W, -1);
    list_scroll.add_css_class("glaze-start-list");
    let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    list_scroll.set_child(Some(&list_box));
    content.append(&list_scroll);

    let tiles_scroll = gtk4::ScrolledWindow::new();
    tiles_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    tiles_scroll.set_hexpand(true);
    let tiles_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    tiles_box.add_css_class("glaze-start-tiles");
    tiles_scroll.set_child(Some(&tiles_box));
    content.append(&tiles_scroll);
    overlay.set_child(Some(&content));

    let rail = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rail.add_css_class("glaze-start-rail");
    rail.set_halign(gtk4::Align::Start);
    rail.set_size_request(RAIL, -1);
    overlay.add_overlay(&rail);

    // Letter grid for jump-to-letter (hidden until a header is clicked).
    let jump = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    jump.add_css_class("glaze-start-jump");
    jump.set_halign(gtk4::Align::Start);
    jump.set_valign(gtk4::Align::Start);
    jump.set_margin_start(RAIL + 16);
    jump.set_margin_top(16);
    jump.set_visible(false);
    overlay.add_overlay(&jump);

    card.append(&overlay);
    let slide = SlideBin::new(&card);
    pop.set_child(Some(&slide));

    let me = Rc::new(Pane {
        pop: pop.clone(),
        card,
        slide,
        content,
        list_box,
        list_scroll,
        tiles_box,
        rail,
        rail_labels: RefCell::new(Vec::new()),
        rail_open: Cell::new(false),
        jump,
        ctx: ctx.clone(),
        anim: Animator::new(
            "start-pane",
            button,
            ctx.motion,
            ctx.cfg.motion.log_frame_times,
        ),
        query: RefCell::new(String::new()),
        groups: RefCell::new(load_groups(ctx.registry.apps())),
        headers: RefCell::new(Vec::new()),
        travel: Cell::new(28.0),
        closing: Cell::new(false),
        prog: Cell::new(0.0),
        closed_at: Cell::new(None),
        kbd: RefCell::new(None),
        position,
    });
    me.build_rail();
    me.build_list();
    me.build_tiles();

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
            // A key toggle (lone Super) is never swallowed; the guard only
            // absorbs the pointer click that dismissed the popover a moment ago.
            let forced = crate::launcher::take_forced_toggle();
            let just_closed = !forced
                && m.closed_at
                    .get()
                    .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(250));
            if m.pop.is_visible() {
                if m.closing.get() {
                    m.reopen();
                } else {
                    m.close();
                }
            } else if just_closed {
                m.closed_at.set(None);
            } else {
                *m.kbd.borrow_mut() = Some(menu::hold_keyboard(b.upcast_ref()));
                b.add_css_class("glaze-open");
                m.open();
            }
        });
    }
    {
        let m = me.clone();
        let b = button.clone();
        pop.connect_closed(move |_| {
            eprintln!(
                "[glaze-shell] start pane: closed (closing={})",
                m.closing.get()
            );
            m.closed_at.set(Some(std::time::Instant::now()));
            if let Some(r) = m.kbd.borrow_mut().take() {
                r();
            }
            b.remove_css_class("glaze-open");
            m.set_rail(false);
            m.jump.set_visible(false);
            if !m.query.borrow().is_empty() {
                m.query.borrow_mut().clear();
                m.build_list();
            }
        });
    }
}

impl Pane {
    fn apply_progress(&self, p: f64) {
        let p = p.clamp(0.0, 1.0);
        self.prog.set(p);
        self.card.set_opacity((p / 0.4).min(1.0));
        self.content
            .set_opacity(((p - 0.25) / 0.75).clamp(0.0, 1.0));
        let dir = if self.position == PanelPosition::Top {
            -1.0
        } else {
            1.0
        };
        self.slide.set_offset(dir * self.travel.get() * (1.0 - p));
    }

    fn open(self: &Rc<Self>) {
        eprintln!("[glaze-shell] start pane: open");
        self.closing.set(false);
        self.travel.set(28.0);
        self.apply_progress(0.0);
        let weak = Rc::downgrade(self);
        crate::catcher::guard(
            &self.pop,
            Rc::new(move || {
                if let Some(m) = weak.upgrade() {
                    m.close();
                }
            }),
        );
        self.pop.popup();
        self.pop.grab_focus();
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

    /// Toggle pressed while the close animation runs: turn around from the
    /// current progress (spring keeps its velocity), no restart from zero.
    fn reopen(self: &Rc<Self>) {
        self.closing.set(false);
        let me = Rc::downgrade(self);
        self.anim.animate(
            "p",
            self.prog.get(),
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

    fn close(self: &Rc<Self>) {
        if self.closing.replace(true) {
            return;
        }
        self.travel.set(28.0);
        let me = Rc::downgrade(self);
        let me2 = self.clone();
        let from = self.prog.get();
        self.anim.animate(
            "p",
            from,
            0.0,
            self.ctx.tokens.metrics.motion_micro_ms as f64,
            move |v| {
                if let Some(m) = me.upgrade() {
                    m.apply_progress(v);
                }
            },
            Some(Box::new(move || {
                if me2.closing.get() {
                    me2.pop.popdown();
                }
            })),
        );
    }

    fn launch(self: &Rc<Self>, id: &str) {
        if let Some(app) = self.ctx.registry.get(id) {
            panel_ui::launch(app);
        }
        self.close();
    }

    // ---- rail --------------------------------------------------------------

    fn rail_item(
        self: &Rc<Self>,
        name: &str,
        glyph: Glyph,
        label: &str,
        on: impl Fn(&Rc<Pane>) + 'static,
    ) -> gtk4::Button {
        let b = gtk4::Button::new();
        b.add_css_class("glaze-rail-item");
        b.set_tooltip_text(Some(label));
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        let icon = icons::icon(name, glyph, 18, self.ctx.ink(), self.ctx.stroke());
        icon.set_size_request(RAIL - 2, -1);
        line.append(&icon);
        let l = gtk4::Label::new(Some(label));
        l.set_xalign(0.0);
        l.set_visible(false);
        line.append(&l);
        self.rail_labels.borrow_mut().push(l);
        b.set_child(Some(&line));
        let m = Rc::downgrade(self);
        b.connect_clicked(move |_| {
            if let Some(m) = m.upgrade() {
                on(&m);
            }
        });
        b
    }

    fn build_rail(self: &Rc<Self>) {
        let menu_b = self.rail_item("apps", Glyph::Window, "Start", |m| {
            m.set_rail(!m.rail_open.get())
        });
        self.rail.append(&menu_b);
        let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        self.rail.append(&spacer);
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let home = glaze_common::paths::home_dir().unwrap_or_default();
        let (docs, pics) = (
            home.join("Documents").display().to_string(),
            home.join("Pictures").display().to_string(),
        );
        let items: Vec<gtk4::Button> = vec![
            self.rail_item("lock", Glyph::Lock, &user, |m| {
                let h = glaze_common::paths::home_dir().unwrap_or_default();
                crate::classic_menu::open_path(&h.display().to_string());
                m.close();
            }),
            self.rail_item("folder", Glyph::Folder, "Documents", move |m| {
                crate::classic_menu::open_path(&docs);
                m.close();
            }),
            self.rail_item("folder", Glyph::Folder, "Pictures", move |m| {
                crate::classic_menu::open_path(&pics);
                m.close();
            }),
            self.rail_item("settings", Glyph::Settings, "Settings", |m| {
                m.launch("glaze-settings")
            }),
            self.rail_item("power", Glyph::Power, "Power", |m| m.power_menu()),
        ];
        for b in items {
            self.rail.append(&b);
        }
    }

    /// Expand the rail over the list (labels shown) or collapse it.
    fn set_rail(self: &Rc<Self>, open: bool) {
        if self.rail_open.replace(open) == open {
            return;
        }
        if open {
            self.rail.add_css_class("glaze-rail-open");
            for l in self.rail_labels.borrow().iter() {
                l.set_visible(true);
            }
        }
        let rail = self.rail.clone();
        let me = Rc::downgrade(self);
        let (from, to) = if open {
            (RAIL, RAIL_OPEN)
        } else {
            (RAIL_OPEN, RAIL)
        };
        self.anim.animate(
            "rail",
            from as f64,
            to as f64,
            self.ctx.tokens.metrics.motion_menu_ms as f64,
            move |v| rail.set_size_request(v.round() as i32, -1),
            Some(Box::new(move || {
                if let Some(m) = me.upgrade() {
                    if !m.rail_open.get() {
                        m.rail.remove_css_class("glaze-rail-open");
                        for l in m.rail_labels.borrow().iter() {
                            l.set_visible(false);
                        }
                    }
                }
            })),
        );
    }

    fn power_menu(self: &Rc<Self>) {
        let mk = |icon: &'static str, label: &str, argv: &'static [&'static str]| {
            Entry::item(Some(icon), label, move || {
                crate::classic_menu::run_argv(argv);
            })
        };
        let v = vec![
            mk("lock", "Lock", &["loginctl", "lock-session"]),
            mk("sleep", "Sleep", &["systemctl", "suspend"]),
            mk("logout", "Log off", &["swaymsg", "exit"]),
            Entry::Separator,
            mk("restart", "Restart", &["systemctl", "reboot"]),
            mk("power", "Shut down", &["systemctl", "poweroff"]),
        ];
        menu::popup(&self.rail, None, gtk4::PositionType::Right, &self.ctx, v);
    }

    // ---- A-Z list ----------------------------------------------------------

    fn build_list(self: &Rc<Self>) {
        while let Some(c) = self.list_box.first_child() {
            self.list_box.remove(&c);
        }
        self.headers.borrow_mut().clear();
        let q = self.query.borrow().to_lowercase();
        let mut apps: Vec<&AppEntry> = self
            .ctx
            .registry
            .apps()
            .iter()
            .filter(|a| !a.no_display)
            .filter(|a| q.is_empty() || a.search_blob().contains(&q))
            .collect();
        apps.sort_by_key(|a| a.name.to_lowercase());
        if !q.is_empty() {
            let h = gtk4::Label::new(Some(&if apps.is_empty() {
                format!("No apps match \u{201c}{q}\u{201d}")
            } else {
                format!("Apps matching \u{201c}{q}\u{201d}")
            }));
            h.add_css_class("glaze-start-letter-label");
            h.set_xalign(0.0);
            self.list_box.append(&h);
        }
        let mut current: Option<char> = None;
        for a in apps {
            let first = a
                .name
                .chars()
                .next()
                .map(|c| {
                    if c.is_ascii_alphabetic() {
                        c.to_ascii_uppercase()
                    } else {
                        '#'
                    }
                })
                .unwrap_or('#');
            if q.is_empty() && current != Some(first) {
                current = Some(first);
                let hb = gtk4::Button::with_label(&first.to_string());
                hb.add_css_class("glaze-start-letter");
                hb.set_halign(gtk4::Align::Start);
                let m = Rc::downgrade(self);
                hb.connect_clicked(move |_| {
                    if let Some(m) = m.upgrade() {
                        m.show_jump();
                    }
                });
                self.list_box.append(&hb);
                self.headers.borrow_mut().push((first, hb.upcast()));
            }
            let row = gtk4::Button::new();
            row.add_css_class("glaze-start-row");
            let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
            line.append(&panel_ui::app_icon_sized(&self.ctx, &a.id, Some(a), 24));
            let l = gtk4::Label::new(Some(&a.name));
            l.set_xalign(0.0);
            l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            line.append(&l);
            row.set_child(Some(&line));
            let id = a.id.clone();
            let m = Rc::downgrade(self);
            row.connect_clicked(move |_| {
                if let Some(m) = m.upgrade() {
                    m.launch(&id);
                }
            });
            // Right-click: pin to Start (adds a small tile) / open.
            let id2 = a.id.clone();
            let m2 = Rc::downgrade(self);
            let g = gtk4::GestureClick::new();
            g.set_button(3);
            let rw = row.clone();
            g.connect_released(move |_, _, _, _| {
                let Some(m) = m2.upgrade() else { return };
                let (ma, ia) = (m.clone(), id2.clone());
                let (mb, ib) = (m.clone(), id2.clone());
                menu::popup(
                    &rw,
                    None,
                    gtk4::PositionType::Right,
                    &m.ctx,
                    vec![
                        Entry::item(Some("start"), "Open", move || ma.launch(&ia)),
                        Entry::item(Some("pin"), "Pin to Start", move || mb.pin_tile(&ib)),
                    ],
                );
            });
            row.add_controller(g);
            self.list_box.append(&row);
        }
    }

    fn show_jump(self: &Rc<Self>) {
        while let Some(c) = self.jump.first_child() {
            self.jump.remove(&c);
        }
        let grid = gtk4::Grid::new();
        grid.set_row_spacing(2);
        grid.set_column_spacing(2);
        let present: Vec<char> = self.headers.borrow().iter().map(|(c, _)| *c).collect();
        let letters: Vec<char> = std::iter::once('#').chain('A'..='Z').collect();
        for (i, c) in letters.iter().enumerate() {
            let b = gtk4::Button::with_label(&c.to_string());
            b.add_css_class("glaze-start-jump-key");
            b.set_sensitive(present.contains(c));
            let (m, c) = (Rc::downgrade(self), *c);
            b.connect_clicked(move |_| {
                if let Some(m) = m.upgrade() {
                    m.jump_to(c);
                }
            });
            grid.attach(&b, (i % 4) as i32, (i / 4) as i32, 1, 1);
        }
        self.jump.append(&grid);
        self.jump.set_visible(true);
    }

    fn jump_to(self: &Rc<Self>, c: char) {
        self.jump.set_visible(false);
        let target = self
            .headers
            .borrow()
            .iter()
            .find(|(h, _)| *h == c)
            .map(|(_, w)| w.clone());
        let Some(w) = target else { return };
        let y = w
            .compute_point(&self.list_box, &gtk4::graphene::Point::new(0.0, 0.0))
            .map(|p| p.y() as f64)
            .unwrap_or(0.0);
        let adj = self.list_scroll.vadjustment();
        let from = adj.value();
        let to = y.min(adj.upper() - adj.page_size()).max(0.0);
        self.anim.animate(
            "scroll",
            from,
            to,
            self.ctx.tokens.metrics.motion_menu_ms as f64,
            move |v| adj.set_value(v),
            None,
        );
    }

    fn on_key(self: &Rc<Self>, k: gdk::Key) -> glib::Propagation {
        match k {
            gdk::Key::Escape => {
                if self.jump.is_visible() {
                    self.jump.set_visible(false);
                } else if self.rail_open.get() {
                    self.set_rail(false);
                } else if !self.query.borrow().is_empty() {
                    self.query.borrow_mut().clear();
                    self.build_list();
                } else {
                    self.close();
                }
                glib::Propagation::Stop
            }
            gdk::Key::BackSpace => {
                if self.query.borrow_mut().pop().is_some() {
                    self.build_list();
                }
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                // Enter launches the first match while searching.
                if !self.query.borrow().is_empty() {
                    let q = self.query.borrow().to_lowercase();
                    let mut apps: Vec<&AppEntry> = self
                        .ctx
                        .registry
                        .apps()
                        .iter()
                        .filter(|a| !a.no_display && a.search_blob().contains(&q))
                        .collect();
                    apps.sort_by_key(|a| a.name.to_lowercase());
                    if let Some(a) = apps.first() {
                        let id = a.id.clone();
                        self.launch(&id);
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
            _ => match k.to_unicode().filter(|c| !c.is_control()) {
                Some(c) => {
                    self.query.borrow_mut().push(c);
                    self.build_list();
                    self.list_scroll.vadjustment().set_value(0.0);
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            },
        }
    }

    // ---- tiles -------------------------------------------------------------

    fn build_tiles(self: &Rc<Self>) {
        while let Some(c) = self.tiles_box.first_child() {
            self.tiles_box.remove(&c);
        }
        let groups = self.groups.borrow().clone();
        for (gi, g) in groups.iter().enumerate() {
            let h = gtk4::Label::new(Some(&g.name));
            h.add_css_class("glaze-tile-group");
            h.set_xalign(0.0);
            self.tiles_box.append(&h);
            let grid = gtk4::Grid::new();
            grid.set_row_spacing(GAP as u32);
            grid.set_column_spacing(GAP as u32);
            grid.set_column_homogeneous(true);
            let places = pack(&g.tiles);
            let last_row = places
                .iter()
                .zip(&g.tiles)
                .map(|((_, r), t)| r + t.size.span().1)
                .max()
                .unwrap_or(0);
            for (ti, (t, (col, row))) in g.tiles.iter().zip(places).enumerate() {
                let (w, hh) = t.size.span();
                let tile = self.tile_widget(t, gi, ti, ti % 3);
                grid.attach(&tile, col as i32, row as i32, w as i32, hh as i32);
            }
            // Keep the columns at CELL even with only wide tiles.
            for c in 0..COLS {
                let ghost = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                ghost.set_size_request(CELL, 0);
                grid.attach(&ghost, c as i32, last_row as i32, 1, 1);
            }
            self.tiles_box.append(&grid);
        }
    }

    fn tile_widget(self: &Rc<Self>, t: &Tile, gi: usize, ti: usize, tone: usize) -> gtk4::Widget {
        let app = self.ctx.registry.get(&t.app).cloned();
        let (w, h) = t.size.span();
        let b = gtk4::Button::new();
        b.add_css_class("glaze-tile");
        b.add_css_class(["glaze-tile-accent", "glaze-tile-ink", "glaze-tile-paper"][tone]);
        b.set_size_request(
            CELL * w as i32 + GAP * (w as i32 - 1),
            CELL * h as i32 + GAP * (h as i32 - 1),
        );
        let ov = gtk4::Overlay::new();
        let px = if t.size == TileSize::Large { 48 } else { 32 };
        let icon = panel_ui::app_icon_sized(&self.ctx, &t.app, app.as_ref(), px);
        icon.set_halign(gtk4::Align::Center);
        icon.set_valign(gtk4::Align::Center);
        ov.set_child(Some(&icon));
        let name = app
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| t.app.clone());
        let l = gtk4::Label::new(Some(&name));
        l.add_css_class("glaze-tile-name");
        l.set_halign(gtk4::Align::Start);
        l.set_valign(gtk4::Align::End);
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        l.set_max_width_chars(if w > 1 { 22 } else { 10 });
        ov.add_overlay(&l);
        b.set_child(Some(&ov));
        b.set_tooltip_text(Some(&name));
        {
            let (m, id) = (Rc::downgrade(self), t.app.clone());
            b.connect_clicked(move |_| {
                if let Some(m) = m.upgrade() {
                    m.launch(&id);
                }
            });
        }
        // Right-click: unpin / resize.
        {
            let m = Rc::downgrade(self);
            let g = gtk4::GestureClick::new();
            g.set_button(3);
            let bw = b.clone();
            g.connect_released(move |_, _, _, _| {
                let Some(m) = m.upgrade() else { return };
                let size = |s: TileSize, label: &str| {
                    let m = m.clone();
                    Entry::item(None, label, move || m.resize_tile(gi, ti, s))
                };
                let mu = m.clone();
                menu::popup(
                    &bw,
                    None,
                    gtk4::PositionType::Right,
                    &m.ctx,
                    vec![
                        Entry::item(Some("unpin"), "Unpin from Start", move || {
                            mu.unpin_tile(gi, ti)
                        }),
                        Entry::sub(
                            Some("layout-tile"),
                            "Resize",
                            vec![
                                size(TileSize::Small, "Small"),
                                size(TileSize::Wide, "Wide"),
                                size(TileSize::Large, "Large"),
                            ],
                        ),
                    ],
                );
            });
            b.add_controller(g);
        }
        // Drag to reorder: drop a tile on another to move it before it.
        let src = gtk4::DragSource::new();
        src.set_actions(gdk::DragAction::MOVE);
        let payload = format!("{gi}:{ti}");
        src.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(&payload.to_value()))
        });
        b.add_controller(src);
        let dst = gtk4::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
        let m = Rc::downgrade(self);
        dst.connect_drop(move |_, v, _, _| {
            let Some(m) = m.upgrade() else { return false };
            let Ok(s) = v.get::<String>() else {
                return false;
            };
            let mut it = s.split(':').filter_map(|p| p.parse::<usize>().ok());
            let (Some(sg), Some(st)) = (it.next(), it.next()) else {
                return false;
            };
            m.move_tile((sg, st), (gi, ti));
            true
        });
        b.add_controller(dst);
        b.upcast()
    }

    fn commit_tiles(self: &Rc<Self>) {
        save_groups(&self.groups.borrow());
        self.build_tiles();
    }

    fn move_tile(self: &Rc<Self>, from: (usize, usize), to: (usize, usize)) {
        if from == to {
            return;
        }
        {
            let mut g = self.groups.borrow_mut();
            if from.0 >= g.len() || from.1 >= g[from.0].tiles.len() || to.0 >= g.len() {
                return;
            }
            let t = g[from.0].tiles.remove(from.1);
            let idx = to.1.min(g[to.0].tiles.len());
            g[to.0].tiles.insert(idx, t);
        }
        self.commit_tiles();
    }

    fn resize_tile(self: &Rc<Self>, gi: usize, ti: usize, size: TileSize) {
        if let Some(t) = self
            .groups
            .borrow_mut()
            .get_mut(gi)
            .and_then(|g| g.tiles.get_mut(ti))
        {
            t.size = size;
        }
        self.commit_tiles();
    }

    fn unpin_tile(self: &Rc<Self>, gi: usize, ti: usize) {
        if let Some(g) = self.groups.borrow_mut().get_mut(gi) {
            if ti < g.tiles.len() {
                g.tiles.remove(ti);
            }
        }
        self.commit_tiles();
    }

    fn pin_tile(self: &Rc<Self>, id: &str) {
        {
            let mut g = self.groups.borrow_mut();
            if g.iter().any(|g| g.tiles.iter().any(|t| t.app == id)) {
                return;
            }
            if g.is_empty() {
                g.push(Group {
                    name: "Pinned".into(),
                    tiles: Vec::new(),
                });
            }
            g[0].tiles.push(Tile {
                app: id.to_string(),
                size: TileSize::Small,
            });
        }
        self.commit_tiles();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_fills_rows_and_respects_spans() {
        let t = |s| Tile {
            app: "a".into(),
            size: s,
        };
        let p = pack(&[
            t(TileSize::Large),
            t(TileSize::Small),
            t(TileSize::Small),
            t(TileSize::Wide),
        ]);
        assert_eq!(p, vec![(0, 0), (2, 0), (2, 1), (0, 2)]);
    }
}
