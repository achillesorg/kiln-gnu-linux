//! Minimise / maximise / close buttons on Sway-drawn title bars.
//!
//! Sway cannot draw title-bar buttons. Apps with client-side decorations
//! (GTK 4/libadwaita, Firefox) bring their own; apps that ask for server-side
//! decorations (Thunar, most GTK 3, Qt and X11 apps) get Sway's plain title
//! bar. For every such window the shell places a small layer-shell strip
//! over the right end of the title bar, coloured like that bar (focused or
//! not). Windows are told apart by Sway's `border` kind: only `normal`
//! borders carry a Sway title bar, so a CSD window never gets a second set.
//!
//! A strip is shown only while its window's title bar is actually visible:
//! on a visible workspace, not full screen, not in a tabbed/stacked
//! container, not being animated, and not covered by a window stacked above
//! it. Geometry is re-read on every window event and polled while strips
//! are up (fast for a moment after anything moved, so a dragged window's
//! buttons follow it). The same poll feeds drag-to-edge snapping
//! (`dragsnap.rs`). The Glaze compositor draws real frame buttons and never
//! uses this.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::icons;
use crate::ink::Glyph;
use crate::panel_ui::Ctx;
use crate::swayipc::{self, View};

const BUTTON_W: i32 = 26;
const BUTTON_H: i32 = 21;
const STRIP_W: i32 = 3 * BUTTON_W + 2;
/// Gap between the strip and the outer right edge of the title bar.
const RIGHT_GAP: i32 = 4;

/// Placement of one strip: (left, top, maximised, focused).
type Place = (i32, i32, bool, bool);

struct Strip {
    window: gtk4::Window,
    max_button: gtk4::Button,
    last: Cell<Option<Place>>,
    /// The compositor closed the surface (or GTK destroyed the window):
    /// the strip is rebuilt on its next use.
    dead: Rc<Cell<bool>>,
}

pub struct WinControls {
    strips: RefCell<HashMap<i64, Strip>>,
    /// Last full layout, to tell the poller whether anything moved.
    layout: RefCell<Vec<(i64, Option<Place>)>>,
    drag: RefCell<crate::dragsnap::DragSnap>,
    ctx: Ctx,
    me: RefCell<Weak<WinControls>>,
}

/// Where the strip of `v` goes, if its title bar can take one.
fn place(v: &View) -> Option<(i32, i32)> {
    if !v.visible || !v.has_titlebar() || v.animating || v.width < STRIP_W + 80 {
        return None;
    }
    let (x, y, w, _) = v.frame();
    let left = x + w - STRIP_W - RIGHT_GAP;
    let top = y + (v.title_height - BUTTON_H) / 2;
    // Only over a title bar end that is on screen: a window dragged partly
    // off an edge has its buttons off screen too, and a layer surface
    // pushed past the output is closed by the compositor.
    let (ax, ay, aw, ah) = v.area;
    let inside = left >= ax && left + STRIP_W <= ax + aw && top >= ay && top + BUTTON_H <= ay + ah;
    inside.then_some((left, top))
}

fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Each view's strip placement (or `None`), honouring stacking: a strip is
/// dropped when any window above its own on the same workspace covers it,
/// and every strip of a workspace with a full-screen window is dropped.
pub fn layout(views: &[View]) -> Vec<(i64, Option<(i32, i32)>)> {
    views
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let spot = place(v).filter(|&(left, top)| {
                let r = (left, top, STRIP_W, BUTTON_H);
                let same_ws = |o: &&View| o.workspace == v.workspace;
                let fullscreen = views.iter().filter(same_ws).any(|o| o.fullscreen);
                let covered = views[i + 1..]
                    .iter()
                    .filter(same_ws)
                    .any(|o| o.floating && overlaps(r, o.frame()));
                !fullscreen && !covered
            });
            (v.id, spot)
        })
        .collect()
}

impl WinControls {
    pub fn new(ctx: &Ctx) -> Option<Rc<Self>> {
        let sway = ctx.backend.as_ref().map(|b| b.name()) == Some("sway-ipc");
        if !gtk4_layer_shell::is_supported() || !sway || !swayipc::available() {
            return None;
        }
        install_css(ctx);
        let me = Rc::new(WinControls {
            strips: RefCell::new(HashMap::new()),
            layout: RefCell::new(Vec::new()),
            drag: RefCell::new(crate::dragsnap::DragSnap::default()),
            ctx: ctx.clone(),
            me: RefCell::new(Weak::new()),
        });
        *me.me.borrow_mut() = Rc::downgrade(&me);
        Some(me)
    }

    fn make_strip(&self, id: i64) -> Strip {
        let ctx = &self.ctx;
        let window = gtk4::Window::new();
        window.set_title(Some("Glaze window controls"));
        window.init_layer_shell();
        window.set_namespace(Some("glaze-window-controls"));
        window.set_layer(Layer::Top);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_exclusive_zone(-1);
        // Anchored top+left only: keep a real size so the layer surface never
        // requests width 0 (a protocol error that kills the shell).
        window.set_default_size(STRIP_W, BUTTON_H);
        window.set_size_request(STRIP_W, BUTTON_H);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_decorated(false);
        window.add_css_class("glaze-wincontrols-window");
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 1);
        row.add_css_class("glaze-wincontrols");
        let fg = ctx.tokens.palette.ink;
        let mk = |name: &str, glyph: Glyph, tip: &str| {
            let b = gtk4::Button::new();
            b.add_css_class("glaze-wincontrol");
            b.set_size_request(BUTTON_W, BUTTON_H);
            b.set_focus_on_click(false);
            b.set_tooltip_text(Some(tip));
            b.set_child(Some(&icons::icon(name, glyph, 14, fg, ctx.stroke() + 0.25)));
            b
        };
        let min_b = mk("window-minimise", Glyph::Window, "Minimise");
        let max_b = mk("window-maximise", Glyph::Window, "Maximise");
        let close_b = mk("window-close", Glyph::Close, "Close");
        close_b.add_css_class("glaze-wincontrol-close");
        row.append(&min_b);
        row.append(&max_b);
        row.append(&close_b);
        window.set_child(Some(&row));
        let key = id.to_string();
        for (b, what) in [(min_b, 0), (max_b.clone(), 1), (close_b, 2)] {
            let me = self.me.borrow().clone();
            let key = key.clone();
            b.connect_clicked(move |_| {
                let Some(m) = me.upgrade() else { return };
                let Some(be) = m.ctx.backend.clone() else {
                    return;
                };
                if what != 1 {
                    // The window is about to fade away: so are its buttons.
                    m.hide(id);
                }
                let r = match what {
                    0 => be.minimize_window(&key),
                    // Same work-area logic and restore memory as Super+Up.
                    1 if crate::panel_ui::snap(&key, "toggle-maximise") => Ok(()),
                    1 => be.toggle_maximize(&key),
                    _ => be.close_window(&key),
                };
                if let Err(e) = r {
                    eprintln!("[glaze-shell] window control on {key}: {e}");
                }
                (m.ctx.refresh)();
            });
        }
        let dead = Rc::new(Cell::new(false));
        {
            let dead = dead.clone();
            window.connect_close_request(move |_| {
                dead.set(true);
                glib::Propagation::Proceed
            });
        }
        Strip {
            window,
            max_button: max_b,
            last: Cell::new(None),
            dead,
        }
    }

    fn hide(&self, id: i64) {
        if let Some(s) = self.strips.borrow().get(&id) {
            if s.window.is_visible() {
                s.window.set_visible(false);
            }
            s.last.set(None);
        }
    }

    /// Show, move or hide one strip.
    fn apply(&self, id: i64, p: Place) {
        let dead = self.strips.borrow().get(&id).map(|s| s.dead.get());
        if dead == Some(true) {
            if let Some(s) = self.strips.borrow_mut().remove(&id) {
                retire(s.window);
            }
        }
        if !self.strips.borrow().contains_key(&id) {
            let s = self.make_strip(id);
            self.strips.borrow_mut().insert(id, s);
        }
        let strips = self.strips.borrow();
        let Some(s) = strips.get(&id) else { return };
        let prev = s.last.get();
        if prev == Some(p) && s.window.is_visible() {
            return;
        }
        s.window.set_margin(Edge::Left, p.0);
        s.window.set_margin(Edge::Top, p.1);
        if prev.map(|l| l.2) != Some(p.2) {
            let (name, tip) = if p.2 {
                ("window-restore", "Restore down")
            } else {
                ("window-maximise", "Maximise")
            };
            s.max_button.set_tooltip_text(Some(tip));
            s.max_button.set_child(Some(&icons::icon(
                name,
                Glyph::Window,
                14,
                self.ctx.tokens.palette.ink,
                self.ctx.stroke() + 0.25,
            )));
        }
        if prev.map(|l| l.3) != Some(p.3) {
            let (on, off) = if p.3 {
                ("glaze-wc-focused", "glaze-wc-other")
            } else {
                ("glaze-wc-other", "glaze-wc-focused")
            };
            s.window.remove_css_class(off);
            s.window.add_css_class(on);
        }
        s.last.set(Some(p));
        if !s.window.is_visible() {
            s.window.present();
        }
    }

    /// Re-read every window and place, show or hide each strip. Returns
    /// true when anything changed since the last call (the poller then
    /// follows closely for a moment).
    pub fn sync(&self) -> bool {
        let Some(views) = swayipc::views() else {
            return false;
        };
        let spots = layout(&views);
        let mut now: Vec<(i64, Option<Place>)> = Vec::with_capacity(spots.len());
        for (v, (id, spot)) in views.iter().zip(spots) {
            let place = spot.map(|(left, top)| (left, top, v.maximised(), v.focused));
            match place {
                Some(p) => self.apply(id, p),
                None => self.hide(id),
            }
            now.push((id, place));
        }
        // Windows that are gone (closed, or minimised to the scratchpad).
        let gone: Vec<i64> = self
            .strips
            .borrow()
            .keys()
            .filter(|id| !now.iter().any(|(n, _)| n == *id))
            .copied()
            .collect();
        for id in gone {
            if let Some(s) = self.strips.borrow_mut().remove(&id) {
                retire(s.window);
            }
        }
        let (dragged, snaps) = self
            .drag
            .borrow_mut()
            .update(&views, std::time::Instant::now());
        for s in &snaps {
            self.hide(s.id);
            crate::dragsnap::perform(s);
        }
        let changed = *self.layout.borrow() != now;
        *self.layout.borrow_mut() = now;
        changed || dragged || self.drag.borrow().busy()
    }

    /// Worth polling: buttons on screen, or a floating window that could be
    /// dragged to an edge.
    pub fn wants_poll(&self) -> bool {
        self.is_shown() || self.drag.borrow().watching()
    }

    /// At least one strip is on screen.
    pub fn is_shown(&self) -> bool {
        self.strips.borrow().values().any(|s| s.window.is_visible())
    }

    pub fn destroy(&self) {
        for (_, s) in self.strips.borrow_mut().drain() {
            retire(s.window);
        }
    }
}

/// Unmap now, destroy on idle: never commit an empty (0-width) surface.
fn retire(w: gtk4::Window) {
    w.set_visible(false);
    glib::idle_add_local_once(move || w.destroy());
}

/// The strip's background matches the Sway title bar it sits on (the shell
/// paints those bars from the same tokens: `surface_raised` focused,
/// `panel` otherwise).
fn install_css(ctx: &Ctx) {
    thread_local! {
        static PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
    }
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    let p = &ctx.tokens.palette;
    let hex = |c: glaze_common::color::Color| c.to_rgb().to_hex();
    let css = format!(
        "window.glaze-wincontrols-window.glaze-wc-focused {{ background-color: {}; }}\n\
         window.glaze-wincontrols-window.glaze-wc-other {{ background-color: {}; }}\n",
        hex(p.surface_raised),
        hex(p.panel),
    );
    PROVIDER.with(|cell| {
        let mut cell = cell.borrow_mut();
        let provider = cell.get_or_insert_with(|| {
            let pr = gtk4::CssProvider::new();
            gtk4::style_context_add_provider_for_display(
                &display,
                &pr,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
            pr
        });
        provider.load_from_string(&css);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: i64, frame: (i32, i32, i32, i32), title: i32, floating: bool) -> View {
        View {
            id,
            x: frame.0,
            y: frame.1 + title,
            width: frame.2,
            height: frame.3 - title,
            title_height: title,
            border_width: 6,
            floating,
            focused: false,
            fullscreen: false,
            visible: true,
            tabbed: false,
            workspace: "1".into(),
            area: (0, 0, 1280, 766),
            animating: false,
            pid: 0,
        }
    }

    #[test]
    fn every_uncovered_server_side_title_bar_gets_buttons() {
        let views = vec![
            view(1, (0, 0, 640, 766), 25, true),
            view(2, (700, 100, 500, 400), 25, true),
            // CSD: no Sway title bar, the app draws its own buttons.
            view(3, (100, 500, 300, 200), 0, true),
        ];
        let l = layout(&views);
        assert_eq!(l[0], (1, Some((640 - STRIP_W - RIGHT_GAP, 2))));
        assert!(l[1].1.is_some());
        assert_eq!(l[2], (3, None));
    }

    #[test]
    fn a_window_above_hides_the_buttons_it_covers() {
        let views = vec![
            view(1, (0, 0, 640, 766), 25, true),
            // Stacked above window 1, covering its top-right corner.
            view(2, (500, 5, 400, 300), 25, true),
        ];
        let l = layout(&views);
        assert_eq!(l[0].1, None);
        assert!(l[1].1.is_some());
    }

    #[test]
    fn title_bar_ends_off_screen_get_nothing() {
        // Dragged past the right edge, and above the top.
        let views = vec![
            view(1, (900, 100, 640, 400), 25, true),
            view(2, (100, -12, 640, 400), 25, true),
            view(3, (-300, 100, 640, 400), 25, true),
        ];
        let l = layout(&views);
        assert_eq!(l[0].1, None);
        assert_eq!(l[1].1, None);
        // Its right end is still on screen.
        assert_eq!(l[2].1, Some((-300 + 640 - STRIP_W - RIGHT_GAP, 102)));
    }

    #[test]
    fn full_screen_and_hidden_workspaces_get_nothing() {
        let mut a = view(1, (0, 0, 640, 766), 25, true);
        let mut b = view(2, (700, 0, 500, 400), 0, true);
        b.fullscreen = true;
        assert_eq!(layout(&[a.clone(), b])[0].1, None);
        a.visible = false;
        assert_eq!(layout(&[a])[0].1, None);
    }
}
