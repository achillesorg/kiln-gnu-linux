//! Start menu: style, brand strip, icon size, placement, size, content and
//! the pinned-apps editor. All `[launcher]` keys, reloaded live by the shell.

use gtk4 as gtk;
use gtk4::prelude::*;

use glaze_common::config::{MenuAnchor, MenuStyle};

use crate::catalog::Page;
use crate::desktop_apps::{self, App as DesktopApp};
use crate::store::Doc;
use crate::ui::widgets::{self, Card, OrderItem, PageBuilder};
use crate::ui::App;

fn int(
    a: &App,
    path: &'static str,
    label: &'static str,
) -> impl Fn(&App, i64) -> Result<(), String> {
    let _ = a;
    move |a, v| a.set(Doc::Main, path, toml::Value::Integer(v), label)
}

pub fn build(app: &App) -> gtk::Widget {
    let cfg = app.cfg();
    let l = &cfg.launcher;
    let page = PageBuilder::new(app, Page::StartMenu);

    let style = Card::new("Style", None);
    style.add(
        &widgets::enum_choice(
            app,
            "launcher.style",
            Doc::Main,
            "launcher.style",
            &[
                (MenuStyle::Classic, "Classic cascading"),
                (MenuStyle::Grid, "Compact grid"),
            ],
            l.style,
            Some("Classic keeps the Windows 2000 structure: brand strip and cascading categories."),
        )
        .root,
    );
    style.add(
        &widgets::switch(
            app,
            "launcher.show_strip",
            Doc::Main,
            "launcher.show_strip",
            l.show_strip,
            Some("The vertical Kiln strip on the left of the classic menu."),
        )
        .root,
    );
    style.add(
        &widgets::spin(
            app,
            "launcher.icon_size",
            16.0,
            32.0,
            l.icon_size as f64,
            Some("Classic menu item icons, in pixels."),
            int(app, "launcher.icon_size", "Menu icon size"),
        )
        .0
        .root,
    );
    page.add(&style.frame);

    let place = Card::new("Placement", None);
    place.add(
        &widgets::enum_choice(
            app,
            "launcher.anchor",
            Doc::Main,
            "launcher.anchor",
            &[
                (MenuAnchor::StartButton, "Next to Start"),
                (MenuAnchor::Center, "Centred"),
            ],
            l.anchor,
            None,
        )
        .root,
    );
    place.add(
        &widgets::spin(
            app,
            "launcher.width",
            320.0,
            900.0,
            l.width as f64,
            Some("320 to 900 px."),
            int(app, "launcher.width", "Menu width"),
        )
        .0
        .root,
    );
    place.add(
        &widgets::spin(
            app,
            "launcher.height",
            360.0,
            1000.0,
            l.height as f64,
            Some("360 to 1000 px."),
            int(app, "launcher.height", "Menu height"),
        )
        .0
        .root,
    );
    page.add(&place.frame);

    let content = Card::new("Content", None);
    content.add(
        &widgets::switch(
            app,
            "launcher.show_recent",
            Doc::Main,
            "launcher.show_recent",
            l.show_recent,
            Some("Recently used apps above the list when the search box is empty."),
        )
        .root,
    );
    content.add(
        &widgets::spin(
            app,
            "launcher.search_limit",
            1.0,
            50.0,
            l.search_limit as f64,
            Some("How many matches a search shows."),
            int(app, "launcher.search_limit", "Search results"),
        )
        .0
        .root,
    );
    content.add(
        &widgets::switch(
            app,
            "launcher.show_power",
            Doc::Main,
            "launcher.show_power",
            l.show_power,
            Some("Lock, sleep, restart and shut down at the bottom of the menu."),
        )
        .root,
    );
    page.add(&content.frame);

    // Pinned apps ------------------------------------------------------------
    let pins = Card::new(
        "Pinned apps",
        Some(if l.pinned.is_empty() && cfg.panel.default_pins {
            "Nothing pinned yet: the taskbar suggests a file manager, terminal and browser."
        } else {
            "Shown on the taskbar and at the top of the menu, in this order."
        }),
    );
    pins.add(
        &widgets::spin(
            app,
            "launcher.columns",
            2.0,
            8.0,
            l.columns as f64,
            Some("Columns of the pinned grid."),
            int(app, "launcher.columns", "Grid columns"),
        )
        .0
        .root,
    );
    let apps = desktop_apps::scan(&desktop_apps::app_dirs());
    let name_of = |id: &str| -> String {
        apps.iter()
            .find(|a| a.id == id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| format!("{id} (not installed)"))
    };
    let items: Vec<OrderItem> = l
        .pinned
        .iter()
        .map(|id| OrderItem {
            key: id.clone(),
            label: name_of(id),
            on: true,
            removable: true,
        })
        .collect();
    let ol = widgets::order_list(app, items, false, |a, items| {
        let v: Vec<toml::Value> = items
            .iter()
            .map(|i| toml::Value::String(i.key.clone()))
            .collect();
        a.apply(
            vec![
                crate::store::Change::Set(
                    Doc::Main,
                    "launcher.pinned".into(),
                    toml::Value::Array(v),
                ),
                crate::store::Change::Set(Doc::Main, "panel.default_pins".into(), false.into()),
            ],
            "Pinned apps",
            false,
        )
    });
    let visible: Vec<&DesktopApp> = apps
        .iter()
        .filter(|a| !a.hidden && !l.pinned.contains(&a.id))
        .collect();
    let mut labels = vec!["Choose an app to pin…".to_string()];
    labels.extend(visible.iter().map(|a| a.name.clone()));
    let ids: Vec<String> = visible.iter().map(|a| a.id.clone()).collect();
    let (r, dd) = widgets::dropdown(
        app,
        "launcher.pinned",
        &labels,
        0,
        None,
        true,
        move |a, i| {
            if i == 0 {
                return Ok(());
            }
            let mut cur = a.cfg().launcher.pinned;
            cur.push(ids[i - 1].clone());
            let v: Vec<toml::Value> = cur.into_iter().map(toml::Value::String).collect();
            a.apply(
                vec![
                    crate::store::Change::Set(
                        Doc::Main,
                        "launcher.pinned".into(),
                        toml::Value::Array(v),
                    ),
                    crate::store::Change::Set(Doc::Main, "panel.default_pins".into(), false.into()),
                ],
                "Pin app",
                false,
            )?;
            let a2 = a.clone();
            gtk::glib::idle_add_local_once(move || a2.rebuild(Some("launcher.pinned")));
            Ok(())
        },
    );
    dd.update_property(&[gtk::accessible::Property::Label("Pin an app")]);
    r.root.append(&ol.list);
    if l.pinned.is_empty() {
        let e = gtk::Label::new(Some("No pinned apps."));
        e.add_css_class("fs-row-sub");
        e.set_xalign(0.0);
        r.root.append(&e);
    }
    pins.add(&r.root);
    page.add(&pins.frame);
    page.finish()
}
