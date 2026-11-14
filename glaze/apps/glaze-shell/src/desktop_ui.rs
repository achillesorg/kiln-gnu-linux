//! Desktop surface: wallpaper, icons and a real context menu.
//!
//! Every decision — which icons exist, where they sit, what the context menu
//! contains — comes from the tested `glaze-desktop` model. This module only
//! turns that into GTK widgets and spawns the argv the model resolved, so an
//! icon that points at an application is guaranteed to exist in the registry.

use std::cell::RefCell;
use std::process::{Command, Stdio};
use std::rc::Rc;

use glaze_common::config::GlazeConfig;
use glaze_common::paths;
use glaze_desktop::context_menu::{self, MenuModel};
use glaze_desktop::icons::{self, DesktopIcon};
use glaze_desktop::shortcuts::ShortcutFile;
use glaze_i18n::tr;
use glaze_service::registry::AppRegistry;
use gtk4::prelude::*;
use gtk4::{gdk, glib};

/// Callback invoked with a context-menu item id and the icon it applies to
/// (`None` for the background menu).
type MenuAction = Rc<dyn Fn(&str, Option<DesktopIcon>)>;

/// The live desktop icon layer, once built (the menu actions need it).
type IconsHolder = Rc<RefCell<Option<Rc<crate::desktop_icons::DesktopIcons>>>>;

/// Build the desktop surface widget.
///
/// The wallpaper is the configured file when it exists; otherwise the surface
/// uses the theme's `glaze-desktop` canvas colour. Icons are `glaze-desktop`
/// entries (persisted shortcuts plus Home and Trash) positioned by its grid.
pub fn build(
    registry: Rc<AppRegistry>,
    cfg: &GlazeConfig,
    wallpaper: Option<&std::path::Path>,
    show_icons: bool,
) -> (gtk4::Widget, gtk4::Stack) {
    // The wallpaper lives in a cross-fading stack so a change in the config
    // (Settings, scheme switch) swaps it live without a restart.
    let base = gtk4::Stack::new();
    base.set_transition_type(gtk4::StackTransitionType::Crossfade);
    base.set_transition_duration(400);
    set_wallpaper(&base, wallpaper, 0);
    let overlay = gtk4::Overlay::new();
    overlay.add_css_class("glaze-desktop");
    overlay.set_child(Some(&base));

    let has_wallpaper = wallpaper.is_some();
    let _ = cfg;
    let holder: IconsHolder = Rc::new(RefCell::new(None));
    let on_action = action_handler(registry.clone(), holder.clone());
    let bg_menu: Rc<dyn Fn(f64, f64)> = {
        let (on_action, overlay2) = (on_action.clone(), overlay.clone());
        Rc::new(move |x: f64, y: f64| {
            let model = context_menu::desktop_menu(None, has_wallpaper);
            let mut entries = menu_entries(&model, None, on_action.clone());
            let (a, b) = (on_action.clone(), on_action.clone());
            entries.push(crate::menu::Entry::Separator);
            entries.push(crate::menu::Entry::item(Some("settings"), "Display settings", move || {
                (*a)("display_settings", None)
            }));
            entries.push(crate::menu::Entry::item(Some("settings"), "Personalise", move || {
                (*b)("personalise", None)
            }));
            crate::menu::popup_default(
                &overlay2,
                Some(gdk::Rectangle::new(x as i32, y as i32, 1, 1)),
                gtk4::PositionType::Bottom,
                entries,
            );
        })
    };
    if show_icons {
        let shortcuts = ShortcutFile::load_from(&ShortcutFile::default_path());
        // Home and Trash come from desktop_icons; the model adds shortcuts.
        let extra = icons::build_icons(registry.apps(), &shortcuts.shortcuts, false, false);
        let di = crate::desktop_icons::DesktopIcons::new(registry.clone(), extra, bg_menu);
        overlay.add_overlay(&di.fixed);
        overlay.add_overlay(di.marquee());
        eprintln!("[glaze-shell] desktop: icons on (Home, Trash, ~/Desktop, shortcuts)");
        *holder.borrow_mut() = Some(di);
    } else {
        // No icons: right-click on the wallpaper still opens the menu.
        let g = gtk4::GestureClick::new();
        g.set_button(3);
        g.connect_pressed(move |_, _, x, y| bg_menu(x, y));
        overlay.add_controller(g);
    }
    (overlay.upcast(), base)
}

/// Show `path` (or the theme canvas colour) in the wallpaper stack,
/// cross-fading from the current picture over `fade_ms` (0 = instant).
pub fn set_wallpaper(stack: &gtk4::Stack, path: Option<&std::path::Path>, fade_ms: u32) {
    let next: gtk4::Widget = match path {
        Some(path) if fade_ms > 0 || stack.visible_child().is_some() => {
            // Decode off the main loop (an SVG wallpaper takes ~1-2 s on
            // llvmpipe), then cross-fade once it is ready.
            let st = stack.clone();
            let path = path.to_path_buf();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(gdk::Texture::from_filename(&path).map_err(|e| e.to_string()));
            });
            glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
                match rx.try_recv() {
                    Ok(Ok(tex)) => {
                        let picture = gtk4::Picture::for_paintable(&tex);
                        picture.set_content_fit(gtk4::ContentFit::Cover);
                        picture.set_can_shrink(true);
                        show_wall(&st, picture.upcast(), fade_ms);
                        glib::ControlFlow::Break
                    }
                    Ok(Err(e)) => {
                        eprintln!("[glaze-shell] wallpaper: {e}");
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(_) => glib::ControlFlow::Break,
                }
            });
            return;
        }
        Some(path) => {
            let picture = gtk4::Picture::for_filename(path);
            picture.set_content_fit(gtk4::ContentFit::Cover);
            picture.set_can_shrink(true);
            picture.upcast()
        }
        None => {
            let blank = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            blank.add_css_class("glaze-desktop");
            blank.upcast()
        }
    };
    show_wall(stack, next, fade_ms);
}

fn show_wall(stack: &gtk4::Stack, next: gtk4::Widget, fade_ms: u32) {
    let old = stack.visible_child();
    stack.set_transition_duration(fade_ms);
    stack.add_child(&next);
    stack.set_visible_child(&next);
    if let Some(old) = old {
        let st = stack.clone();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(fade_ms as u64 + 100),
            move || {
                if old.parent().is_some() {
                    st.remove(&old);
                }
            },
        );
    }
}




/// A context-menu body built from the shared menu model.
/// Map the shared desktop menu model onto Glaze menu entries, adding the
/// `New >` and `Arrange icons >` cascades.
fn menu_entries(
    model: &MenuModel,
    icon: Option<DesktopIcon>,
    on_action: MenuAction,
) -> Vec<crate::menu::Entry> {
    use crate::menu::Entry;
    let mut out = Vec::new();
    for item in &model.items {
        let act = {
            let (id, icon, on_action) = (item.id, icon.clone(), on_action.clone());
            move || (*on_action)(id, icon.clone())
        };
        let glyph = match item.id {
            context_menu::ids::OPEN => Some("folder"),
            context_menu::ids::CHANGE_WALLPAPER => Some("settings"),
            context_menu::ids::EMPTY_TRASH => Some("window-close"),
            _ => None,
        };
        let e = match item.id {
            context_menu::ids::NEW_FOLDER => {
                let (a, b) = (on_action.clone(), on_action.clone());
                Entry::sub(
                    Some("folder"),
                    "New",
                    vec![
                        Entry::item(Some("folder"), "Folder", move || (*a)("new_folder", None)),
                        Entry::item(Some("terminal"), "Text file", move || {
                            (*b)("new_text_file", None)
                        }),
                    ],
                )
            }
            context_menu::ids::ARRANGE_ICONS => {
                let a = on_action.clone();
                let b = on_action.clone();
                Entry::sub(
                    Some("layout-tile"),
                    tr(item.label_key),
                    vec![
                        Entry::item(None, "By name", move || (*a)("arrange_by_name", None)),
                        Entry::item(None, "Line up", move || {
                            (*b)(context_menu::ids::ARRANGE_ICONS, None)
                        }),
                    ],
                )
            }
            _ => {
                let mut e = Entry::item(glyph, tr(item.label_key), act).enabled_if(item.enabled);
                if item.danger {
                    e = e.danger();
                }
                e
            }
        };
        if matches!(
            item.id,
            context_menu::ids::CHANGE_WALLPAPER | context_menu::ids::PROPERTIES
        ) && !out.is_empty()
        {
            out.push(Entry::Separator);
        }
        out.push(e);
    }
    out
}

/// What each menu id does. Anything not implemented says so plainly.
fn action_handler(registry: Rc<AppRegistry>, holder: IconsHolder) -> MenuAction {
    Rc::new(move |id: &str, icon: Option<DesktopIcon>| match id {
        context_menu::ids::OPEN => match icon {
            Some(icon) => activate(&icon, &registry),
            None => eprintln!("[glaze-shell] desktop: nothing selected to open"),
        },
        context_menu::ids::ARRANGE_ICONS | "arrange_by_name" => {
            if let Some(di) = holder.borrow().as_ref() {
                di.arrange_by_name();
            }
        }
        context_menu::ids::CHANGE_WALLPAPER => launch_settings(&registry),
        "display_settings" => {
            crate::classic_menu::run_argv(&["glaze-settings", "--page", "displays"]);
        }
        "personalise" => {
            crate::classic_menu::run_argv(&["glaze-settings", "--page", "appearance"]);
        }
        "new_folder" | "new_text_file" => create_on_desktop(id == "new_folder"),
        context_menu::ids::RESET_WALLPAPER => reset_wallpaper(),
        other => eprintln!("[glaze-shell] desktop: '{other}' is not implemented yet"),
    })
}

/// Create `~/Desktop/New folder[ n]` or `New text file[ n].txt` and say so.
fn create_on_desktop(folder: bool) {
    let dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("Desktop");
    let _ = std::fs::create_dir_all(&dir);
    let (stem, ext) = if folder {
        ("New folder", "")
    } else {
        ("New text file", ".txt")
    };
    let mut n = 1;
    let path = loop {
        let name = if n == 1 {
            format!("{stem}{ext}")
        } else {
            format!("{stem} {n}{ext}")
        };
        let p = dir.join(name);
        if !p.exists() {
            break p;
        }
        n += 1;
    };
    let r = if folder {
        std::fs::create_dir(&path)
    } else {
        std::fs::write(&path, b"")
    };
    match r {
        Ok(()) => crate::toast::show("Created", &path.display().to_string()),
        Err(e) => crate::toast::show("Could not create", &format!("{}: {e}", path.display())),
    }
}

/// Activate an icon by spawning the argv `glaze-desktop` resolved.
fn activate(icon: &DesktopIcon, registry: &AppRegistry) {
    match icons::resolve_target(icon, registry.apps()) {
        Some(argv) if !argv.is_empty() => match spawn_detached(&argv) {
            Ok(()) => eprintln!("[glaze-shell] desktop: launched '{}'", icon.label),
            Err(e) => eprintln!(
                "[glaze-shell] desktop: failed to launch '{}': {e}",
                icon.label
            ),
        },
        _ => eprintln!(
            "[glaze-shell] desktop: '{}' has no resolvable command yet",
            icon.label
        ),
    }
}

/// Spawn an argv, detached, with no shell (`GLAZE.md` §4.1, §7.9).
fn spawn_detached(argv: &[String]) -> std::io::Result<()> {
    let (program, args) = argv.split_first().expect("non-empty argv");
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Open Glaze Settings from the registry, when it is installed.
fn launch_settings(registry: &AppRegistry) {
    let candidate = registry
        .apps()
        .iter()
        .find(|a| a.id == "glaze-settings" || a.id == "org.glaze.Settings");
    match candidate {
        Some(app) => match glaze_service::launch::launch(app) {
            Ok(_child) => eprintln!("[glaze-shell] desktop: opened Glaze Settings"),
            Err(e) => eprintln!("[glaze-shell] desktop: could not open settings: {e}"),
        },
        None => eprintln!("[glaze-shell] desktop: Glaze Settings is not installed"),
    }
}

/// Clear the configured wallpaper and persist it; the running surface keeps the
/// current image until the shell restarts (says so instead of pretending).
fn reset_wallpaper() {
    let path = paths::config_file();
    let mut cfg = GlazeConfig::load_from(&path).unwrap_or_default();
    cfg.desktop.wallpaper = None;
    match cfg.save_to(&path) {
        Ok(()) => eprintln!(
            "[glaze-shell] desktop: wallpaper reset in {} (restart glaze-shell to apply)",
            path.display()
        ),
        Err(e) => eprintln!(
            "[glaze-shell] desktop: could not save {}: {e}",
            path.display()
        ),
    }
}
