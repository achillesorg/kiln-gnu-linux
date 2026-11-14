//! Minimise / maximise / close buttons on the focused window's title bar in
//! the live Sway session.
//!
//! Sway cannot draw extra title-bar buttons, so the shell places a small
//! layer-shell strip over the right end of the focused window's title bar
//! (geometry from the backend's `frames()`), recoloured from the theme.
//! Only the focused window gets the strip: it is the top-most window, so the
//! overlay never floats over another window. Every window's controls are
//! also in the taskbar context menu and hover card. The Glaze compositor
//! draws real frame buttons instead (agent B).

use std::cell::RefCell;
use std::rc::Rc;

use glaze_backend::Frame;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::icons;
use crate::ink::Glyph;
use crate::panel_ui::Ctx;

const BUTTON_W: i32 = 26;
const BUTTON_H: i32 = 21;

pub struct WinControls {
    window: gtk4::Window,
    max_button: gtk4::Button,
    target: RefCell<Option<Frame>>,
    /// Last placement (left, top, maximised): skip identical updates (the
    /// strip is re-checked every 300 ms while shown).
    last: std::cell::Cell<Option<(i32, i32, bool)>>,
    ctx: Ctx,
}

impl WinControls {
    pub fn new(ctx: &Ctx) -> Option<Rc<Self>> {
        if !gtk4_layer_shell::is_supported() || ctx.backend.is_none() {
            return None;
        }
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
        window.set_default_size(3 * BUTTON_W + 6, BUTTON_H);
        window.set_size_request(3 * BUTTON_W + 6, BUTTON_H);
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
        let me = Rc::new(WinControls {
            window,
            max_button: max_b.clone(),
            target: RefCell::new(None),
            last: std::cell::Cell::new(None),
            ctx: ctx.clone(),
        });
        for (b, what) in [(min_b, 0), (max_b, 1), (close_b, 2)] {
            let m = Rc::downgrade(&me);
            b.connect_clicked(move |_| {
                let Some(m) = m.upgrade() else { return };
                let Some(id) = m.target.borrow().as_ref().map(|f| f.id.clone()) else {
                    return;
                };
                let Some(be) = &m.ctx.backend else { return };
                let r = match what {
                    0 => be.minimize_window(&id),
                    // Same work-area logic and restore memory as Super+Up.
                    1 if crate::panel_ui::snap(&id, "toggle-maximise") => Ok(()),
                    1 => be.toggle_maximize(&id),
                    _ => be.close_window(&id),
                };
                if let Err(e) = r {
                    eprintln!("[glaze-shell] window control on {id}: {e}");
                }
                if what != 1 {
                    m.window.set_visible(false);
                }
                (m.ctx.refresh)();
            });
        }
        Some(me)
    }

    /// Place the strip on `focused` (or hide it).
    pub fn follow(&self, focused: Option<&Frame>) {
        let Some(f) = focused.filter(|f| f.title_height >= 16 && f.width > 160) else {
            if self.window.is_visible() {
                self.window.set_visible(false);
            }
            self.target.replace(None);
            return;
        };
        let strip_w = 3 * BUTTON_W + 2 + 4;
        let left = f.x + f.width - strip_w - 4;
        let top = f.y + (f.title_height - BUTTON_H) / 2;
        let maximized = self
            .ctx
            .backend
            .as_ref()
            .map(|b| b.is_maximized(&f.id))
            .unwrap_or(false);
        let key = (left, top, maximized);
        let same_target = self.target.borrow().as_ref().map(|t| t.id == f.id) == Some(true);
        if same_target && self.last.get() == Some(key) && self.window.is_visible() {
            return;
        }
        self.window.set_margin(Edge::Left, left.max(0));
        self.window.set_margin(Edge::Top, top.max(0));
        if self.last.get().map(|l| l.2) != Some(maximized) {
            let (name, tip) = if maximized {
                ("window-restore", "Restore down")
            } else {
                ("window-maximise", "Maximise")
            };
            self.max_button.set_tooltip_text(Some(tip));
            self.max_button.set_child(Some(&icons::icon(
                name,
                Glyph::Window,
                14,
                self.ctx.tokens.palette.ink,
                self.ctx.stroke() + 0.25,
            )));
        }
        self.last.set(Some(key));
        self.target.replace(Some(f.clone()));
        if !self.window.is_visible() {
            self.window.present();
        }
    }

    /// The strip is currently placed on a window.
    pub fn is_shown(&self) -> bool {
        self.window.is_visible()
    }

    pub fn destroy(&self) {
        self.window.set_visible(false);
        let w = self.window.clone();
        glib::idle_add_local_once(move || w.destroy());
    }
}
