//! The start menu: a paper card anchored to the Start button.
//!
//! Layout (top to bottom): Kiln header, search field, then either the home
//! view (pinned grid, recent apps, all apps by category) or search results,
//! and a footer with the user name and the power row (lock, sleep, log out,
//! restart, shut down; the last three ask for confirmation inline).
//!
//! Keyboard: typing goes to the search field; arrows move over the pinned
//! grid and the lists (`glaze_shell_core::startmenu::move_selection`); Enter
//! launches; Esc clears the query, then closes. Opening and closing fade
//! with the frame-clock animator (instant under reduced motion). Size,
//! columns, placement and the power row come from `[launcher]`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glaze_common::config::{MenuAnchor, PanelPosition};
use glaze_common::model::AppEntry;
use glaze_service::search::search_apps;
use glaze_shell_core::startmenu::{self, Nav, RecentApps};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::anim::Animator;
use crate::icons;
use crate::ink::Glyph;
use crate::panel_ui::{self, Ctx};

/// (icon, fallback glyph, label, confirmation question, argv).
type PowerAction = (
    &'static str,
    Glyph,
    &'static str,
    Option<&'static str>,
    &'static [&'static str],
);

struct Menu {
    pop: gtk4::Popover,
    card: gtk4::Box,
    entry: gtk4::Entry,
    home: gtk4::ScrolledWindow,
    results: gtk4::ScrolledWindow,
    results_box: gtk4::Box,
    footer: gtk4::Box,
    confirm: gtk4::Box,
    /// Navigable buttons in visual order, with the app id they launch.
    items: RefCell<Vec<(gtk4::Button, String)>>,
    grid_len: Cell<usize>,
    selected: Cell<usize>,
    recent: RefCell<RecentApps>,
    ctx: Ctx,
    anim: Animator,
    closing: Cell<bool>,
}

thread_local! {
    /// The panel's Start button, so a lone Super tap (SIGUSR1) can toggle the menu.
    static START_BUTTON: std::cell::RefCell<Option<glib::WeakRef<gtk4::Button>>> =
        const { std::cell::RefCell::new(None) };
}

thread_local! {
    /// Set by a key toggle so the Start button's click handler skips its
    /// "just dismissed by a pointer click" guard.
    static FORCED_TOGGLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// True once (and clears) when the current click came from a key toggle.
pub fn take_forced_toggle() -> bool {
    FORCED_TOGGLE.with(|f| f.replace(false))
}

/// Toggle the start menu as if the Start button was clicked (open or close).
pub fn toggle_start() {
    let button = START_BUTTON.with(|b| b.borrow().as_ref().and_then(|w| w.upgrade()));
    if let Some(button) = button {
        FORCED_TOGGLE.with(|f| f.set(true));
        button.emit_clicked();
        FORCED_TOGGLE.with(|f| f.set(false));
    }
}

/// Wire the start menu to `button`.
pub fn attach(button: &gtk4::Button, ctx: &Ctx, position: PanelPosition) {
    START_BUTTON.with(|b| *b.borrow_mut() = Some(button.downgrade()));
    if ctx.cfg.launcher.style == glaze_common::config::MenuStyle::TwoPane {
        crate::start_pane::attach(button, ctx, position);
        return;
    }
    if ctx.cfg.launcher.style == glaze_common::config::MenuStyle::Classic {
        crate::classic_menu::attach(button, ctx, position);
        return;
    }
    let lc = &ctx.cfg.launcher;
    let pop = gtk4::Popover::new();
    pop.add_css_class("glaze-popover");
    pop.add_css_class("glaze-start-menu");
    pop.set_has_arrow(false);
    pop.set_autohide(true);
    pop.set_parent(button);
    pop.set_position(panel_ui::popover_position(position));
    if lc.anchor == MenuAnchor::StartButton {
        pop.set_halign(gtk4::Align::Start);
    }
    pop.set_offset(
        0,
        if position == PanelPosition::Bottom {
            -6
        } else {
            6
        },
    );

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    card.set_size_request(lc.width, lc.height);

    // Header: brand symbol + name (Kiln only on Kiln).
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.set_margin_start(4);
    header.set_margin_top(2);
    let dark = ctx.tokens.scheme == glaze_common::config::ColorScheme::Dark;
    let brand = icons::brand(&ctx.cfg.branding);
    if let Some(sym) = icons::brand_symbol(&brand, 22, dark) {
        header.append(&sym);
    }
    let title = gtk4::Label::new(Some(&brand.short_name()));
    title.add_css_class("glaze-menu-title");
    header.append(&title);
    if brand.is_kiln && brand.name == "Kiln GNU/Linux" {
        let subtitle = gtk4::Label::new(Some("GNU/Linux"));
        subtitle.add_css_class("glaze-card-sub");
        subtitle.set_valign(gtk4::Align::Center);
        subtitle.set_margin_top(3);
        header.append(&subtitle);
    }
    card.append(&header);

    // Search.
    let search_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    search_row.add_css_class("glaze-search");
    let glyph = icons::icon(
        "search",
        Glyph::Search,
        16,
        ctx.tokens.palette.text_secondary,
        ctx.stroke(),
    );
    glyph.set_margin_start(9);
    glyph.set_margin_end(2);
    search_row.append(&glyph);
    let entry = gtk4::Entry::new();
    entry.add_css_class("glaze-search-entry");
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("Type to search apps"));
    search_row.append(&entry);
    card.append(&search_row);

    let home = gtk4::ScrolledWindow::new();
    home.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    home.set_vexpand(true);
    let results = gtk4::ScrolledWindow::new();
    results.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    results.set_vexpand(true);
    results.set_visible(false);
    let results_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    results.set_child(Some(&results_box));
    card.append(&home);
    card.append(&results);

    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.add_css_class("glaze-menu-sep");
    card.append(&sep);
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    let confirm = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    confirm.set_visible(false);
    card.append(&footer);
    card.append(&confirm);
    pop.set_child(Some(&card));

    let menu = Rc::new(Menu {
        pop: pop.clone(),
        card,
        entry: entry.clone(),
        home,
        results,
        results_box,
        footer,
        confirm,
        items: RefCell::new(Vec::new()),
        grid_len: Cell::new(0),
        selected: Cell::new(0),
        recent: RefCell::new(RecentApps::load(&RecentApps::default_path(), 8)),
        ctx: ctx.clone(),
        // Driven by the panel surface's frame clock (the popover's own
        // clock stalled on the VM).
        anim: Animator::new(
            "start-menu",
            button,
            ctx.motion,
            ctx.cfg.motion.log_frame_times,
        ),
        closing: Cell::new(false),
    });
    menu.build_footer();

    {
        let m = menu.clone();
        entry.connect_changed(move |e| m.show_query(&e.text()));
    }
    {
        let m = menu.clone();
        entry.connect_activate(move |_| m.activate_selected());
    }
    {
        let key = gtk4::EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let m = menu.clone();
        key.connect_key_pressed(move |_, keyval, _, _| {
            let nav = match keyval {
                gdk::Key::Up => Some(Nav::Up),
                gdk::Key::Down => Some(Nav::Down),
                gdk::Key::Left if m.entry.text().is_empty() => Some(Nav::Left),
                gdk::Key::Right if m.entry.text().is_empty() => Some(Nav::Right),
                gdk::Key::Page_Up => Some(Nav::Home),
                gdk::Key::Page_Down => Some(Nav::End),
                _ => None,
            };
            if let Some(nav) = nav {
                m.navigate(nav);
                return glib::Propagation::Stop;
            }
            match keyval {
                gdk::Key::Escape => {
                    if m.confirm.is_visible() {
                        m.hide_confirm();
                    } else if !m.entry.text().is_empty() {
                        m.entry.set_text("");
                    } else {
                        m.close();
                    }
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    m.activate_selected();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        pop.add_controller(key);
    }
    {
        let m = menu.clone();
        let b = button.clone();
        button.connect_clicked(move |_| {
            if m.pop.is_visible() {
                m.close();
            } else {
                m.open();
                b.add_css_class("glaze-open");
            }
        });
    }
    {
        let b = button.clone();
        pop.connect_closed(move |_| b.remove_css_class("glaze-open"));
    }
}

impl Menu {
    fn open(self: &Rc<Self>) {
        self.closing.set(false);
        self.hide_confirm();
        self.entry.set_text("");
        self.build_home();
        self.card.set_opacity(0.0);
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
        self.entry.grab_focus();
        let c = self.card.clone();
        self.anim.animate(
            "o",
            0.0,
            1.0,
            self.ctx.tokens.metrics.motion_menu_ms as f64 + 40.0,
            move |v| c.set_opacity(v),
            None,
        );
        // Slide up a few px from the button (margin on the card).
        let c = self.card.clone();
        let up = self.ctx.cfg.panel.position == PanelPosition::Bottom;
        self.anim.animate(
            "y",
            10.0,
            0.0,
            self.ctx.tokens.metrics.motion_menu_ms as f64 + 80.0,
            move |v| {
                let v = v.round() as i32;
                if up {
                    c.set_margin_top(v);
                } else {
                    c.set_margin_bottom(v);
                }
            },
            None,
        );
    }

    fn close(self: &Rc<Self>) {
        if self.closing.replace(true) {
            return;
        }
        let pop = self.pop.clone();
        let c = self.card.clone();
        let me = self.clone();
        self.anim.animate(
            "o",
            self.card.opacity(),
            0.0,
            self.ctx.tokens.metrics.motion_micro_ms as f64,
            move |v| c.set_opacity(v),
            Some(Box::new(move || {
                if me.closing.get() {
                    pop.popdown();
                }
            })),
        );
    }

    fn section_title(text: &str) -> gtk4::Label {
        let l = gtk4::Label::new(Some(text));
        l.add_css_class("glaze-section-title");
        l.set_xalign(0.0);
        l
    }

    fn icon_for(&self, app: &AppEntry, px: i32) -> gtk4::Widget {
        panel_ui::app_icon_sized(&self.ctx, &app.id, Some(app), px)
    }

    fn tile(self: &Rc<Self>, app: &AppEntry) -> gtk4::Button {
        let b = gtk4::Button::new();
        b.add_css_class("glaze-app-tile");
        b.set_focus_on_click(false);
        let col = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        col.set_halign(gtk4::Align::Center);
        col.append(&self.icon_for(app, 26));
        let l = gtk4::Label::new(Some(&app.name));
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        l.set_max_width_chars(10);
        l.set_justify(gtk4::Justification::Center);
        col.append(&l);
        b.set_child(Some(&col));
        b.set_tooltip_text(app.comment.as_deref().or(Some(&app.name)));
        self.wire(&b, &app.id);
        b
    }

    fn row(self: &Rc<Self>, app: &AppEntry, detail: Option<&str>) -> gtk4::Button {
        let b = gtk4::Button::new();
        b.add_css_class("glaze-app-row");
        b.set_focus_on_click(false);
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        line.append(&self.icon_for(app, 20));
        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        text.set_valign(gtk4::Align::Center);
        let l = gtk4::Label::new(Some(&app.name));
        l.set_xalign(0.0);
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        text.append(&l);
        if let Some(d) = detail.filter(|d| !d.is_empty()) {
            let s = gtk4::Label::new(Some(d));
            s.add_css_class("glaze-card-sub");
            s.set_xalign(0.0);
            s.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            text.append(&s);
        }
        line.append(&text);
        b.set_child(Some(&line));
        self.wire(&b, &app.id);
        b
    }

    fn wire(self: &Rc<Self>, b: &gtk4::Button, id: &str) {
        let m = self.clone();
        let id2 = id.to_string();
        b.connect_clicked(move |_| m.launch_id(&id2));
        let m = self.clone();
        let b2 = b.clone();
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter(move |_, _, _| {
            let idx = m.items.borrow().iter().position(|(x, _)| x == &b2);
            if let Some(i) = idx {
                m.select(i, false);
            }
        });
        b.add_controller(motion);
        self.items.borrow_mut().push((b.clone(), id.to_string()));
    }

    fn build_home(self: &Rc<Self>) {
        self.items.borrow_mut().clear();
        let apps = self.ctx.registry.apps();
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        body.set_margin_end(6);

        // Pinned grid.
        let pins: Vec<&AppEntry> = self
            .ctx
            .pins()
            .iter()
            .filter_map(|id| apps.iter().find(|a| &a.id == id))
            .collect();
        let cols = self.ctx.cfg.launcher.columns.max(2);
        if !pins.is_empty() {
            body.append(&Self::section_title("Pinned"));
            let grid = gtk4::Grid::new();
            grid.set_column_spacing(4);
            grid.set_row_spacing(4);
            grid.set_halign(gtk4::Align::Start);
            // Fixed-width tiles so a short pin list does not stretch.
            let usable = self.ctx.cfg.launcher.width - 24 - 10;
            let tile_w = ((usable - 4 * (cols as i32 - 1)) / cols as i32).max(56);
            for (i, app) in pins.iter().enumerate() {
                let t = self.tile(app);
                t.set_size_request(tile_w, -1);
                grid.attach(&t, (i % cols) as i32, (i / cols) as i32, 1, 1);
            }
            body.append(&grid);
        }
        self.grid_len.set(self.items.borrow().len());

        if self.ctx.cfg.launcher.show_recent {
            let recent = self.recent.borrow().installed(apps);
            if !recent.is_empty() {
                body.append(&Self::section_title("Recent"));
                for app in recent.into_iter().take(4) {
                    body.append(&self.row(app, app.comment.as_deref()));
                }
            }
        }

        body.append(&Self::section_title("All apps"));
        for (section, list) in startmenu::grouped(apps) {
            let h = gtk4::Label::new(Some(section));
            h.add_css_class("glaze-category");
            h.set_xalign(0.0);
            body.append(&h);
            for app in list {
                body.append(&self.row(app, None));
            }
        }
        self.home.set_child(Some(&body));
        self.home.set_visible(true);
        self.results.set_visible(false);
        self.select(0, true);
    }

    fn show_query(self: &Rc<Self>, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            self.build_home();
            return;
        }
        self.items.borrow_mut().clear();
        self.grid_len.set(0);
        while let Some(c) = self.results_box.first_child() {
            self.results_box.remove(&c);
        }
        let apps = self.ctx.registry.apps();
        let hits = search_apps(apps, q, self.ctx.cfg.launcher.search_limit.max(12));
        if hits.is_empty() {
            let l = gtk4::Label::new(Some(&format!("No apps match \u{201c}{q}\u{201d}")));
            l.add_css_class("glaze-card-sub");
            l.set_margin_top(16);
            self.results_box.append(&l);
        } else {
            self.results_box
                .append(&Self::section_title("Best matches"));
            for hit in hits {
                if let Some(app) = apps.iter().find(|a| a.id == hit.app_id) {
                    let detail = app.comment.clone().or_else(|| app.generic_name.clone());
                    self.results_box.append(&self.row(app, detail.as_deref()));
                }
            }
        }
        self.home.set_visible(false);
        self.results.set_visible(true);
        self.select(0, true);
    }

    fn select(&self, i: usize, scroll: bool) {
        let items = self.items.borrow();
        for (n, (b, _)) in items.iter().enumerate() {
            if n == i {
                b.add_css_class("glaze-selected");
            } else {
                b.remove_css_class("glaze-selected");
            }
        }
        self.selected.set(i);
        if scroll {
            if let Some((b, _)) = items.get(i) {
                let sw = if self.results.is_visible() {
                    &self.results
                } else {
                    &self.home
                };
                if let Some(child) = sw.child() {
                    if let Some(p) = b.compute_point(&child, &gtk4::graphene::Point::new(0.0, 0.0))
                    {
                        let adj = sw.vadjustment();
                        let y = p.y() as f64;
                        let h = b.height() as f64;
                        if y < adj.value() {
                            adj.set_value(y - 24.0);
                        } else if y + h > adj.value() + adj.page_size() {
                            adj.set_value(y + h - adj.page_size() + 8.0);
                        }
                    }
                }
            }
        }
    }

    fn navigate(&self, nav: Nav) {
        let len = self.items.borrow().len();
        let next = startmenu::move_selection(
            self.selected.get(),
            len,
            self.grid_len.get(),
            self.ctx.cfg.launcher.columns.max(2),
            nav,
        );
        self.select(next, true);
    }

    fn activate_selected(self: &Rc<Self>) {
        let id = self
            .items
            .borrow()
            .get(self.selected.get())
            .map(|(_, id)| id.clone());
        if let Some(id) = id {
            self.launch_id(&id);
        }
    }

    fn launch_id(self: &Rc<Self>, id: &str) {
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

    fn build_footer(self: &Rc<Self>) {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let who = gtk4::Label::new(Some(&user));
        who.add_css_class("glaze-user");
        who.set_hexpand(true);
        who.set_xalign(0.0);
        who.set_margin_start(6);
        self.footer.append(&who);
        if !self.ctx.cfg.launcher.show_power {
            return;
        }
        let actions: [PowerAction; 5] = [
            (
                "lock",
                Glyph::Lock,
                "Lock",
                None,
                &["sh", "-c", crate::LOCK_SH],
            ),
            (
                "sleep",
                Glyph::Moon,
                "Sleep",
                None,
                &["systemctl", "suspend"],
            ),
            (
                "logout",
                Glyph::Logout,
                "Log out",
                Some("Log out now?"),
                &["sh", "-c", crate::LOGOUT_SH],
            ),
            (
                "restart",
                Glyph::Restart,
                "Restart",
                Some("Restart now?"),
                &["systemctl", "reboot"],
            ),
            (
                "power",
                Glyph::Power,
                "Shut down",
                Some("Shut down now?"),
                &["systemctl", "poweroff"],
            ),
        ];
        for (icon, glyph, tip, confirm, argv) in actions {
            let b = gtk4::Button::new();
            b.add_css_class("glaze-power-button");
            b.set_tooltip_text(Some(tip));
            b.set_child(Some(&icons::icon(
                icon,
                glyph,
                18,
                self.ctx.ink(),
                self.ctx.stroke(),
            )));
            let m = self.clone();
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            let tip = tip.to_string();
            let confirm = confirm.map(str::to_string);
            b.connect_clicked(move |_| match &confirm {
                Some(q) => m.ask(q, &tip, argv.clone()),
                None => {
                    m.close();
                    run_power(&argv);
                }
            });
            self.footer.append(&b);
        }
    }

    fn ask(self: &Rc<Self>, question: &str, verb: &str, argv: Vec<String>) {
        while let Some(c) = self.confirm.first_child() {
            self.confirm.remove(&c);
        }
        let q = gtk4::Label::new(Some(question));
        q.set_hexpand(true);
        q.set_xalign(0.0);
        q.set_margin_start(6);
        self.confirm.append(&q);
        let cancel = panel_ui::menu_item("Cancel", {
            let m = self.clone();
            move || m.hide_confirm()
        });
        let go = panel_ui::menu_item(verb, {
            let m = self.clone();
            move || {
                m.close();
                run_power(&argv);
            }
        });
        go.add_css_class("glaze-confirm");
        self.confirm.append(&cancel);
        self.confirm.append(&go);
        self.footer.set_visible(false);
        self.confirm.set_visible(true);
        go.grab_focus();
    }

    fn hide_confirm(&self) {
        self.confirm.set_visible(false);
        self.footer.set_visible(true);
        self.entry.grab_focus();
    }
}

fn run_power(argv: &[String]) {
    let Some((prog, args)) = argv.split_first() else {
        return;
    };
    eprintln!("[glaze-shell] power: {}", argv.join(" "));
    match std::process::Command::new(prog).args(args).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("[glaze-shell] power action {prog} failed: {e}"),
    }
}
