//! Default apps: XDG `mimeapps.list` defaults (honoured by every launcher)
//! plus the Glaze terminal choice (sidecar, pending a schema key).

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::catalog::Page;
use crate::desktop_apps::{self, ROLES};
use crate::store::Doc;
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::App;

pub fn build(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::DefaultApps);
    let apps = desktop_apps::scan(&desktop_apps::app_dirs());
    let files = desktop_apps::mimeapps_files();
    let caches = desktop_apps::mimeinfo_caches();
    let user_file = files[0].clone();
    let card = Card::new(
        "Applications",
        Some(&format!(
            "Saved to {} (the XDG standard), so links and files open the same everywhere.",
            user_file.display()
        )),
    );
    for role in ROLES {
        let id: &'static str = match role.id {
            "browser" => "default_apps.browser",
            "files" => "default_apps.files",
            "editor" => "default_apps.editor",
            "images" => "default_apps.images",
            "pdf" => "default_apps.pdf",
            _ => "default_apps.mail",
        };
        let mut cands = desktop_apps::candidates(&apps, role);
        let explicit = desktop_apps::effective_default(&files, role.mimes[0]);
        let system = desktop_apps::system_default(&caches, role.mimes[0], &apps);
        let current = explicit.clone().or_else(|| system.clone());
        // A default that is installed but hidden from menus (e.g. a browser's
        // "userapp-*.desktop") is still the default: list it.
        if let Some(c) = &current {
            if !cands.iter().any(|a| a.id == *c) {
                if let Some(a) = apps.iter().find(|a| a.id == *c) {
                    cands.insert(0, a);
                }
            }
        }
        if cands.is_empty() {
            let l = gtk::Label::new(Some("No installed app"));
            l.add_css_class("fs-row-sub");
            let r = widgets::row(
                app,
                id,
                Some(&format!("Nothing installed handles {}.", role.mimes[0])),
                &l,
            );
            card.add(&r.root);
            continue;
        }
        let mut labels: Vec<String> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        if current
            .as_ref()
            .is_none_or(|c| !cands.iter().any(|a| a.id == *c))
        {
            labels.push(match &current {
                Some(c) => format!("{c} (not installed)"),
                None => "Not set".into(),
            });
            ids.push(String::new());
        }
        for a in &cands {
            let sys = explicit.is_none() && system.as_deref() == Some(a.id.as_str());
            labels.push(if sys {
                format!("{} (system choice)", a.name)
            } else {
                a.name.clone()
            });
            ids.push(a.id.clone());
        }
        let sel = current
            .as_ref()
            .and_then(|c| ids.iter().position(|i| i == c))
            .unwrap_or(0);
        let uf = user_file.clone();
        let role = *role;
        let (r, _) = widgets::dropdown(app, id, &labels, sel, None, false, move |a, i| {
            if ids[i].is_empty() {
                return Err("choose an installed app".into());
            }
            desktop_apps::set_role_default(&uf, &role, &ids[i])?;
            a.toast(&format!("{}: {}", role.title, ids[i]), false);
            Ok(())
        });
        card.add(&r.root);
    }
    // Terminal (Glaze-specific; no XDG standard).
    let terms = desktop_apps::terminals(&apps);
    let cur = app
        .extra("default_apps.terminal")
        .and_then(|v| v.as_str().map(str::to_string));
    let mut labels = vec!["Automatic (first installed)".to_string()];
    labels.extend(terms.iter().map(|t| t.name.clone()));
    let ids: Vec<String> = terms.iter().map(|t| t.id.clone()).collect();
    let sel = cur
        .and_then(|c| ids.iter().position(|i| *i == c))
        .map(|i| i + 1)
        .unwrap_or(0);
    let (r, _) = widgets::dropdown(
        app,
        "default_apps.terminal",
        &labels,
        sel,
        Some("Used by Super+Return and the taskbar's suggested pins."),
        false,
        move |a, i| {
            if i == 0 {
                a.remove(Doc::Extra, "default_apps.terminal", "Terminal: automatic")
            } else {
                a.set(
                    Doc::Extra,
                    "default_apps.terminal",
                    ids[i - 1].clone().into(),
                    "Terminal",
                )
            }
        },
    );
    card.add(&r.root);
    page.add(&card.frame);
    page.finish()
}
