//! Toasts: small paper cards above the taskbar's far end for shell-level
//! messages (an app that failed to start, a failed window action). They
//! stack upwards, fade in/out on the frame clock and close on click or after
//! a few seconds. Nothing here talks to the network.

use std::cell::RefCell;
use std::time::Duration;

use glaze_common::config::PanelPosition;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::anim::Animator;
use crate::panel_ui::Ctx;

thread_local! {
    static CTX: RefCell<Option<(Ctx, PanelPosition, i32)>> = const { RefCell::new(None) };
    static OPEN: RefCell<Vec<gtk4::Window>> = const { RefCell::new(Vec::new()) };
}

/// Remember the current shell context (called on every taskbar rebuild).
pub fn set_context(ctx: &Ctx, position: PanelPosition, panel_thickness: i32) {
    CTX.with(|c| *c.borrow_mut() = Some((ctx.clone(), position, panel_thickness)));
}

/// Show a toast. Safe to call before the shell is up (it then only logs).
pub fn show(title: &str, body: &str) {
    eprintln!("[glaze-shell] toast: {title}: {body}");
    let Some((ctx, position, thickness)) = CTX.with(|c| c.borrow().clone()) else {
        return;
    };
    if !gtk4_layer_shell::is_supported() {
        return;
    }
    let window = gtk4::Window::new();
    window.set_title(Some("Glaze notice"));
    window.add_css_class("glaze-toast-window");
    window.init_layer_shell();
    window.set_namespace(Some("glaze-toast"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    // Not anchored left+right, so the surface must never ask for width 0
    // (wlroots kills the client: "width 0 requested without setting left
    // and right anchors").
    window.set_default_size(300, 60);
    window.set_size_request(260, 40);
    let gap = 10;
    let stacked: i32 = OPEN.with(|o| o.borrow().iter().map(|w| w.height().max(60) + 8).sum());
    let (v_edge, h_edge) = match position {
        PanelPosition::Top => (Edge::Top, Edge::Right),
        _ => (Edge::Bottom, Edge::Right),
    };
    window.set_anchor(v_edge, true);
    window.set_anchor(h_edge, true);
    let along = if matches!(position, PanelPosition::Top | PanelPosition::Bottom) {
        thickness
    } else {
        0
    };
    window.set_margin(v_edge, along + gap + stacked);
    window.set_margin(
        h_edge,
        gap + if position == PanelPosition::Right {
            thickness
        } else {
            0
        },
    );

    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    card.add_css_class("glaze-toast");
    let mark = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    mark.add_css_class("glaze-toast-mark");
    card.append(&mark);
    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("glaze-toast-title");
    t.set_xalign(0.0);
    let b = gtk4::Label::new(Some(body));
    b.add_css_class("glaze-toast-body");
    b.set_xalign(0.0);
    b.set_wrap(true);
    b.set_max_width_chars(46);
    b.set_selectable(false);
    text.append(&t);
    text.append(&b);
    card.append(&text);
    window.set_child(Some(&card));
    card.set_opacity(0.0);
    window.present();
    OPEN.with(|o| o.borrow_mut().push(window.clone()));

    let anim = Animator::new("toast", &card, ctx.motion, ctx.cfg.motion.log_frame_times);
    let c = card.clone();
    anim.animate(
        "o",
        0.0,
        1.0,
        ctx.tokens.metrics.motion_menu_ms as f64,
        move |v| c.set_opacity(v),
        None,
    );
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let close = {
        let closed = closed.clone();
        let (w, c, a, ms) = (
            window.clone(),
            card.clone(),
            anim.clone(),
            ctx.tokens.metrics.motion_micro_ms as f64,
        );
        move || {
            // Click and the 8 s timeout both close: only the first counts.
            if closed.replace(true) {
                return;
            }
            let (w2, c2) = (w.clone(), c.clone());
            a.animate(
                "o",
                c.opacity(),
                0.0,
                ms,
                move |v| c2.set_opacity(v),
                Some(Box::new(move || {
                    OPEN.with(|o| o.borrow_mut().retain(|x| x != &w2));
                    // Unmap first, destroy on idle: never commit an empty
                    // (0-width) layer surface.
                    w2.set_visible(false);
                    let w3 = w2.clone();
                    gtk4::glib::idle_add_local_once(move || w3.destroy());
                })),
            );
        }
    };
    let click = gtk4::GestureClick::new();
    {
        let close = close.clone();
        click.connect_released(move |_, _, _, _| close());
    }
    card.add_controller(click);
    glib::timeout_add_local_once(Duration::from_secs(8), close);
}
