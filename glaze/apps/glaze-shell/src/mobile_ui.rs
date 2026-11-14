//! Mobile / touch layout: bottom navigation bar and the app-grid launcher.
//!
//! All decisions (what each button does, grid geometry, paging, swipe
//! classification) come from the tested `glaze_shell_core::mobile` model; this
//! module binds them to GTK widgets. Every control is >= 48px.

use std::cell::RefCell;
use std::rc::Rc;

use glaze_common::config::GlazeConfig;
use glaze_common::theme::ThemeTokens;
use glaze_i18n::{tr, Keys};
use glaze_service::registry::AppRegistry;
use glaze_shell_core::mobile::{
    build_grid, classify_swipe, nav_bar_height, order_with_pinned, GridSpec, MobileState,
    NavAction, NavEffect, Swipe,
};
use gtk4::prelude::*;
use gtk4_layer_shell::Edge;

/// Fallback surface size before GTK reports one (typical phone, logical px).
const FALLBACK_W: i32 = 360;
const FALLBACK_H: i32 = 640;

/// The two windows of the mobile shell.
pub struct MobileShell {
    pub nav: gtk4::Window,
    pub grid: gtk4::Window,
}

struct Ctx {
    state: RefCell<MobileState>,
    apps: Vec<glaze_common::model::AppEntry>,
    registry: Rc<AppRegistry>,
    touch: i32,
    grid_window: gtk4::Window,
    body: gtk4::Box,
    dots: gtk4::Box,
    entry: gtk4::SearchEntry,
}

pub fn build(
    cfg: &GlazeConfig,
    tokens: &ThemeTokens,
    registry: Rc<AppRegistry>,
    force_windowed: bool,
    screen: (i32, i32),
) -> MobileShell {
    let bar_h = nav_bar_height(&tokens.metrics);
    let touch = tokens.metrics.touch_target;
    let layer = !force_windowed && gtk4_layer_shell::is_supported();

    // --- app grid window ---------------------------------------------------
    let title = gtk4::Label::new(Some("Apps"));
    title.add_css_class("glaze-display");
    title.set_xalign(0.0);
    title.set_markup("<span size='x-large'>Apps</span>");

    let entry = gtk4::SearchEntry::new();
    entry.add_css_class("glaze-app-grid-search");
    entry.set_placeholder_text(Some(&tr(Keys::SEARCH)));

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.set_vexpand(true);
    body.set_hexpand(true);

    let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    dots.set_halign(gtk4::Align::Center);
    dots.set_margin_top(6);
    dots.set_margin_bottom(6);

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.add_css_class("glaze-app-grid");
    root.append(&title);
    root.append(&entry);
    root.append(&body);
    root.append(&dots);

    let grid_window = gtk4::Window::new();
    grid_window.set_title(Some("Glaze Apps"));
    grid_window.set_child(Some(&root));
    grid_window.set_decorated(false);

    let nav = gtk4::Window::new();
    nav.set_title(Some("Glaze Navigation"));
    nav.set_decorated(false);

    if layer {
        use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
        grid_window.init_layer_shell();
        grid_window.set_namespace(Some("glaze-app-grid"));
        grid_window.set_layer(Layer::Top);
        for e in [Edge::Top, Edge::Left, Edge::Right, Edge::Bottom] {
            grid_window.set_anchor(e, true);
        }
        grid_window.set_margin(Edge::Bottom, bar_h);
        grid_window.set_exclusive_zone(0);
        grid_window.set_keyboard_mode(KeyboardMode::OnDemand);

        nav.init_layer_shell();
        nav.set_namespace(Some("glaze-nav-bar"));
        nav.set_layer(Layer::Top);
        for e in [Edge::Left, Edge::Right, Edge::Bottom] {
            nav.set_anchor(e, true);
        }
        nav.set_exclusive_zone(bar_h);
        nav.set_keyboard_mode(KeyboardMode::None);
        eprintln!("[glaze-shell] mobile: layer-shell nav bar {bar_h}px + app grid");
    } else {
        grid_window.set_default_size(FALLBACK_W, FALLBACK_H);
        nav.set_default_size(FALLBACK_W, bar_h);
        eprintln!("[glaze-shell] mobile: windowed fallback (no layer-shell)");
    }

    let ctx = Rc::new(Ctx {
        state: RefCell::new(MobileState::new()),
        apps: order_with_pinned(registry.apps(), cfg),
        registry,
        touch,
        grid_window: grid_window.clone(),
        body,
        dots,
        entry: entry.clone(),
    });

    // --- nav bar -----------------------------------------------------------
    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    bar.add_css_class("glaze-nav-bar");
    bar.set_homogeneous(true);
    bar.set_height_request(bar_h);
    let mut buttons = Vec::new();
    for action in NavAction::ALL {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_halign(gtk4::Align::Center);
        content.set_valign(gtk4::Align::Center);
        let img = gtk4::Image::from_icon_name(action.icon_name());
        img.set_pixel_size(22);
        let label = gtk4::Label::new(Some(action.label()));
        label.set_css_classes(&["caption"]);
        content.append(&img);
        content.append(&label);
        let button = gtk4::Button::new();
        button.add_css_class("glaze-nav-button");
        button.set_child(Some(&content));
        button.set_size_request(touch, touch);
        button.set_margin_top(8);
        button.set_margin_bottom(8);
        button.set_margin_start(12);
        button.set_margin_end(12);
        button.set_tooltip_text(Some(action.label()));
        {
            let ctx = ctx.clone();
            button.connect_clicked(move |_| {
                let effect = ctx.state.borrow_mut().nav(action);
                apply_effect(&ctx, effect);
            });
        }
        bar.append(&button);
        buttons.push((action, button));
    }
    nav.set_child(Some(&bar));
    {
        let apps_button = buttons
            .iter()
            .find(|(a, _)| *a == NavAction::Apps)
            .map(|(_, b)| b.clone());
        if let Some(b) = apps_button {
            ctx.grid_window.connect_visible_notify(move |w| {
                if w.is_visible() {
                    b.add_css_class("glaze-open");
                } else {
                    b.remove_css_class("glaze-open");
                }
            });
        }
    }

    // Edge swipe up from the bar: Home / Apps (classified by the core).
    {
        let drag = gtk4::GestureDrag::new();
        let start = Rc::new(RefCell::new((0.0f64, 0.0f64)));
        {
            let start = start.clone();
            drag.connect_drag_begin(move |_, x, y| *start.borrow_mut() = (x, y));
        }
        let ctx = ctx.clone();
        let bar_for = bar.clone();
        drag.connect_drag_end(move |_, dx, dy| {
            let (sw, sh) = (screen.0.max(1) as f64, screen.1.max(1) as f64);
            let (x0, y0) = *start.borrow();
            let bar_h = bar_for.height().max(1) as f64;
            // Translate bar-local coordinates to screen coordinates (bar is
            // docked to the bottom edge).
            let swipe = Swipe {
                start: (x0, sh - (bar_h - y0)),
                end: (x0 + dx, sh - (bar_h - y0) + dy),
                duration_ms: 0,
            };
            if let Some(g) = classify_swipe(swipe, (sw, sh)) {
                let effect = ctx.state.borrow_mut().gesture(g);
                apply_effect(&ctx, effect);
            }
        });
        bar.add_controller(drag);
    }

    // --- grid behaviour ----------------------------------------------------
    {
        let ctx = ctx.clone();
        entry.connect_search_changed(move |e| {
            let text = e.text().to_string();
            if ctx.state.borrow().search() != text {
                ctx.state.borrow_mut().set_search(&text);
                rebuild(&ctx);
            }
        });
    }
    {
        // Horizontal fling pages the grid.
        let swipe = gtk4::GestureSwipe::new();
        let ctx = ctx.clone();
        swipe.connect_swipe(move |_, vx, vy| {
            if vx.abs() > 300.0 && vx.abs() > vy.abs() * 1.5 {
                let (spec, pages) = current_pages(&ctx);
                let _ = spec;
                let cur = ctx.state.borrow().page() as i64;
                let next = if vx < 0.0 { cur + 1 } else { cur - 1 }.max(0) as usize;
                ctx.state.borrow_mut().set_page(next, pages.page_count());
                rebuild(&ctx);
            }
        });
        root.add_controller(swipe);
    }
    {
        let ctx = ctx.clone();
        let key = gtk4::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Escape {
                let effect = ctx.state.borrow_mut().nav(NavAction::Back);
                apply_effect(&ctx, effect);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        grid_window.add_controller(key);
    }
    let _ = screen;

    MobileShell {
        nav,
        grid: grid_window,
    }
}

fn apply_effect(ctx: &Rc<Ctx>, effect: NavEffect) {
    match effect {
        NavEffect::ShowGrid => {
            ctx.entry.set_text("");
            rebuild(ctx);
            ctx.grid_window.present();
            ctx.entry.grab_focus();
        }
        NavEffect::HideGrid => ctx.grid_window.set_visible(false),
        NavEffect::BackUnhandled => eprintln!(
            "[glaze-shell] mobile: Back pressed with nothing to close; in-app back needs the compositor"
        ),
        NavEffect::None => {
            // Search cleared by Back: keep the entry in sync.
            if ctx.state.borrow().search().is_empty() && !ctx.entry.text().is_empty() {
                ctx.entry.set_text("");
                rebuild(ctx);
            }
        }
    }
}

fn current_pages(ctx: &Rc<Ctx>) -> (GridSpec, glaze_shell_core::mobile::GridPages) {
    let w = if ctx.body.width() > 0 {
        ctx.body.width()
    } else {
        FALLBACK_W
    };
    let h = if ctx.body.height() > 0 {
        ctx.body.height()
    } else {
        FALLBACK_H - 200
    };
    let spec = GridSpec::for_area(w, h, ctx.touch);
    let query = ctx.state.borrow().search().to_string();
    (spec, build_grid(&ctx.apps, &query, spec))
}

/// Re-render the current page of the grid and the page dots.
fn rebuild(ctx: &Rc<Ctx>) {
    let (spec, pages) = current_pages(ctx);
    let page = ctx.state.borrow().page().min(pages.page_count() - 1);
    ctx.state.borrow_mut().set_page(page, pages.page_count());

    while let Some(c) = ctx.body.first_child() {
        ctx.body.remove(&c);
    }
    let grid = gtk4::Grid::new();
    grid.set_row_homogeneous(true);
    grid.set_column_homogeneous(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);
    for (i, hit) in pages.page(page).iter().enumerate() {
        let tile = gtk4::Button::new();
        tile.add_css_class("glaze-app-tile");
        let v = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let img =
            gtk4::Image::from_icon_name(hit.icon.as_deref().unwrap_or("application-x-executable"));
        img.set_pixel_size(48);
        let label = gtk4::Label::new(Some(&hit.name));
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(12);
        v.append(&img);
        v.append(&label);
        tile.set_child(Some(&v));
        let id = hit.app_id.clone();
        let ctx2 = ctx.clone();
        tile.connect_clicked(move |_| match ctx2.registry.get(&id) {
            Some(app) => match glaze_service::launch::launch(app) {
                Ok(_c) => {
                    eprintln!("[glaze-shell] mobile: launched {} ({id})", app.name);
                    let effect = ctx2.state.borrow_mut().app_launched();
                    apply_effect(&ctx2, effect);
                }
                Err(e) => eprintln!("[glaze-shell] mobile: failed to launch {id}: {e}"),
            },
            None => eprintln!("[glaze-shell] mobile: {id} is no longer registered"),
        });
        grid.attach(
            &tile,
            (i % spec.columns) as i32,
            (i / spec.columns) as i32,
            1,
            1,
        );
    }
    if pages.total() == 0 {
        let empty = gtk4::Label::new(Some(&tr(Keys::NO_APPLICATIONS_FOUND)));
        empty.add_css_class("glaze-launcher-empty");
        ctx.body.append(&empty);
    } else {
        ctx.body.append(&grid);
    }

    while let Some(c) = ctx.dots.first_child() {
        ctx.dots.remove(&c);
    }
    if pages.page_count() > 1 {
        for p in 0..pages.page_count() {
            let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            dot.add_css_class("glaze-page-dot");
            if p == page {
                dot.add_css_class("glaze-selected");
            }
            ctx.dots.append(&dot);
        }
    }
}
