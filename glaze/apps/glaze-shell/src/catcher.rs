//! Click catcher: lets a Start menu animate its close on an outside click.
//!
//! An autohide popover is dismissed by the compositor (`xdg_popup.popup_done`)
//! the instant the user clicks another client, so there is no frame left to
//! animate. While a menu is open the shell maps one transparent layer-shell
//! surface over the work area (exclusive zone 0: it stops at the taskbar).
//! It belongs to the shell itself, so the popup grab is not broken by a
//! click on it; the click lands here instead and runs the menu's own
//! animated close (fade + slide, instant under reduced motion). Clicks on
//! the taskbar still reach the taskbar.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

thread_local! {
    static CATCHER: RefCell<Option<gtk4::Window>> = const { RefCell::new(None) };
    static ON_CLICK: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

fn window() -> Option<gtk4::Window> {
    if !gtk4_layer_shell::is_supported() {
        return None;
    }
    CATCHER.with(|c| {
        if let Some(w) = c.borrow().as_ref() {
            return Some(w.clone());
        }
        let w = gtk4::Window::new();
        w.set_title(Some("Glaze menu dismiss"));
        w.add_css_class("glaze-click-catcher");
        w.init_layer_shell();
        w.set_namespace(Some("glaze-click-catcher"));
        w.set_layer(Layer::Top);
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            w.set_anchor(e, true);
        }
        // 0 = stay inside other surfaces' exclusive zones (not over the bar).
        w.set_exclusive_zone(0);
        w.set_keyboard_mode(KeyboardMode::None);
        w.set_decorated(false);
        let area = gtk4::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        w.set_child(Some(&area));
        let css = gtk4::CssProvider::new();
        css.load_from_string(
            "window.glaze-click-catcher, window.glaze-click-catcher > * \
             { background: none; background-color: transparent; box-shadow: none; }",
        );
        gtk4::style_context_add_provider_for_display(
            &WidgetExt::display(&w),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.connect_pressed(|_, _, _, _| {
            if let Some(f) = ON_CLICK.with(|o| o.borrow().clone()) {
                f();
            }
        });
        w.add_controller(click);
        *c.borrow_mut() = Some(w.clone());
        Some(w)
    })
}

/// Map the catcher for `pop`; a click on it calls `close` (the menu's
/// animated close). It unmaps itself when the popover closes, however
/// that happens.
pub fn guard(pop: &gtk4::Popover, close: Rc<dyn Fn()>) {
    let Some(w) = window() else { return };
    ON_CLICK.with(|o| *o.borrow_mut() = Some(close));
    if !w.is_visible() {
        w.present();
    }
    // One-shot: unmap on this popover's next `closed`.
    let handler: Rc<RefCell<Option<gtk4::glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
    let h2 = handler.clone();
    let id = pop.connect_closed(move |p| {
        release();
        if let Some(id) = h2.borrow_mut().take() {
            p.disconnect(id);
        }
    });
    *handler.borrow_mut() = Some(id);
}

/// Unmap the catcher (the menu closed).
pub fn release() {
    ON_CLICK.with(|o| *o.borrow_mut() = None);
    CATCHER.with(|c| {
        if let Some(w) = c.borrow().as_ref() {
            if w.is_visible() {
                w.set_visible(false);
            }
        }
    });
}

/// Drop the surface (shell rebuild / exit).
pub fn destroy() {
    release();
    CATCHER.with(|c| {
        if let Some(w) = c.borrow_mut().take() {
            gtk4::glib::idle_add_local_once(move || w.destroy());
        }
    });
}
