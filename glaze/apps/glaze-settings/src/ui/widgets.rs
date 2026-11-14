//! Building blocks: page scaffold, paper cards, setting rows and the
//! controls bound to the store (switch, segmented choice, dropdown, slider,
//! spin button, validated entry), confirm dialogs and small animations.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::catalog::{self, Effect, Page};
use crate::store::Doc;
use crate::ui::{App, IconTone};

pub fn header_button(icon: &'static str, label: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("fs-btn");
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    if let Some(svg) = crate::assets::icon(icon) {
        // Header glyphs use the theme ink via CSS colour at render time; a
        // neutral ink keeps them legible before the first style pass.
        let _ = svg;
    }
    let l = gtk::Label::new(Some(label));
    bx.append(&l);
    b.set_child(Some(&bx));
    b.update_property(&[gtk::accessible::Property::Label(
        label.trim_end_matches('…'),
    )]);
    b
}

/// Page scaffold: scrolled column with serif title and blurb.
pub struct PageBuilder {
    pub content: gtk::Box,
    pub scroller: gtk::ScrolledWindow,
}

impl PageBuilder {
    pub fn new(app: &App, page: Page) -> PageBuilder {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
        content.add_css_class("fs-page");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        head.append(&app.icon(page.icon(), 30, IconTone::Ink));
        let tv = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let t = gtk::Label::new(Some(page.title()));
        t.add_css_class("fs-page-title");
        t.set_xalign(0.0);
        t.set_accessible_role(gtk::AccessibleRole::Heading);
        let b = gtk::Label::new(Some(page.blurb()));
        b.add_css_class("fs-page-blurb");
        b.set_xalign(0.0);
        b.set_wrap(true);
        tv.append(&t);
        tv.append(&b);
        head.append(&tv);
        content.append(&head);
        let clamp = gtk::Box::new(gtk::Orientation::Vertical, 0);
        clamp.set_halign(gtk::Align::Fill);
        clamp.set_hexpand(true);
        clamp.append(&content);
        content.set_size_request(560, -1);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&clamp));
        // Centre a column of at most 880 px: the horizontal adjustment's
        // page size is the scroller's width (also with policy Never).
        {
            let c = clamp.clone();
            scroller.hadjustment().connect_page_size_notify(move |adj| {
                let m = ((adj.page_size() as i32 - 880) / 2).max(0);
                if c.margin_start() != m {
                    c.set_margin_start(m);
                    c.set_margin_end(m);
                }
            });
        }
        app.register_scroller(page, &scroller);
        PageBuilder { content, scroller }
    }

    pub fn add(&self, w: &impl IsA<gtk::Widget>) {
        self.content.append(w);
    }

    pub fn finish(self) -> gtk::Widget {
        self.scroller.upcast()
    }
}

/// A paper card with a serif title and optional note.
pub struct Card {
    pub frame: gtk::Box,
    body: gtk::Box,
    count: Cell<usize>,
    pub note: gtk::Label,
}

impl Card {
    pub fn new(title: &str, note: Option<&str>) -> Card {
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("fs-card");
        let head = gtk::Box::new(gtk::Orientation::Vertical, 2);
        head.add_css_class("fs-card-head");
        let t = gtk::Label::new(Some(title));
        t.add_css_class("fs-card-title");
        t.set_xalign(0.0);
        t.set_accessible_role(gtk::AccessibleRole::Heading);
        head.append(&t);
        let n = gtk::Label::new(note);
        n.add_css_class("fs-card-note");
        n.set_xalign(0.0);
        n.set_wrap(true);
        n.set_visible(note.is_some());
        head.append(&n);
        frame.append(&head);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_margin_bottom(6);
        frame.append(&body);
        Card {
            frame,
            body,
            count: Cell::new(0),
            note: n,
        }
    }

    pub fn add(&self, w: &impl IsA<gtk::Widget>) {
        if self.count.get() > 0 {
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.add_css_class("fs-sep");
            self.body.append(&sep);
        }
        self.count.set(self.count.get() + 1);
        self.body.append(w);
    }
}

/// A setting row: title (from the catalogue) + subtitle on the left, control
/// on the right, inline error underneath. Registered for search jumps.
pub struct Row {
    pub root: gtk::Box,
    pub line: gtk::Box,
    pub error: gtk::Label,
    pub sub: gtk::Label,
}

impl Row {
    pub fn set_error(&self, msg: Option<&str>) {
        match msg {
            Some(m) => {
                self.error.set_text(m);
                self.error.set_visible(true);
            }
            None => self.error.set_visible(false),
        }
    }
}

pub fn row(app: &App, id: &'static str, sub: Option<&str>, control: &impl IsA<gtk::Widget>) -> Row {
    let e = catalog::find(id);
    let title_text = e.as_ref().map(|e| e.title).unwrap_or(id);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.add_css_class("fs-row");
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let left = gtk::Box::new(gtk::Orientation::Vertical, 2);
    left.set_hexpand(true);
    left.set_valign(gtk::Align::Center);
    let tl = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(title_text));
    title.add_css_class("fs-row-title");
    title.set_xalign(0.0);
    tl.append(&title);
    if let Some(e) = &e {
        match e.effect {
            Effect::Pending => {
                let b = gtk::Label::new(Some("saved only"));
                b.add_css_class("fs-badge");
                b.set_tooltip_text(Some(
                    "Saved to settings.toml. No Glaze component reads this yet (see docs/settings-needs.md).",
                ));
                tl.append(&b);
            }
            Effect::Immediate => {
                let b = gtk::Label::new(Some("applies now"));
                b.add_css_class("fs-badge");
                b.set_tooltip_text(Some("Applied to the running session immediately."));
                tl.append(&b);
            }
            _ => {}
        }
    }
    left.append(&tl);
    let s = gtk::Label::new(sub);
    s.add_css_class("fs-row-sub");
    s.set_xalign(0.0);
    s.set_wrap(true);
    s.set_visible(sub.is_some());
    left.append(&s);
    line.append(&left);
    let c = control.clone().upcast::<gtk::Widget>();
    c.set_valign(gtk::Align::Center);
    line.append(&c);
    root.append(&line);
    let error = gtk::Label::new(None);
    error.add_css_class("fs-error");
    error.set_xalign(0.0);
    error.set_wrap(true);
    error.set_visible(false);
    root.append(&error);
    c.update_relation(&[gtk::accessible::Relation::LabelledBy(&[title.upcast_ref()])]);
    if sub.is_some() {
        c.update_relation(&[gtk::accessible::Relation::DescribedBy(&[s.upcast_ref()])]);
    }
    c.update_property(&[gtk::accessible::Property::Label(title_text)]);
    app.register(id, &root, &first_focusable(&c));
    Row {
        root,
        line,
        error,
        sub: s,
    }
}

fn first_focusable(w: &gtk::Widget) -> gtk::Widget {
    if w.is_focusable() || w.is::<gtk::Button>() || w.is::<gtk::Switch>() || w.is::<gtk::Scale>() {
        return w.clone();
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        if ch.is::<gtk::Button>()
            || ch.is::<gtk::Switch>()
            || ch.is::<gtk::Entry>()
            || ch.is::<gtk::SpinButton>()
            || ch.is::<gtk::DropDown>()
            || ch.is::<gtk::Scale>()
        {
            return ch;
        }
        let inner = first_focusable(&ch);
        if inner != ch {
            return inner;
        }
        c = ch.next_sibling();
    }
    w.clone()
}

/// A plain informational row (label + value).
pub fn info_row(title: &str, value: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    b.add_css_class("fs-row");
    let t = gtk::Label::new(Some(title));
    t.add_css_class("fs-row-title");
    t.set_xalign(0.0);
    t.set_size_request(170, -1);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("fs-value");
    v.set_xalign(0.0);
    v.set_hexpand(true);
    v.set_wrap(true);
    v.set_selectable(true);
    b.append(&t);
    b.append(&v);
    b
}

// ---- bound controls --------------------------------------------------------

/// Switch bound to a boolean key.
pub fn switch(
    app: &App,
    id: &'static str,
    doc: Doc,
    path: &'static str,
    value: bool,
    sub: Option<&str>,
) -> Row {
    let sw = gtk::Switch::new();
    sw.set_active(value);
    let r = row(app, id, sub, &sw);
    let a = app.clone();
    let label = catalog::find(id).map(|e| e.title).unwrap_or(path);
    sw.connect_active_notify(move |s| {
        if a.building.get() {
            return;
        }
        if a.set(doc, path, s.is_active().into(), label).is_err() {
            a.building.set(true);
            s.set_active(!s.is_active());
            a.building.set(false);
        }
    });
    r
}

/// Segmented choice (linked toggle buttons). `on` gets the index; an `Err`
/// reverts the selection and shows the message inline.
pub fn segmented(labels: &[&str], selected: usize) -> (gtk::Box, Vec<gtk::ToggleButton>) {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    b.add_css_class("fs-segmented");
    let mut btns: Vec<gtk::ToggleButton> = Vec::new();
    for (i, l) in labels.iter().enumerate() {
        let t = gtk::ToggleButton::with_label(l);
        if let Some(f) = btns.first() {
            t.set_group(Some(f));
        }
        t.set_active(i == selected);
        b.append(&t);
        btns.push(t);
    }
    (b, btns)
}

pub fn choice<F>(
    app: &App,
    id: &'static str,
    labels: &[&str],
    selected: usize,
    sub: Option<&str>,
    on: F,
) -> Row
where
    F: Fn(&App, usize) -> Result<(), String> + 'static,
{
    let (bx, btns) = segmented(labels, selected);
    let r = row(app, id, sub, &bx);
    let cur = Rc::new(Cell::new(selected));
    let on = Rc::new(on);
    let err = r.error.clone();
    for (i, b) in btns.iter().enumerate() {
        let a = app.clone();
        let cur = cur.clone();
        let on = on.clone();
        let btns2 = btns.clone();
        let err = err.clone();
        b.connect_toggled(move |t| {
            if a.building.get() || !t.is_active() || cur.get() == i {
                return;
            }
            match on(&a, i) {
                Ok(()) => {
                    cur.set(i);
                    err.set_visible(false);
                }
                Err(e) => {
                    err.set_text(&e);
                    err.set_visible(true);
                    a.building.set(true);
                    btns2[cur.get()].set_active(true);
                    a.building.set(false);
                }
            }
        });
    }
    r
}

/// Choice of an enum value stored as a kebab-case string.
pub fn enum_choice<T>(
    app: &App,
    id: &'static str,
    doc: Doc,
    path: &'static str,
    options: &[(T, &str)],
    current: T,
    sub: Option<&str>,
) -> Row
where
    T: serde::Serialize + PartialEq + Copy + 'static,
{
    let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
    let sel = options.iter().position(|(v, _)| *v == current).unwrap_or(0);
    let values: Vec<T> = options.iter().map(|(v, _)| *v).collect();
    let label = catalog::find(id).map(|e| e.title).unwrap_or(path);
    choice(app, id, &labels, sel, sub, move |a, i| {
        a.set(doc, path, crate::store::value_of(&values[i]), label)
    })
}

/// Drop-down list.
pub fn dropdown<F>(
    app: &App,
    id: &'static str,
    labels: &[String],
    selected: usize,
    sub: Option<&str>,
    searchable: bool,
    on: F,
) -> (Row, gtk::DropDown)
where
    F: Fn(&App, usize) -> Result<(), String> + 'static,
{
    let strs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dd = gtk::DropDown::from_strings(&strs);
    dd.set_selected(selected as u32);
    if searchable {
        dd.set_enable_search(true);
        dd.set_expression(Some(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        )));
    }
    let r = row(app, id, sub, &dd);
    let cur = Rc::new(Cell::new(selected as u32));
    let a = app.clone();
    let err = r.error.clone();
    dd.connect_selected_notify(move |d| {
        if a.building.get()
            || d.selected() == cur.get()
            || d.selected() == gtk::INVALID_LIST_POSITION
        {
            return;
        }
        match on(&a, d.selected() as usize) {
            Ok(()) => {
                cur.set(d.selected());
                err.set_visible(false);
            }
            Err(e) => {
                err.set_text(&e);
                err.set_visible(true);
                a.building.set(true);
                d.set_selected(cur.get());
                a.building.set(false);
            }
        }
    });
    (r, dd)
}

/// Slider with a value label; writes are debounced (one undo step per drag).
#[allow(clippy::too_many_arguments)]
pub fn slider<F, L>(
    app: &App,
    id: &'static str,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
    sub: Option<&str>,
    fmt: L,
    on: F,
) -> (Row, gtk::Scale)
where
    F: Fn(&App, f64) -> Result<(), String> + 'static,
    L: Fn(f64) -> String + 'static,
{
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
    s.set_value(value);
    s.set_draw_value(false);
    s.set_size_request(220, -1);
    let val = gtk::Label::new(Some(&fmt(value)));
    val.add_css_class("fs-value");
    val.set_xalign(1.0);
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    b.append(&s);
    b.append(&val);
    let r = row(app, id, sub, &b);
    app.register(id, &r.root, &s);
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let on = Rc::new(on);
    let a = app.clone();
    let err = r.error.clone();
    s.connect_value_changed(move |sc| {
        let v = (sc.value() / step).round() * step;
        val.set_text(&fmt(v));
        if a.building.get() {
            return;
        }
        if let Some(t) = timer.borrow_mut().take() {
            t.remove();
        }
        let a2 = a.clone();
        let on = on.clone();
        let err = err.clone();
        let timer2 = timer.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(280), move || {
            timer2.borrow_mut().take();
            match on(&a2, (v * 1000.0).round() / 1000.0) {
                Ok(()) => err.set_visible(false),
                Err(e) => {
                    err.set_text(&e);
                    err.set_visible(true);
                }
            }
        });
        *timer.borrow_mut() = Some(id);
    });
    (r, s)
}

/// Integer spin button (debounced).
pub fn spin<F>(
    app: &App,
    id: &'static str,
    min: f64,
    max: f64,
    value: f64,
    sub: Option<&str>,
    on: F,
) -> (Row, gtk::SpinButton)
where
    F: Fn(&App, i64) -> Result<(), String> + 'static,
{
    let s = gtk::SpinButton::with_range(min, max, 1.0);
    s.set_value(value);
    s.set_numeric(true);
    let r = row(app, id, sub, &s);
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let on = Rc::new(on);
    let a = app.clone();
    let err = r.error.clone();
    s.connect_value_changed(move |sb| {
        if a.building.get() {
            return;
        }
        let v = sb.value().round() as i64;
        if let Some(t) = timer.borrow_mut().take() {
            t.remove();
        }
        let a2 = a.clone();
        let on = on.clone();
        let err = err.clone();
        let timer2 = timer.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(350), move || {
            timer2.borrow_mut().take();
            match on(&a2, v) {
                Ok(()) => err.set_visible(false),
                Err(e) => {
                    err.set_text(&e);
                    err.set_visible(true);
                }
            }
        });
        *timer.borrow_mut() = Some(id);
    });
    (r, s)
}

/// Text entry validated live; committed on Enter or focus loss.
pub fn entry<V, F>(
    app: &App,
    id: &'static str,
    text: &str,
    placeholder: &str,
    sub: Option<&str>,
    validate: V,
    on: F,
) -> (Row, gtk::Entry)
where
    V: Fn(&str) -> Result<(), String> + 'static,
    F: Fn(&App, &str) -> Result<(), String> + 'static,
{
    let e = gtk::Entry::new();
    e.set_text(text);
    e.set_placeholder_text(Some(placeholder));
    e.set_width_chars(18);
    let r = row(app, id, sub, &e);
    let validate = Rc::new(validate);
    let on = Rc::new(on);
    let last = Rc::new(RefCell::new(text.to_string()));
    {
        let err = r.error.clone();
        let v = validate.clone();
        e.connect_changed(move |e| match v(&e.text()) {
            Ok(()) => {
                e.remove_css_class("error");
                err.set_visible(false);
            }
            Err(m) => {
                e.add_css_class("error");
                err.set_text(&m);
                err.set_visible(true);
            }
        });
    }
    let commit = {
        let a = app.clone();
        let err = r.error.clone();
        let last = last.clone();
        Rc::new(move |e: &gtk::Entry| {
            if a.building.get() {
                return;
            }
            let t = e.text().to_string();
            if *last.borrow() == t {
                return;
            }
            if validate(&t).is_err() {
                return;
            }
            match on(&a, &t) {
                Ok(()) => {
                    *last.borrow_mut() = t;
                    err.set_visible(false);
                }
                Err(m) => {
                    e.add_css_class("error");
                    err.set_text(&m);
                    err.set_visible(true);
                }
            }
        })
    };
    {
        let c = commit.clone();
        e.connect_activate(move |e| c(e));
    }
    {
        let c = commit.clone();
        let fc = gtk::EventControllerFocus::new();
        let ew = e.clone();
        fc.connect_leave(move |_| c(&ew));
        e.add_controller(fc);
    }
    (r, e)
}

/// Small flat "reset to default" button for a row.
pub fn reset_button(app: &App, visible: bool, on: impl Fn(&App) + 'static) -> gtk::Button {
    let b = gtk::Button::new();
    b.set_child(Some(&app.icon("reset", 14, IconTone::Secondary)));
    b.add_css_class("fs-ghost");
    b.add_css_class("fs-icon-btn");
    b.set_tooltip_text(Some("Back to the default"));
    b.update_property(&[gtk::accessible::Property::Label("Reset to default")]);
    b.set_visible(visible);
    let a = app.clone();
    b.connect_clicked(move |_| on(&a));
    b
}

// ---- dialogs ---------------------------------------------------------------

/// A paper modal dialog with a body widget and buttons. Returns the window.
pub fn dialog(
    app: &App,
    title: &str,
    body: &impl IsA<gtk::Widget>,
    buttons: Vec<gtk::Button>,
) -> gtk::Window {
    let w = gtk::Window::builder()
        .transient_for(&app.window)
        .modal(true)
        .title(title)
        .resizable(false)
        .default_width(440)
        .build();
    w.add_css_class("fs-dialog");
    let hb = gtk::HeaderBar::new();
    hb.add_css_class("fs-headerbar");
    hb.set_show_title_buttons(false);
    let tl = gtk::Label::new(Some(title));
    tl.add_css_class("fs-dialog-title");
    hb.set_title_widget(Some(&tl));
    w.set_titlebar(Some(&hb));
    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.add_css_class("fs-dialog-card");
    card.append(body);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::End);
    for b in &buttons {
        b.add_css_class("fs-btn");
        row.append(b);
    }
    card.append(&row);
    w.set_child(Some(&card));
    let key = gtk::EventControllerKey::new();
    let wc = w.clone();
    key.connect_key_pressed(move |_, k, _, _| {
        if k == gtk::gdk::Key::Escape {
            wc.close();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    w.add_controller(key);
    w.present();
    if let Some(b) = buttons.first() {
        b.grab_focus();
    }
    w
}

/// Confirm a destructive or important action. Focus starts on Cancel.
pub fn confirm(
    app: &App,
    title: &str,
    body: &str,
    action: &str,
    destructive: bool,
    on_yes: impl Fn() + 'static,
) {
    let l = gtk::Label::new(Some(body));
    l.set_wrap(true);
    l.set_max_width_chars(52);
    l.set_xalign(0.0);
    let cancel = gtk::Button::with_label("Cancel");
    let yes = gtk::Button::with_label(action);
    yes.add_css_class("fs-primary");
    if destructive {
        yes.add_css_class("fs-danger");
    }
    let w = dialog(app, title, &l, vec![cancel.clone(), yes.clone()]);
    let wc = w.clone();
    cancel.connect_clicked(move |_| wc.close());
    let wc = w.clone();
    yes.connect_clicked(move |_| {
        wc.close();
        on_yes();
    });
}

/// Smoothly scroll an adjustment (eased, frame-clock paced; instant when
/// `ms` is 0).
pub fn animate_adjustment(adj: &gtk::Adjustment, target: f64, ms: u32) {
    let start = adj.value();
    if ms == 0 || (target - start).abs() < 1.0 {
        adj.set_value(target);
        return;
    }
    let adj = adj.clone();
    let t0 = std::time::Instant::now();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let t = (t0.elapsed().as_millis() as f64 / ms as f64).min(1.0);
        let e = 1.0 - (1.0 - t).powi(3);
        adj.set_value(start + (target - start) * e);
        if t >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Empty / unavailable state inside a card.
pub fn empty_state(app: &App, icon: &'static str, title: &str, body: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    b.add_css_class("fs-empty");
    b.append(&app.icon(icon, 36, IconTone::Secondary));
    let v = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("fs-empty-title");
    t.set_xalign(0.0);
    let d = gtk::Label::new(Some(body));
    d.add_css_class("fs-row-sub");
    d.set_xalign(0.0);
    d.set_wrap(true);
    v.append(&t);
    v.append(&d);
    b.append(&v);
    b
}

/// Reorderable checklist (taskbar sections, tray items, pinned apps):
/// drag rows, Alt+Up/Down or the arrow buttons to move, check to show.
pub struct OrderList {
    pub list: gtk::ListBox,
}

#[derive(Clone)]
pub struct OrderItem {
    pub key: String,
    pub label: String,
    pub on: bool,
    pub removable: bool,
}

/// Called with the new order after a move, add or remove.
type OrderChanged = Rc<dyn Fn(&App, &[OrderItem]) -> Result<(), String>>;

pub fn order_list(
    app: &App,
    items: Vec<OrderItem>,
    checkable: bool,
    on_change: impl Fn(&App, &[OrderItem]) -> Result<(), String> + 'static,
) -> OrderList {
    let list = gtk::ListBox::new();
    list.add_css_class("fs-plain");
    list.set_selection_mode(gtk::SelectionMode::None);
    let state = Rc::new(RefCell::new(items));
    let on_change: OrderChanged = Rc::new(on_change);
    fill(app, &list, &state, checkable, &on_change, None);
    OrderList { list }
}

type OnOrder = Rc<dyn Fn(&App, &[OrderItem]) -> Result<(), String>>;

fn fill(
    app: &App,
    list: &gtk::ListBox,
    state: &Rc<RefCell<Vec<OrderItem>>>,
    checkable: bool,
    on_change: &OnOrder,
    focus: Option<(usize, u8)>,
) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let n = state.borrow().len();
    let items = state.borrow().clone();
    for (i, it) in items.iter().enumerate() {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(false);
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        b.set_margin_start(10);
        b.set_margin_end(10);
        b.set_margin_top(3);
        b.set_margin_bottom(3);
        let grip = app.icon("grip", 16, IconTone::Secondary);
        b.append(&grip);
        let mut first: Option<gtk::Widget> = None;
        if checkable {
            let c = gtk::CheckButton::new();
            c.set_active(it.on);
            c.update_property(&[gtk::accessible::Property::Label(&format!(
                "Show {}",
                it.label
            ))]);
            let (st, l, oc, a) = (state.clone(), list.clone(), on_change.clone(), app.clone());
            c.connect_toggled(move |c| {
                let prev = st.borrow().clone();
                st.borrow_mut()[i].on = c.is_active();
                let cur = st.borrow().clone();
                if oc(&a, &cur).is_err() {
                    *st.borrow_mut() = prev;
                }
                fill(&a, &l, &st, checkable, &oc, Some((i, 0)));
            });
            first = Some(c.clone().upcast());
            b.append(&c);
        }
        let l = gtk::Label::new(Some(&it.label));
        l.add_css_class("fs-row-title");
        l.set_xalign(0.0);
        l.set_hexpand(true);
        b.append(&l);
        if checkable && !it.on {
            row.add_css_class("fs-hidden-item");
        }
        let mk = |icon: &'static str, tip: &str, sens: bool| {
            let bt = gtk::Button::new();
            bt.set_child(Some(&app.icon(icon, 14, IconTone::Ink)));
            bt.add_css_class("fs-ghost");
            bt.add_css_class("fs-icon-btn");
            bt.set_tooltip_text(Some(tip));
            bt.update_property(&[gtk::accessible::Property::Label(&format!(
                "{tip}: {}",
                it.label
            ))]);
            bt.set_sensitive(sens);
            bt
        };
        let up = mk("arrow-up", "Move up", i > 0);
        let down = mk("arrow-down", "Move down", i + 1 < n);
        b.append(&up);
        b.append(&down);
        let mover = |delta: i32| {
            let (st, l, oc, a) = (state.clone(), list.clone(), on_change.clone(), app.clone());
            move || {
                let j = i as i32 + delta;
                if j < 0 || j as usize >= st.borrow().len() {
                    return;
                }
                let prev = st.borrow().clone();
                st.borrow_mut().swap(i, j as usize);
                let cur = st.borrow().clone();
                if oc(&a, &cur).is_err() {
                    *st.borrow_mut() = prev;
                }
                fill(
                    &a,
                    &l,
                    &st,
                    checkable,
                    &oc,
                    Some((j as usize, if delta < 0 { 1 } else { 2 })),
                );
            }
        };
        let mu = mover(-1);
        up.connect_clicked(move |_| mu());
        let md = mover(1);
        down.connect_clicked(move |_| md());
        if it.removable {
            let rm = mk("close", "Remove", true);
            let (st, l, oc, a) = (state.clone(), list.clone(), on_change.clone(), app.clone());
            rm.connect_clicked(move |_| {
                let prev = st.borrow().clone();
                st.borrow_mut().remove(i);
                let cur = st.borrow().clone();
                if oc(&a, &cur).is_err() {
                    *st.borrow_mut() = prev;
                }
                fill(&a, &l, &st, checkable, &oc, None);
            });
            b.append(&rm);
        }
        // Alt+Up/Down on any control of the row moves it.
        let key = gtk::EventControllerKey::new();
        let mu = mover(-1);
        let md = mover(1);
        key.connect_key_pressed(move |_, k, _, s| {
            if s.contains(gtk::gdk::ModifierType::ALT_MASK) {
                if k == gtk::gdk::Key::Up {
                    mu();
                    return glib::Propagation::Stop;
                }
                if k == gtk::gdk::Key::Down {
                    md();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        row.add_controller(key);
        // Drag and drop.
        let src = gtk::DragSource::new();
        src.set_actions(gtk::gdk::DragAction::MOVE);
        src.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(&(i as u32).to_value()))
        });
        grip.add_controller(src);
        let drop = gtk::DropTarget::new(u32::static_type(), gtk::gdk::DragAction::MOVE);
        {
            let r2 = row.clone();
            drop.connect_enter(move |_, _, _| {
                r2.add_css_class("fs-drop-target");
                gtk::gdk::DragAction::MOVE
            });
            let r3 = row.clone();
            drop.connect_leave(move |_| r3.remove_css_class("fs-drop-target"));
        }
        let (st, l, oc, a) = (state.clone(), list.clone(), on_change.clone(), app.clone());
        drop.connect_drop(move |_, v, _, _| {
            let Ok(from) = v.get::<u32>() else {
                return false;
            };
            let from = from as usize;
            if from == i || from >= st.borrow().len() {
                return false;
            }
            let prev = st.borrow().clone();
            let it = st.borrow_mut().remove(from);
            st.borrow_mut().insert(i, it);
            let cur = st.borrow().clone();
            if oc(&a, &cur).is_err() {
                *st.borrow_mut() = prev;
            }
            let (a2, l2, st2, oc2) = (a.clone(), l.clone(), st.clone(), oc.clone());
            glib::idle_add_local_once(move || fill(&a2, &l2, &st2, checkable, &oc2, None));
            true
        });
        row.add_controller(drop);
        row.set_child(Some(&b));
        list.append(&row);
        if let Some((fi, which)) = focus {
            if fi == i {
                let target: Option<gtk::Widget> = match which {
                    1 if i > 0 => Some(up.clone().upcast()),
                    1 => Some(down.clone().upcast()),
                    2 if i + 1 < n => Some(down.clone().upcast()),
                    2 => Some(up.clone().upcast()),
                    _ => first.clone(),
                };
                if let Some(t) = target {
                    glib::idle_add_local_once(move || {
                        t.grab_focus();
                    });
                }
            }
        }
    }
}
