//! Windows: default stacking/tiling, per-workspace overrides, gaps, focus
//! behaviour and the keyboard-shortcut editor with conflict detection.
//!
//! The current schema has no keys for these, so they live in the settings
//! sidecar (`[windows]`, `[keybindings]`). Where the running session can
//! apply a value today it is applied immediately as well: the layout mode via
//! glaze-compositor's control socket (`set-window-layout`), gaps and focus
//! behaviour via `swaymsg` in a Sway session.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::catalog::Page;
use crate::keys::{self, Spec};
use crate::store::{Change, Doc};
use crate::sysinfo;
use crate::ui::widgets::{self, Card, PageBuilder};
use crate::ui::App;

fn in_sway() -> bool {
    std::env::var_os("SWAYSOCK").is_some()
}

fn compositor_socket() -> Option<std::path::PathBuf> {
    let p = std::env::var_os("GLAZE_COMPOSITOR_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| glaze_common::paths::runtime_dir().join("glaze/compositor.sock"));
    p.exists().then_some(p)
}

/// Send `set-window-layout` to glaze-compositor (best effort).
fn compositor_layout(mode: &str, workspace: Option<u32>) -> Option<String> {
    use std::io::{BufRead, BufReader, Write};
    let path = compositor_socket()?;
    let mut s = std::os::unix::net::UnixStream::connect(path).ok()?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
    let req = glaze_common::ipc::Request::SetWindowLayout {
        mode: mode.into(),
        workspace,
    };
    s.write_all(req.to_line().ok()?.as_bytes()).ok()?;
    let mut line = String::new();
    BufReader::new(&s).read_line(&mut line).ok()?;
    Some(line)
}

fn sway(args: &[&str]) -> Result<(), String> {
    if !in_sway() {
        return Ok(());
    }
    sysinfo::act("swaymsg", args)
}

fn keymap(app: &App) -> BTreeMap<&'static str, Spec> {
    let t = app.extra("keybindings").and_then(|v| v.as_table().cloned());
    keys::effective(t.as_ref())
}

pub fn build(app: &App) -> gtk::Widget {
    let page = PageBuilder::new(app, Page::Windows);
    let live = if compositor_socket().is_some() {
        "Layouts apply now through glaze-compositor."
    } else if in_sway() {
        "Gaps and focus apply now through Sway; layouts are saved for the Glaze compositor."
    } else {
        "Saved for the Glaze compositor."
    };

    let layout = Card::new("Layout", Some(live));
    let def = app
        .extra("windows.default_layout")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "stacking".into());
    layout.add(
        &widgets::choice(app, "windows.default_layout", &["Stacking", "Tiling"], if def == "tiling" { 1 } else { 0 }, Some("Stacking keeps overlapping Windows 2000-style windows. Super+Shift+T switches at any time."), |a, i| {
            let mode = ["stacking", "tiling"][i];
            a.set(Doc::Extra, "windows.default_layout", mode.into(), &format!("Default layout: {mode}"))?;
            let _ = compositor_layout(mode, None);
            Ok(())
        })
        .root,
    );
    for ws in 0..4u32 {
        let path = format!("windows.workspace_layouts.{}", ws + 1);
        let cur = app
            .extra(&path)
            .and_then(|v| v.as_str().map(str::to_string));
        let sel = match cur.as_deref() {
            Some("stacking") => 1,
            Some("tiling") => 2,
            _ => 0,
        };
        let labels = [
            "Follow default".to_string(),
            "Stacking".to_string(),
            "Tiling".to_string(),
        ];
        let (r, dd) = widgets::dropdown(
            app,
            "windows.workspace_layouts",
            &labels,
            sel,
            Some(&format!("Workspace {}", ws + 1)),
            false,
            move |a, i| {
                let path = format!("windows.workspace_layouts.{}", ws + 1);
                if i == 0 {
                    a.remove(
                        Doc::Extra,
                        &path,
                        &format!("Workspace {} layout: default", ws + 1),
                    )
                } else {
                    let mode = ["stacking", "tiling"][i - 1];
                    a.set(
                        Doc::Extra,
                        &path,
                        mode.into(),
                        &format!("Workspace {} layout: {mode}", ws + 1),
                    )?;
                    let _ = compositor_layout(mode, Some(ws));
                    Ok(())
                }
            },
        );
        if ws == 0 {
            app.register("windows.workspace_layouts", &r.root, &dd);
        }
        layout.add(&r.root);
    }
    let gaps = app
        .extra("windows.gaps")
        .and_then(|v| v.as_integer())
        .unwrap_or(8);
    layout.add(
        &widgets::spin(
            app,
            "windows.gaps",
            0.0,
            48.0,
            gaps as f64,
            Some("Space between tiled windows, in pixels."),
            |a, v| {
                a.set(
                    Doc::Extra,
                    "windows.gaps",
                    toml::Value::Integer(v),
                    "Tiling gaps",
                )?;
                sway(&["gaps", "inner", "all", "set", &v.to_string()])
            },
        )
        .0
        .root,
    );
    page.add(&layout.frame);

    let focus = Card::new("Focus", None);
    let ffm = app
        .extra("windows.focus_follows_mouse")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "no".into());
    let vals = ["no", "yes", "always"];
    focus.add(
        &widgets::choice(
            app,
            "windows.focus_follows_mouse",
            &["Click to focus", "Follow mouse", "Always follow"],
            vals.iter().position(|v| *v == ffm).unwrap_or(0),
            None,
            move |a, i| {
                a.set(
                    Doc::Extra,
                    "windows.focus_follows_mouse",
                    vals[i].into(),
                    "Focus follows mouse",
                )?;
                sway(&["focus_follows_mouse", vals[i]])
            },
        )
        .root,
    );
    let warp = app
        .extra("windows.mouse_warping")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let sw = gtk::Switch::new();
    sw.set_active(warp);
    let r = widgets::row(
        app,
        "windows.mouse_warping",
        Some("Move the pointer to a window focused from the keyboard."),
        &sw,
    );
    let a = app.clone();
    sw.connect_active_notify(move |s| {
        if a.building.get() {
            return;
        }
        if a.set(
            Doc::Extra,
            "windows.mouse_warping",
            s.is_active().into(),
            "Pointer follows focus",
        )
        .is_ok()
        {
            let _ = sway(&[
                "mouse_warping",
                if s.is_active() { "output" } else { "none" },
            ]);
        }
    });
    focus.add(&r.root);
    page.add(&focus.frame);

    // Shortcuts ------------------------------------------------------------------
    let map = keymap(app);
    let conflicts = keys::conflicts(&map);
    let note = if conflicts.is_empty() {
        "Choose Change and press the new keys. Saved for the Glaze compositor; the Sway fallback session keeps its own bindings.".to_string()
    } else {
        format!(
            "{} conflict(s): two actions share a shortcut. Change one of them.",
            conflicts.len()
        )
    };
    let kc = Card::new("Keyboard shortcuts", Some(&note));
    if !conflicts.is_empty() {
        kc.note.add_css_class("fs-error");
    }
    for act in keys::ACTIONS.iter() {
        let spec = map[act.id];
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let cap = gtk::Label::new(Some(&spec.display()));
        cap.add_css_class("fs-keycap");
        if spec == Spec::Disabled {
            cap.add_css_class("fs-disabled-key");
        }
        let change = gtk::Button::with_label("Change…");
        change.update_property(&[gtk::accessible::Property::Label(&format!(
            "Change shortcut for {}",
            act.title
        ))]);
        let is_default = keys::parse_for(act, act.default).ok() == Some(spec);
        let id = act.id;
        let entry_id = act.entry_id;
        let reset = widgets::reset_button(app, !is_default, move |a| {
            if a.remove(
                Doc::Extra,
                &format!("keybindings.{id}"),
                "Shortcut back to default",
            )
            .is_ok()
            {
                a.rebuild(Some(entry_id));
            }
        });
        bx.append(&cap);
        bx.append(&change);
        bx.append(&reset);
        let r = widgets::row(app, act.entry_id, None, &bx);
        app.register(act.entry_id, &r.root, &change);
        if let Some(c) = conflicts
            .iter()
            .find(|c| c.action == act.id || c.other == Some(act.id))
        {
            r.set_error(Some(&c.reason));
        }
        let a = app.clone();
        let act_ref = *act;
        change.connect_clicked(move |_| capture(&a, act_ref));
        kc.add(&r.root);
    }
    page.add(&kc.frame);
    page.finish()
}

/// Capture dialog: press a chord (or type it), see conflicts, save.
fn capture(app: &App, act: keys::Action) {
    let family = act.kind == keys::Kind::Family;
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let l = gtk::Label::new(Some(&format!(
        "Press the new shortcut for “{}”.{} Backspace disables it, Escape cancels.",
        act.title,
        if family {
            " Hold the modifiers and press any digit."
        } else {
            ""
        }
    )));
    l.set_wrap(true);
    l.set_max_width_chars(52);
    l.set_xalign(0.0);
    body.append(&l);
    let shown = gtk::Label::new(Some("Waiting for keys…"));
    shown.add_css_class("fs-capture");
    shown.add_css_class("fs-armed");
    shown.set_focusable(true);
    shown.update_property(&[gtk::accessible::Property::Label("Press the new shortcut")]);
    body.append(&shown);
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(if family {
        "or type, e.g. super+shift"
    } else {
        "or type, e.g. super+shift+q"
    }));
    entry.update_property(&[gtk::accessible::Property::Label("Shortcut as text")]);
    body.append(&entry);
    let msg = gtk::Label::new(None);
    msg.set_xalign(0.0);
    msg.set_wrap(true);
    msg.set_max_width_chars(52);
    body.append(&msg);
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::with_label("Save");
    save.add_css_class("fs-primary");
    save.set_sensitive(false);
    let w = widgets::dialog(
        app,
        "Change shortcut",
        &body,
        vec![cancel.clone(), save.clone()],
    );
    shown.grab_focus();
    let pending: Rc<RefCell<Option<Spec>>> = Rc::new(RefCell::new(None));
    let evaluate = {
        let (a, msg, save, pending, shown) = (
            app.clone(),
            msg.clone(),
            save.clone(),
            pending.clone(),
            shown.clone(),
        );
        Rc::new(move |r: Result<Spec, String>| {
            msg.remove_css_class("fs-error");
            msg.remove_css_class("fs-warn");
            match r {
                Ok(spec) => {
                    shown.set_text(&spec.display());
                    let clash = keys::conflicts_if(&keymap(&a), act.id, spec);
                    save.set_label("Save");
                    msg.set_text("");
                    if let Some(c) = clash.first() {
                        let other = if c.action == act.id {
                            c.other
                        } else {
                            Some(c.action)
                        };
                        match other {
                            Some(o) => {
                                msg.set_text(&format!(
                                    "{}. Saving gives it to “{}” and disables “{}”.",
                                    c.reason,
                                    act.title,
                                    keys::action(o).map(|x| x.title).unwrap_or(o)
                                ));
                                msg.add_css_class("fs-warn");
                                save.set_label("Replace");
                            }
                            None => {
                                msg.set_text(&c.reason);
                                msg.add_css_class("fs-error");
                                save.set_sensitive(false);
                                *pending.borrow_mut() = None;
                                return;
                            }
                        }
                    }
                    save.set_sensitive(true);
                    *pending.borrow_mut() = Some(spec);
                }
                Err(e) => {
                    msg.set_text(&e);
                    msg.add_css_class("fs-error");
                    save.set_sensitive(false);
                    *pending.borrow_mut() = None;
                }
            }
        })
    };
    {
        let ev = evaluate.clone();
        let wc = w.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, k, _, st| {
            if k == gdk::Key::Escape {
                wc.close();
                return glib::Propagation::Stop;
            }
            let modifier = matches!(
                k,
                gdk::Key::Shift_L
                    | gdk::Key::Shift_R
                    | gdk::Key::Control_L
                    | gdk::Key::Control_R
                    | gdk::Key::Alt_L
                    | gdk::Key::Alt_R
                    | gdk::Key::Super_L
                    | gdk::Key::Super_R
                    | gdk::Key::Meta_L
                    | gdk::Key::Meta_R
            );
            if modifier {
                return glib::Propagation::Stop;
            }
            let plain = !st.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SUPER_MASK
                    | gdk::ModifierType::SHIFT_MASK,
            );
            if k == gdk::Key::BackSpace && plain {
                ev(Ok(Spec::Disabled));
                return glib::Propagation::Stop;
            }
            if k == gdk::Key::Tab && plain {
                return glib::Propagation::Proceed;
            }
            let Some(token) = key_token(k) else {
                ev(Err("that key cannot be used in a shortcut".into()));
                return glib::Propagation::Stop;
            };
            let mut parts: Vec<String> = Vec::new();
            if st.contains(gdk::ModifierType::CONTROL_MASK) {
                parts.push("ctrl".into());
            }
            if st.contains(gdk::ModifierType::ALT_MASK) {
                parts.push("alt".into());
            }
            if st.contains(gdk::ModifierType::SUPER_MASK) {
                parts.push("super".into());
            }
            if st.contains(gdk::ModifierType::SHIFT_MASK) {
                parts.push("shift".into());
            }
            if !family {
                parts.push(token);
            }
            ev(keys::parse_for(&act, &parts.join("+")));
            glib::Propagation::Stop
        });
        shown.add_controller(key);
    }
    {
        let ev = evaluate.clone();
        entry.connect_changed(move |e| {
            if !e.text().is_empty() {
                ev(keys::parse_for(&act, &e.text()));
            }
        });
    }
    {
        let s = save.clone();
        entry.connect_activate(move |_| {
            if s.is_sensitive() {
                s.emit_clicked();
            }
        });
    }
    let wc = w.clone();
    cancel.connect_clicked(move |_| wc.close());
    let a = app.clone();
    save.connect_clicked(move |_| {
        let Some(spec) = *pending.borrow() else {
            return;
        };
        let map = keymap(&a);
        let mut ch = vec![Change::Set(
            Doc::Extra,
            format!("keybindings.{}", act.id),
            spec.to_config().into(),
        )];
        for c in keys::conflicts_if(&map, act.id, spec) {
            let other = if c.action == act.id {
                c.other
            } else {
                Some(c.action)
            };
            if let Some(o) = other {
                ch.push(Change::Set(
                    Doc::Extra,
                    format!("keybindings.{o}"),
                    "".into(),
                ));
            }
        }
        w.close();
        if a.apply(
            ch,
            &format!("Shortcut for {}: {}", act.title, spec.display()),
            false,
        )
        .is_ok()
        {
            a.rebuild(Some(act.entry_id));
        }
    });
}

fn key_token(k: gdk::Key) -> Option<String> {
    if let Some(c) = k.to_lower().to_unicode() {
        if c.is_ascii_graphic() {
            return Some(c.to_ascii_lowercase().to_string());
        }
    }
    let named = match k {
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => "tab",
        gdk::Key::Return | gdk::Key::KP_Enter => "return",
        gdk::Key::space => "space",
        gdk::Key::Left => "left",
        gdk::Key::Right => "right",
        gdk::Key::Up => "up",
        gdk::Key::Down => "down",
        gdk::Key::Home => "home",
        gdk::Key::End => "end",
        gdk::Key::Page_Up => "pageup",
        gdk::Key::Page_Down => "pagedown",
        gdk::Key::Delete => "delete",
        gdk::Key::BackSpace => "backspace",
        _ => {
            let n = k.name()?.to_string();
            let f = n.strip_prefix('F')?.parse::<u8>().ok()?;
            return (1..=12).contains(&f).then(|| format!("f{f}"));
        }
    };
    Some(named.to_string())
}
