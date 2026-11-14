//! Minimise / restore "zoom" animation for the Sway session.
//!
//! Sway animates nothing, so the shell draws the Windows 2000 idea in paper
//! form: a window-sized paper outline that shrinks and fades into the task
//! button on minimise, and grows back out of it on restore. It runs on a
//! full-screen overlay layer surface with an empty input region (clicks pass
//! through) that is only mapped while an animation plays. One progress value
//! drives the whole frame; drawing reads it from a `Cell` (no allocation per
//! frame). The Glaze compositor animates real window contents instead.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{cairo, gdk};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::anim::Animator;
use crate::panel_ui::Ctx;

type Rect = (f64, f64, f64, f64);

struct State {
    from: Rect,
    to: Rect,
    p: f64,
    fill: (f64, f64, f64),
    line: (f64, f64, f64),
}

pub struct Ghost {
    window: gtk4::Window,
    area: gtk4::DrawingArea,
    anim: Animator,
    state: Rc<RefCell<State>>,
    busy: Rc<Cell<bool>>,
    ms: f64,
}

fn rgb(c: glaze_common::color::Color) -> (f64, f64, f64) {
    let c = c.to_rgb();
    let h = c.to_hex();
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
    (v(1), v(3), v(5))
}

impl Ghost {
    pub fn new(ctx: &Ctx) -> Option<Rc<Self>> {
        if !gtk4_layer_shell::is_supported() {
            return None;
        }
        let window = gtk4::Window::new();
        window.set_title(Some("Glaze minimise animation"));
        window.add_css_class("glaze-ghost-window");
        window.init_layer_shell();
        window.set_namespace(Some("glaze-ghost"));
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_exclusive_zone(-1);
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(e, true);
        }
        window.set_decorated(false);
        let area = gtk4::DrawingArea::new();
        area.set_can_target(false);
        window.set_child(Some(&area));
        // Clicks go straight through to whatever is below.
        window.connect_realize(|w| {
            if let Some(s) = w.surface() {
                s.set_input_region(&cairo::Region::create());
            }
        });
        let p = &ctx.tokens.palette;
        let state = Rc::new(RefCell::new(State {
            from: (0.0, 0.0, 0.0, 0.0),
            to: (0.0, 0.0, 0.0, 0.0),
            p: 0.0,
            fill: rgb(p.surface_raised),
            line: rgb(p.accent),
        }));
        {
            let st = state.clone();
            area.set_draw_func(move |_, cr, _, _| {
                let s = st.borrow();
                let t = s.p;
                let lerp = |a: f64, b: f64| a + (b - a) * t;
                let (x, y, w, h) = (
                    lerp(s.from.0, s.to.0),
                    lerp(s.from.1, s.to.1),
                    lerp(s.from.2, s.to.2),
                    lerp(s.from.3, s.to.3),
                );
                // Fades as it travels from the window to the button.
                let a = (1.0 - t * 0.85).clamp(0.0, 1.0);
                cr.rectangle(x, y, w.max(1.0), h.max(1.0));
                cr.set_source_rgba(s.fill.0, s.fill.1, s.fill.2, 0.55 * a);
                let _ = cr.fill_preserve();
                cr.set_line_width(1.5);
                cr.set_source_rgba(s.line.0, s.line.1, s.line.2, 0.9 * a);
                let _ = cr.stroke();
            });
        }
        let anim = Animator::new("ghost", &area, ctx.motion, ctx.cfg.motion.log_frame_times);
        Some(Rc::new(Ghost {
            window,
            area,
            anim,
            state,
            busy: Rc::new(Cell::new(false)),
            ms: ctx.tokens.metrics.motion_menu_ms as f64 + 60.0,
        }))
    }

    /// Animate from `from` to `to` (screen rects): window -> button for a
    /// minimise, button -> window for a restore.
    pub fn play(&self, from: Rect, to: Rect) {
        if self.anim.settings().instant() {
            return;
        }
        {
            let mut s = self.state.borrow_mut();
            s.from = from;
            s.to = to;
            s.p = 0.0;
        }
        if !self.window.is_visible() {
            self.window.present();
        }
        self.busy.set(true);
        let (st, area) = (self.state.clone(), self.area.clone());
        let (win, busy) = (self.window.clone(), self.busy.clone());
        self.anim.cancel("p");
        self.anim.animate(
            "p",
            0.0,
            1.0,
            self.ms,
            move |v| {
                st.borrow_mut().p = v;
                area.queue_draw();
            },
            Some(Box::new(move || {
                busy.set(false);
                win.set_visible(false);
            })),
        );
    }

    pub fn destroy(&self) {
        self.window.destroy();
    }
}

/// A monitor's height (logical px) for the panel window `w`, used to turn
/// panel-local coordinates into screen coordinates.
pub fn monitor_size(w: &gtk4::Window) -> Option<(i32, i32)> {
    let surface = w.surface()?;
    let mon = gdk::Display::default()?.monitor_at_surface(&surface)?;
    let g = mon.geometry();
    Some((g.width(), g.height()))
}
