//! GTK4 user interface: window, sidebar, search, page stack, header actions,
//! toasts, and the change pipeline (store -> CSS live preview -> toast with
//! undo).

pub mod pages;
pub mod widgets;
mod wincontrols;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

use glaze_common::config::{GlazeConfig, MotionStyle};
use glaze_design::{ColorScheme, ThemeTokens};

use crate::catalog::{self, Category, Page};
use crate::cli::Options;
use crate::search;
use crate::store::{Change, Doc, Store};
use crate::style::{self, StyleInputs};

pub const APP_ID: &str = "org.glaze.Settings";

pub type App = Rc<AppInner>;

/// A registered row: the row widget (scrolled to and flashed) and the
/// control that receives keyboard focus.
#[derive(Clone)]
pub struct Registered {
    pub row: gtk::Widget,
    pub focus: gtk::Widget,
}

/// An ink glyph to recolour on theme changes: image, icon name, size, tone.
type ThemedIcon = (glib::WeakRef<gtk::Image>, &'static str, i32, IconTone);
/// Runs after every successful change.
pub type ChangeHook = Box<dyn Fn(&App)>;

pub struct AppInner {
    pub store: RefCell<Store>,
    pub window: gtk::ApplicationWindow,
    provider: gtk::CssProvider,
    stack: gtk::Stack,
    nav: gtk::ListBox,
    nav_rows: RefCell<Vec<(Page, gtk::ListBoxRow, gtk::Label)>>,
    search: gtk::SearchEntry,
    results: gtk::ListBox,
    results_title: gtk::Label,
    results_ids: RefCell<Vec<&'static str>>,
    banner: gtk::Box,
    banner_label: gtk::Label,
    rows: RefCell<HashMap<&'static str, Registered>>,
    scrollers: RefCell<HashMap<Page, gtk::ScrolledWindow>>,
    /// Suppresses control handlers while pages are (re)built or synced.
    pub building: Cell<bool>,
    current: Cell<Page>,
    undo_btn: gtk::Button,
    reset_btn: gtk::Button,
    toast_revealer: gtk::Revealer,
    toast_label: gtk::Label,
    toast_undo: gtk::Button,
    toast_timer: RefCell<Option<glib::SourceId>>,
    tokens: RefCell<ThemeTokens>,
    icons: RefCell<Vec<ThemedIcon>>,
    /// Redraw hooks for previews that paint with the tokens.
    pub previews: RefCell<Vec<glib::WeakRef<gtk::DrawingArea>>>,
    /// Callbacks run after any successful change (keep previews in sync).
    pub on_change: RefCell<Vec<ChangeHook>>,
    pub thumbs: RefCell<HashMap<String, gdk::Texture>>,
    page_built: RefCell<HashMap<Page, gtk::Widget>>,
    /// Watches the config directory for changes made by others (the shell,
    /// `glaze-settings --set`, an editor) and keeps the window in sync.
    monitor: RefCell<Option<gio::FileMonitor>>,
    monitor_timer: RefCell<Option<glib::SourceId>>,
    /// The scheme other apps were last switched to via `glaze-colour-scheme`.
    system_scheme: Cell<Option<ColorScheme>>,
    /// Last `appearance.amoled` seen by `sync_system_scheme`.
    system_amoled: Cell<Option<bool>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconTone {
    Ink,
    Secondary,
}

pub fn run(opts: Options) -> ExitCode {
    // Initialise GTK explicitly first: without a display GApplication would
    // otherwise exit 0 silently. Every CssProvider is created later, inside
    // `activate`, never before this point.
    // Kiln runs on software rendering (llvmpipe) for now: GTK's cairo
    // renderer maps the window ~40 % sooner than GL there (measured in the
    // VM: 0.7-1.0 s vs 1.3-1.5 s). An explicit GSK_RENDERER still wins.
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    if let Err(e) = gtk::init() {
        eprintln!(
            "glaze-settings: cannot start the window: {e} (WAYLAND_DISPLAY={:?}, XDG_RUNTIME_DIR={:?})",
            std::env::var("WAYLAND_DISPLAY").unwrap_or_default(),
            std::env::var("XDG_RUNTIME_DIR").unwrap_or_default()
        );
        return ExitCode::FAILURE;
    }
    let app = gtk::Application::builder().application_id(APP_ID).build();
    let opts = Rc::new(opts);
    let holder: Rc<RefCell<Option<App>>> = Rc::new(RefCell::new(None));
    app.connect_activate(move |gapp| {
        if let Some(a) = holder.borrow().as_ref() {
            // Second launch (single instance): raise and focus the window.
            // Without an xdg-activation token Sway only marks it urgent, so
            // also ask the compositor directly; failures are harmless.
            a.window.unminimize();
            a.window.present();
            wincontrols::raise();
            return;
        }
        let a = build(gapp, &opts);
        *holder.borrow_mut() = Some(a);
    });
    let code = app.run_with_args(&["glaze-settings"]);
    if code == glib::ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn texture_dir() -> std::path::PathBuf {
    glaze_common::paths::cache_home().join("glaze/settings/textures")
}

/// Write an embedded paper tile to the cache once and return its URL.
fn texture_url(set: &str, scheme: ColorScheme) -> Option<String> {
    let file = glaze_design::texture::tile_file(set, scheme)?;
    let bytes = crate::assets::texture(&file)?;
    let dir = texture_dir();
    let path = dir.join(&file);
    if std::fs::read(&path).ok().as_deref() != Some(bytes) {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(format!("file://{}", path.display()))
}

/// Durations: 1/speed, snappy shortens, off/reduced zero.
pub fn duration_scale(cfg: &GlazeConfig) -> f64 {
    let base = 1.0 / cfg.motion.speed.clamp(0.25, 4.0);
    match cfg.motion.style {
        MotionStyle::Snappy => base * 0.65,
        MotionStyle::Off => 0.0,
        _ => base,
    }
}

pub fn reduced_motion(cfg: &GlazeConfig) -> bool {
    let system = gtk::Settings::default()
        .map(|s| !s.is_gtk_enable_animations())
        .unwrap_or(false);
    cfg.appearance.reduced_motion || system || cfg.motion.style == MotionStyle::Off
}

fn build(gapp: &gtk::Application, opts: &Options) -> App {
    let t_start = std::time::Instant::now();
    let store = Store::open_default();
    let window = gtk::ApplicationWindow::builder()
        .application(gapp)
        .title("Settings")
        .default_width(1120)
        .default_height(740)
        .build();
    window.set_size_request(900, 560);
    window.add_css_class("fs-window");
    window.set_icon_name(Some("preferences-system"));

    let provider = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 10,
    );

    // ---- header bar ------------------------------------------------------
    let header = gtk::HeaderBar::new();
    header.add_css_class("fs-headerbar");
    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Settings"));
    title.add_css_class("fs-app-title");
    let sub = gtk::Label::new(Some(&format!(
        "Glaze · {}",
        glaze_common::brand::Brand::detect(&store.config().branding).name
    )));
    sub.add_css_class("fs-app-sub");
    title_box.append(&title);
    title_box.append(&sub);
    header.set_title_widget(Some(&title_box));
    let undo_btn = widgets::header_button("undo", "Undo");
    undo_btn.set_sensitive(false);
    header.pack_start(&undo_btn);
    let export_btn = widgets::header_button("export", "Export theme…");
    let import_btn = widgets::header_button("import", "Import theme…");
    let reset_btn = widgets::header_button("reset", "Reset section…");
    header.pack_end(&wincontrols::build(&window, &header));
    header.pack_end(&export_btn);
    header.pack_end(&import_btn);
    header.pack_end(&reset_btn);
    window.set_titlebar(Some(&header));

    // ---- sidebar -----------------------------------------------------------
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.add_css_class("fs-sidebar");
    sidebar.set_size_request(250, -1);
    let search = gtk::SearchEntry::new();
    search.add_css_class("fs-search");
    search.set_placeholder_text(Some("Search settings"));
    search.update_property(&[gtk::accessible::Property::Label("Search settings")]);
    sidebar.append(&search);
    let nav_scroll = gtk::ScrolledWindow::new();
    nav_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    nav_scroll.set_vexpand(true);
    let nav = gtk::ListBox::new();
    nav.add_css_class("fs-nav");
    nav.set_selection_mode(gtk::SelectionMode::Single);
    nav.update_property(&[gtk::accessible::Property::Label("Categories")]);
    nav_scroll.set_child(Some(&nav));
    sidebar.append(&nav_scroll);
    let foot = gtk::Label::new(Some(&format!(
        "Glaze {} · settings {}",
        glaze_common::VERSION,
        env!("CARGO_PKG_VERSION")
    )));
    foot.add_css_class("fs-sidebar-foot");
    foot.set_halign(gtk::Align::Start);
    sidebar.append(&foot);

    // ---- content -----------------------------------------------------------
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    let results = gtk::ListBox::new();
    results.add_css_class("fs-results");
    results.set_selection_mode(gtk::SelectionMode::Single);
    results.update_property(&[gtk::accessible::Property::Label("Search results")]);
    let results_title = gtk::Label::new(None);
    results_title.add_css_class("fs-page-title");
    results_title.set_halign(gtk::Align::Start);
    results_title.set_wrap(true);
    let results_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    results_page.add_css_class("fs-page");
    results_page.append(&results_title);
    results_page.append(&results);
    let results_scroll = gtk::ScrolledWindow::new();
    results_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    results_scroll.set_child(Some(&results_page));
    stack.add_named(&results_scroll, Some("search"));

    let banner = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    banner.add_css_class("fs-banner");
    banner.set_visible(false);
    let banner_label = gtk::Label::new(None);
    banner_label.set_wrap(true);
    banner_label.set_xalign(0.0);
    banner_label.set_hexpand(true);
    banner.append(&banner_label);
    let banner_reset = gtk::Button::with_label("Reset to defaults (keeps a backup)");
    banner_reset.add_css_class("fs-btn");
    banner_reset.add_css_class("fs-danger");
    banner.append(&banner_reset);

    let toast_revealer = gtk::Revealer::new();
    toast_revealer.set_transition_type(gtk::RevealerTransitionType::SlideUp);
    toast_revealer.set_halign(gtk::Align::Center);
    toast_revealer.set_valign(gtk::Align::End);
    let toast = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    toast.add_css_class("fs-toast");
    let toast_label = gtk::Label::new(None);
    toast_label.set_max_width_chars(70);
    toast_label.set_wrap(true);
    let toast_undo = gtk::Button::with_label("Undo");
    toast_undo.add_css_class("fs-btn");
    toast.append(&toast_label);
    toast.append(&toast_undo);
    toast.update_property(&[gtk::accessible::Property::Label("Status")]);
    toast_revealer.set_child(Some(&toast));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&banner);
    content.append(&stack);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&content));
    overlay.add_overlay(&toast_revealer);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.add_css_class("fs-body");
    body.append(&sidebar);
    body.append(&overlay);
    window.set_child(Some(&body));

    let cfg = store.config();
    let (tokens, _, _) = glaze_common::config_check::load_tokens(&effective_for_window(&cfg));
    let app: App = Rc::new(AppInner {
        store: RefCell::new(store),
        window: window.clone(),
        provider,
        stack: stack.clone(),
        nav: nav.clone(),
        nav_rows: RefCell::new(Vec::new()),
        search: search.clone(),
        results: results.clone(),
        results_title,
        results_ids: RefCell::new(Vec::new()),
        banner: banner.clone(),
        banner_label,
        rows: RefCell::new(HashMap::new()),
        scrollers: RefCell::new(HashMap::new()),
        building: Cell::new(false),
        current: Cell::new(opts.page.unwrap_or(Page::Appearance)),
        undo_btn: undo_btn.clone(),
        reset_btn: reset_btn.clone(),
        toast_revealer,
        toast_label,
        toast_undo: toast_undo.clone(),
        toast_timer: RefCell::new(None),
        tokens: RefCell::new(tokens),
        icons: RefCell::new(Vec::new()),
        previews: RefCell::new(Vec::new()),
        on_change: RefCell::new(Vec::new()),
        thumbs: RefCell::new(HashMap::new()),
        page_built: RefCell::new(HashMap::new()),
        monitor: RefCell::new(None),
        monitor_timer: RefCell::new(None),
        system_scheme: Cell::new(crate::colourscheme::last_applied()),
        system_amoled: Cell::new(None),
    });
    app.watch_config();
    // Migrate the settings-0.2 sidecar `scheme_mode` into config.toml,
    // where glaze-common now has it.
    if app.extra("appearance.scheme_mode").is_some() {
        let auto = app
            .extra("appearance.scheme_mode")
            .and_then(|v| v.as_str().map(|m| m == "auto"))
            .unwrap_or(false);
        let mut ch = vec![crate::store::Change::Remove(
            Doc::Extra,
            "appearance.scheme_mode".into(),
        )];
        if auto {
            ch.push(crate::store::Change::Set(
                Doc::Main,
                "appearance.scheme_mode".into(),
                "auto".into(),
            ));
        }
        let _ = app
            .store
            .borrow_mut()
            .apply(ch, "Move scheme mode to config.toml", false);
    }
    {
        // Automatic scheme: repaint when 07:00 / 19:00 passes.
        let weak = Rc::downgrade(&app);
        let last = Cell::new(effective_for_window(&app.cfg()).appearance.scheme);
        glib::timeout_add_seconds_local(60, move || {
            let Some(a) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = effective_for_window(&a.cfg()).appearance.scheme;
            if now != last.get() {
                last.set(now);
                a.apply_style();
                a.sync_system_scheme();
            }
            glib::ControlFlow::Continue
        });
    }
    app.apply_style();
    // Bring other toolkits in line with Glaze's scheme (no-op when they match).
    app.sync_system_scheme();

    // Sidebar rows, grouped like the Windows 2000 Control Panel.
    for cat in Category::ALL {
        let heading = gtk::Label::new(Some(&cat.title().to_uppercase()));
        heading.add_css_class("fs-sidebar-heading");
        heading.set_halign(gtk::Align::Start);
        // A plain, non-focusable row: GtkListBox headers did not paint their
        // label in this layout.
        let hrow = gtk::ListBoxRow::new();
        hrow.set_selectable(false);
        hrow.set_activatable(false);
        hrow.set_can_focus(false);
        hrow.add_css_class("fs-nav-heading");
        hrow.set_child(Some(&heading));
        hrow.set_accessible_role(gtk::AccessibleRole::Heading);
        nav.append(&hrow);
        for p in Page::ALL.into_iter().filter(|p| p.category() == cat) {
            let row = gtk::ListBoxRow::new();
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            b.append(&app.icon(p.icon(), 18, IconTone::Ink));
            let l = gtk::Label::new(Some(p.title()));
            l.set_hexpand(true);
            l.set_xalign(0.0);
            b.append(&l);
            let count = gtk::Label::new(None);
            count.add_css_class("fs-nav-count");
            count.set_visible(false);
            b.append(&count);
            row.set_child(Some(&b));
            row.update_property(&[gtk::accessible::Property::Label(p.title())]);
            row.set_tooltip_text(Some(p.blurb()));
            nav.append(&row);
            app.nav_rows.borrow_mut().push((p, row, count));
        }
    }

    // Pages are built lazily (`ensure_page`) the first time they are shown:
    // several probe the system (wpctl, nmcli, upower, .desktop scans), and
    // building all of them before the first frame took ~4 s on the VM.
    for p in Page::ALL {
        let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stack.add_named(&placeholder, Some(p.slug()));
    }

    // ---- wiring ------------------------------------------------------------
    {
        let a = app.clone();
        nav.connect_row_selected(move |_, row| {
            if a.building.get() {
                return;
            }
            let Some(row) = row else { return };
            let page = a
                .nav_rows
                .borrow()
                .iter()
                .find(|(_, r, _)| r == row)
                .map(|(p, _, _)| *p);
            if let Some(p) = page {
                if !a.search.text().is_empty() {
                    a.building.set(true);
                    a.search.set_text("");
                    a.building.set(false);
                    a.update_nav_counts(&[]);
                }
                a.show_page(p);
            }
        });
    }
    {
        let a = app.clone();
        search.connect_search_changed(move |e| {
            if !a.building.get() {
                a.run_search(&e.text());
            }
        });
    }
    {
        let a = app.clone();
        search.connect_activate(move |_| {
            let first = a.results_ids.borrow().first().copied();
            if let Some(id) = first {
                a.jump(id);
            }
        });
    }
    {
        let a = app.clone();
        search.connect_stop_search(move |e| {
            e.set_text("");
            a.show_page(a.current.get());
        });
    }
    {
        // Down from the search box moves into the results.
        let a = app.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, k, _, _| {
            if k == gdk::Key::Down
                && !a.results_ids.borrow().is_empty()
                && a.stack.visible_child_name().as_deref() == Some("search")
            {
                if let Some(r) = a.results.row_at_index(0) {
                    a.results.select_row(Some(&r));
                    r.grab_focus();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        search.add_controller(key);
    }
    {
        let a = app.clone();
        results.connect_row_activated(move |_, row| {
            let id = a.results_ids.borrow().get(row.index() as usize).copied();
            if let Some(id) = id {
                a.jump(id);
            }
        });
    }
    {
        let a = app.clone();
        undo_btn.connect_clicked(move |_| a.undo());
    }
    {
        let a = app.clone();
        toast_undo.connect_clicked(move |_| a.undo());
    }
    {
        let a = app.clone();
        reset_btn.connect_clicked(move |_| a.confirm_reset_section());
    }
    {
        let a = app.clone();
        import_btn.connect_clicked(move |_| pages::appearance::import_dialog(&a));
    }
    {
        let a = app.clone();
        export_btn.connect_clicked(move |_| pages::appearance::export_dialog(&a));
    }
    {
        let a = app.clone();
        banner_reset.connect_clicked(move |_| {
            let a2 = a.clone();
            widgets::confirm(
                &a,
                "Reset config.toml?",
                "The file does not parse, so Glaze is using defaults. Resetting replaces it with defaults; the broken file is kept as a backup.",
                "Reset",
                true,
                move || {
                    let r = a2.store.borrow_mut().reset_broken_main();
                    match r {
                        Ok(b) => {
                            let where_ = b.map(|p| p.display().to_string()).unwrap_or_default();
                            a2.after_change(&format!("Reset config.toml (backup: {where_})"));
                            a2.rebuild(None);
                        }
                        Err(e) => a2.toast_error(&e),
                    }
                },
            );
        });
    }
    // Window shortcuts: Ctrl+F search, Ctrl+Z undo, Escape clears search,
    // Alt+Up/Down move between categories.
    {
        let a = app.clone();
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed(move |_, k, _, state| {
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let alt = state.contains(gdk::ModifierType::ALT_MASK);
            if ctrl && (k == gdk::Key::f || k == gdk::Key::F) {
                a.search.grab_focus();
                return glib::Propagation::Stop;
            }
            if ctrl && (k == gdk::Key::z || k == gdk::Key::Z) && a.store.borrow().can_undo() {
                // Leave text undo to entries that have focus.
                let in_entry = GtkWindowExt::focus(&a.window).is_some_and(|w| {
                    w.is::<gtk::Text>() || w.ancestor(gtk::Entry::static_type()).is_some()
                });
                if !in_entry {
                    a.undo();
                    return glib::Propagation::Stop;
                }
            }
            if alt && (k == gdk::Key::Up || k == gdk::Key::Down) {
                let idx = Page::ALL
                    .iter()
                    .position(|p| *p == a.current.get())
                    .unwrap_or(0) as i32;
                let next = if k == gdk::Key::Up { idx - 1 } else { idx + 1 };
                if let Some(p) = Page::ALL.get(next.rem_euclid(Page::ALL.len() as i32) as usize) {
                    a.show_page(*p);
                    a.focus_first_control();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key);
    }

    app.refresh_header();
    app.refresh_banner();
    app.show_page(app.current.get());
    window.present();
    if std::env::var_os("GLAZE_SETTINGS_TRACE").is_some() {
        eprintln!(
            "[glaze-settings] window presented {} ms after activate",
            t_start.elapsed().as_millis()
        );
        window.connect_map(move |_| {
            eprintln!(
                "[glaze-settings] mapped {} ms after activate",
                t_start.elapsed().as_millis()
            )
        });
    }

    if let Some(q) = &opts.search {
        search.set_text(q);
        search.grab_focus();
        search.set_position(-1);
    } else if let Some(id) = opts.focus.clone() {
        let a = app.clone();
        glib::timeout_add_local_once(Duration::from_millis(250), move || {
            a.jump(catalog::find(&id).map(|e| e.id).unwrap_or("about.system"))
        });
    } else {
        // Keyboard users start in the category list.
        let a = app.clone();
        glib::idle_add_local_once(move || {
            if let Some((_, row, _)) = a
                .nav_rows
                .borrow()
                .iter()
                .find(|(p, _, _)| *p == a.current.get())
            {
                row.grab_focus();
            }
        });
    }
    app
}

/// The configuration the window paints with: `appearance.scheme_mode =
/// "auto"` resolves by local time exactly like the shell
/// (`SchemeMode::resolve`).
fn effective_for_window(cfg: &GlazeConfig) -> GlazeConfig {
    let mut c = cfg.clone();
    let hour = glib::DateTime::now_local()
        .map(|d| d.hour() as u32)
        .unwrap_or(12);
    c.appearance.scheme = c.appearance.scheme_mode.resolve(c.appearance.scheme, hour);
    c
}

impl AppInner {
    pub fn cfg(&self) -> GlazeConfig {
        self.store.borrow().config()
    }

    pub fn extra(&self, path: &str) -> Option<toml::Value> {
        self.store.borrow().get(Doc::Extra, path)
    }

    pub fn main_value(&self, path: &str) -> Option<toml::Value> {
        self.store.borrow().get(Doc::Main, path)
    }

    pub fn tokens(&self) -> ThemeTokens {
        self.tokens.borrow().clone()
    }

    /// Re-emit CSS for the current configuration (the window is its own live
    /// preview) and recolour ink glyphs.
    pub fn apply_style(&self) {
        let cfg = self.cfg();
        let (tokens, _, _) = glaze_common::config_check::load_tokens(&effective_for_window(&cfg));
        let inputs = StyleInputs {
            reduced_motion: reduced_motion(&cfg),
            duration_scale: duration_scale(&cfg),
            reduced_effects: cfg.appearance.reduced_effects,
            texture_url: texture_url(&cfg.appearance.texture, tokens.scheme),
            texture_intensity: cfg.appearance.texture_intensity,
        };
        self.provider
            .load_from_string(&style::css(&tokens, &inputs));
        if let Some(s) = gtk::Settings::default() {
            s.set_gtk_application_prefer_dark_theme(tokens.scheme == ColorScheme::Dark);
        }
        let ms = if inputs.reduced_motion {
            0
        } else {
            (tokens.metrics.motion_menu_ms as f64 * inputs.duration_scale).round() as u32
        };
        self.stack.set_transition_duration(ms);
        self.toast_revealer.set_transition_duration(ms);
        *self.tokens.borrow_mut() = tokens;
        self.recolor_icons();
        self.previews.borrow_mut().retain(|w| match w.upgrade() {
            Some(d) => {
                d.queue_draw();
                true
            }
            None => false,
        });
    }

    pub fn motion_ms(&self, base: u32) -> u32 {
        let cfg = self.cfg();
        if reduced_motion(&cfg) {
            0
        } else {
            (base as f64 * duration_scale(&cfg)).round() as u32
        }
    }

    fn tone_hex(&self, tone: IconTone) -> String {
        let t = self.tokens.borrow();
        match tone {
            IconTone::Ink => t.palette.ink,
            IconTone::Secondary => t.palette.text_secondary,
        }
        .to_rgb()
        .to_hex()
    }

    fn render_icon(&self, name: &str, size: i32, tone: IconTone) -> Option<gdk::Texture> {
        let svg = crate::assets::icon(name)?;
        let scale = self.window.scale_factor().max(1);
        let px = size * scale;
        let svg = crate::assets::colored(svg, &self.tone_hex(tone))
            .replacen("width=\"24\"", &format!("width=\"{px}\""), 1)
            .replacen("height=\"24\"", &format!("height=\"{px}\""), 1);
        gdk::Texture::from_bytes(&glib::Bytes::from_owned(svg.into_bytes())).ok()
    }

    /// An ink glyph that follows the theme (recoloured on every style change).
    pub fn icon(&self, name: &'static str, size: i32, tone: IconTone) -> gtk::Image {
        let img = gtk::Image::new();
        img.set_pixel_size(size);
        img.set_accessible_role(gtk::AccessibleRole::Presentation);
        if let Some(t) = self.render_icon(name, size, tone) {
            img.set_paintable(Some(&t));
        }
        self.icons
            .borrow_mut()
            .push((img.downgrade(), name, size, tone));
        img
    }

    fn recolor_icons(&self) {
        let list: Vec<_> = self.icons.borrow().clone();
        let mut keep = Vec::new();
        for (w, name, size, tone) in list {
            if let Some(img) = w.upgrade() {
                if let Some(t) = self.render_icon(name, size, tone) {
                    img.set_paintable(Some(&t));
                }
                keep.push((w, name, size, tone));
            }
        }
        *self.icons.borrow_mut() = keep;
    }

    pub fn register(
        &self,
        id: &'static str,
        row: &impl IsA<gtk::Widget>,
        focus: &impl IsA<gtk::Widget>,
    ) {
        self.rows.borrow_mut().insert(
            id,
            Registered {
                row: row.clone().upcast(),
                focus: focus.clone().upcast(),
            },
        );
    }

    fn watch_config(self: &Rc<Self>) {
        let Some(dir) = self
            .store
            .borrow()
            .main_path()
            .parent()
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let mon = match gio::File::for_path(&dir)
            .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "glaze-settings: not watching {} ({e}); external changes show after a restart",
                    dir.display()
                );
                return;
            }
        };
        let weak = Rc::downgrade(self);
        mon.connect_changed(move |_, f, _, _| {
            if f.path().and_then(|p| p.extension().map(|e| e == "toml")) != Some(true) {
                return;
            }
            let Some(a) = weak.upgrade() else { return };
            if let Some(id) = a.monitor_timer.borrow_mut().take() {
                id.remove();
            }
            let a2 = a.clone();
            let id = glib::timeout_add_local_once(Duration::from_millis(250), move || {
                a2.monitor_timer.borrow_mut().take();
                // Our own writes update the store's stamps, so only changes
                // made by someone else get here as `true`.
                let changed = a2.store.borrow_mut().refresh_if_changed();
                if changed {
                    a2.apply_style();
                    a2.rebuild(None);
                    a2.sync_system_scheme();
                }
            });
            *a.monitor_timer.borrow_mut() = Some(id);
        });
        *self.monitor.borrow_mut() = Some(mon);
    }

    pub fn register_scroller(&self, page: Page, s: &gtk::ScrolledWindow) {
        self.scrollers.borrow_mut().insert(page, s.clone());
    }

    fn build_page(self: &Rc<Self>, p: Page) -> gtk::Widget {
        let w = pages::build(self, p);
        self.page_built.borrow_mut().insert(p, w.clone());
        w
    }

    /// Rebuild every page from the store (after undo, reset or import) and
    /// put focus back on `refocus` when given.
    pub fn rebuild(self: &Rc<Self>, refocus: Option<&'static str>) {
        self.building.set(true);
        let visible = self.current.get();
        let scroll = self
            .scrollers
            .borrow()
            .get(&visible)
            .map(|s| s.vadjustment().value())
            .unwrap_or(0.0);
        self.rows.borrow_mut().clear();
        self.scrollers.borrow_mut().clear();
        self.on_change.borrow_mut().clear();
        let old: Vec<(Page, gtk::Widget)> = self
            .page_built
            .borrow()
            .iter()
            .map(|(p, w)| (*p, w.clone()))
            .collect();
        let tr = self.stack.transition_duration();
        self.stack.set_transition_duration(0);
        for (p, w) in old {
            let nw = self.build_page(p);
            self.stack.remove(&w);
            self.stack.add_named(&nw, Some(p.slug()));
        }
        if self.search.text().is_empty() {
            self.stack.set_visible_child_name(visible.slug());
        } else {
            self.stack.set_visible_child_name("search");
        }
        self.stack.set_transition_duration(tr);
        self.building.set(false);
        let a = self.clone();
        glib::idle_add_local_once(move || {
            if let Some(s) = a.scrollers.borrow().get(&visible) {
                s.vadjustment().set_value(scroll);
            }
            if let Some(id) = refocus {
                if let Some(r) = a.rows.borrow().get(id) {
                    r.focus.grab_focus();
                }
            }
        });
        self.refresh_header();
        self.refresh_banner();
    }

    /// Build page `p` if it has not been built yet (replacing its placeholder).
    pub fn ensure_page(self: &Rc<Self>, p: Page) {
        if self.page_built.borrow().contains_key(&p) {
            return;
        }
        let tr = self.stack.transition_duration();
        let was_building = self.building.replace(true);
        let t0 = std::time::Instant::now();
        let w = self.build_page(p);
        if std::env::var_os("GLAZE_SETTINGS_TRACE").is_some() {
            eprintln!(
                "[glaze-settings] built page {} in {} ms",
                p.slug(),
                t0.elapsed().as_millis()
            );
        }
        if let Some(old) = self.stack.child_by_name(p.slug()) {
            let visible = self.stack.visible_child_name();
            self.stack.set_transition_duration(0);
            self.stack.remove(&old);
            self.stack.add_named(&w, Some(p.slug()));
            if let Some(v) = visible {
                self.stack.set_visible_child_name(&v);
            }
            self.stack.set_transition_duration(tr);
        } else {
            self.stack.add_named(&w, Some(p.slug()));
        }
        self.building.set(was_building);
    }

    pub fn show_page(self: &Rc<Self>, p: Page) {
        self.ensure_page(p);
        self.current.set(p);
        self.stack.set_visible_child_name(p.slug());
        self.building.set(true);
        if let Some((_, row, _)) = self.nav_rows.borrow().iter().find(|(q, _, _)| *q == p) {
            self.nav.select_row(Some(row));
        }
        self.building.set(false);
        self.refresh_header();
    }

    fn focus_first_control(self: &Rc<Self>) {
        let p = self.current.get();
        let first = catalog::all()
            .into_iter()
            .find(|e| e.page == p)
            .map(|e| e.id);
        if let Some(id) = first {
            if let Some(r) = self.rows.borrow().get(id) {
                r.focus.grab_focus();
            }
        }
    }

    fn run_search(self: &Rc<Self>, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            self.results_ids.borrow_mut().clear();
            self.update_nav_counts(&[]);
            self.show_page(self.current.get());
            return;
        }
        let all = catalog::all();
        let hits = search::search(&all, q);
        while let Some(c) = self.results.first_child() {
            self.results.remove(&c);
        }
        let mut ids = Vec::new();
        let mut per_page: Vec<Page> = Vec::new();
        for h in &hits {
            let e = &all[h.index];
            ids.push(e.id);
            per_page.push(e.page);
            let row = gtk::ListBoxRow::new();
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            b.append(&self.icon(e.page.icon(), 18, IconTone::Secondary));
            let v = gtk::Box::new(gtk::Orientation::Vertical, 1);
            let t = gtk::Label::new(Some(e.title));
            t.set_xalign(0.0);
            t.add_css_class("fs-row-title");
            let c = gtk::Label::new(Some(&format!("{} › {}", e.page.title(), e.group)));
            c.set_xalign(0.0);
            c.add_css_class("fs-result-crumb");
            v.append(&t);
            v.append(&c);
            b.append(&v);
            row.set_child(Some(&b));
            row.update_property(&[gtk::accessible::Property::Label(&format!(
                "{}, {} › {}",
                e.title,
                e.page.title(),
                e.group
            ))]);
            self.results.append(&row);
        }
        if hits.is_empty() {
            self.results_title
                .set_text(&format!("No settings match “{q}”"));
            let row = gtk::ListBoxRow::new();
            row.set_activatable(false);
            row.set_selectable(false);
            let l = gtk::Label::new(Some(
                "Try a shorter word, or words like colour, clock, tiling, wallpaper, shortcut.",
            ));
            l.add_css_class("fs-row-sub");
            l.set_xalign(0.0);
            row.set_child(Some(&l));
            self.results.append(&row);
        } else {
            let n = hits.len();
            self.results_title.set_text(&format!(
                "{n} setting{} match “{q}”",
                if n == 1 { "" } else { "s" }
            ));
        }
        *self.results_ids.borrow_mut() = ids;
        self.update_nav_counts(&per_page);
        self.stack.set_visible_child_name("search");
    }

    fn update_nav_counts(&self, hits: &[Page]) {
        for (p, row, count) in self.nav_rows.borrow().iter() {
            let n = hits.iter().filter(|h| *h == p).count();
            if hits.is_empty() {
                count.set_visible(false);
                row.remove_css_class("fs-dim");
            } else {
                count.set_text(&n.to_string());
                count.set_visible(n > 0);
                if n == 0 {
                    row.add_css_class("fs-dim");
                } else {
                    row.remove_css_class("fs-dim");
                }
            }
        }
    }

    /// Open the page of a setting, scroll its row into view, flash it and
    /// focus its control.
    pub fn jump(self: &Rc<Self>, id: &'static str) {
        let Some(e) = catalog::find(id) else { return };
        self.show_page(e.page);
        let a = self.clone();
        // Let the page lay out first.
        glib::timeout_add_local_once(Duration::from_millis(60), move || {
            let reg = a.rows.borrow().get(id).cloned();
            let Some(reg) = reg else { return };
            if let Some(s) = a.scrollers.borrow().get(&e.page) {
                if let Some(child) = s.child() {
                    if let Some(pt) = reg
                        .row
                        .compute_point(&child, &gtk::graphene::Point::new(0.0, 0.0))
                    {
                        let adj = s.vadjustment();
                        let target = (pt.y() as f64 - 24.0)
                            .clamp(0.0, (adj.upper() - adj.page_size()).max(0.0));
                        widgets::animate_adjustment(&adj, target, a.motion_ms(260));
                    }
                }
            }
            reg.row.add_css_class("fs-flash");
            reg.focus.grab_focus();
            let row = reg.row.clone();
            glib::timeout_add_local_once(Duration::from_millis(1100), move || {
                row.remove_css_class("fs-flash")
            });
        });
    }

    // ---- change pipeline ----------------------------------------------------

    /// Write one value; on success refresh the preview and show a toast.
    pub fn set(
        self: &Rc<Self>,
        doc: Doc,
        path: &str,
        value: toml::Value,
        label: &str,
    ) -> Result<(), String> {
        let r = self.store.borrow_mut().set(doc, path, value, label);
        match &r {
            Ok(()) => self.after_change(label),
            Err(e) => self.toast_error(e),
        }
        r
    }

    pub fn remove(self: &Rc<Self>, doc: Doc, path: &str, label: &str) -> Result<(), String> {
        let r = self.store.borrow_mut().remove(doc, path, label);
        match &r {
            Ok(()) => self.after_change(label),
            Err(e) => self.toast_error(e),
        }
        r
    }

    pub fn apply(
        self: &Rc<Self>,
        changes: Vec<Change>,
        label: &str,
        destructive: bool,
    ) -> Result<(), String> {
        let r = self.store.borrow_mut().apply(changes, label, destructive);
        match &r {
            Ok(()) => self.after_change(label),
            Err(e) => self.toast_error(e),
        }
        r
    }

    /// Switch GTK, libadwaita, portal and Qt apps (`glaze-colour-scheme`)
    /// when Glaze's effective scheme differs from the one last applied.
    pub fn sync_system_scheme(self: &Rc<Self>) {
        let cfg = effective_for_window(&self.cfg());
        let want = cfg.appearance.scheme;
        let amoled = cfg.appearance.amoled;
        // `glaze-colour-scheme` reads `appearance.amoled` from config.toml
        // itself (GTK/foot/sway black overrides), so a flip re-runs it too.
        let amoled_changed = self.system_amoled.get().is_some_and(|a| a != amoled);
        self.system_amoled.set(Some(amoled));
        if (self.system_scheme.get() == Some(want) && !amoled_changed)
            || crate::colourscheme::tool().is_none()
        {
            return;
        }
        self.system_scheme.set(Some(want));
        let job = gio::spawn_blocking(move || crate::colourscheme::apply(want));
        let a = self.clone();
        glib::spawn_future_local(async move {
            match job.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    a.system_scheme.set(None);
                    a.toast_error(&e);
                }
                Err(_) => a.system_scheme.set(None),
            }
        });
    }

    pub fn after_change(self: &Rc<Self>, label: &str) {
        self.apply_style();
        self.sync_system_scheme();
        self.refresh_header();
        self.refresh_banner();
        let hooks = std::mem::take(&mut *self.on_change.borrow_mut());
        for h in &hooks {
            h(self);
        }
        let mut cur = self.on_change.borrow_mut();
        let added = std::mem::take(&mut *cur);
        *cur = hooks;
        cur.extend(added);
        drop(cur);
        notify_service();
        self.toast(&format!("Saved: {label}"), true);
    }

    pub fn undo(self: &Rc<Self>) {
        let r = self.store.borrow_mut().undo();
        match r {
            Ok(Some(label)) => {
                self.apply_style();
                let focus_id = self.focused_id();
                self.rebuild(focus_id);
                notify_service();
                self.toast(&format!("Undone: {label}"), false);
            }
            Ok(None) => self.toast("Nothing to undo", false),
            Err(e) => self.toast_error(&e),
        }
    }

    fn focused_id(&self) -> Option<&'static str> {
        let f = GtkWindowExt::focus(&self.window)?;
        self.rows
            .borrow()
            .iter()
            .find(|(_, r)| f == r.focus || f.is_ancestor(&r.focus))
            .map(|(id, _)| *id)
    }

    fn confirm_reset_section(self: &Rc<Self>) {
        let page = self.current.get();
        let changes = page.reset_changes();
        if changes.is_empty() {
            self.toast(&format!("{} has nothing to reset", page.title()), false);
            return;
        }
        let a = self.clone();
        let extra = if page == Page::StartMenu {
            " This also clears your pinned apps."
        } else {
            ""
        };
        widgets::confirm(
            self,
            &format!("Reset {} settings?", page.title()),
            &format!(
                "Every {} setting goes back to its default.{extra} A backup of the current files is kept, and Undo brings them back.",
                page.title().to_lowercase()
            ),
            "Reset",
            true,
            move || {
                if a.apply(changes.clone(), &format!("Reset {}", page.title()), true).is_ok() {
                    a.rebuild(None);
                }
            },
        );
    }

    pub fn refresh_header(&self) {
        let s = self.store.borrow();
        self.undo_btn.set_sensitive(s.can_undo());
        let tip = s
            .undo_label()
            .map(|l| format!("Undo “{l}” (Ctrl+Z)"))
            .unwrap_or_else(|| "Nothing to undo".into());
        self.undo_btn.set_tooltip_text(Some(&tip));
        let page = self.current.get();
        self.reset_btn
            .set_sensitive(!page.reset_changes().is_empty());
        self.reset_btn.set_tooltip_text(Some(&format!(
            "Reset every {} setting to its default",
            page.title().to_lowercase()
        )));
    }

    pub fn refresh_banner(&self) {
        let ro = self.store.borrow().read_only().map(str::to_string);
        match ro {
            Some(why) => {
                self.banner_label.set_text(&format!(
                    "config.toml cannot be read ({why}). Glaze is running on defaults and changes are not saved until the file is fixed or reset."
                ));
                self.banner.set_visible(true);
            }
            None => self.banner.set_visible(false),
        }
    }

    pub fn toast(self: &Rc<Self>, msg: &str, offer_undo: bool) {
        self.toast_label.set_text(msg);
        self.toast_undo
            .set_visible(offer_undo && self.store.borrow().can_undo());
        if let Some(c) = self.toast_revealer.child() {
            c.remove_css_class("fs-toast-error");
        }
        self.show_toast();
    }

    pub fn toast_error(self: &Rc<Self>, msg: &str) {
        self.toast_label.set_text(&format!("Not saved: {msg}"));
        self.toast_undo.set_visible(false);
        if let Some(c) = self.toast_revealer.child() {
            c.add_css_class("fs-toast-error");
        }
        self.show_toast();
    }

    fn show_toast(self: &Rc<Self>) {
        self.toast_revealer.set_reveal_child(true);
        if let Some(id) = self.toast_timer.borrow_mut().take() {
            id.remove();
        }
        let a = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(4500), move || {
            a.toast_timer.borrow_mut().take();
            a.toast_revealer.set_reveal_child(false);
        });
        *self.toast_timer.borrow_mut() = Some(id);
    }
}

/// Ask glaze-service to reload (best effort; the shell already reloads the
/// config through its file monitor).
fn notify_service() {
    let path = glaze_common::paths::control_socket();
    if !path.exists() {
        return;
    }
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let Ok(mut s) = std::os::unix::net::UnixStream::connect(&path) else {
            return;
        };
        let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
        let hello = glaze_common::ipc::Request::Hello {
            protocol: glaze_common::ipc::PROTOCOL_VERSION,
            client: "glaze-settings".into(),
        };
        let mut line = String::new();
        if let Ok(l) = hello.to_line() {
            let _ = s.write_all(l.as_bytes());
            let _ = BufReader::new(&s).read_line(&mut line);
        }
        if let Ok(l) = glaze_common::ipc::Request::Reload.to_line() {
            let _ = s.write_all(l.as_bytes());
            line.clear();
            let _ = BufReader::new(&s).read_line(&mut line);
        }
    });
}

#[allow(dead_code)]
fn _assert_gio(_: &gio::Cancellable) {}
