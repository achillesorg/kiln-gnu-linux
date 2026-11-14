//! Desktop and icon context menus, as portable data.
//!
//! `GLAZE.md` §3.1 requires a desktop context menu; §6.1 requires right-click
//! menus on shell affordances. This module produces the *menu model* — a list of
//! items with stable ids, translation keys and enablement flags — so the rules
//! ("nothing destructive in the background menu", "Paste is only enabled when
//! there is something to paste") are unit-tested while the GTK view simply
//! renders the list.
//!
//! Labels are translation keys in the `menu.*` namespace used by `glaze-i18n`
//! (`glaze_i18n::tr`). The keys are defined here; the i18n catalogue owns their
//! localised strings.

use crate::icons::{DesktopIcon, IconTarget};

/// Stable item ids, for the view layer and tests.
pub mod ids {
    pub const OPEN: &str = "open";
    pub const OPEN_WITH: &str = "open_with";
    pub const RENAME: &str = "rename";
    pub const PROPERTIES: &str = "properties";
    pub const NEW_FOLDER: &str = "new_folder";
    pub const PASTE: &str = "paste";
    pub const ARRANGE_ICONS: &str = "arrange_icons";
    pub const CHANGE_WALLPAPER: &str = "change_wallpaper";
    pub const RESET_WALLPAPER: &str = "reset_wallpaper";
    pub const EMPTY_TRASH: &str = "empty_trash";
    pub const EDIT_COMMAND: &str = "edit_command";
}

/// Translation keys in the `menu.*` namespace (see `glaze-i18n`).
pub mod labels {
    pub const OPEN: &str = "menu.open";
    pub const OPEN_WITH: &str = "menu.open_with";
    pub const RENAME: &str = "menu.rename";
    pub const PROPERTIES: &str = "menu.properties";
    pub const NEW_FOLDER: &str = "menu.new_folder";
    pub const PASTE: &str = "menu.paste";
    pub const ARRANGE_ICONS: &str = "menu.arrange_icons";
    pub const CHANGE_WALLPAPER: &str = "menu.change_wallpaper";
    pub const RESET_WALLPAPER: &str = "menu.reset_wallpaper";
    pub const EMPTY_TRASH: &str = "menu.empty_trash";
    pub const EDIT_COMMAND: &str = "menu.edit_command";
}

/// One entry in a context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItem {
    /// Stable id, unique within a menu.
    pub id: &'static str,
    /// Translation key for the visible label.
    pub label_key: &'static str,
    /// Whether the item can be activated in the current context.
    pub enabled: bool,
    /// True only for genuinely destructive actions (data loss, not undoable).
    pub danger: bool,
}

/// A ready-to-render context menu.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuModel {
    /// Items top-to-bottom.
    pub items: Vec<MenuItem>,
}

impl MenuModel {
    /// Look up an item by id.
    pub fn find(&self, id: &str) -> Option<&MenuItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// True when `id` appears at least once.
    pub fn contains(&self, id: &str) -> bool {
        self.find(id).is_some()
    }

    /// The item ids in order.
    pub fn ids(&self) -> Vec<&'static str> {
        self.items.iter().map(|i| i.id).collect()
    }

    /// True when any item is destructive.
    pub fn has_danger(&self) -> bool {
        self.items.iter().any(|i| i.danger)
    }

    /// True when the menu has no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

fn enabled(id: &'static str, label_key: &'static str) -> MenuItem {
    MenuItem {
        id,
        label_key,
        enabled: true,
        danger: false,
    }
}

fn disabled(id: &'static str, label_key: &'static str) -> MenuItem {
    MenuItem {
        id,
        label_key,
        enabled: false,
        danger: false,
    }
}

fn danger(id: &'static str, label_key: &'static str) -> MenuItem {
    MenuItem {
        id,
        label_key,
        enabled: true,
        danger: true,
    }
}

/// The desktop background context menu (right-click on empty desktop).
///
/// When `target` is `Some`, the user right-clicked with that icon as the current
/// selection, so `Open` for it is offered first. `has_wallpaper` adds a
/// "Reset to default background" item only when there is a wallpaper to remove.
///
/// The background menu is deliberately non-destructive: nothing here deletes
/// user data.
pub fn desktop_menu(target: Option<&DesktopIcon>, has_wallpaper: bool) -> MenuModel {
    let mut items = Vec::with_capacity(8);
    if target.is_some() {
        items.push(enabled(ids::OPEN, labels::OPEN));
    }
    items.push(enabled(ids::NEW_FOLDER, labels::NEW_FOLDER));
    // No clipboard source exists in this model, so Paste cannot yet be honest
    // about having content; it is present but disabled.
    items.push(disabled(ids::PASTE, labels::PASTE));
    items.push(enabled(ids::ARRANGE_ICONS, labels::ARRANGE_ICONS));
    items.push(enabled(ids::CHANGE_WALLPAPER, labels::CHANGE_WALLPAPER));
    if has_wallpaper {
        items.push(enabled(ids::RESET_WALLPAPER, labels::RESET_WALLPAPER));
    }
    items.push(enabled(ids::PROPERTIES, labels::PROPERTIES));
    MenuModel { items }
}

/// The context menu for a single desktop icon.
///
/// `Open With` is only meaningful for a file (an application shortcut is
/// already the "with"), `Edit Command` only exists for user-created command
/// shortcuts, and `Empty Trash` is the one genuinely destructive action.
pub fn icon_menu(icon: &DesktopIcon) -> MenuModel {
    let mut items = Vec::with_capacity(6);
    items.push(enabled(ids::OPEN, labels::OPEN));

    if matches!(icon.target, IconTarget::Trash) {
        items.push(danger(ids::EMPTY_TRASH, labels::EMPTY_TRASH));
    } else if matches!(icon.target, IconTarget::File(_)) {
        items.push(enabled(ids::OPEN_WITH, labels::OPEN_WITH));
    } else {
        items.push(disabled(ids::OPEN_WITH, labels::OPEN_WITH));
    }

    items.push(enabled(ids::RENAME, labels::RENAME));
    if matches!(icon.target, IconTarget::Custom { .. }) {
        items.push(enabled(ids::EDIT_COMMAND, labels::EDIT_COMMAND));
    }
    items.push(enabled(ids::PROPERTIES, labels::PROPERTIES));
    MenuModel { items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn icon(target: IconTarget) -> DesktopIcon {
        DesktopIcon {
            app_id: None,
            label: "Icon".into(),
            icon: None,
            target,
        }
    }

    fn app_icon() -> DesktopIcon {
        DesktopIcon {
            app_id: Some("org.mozilla.firefox".into()),
            label: "Firefox".into(),
            icon: Some("firefox".into()),
            target: IconTarget::Application("org.mozilla.firefox".into()),
        }
    }

    fn assert_unique_ids(model: &MenuModel) {
        let mut seen = BTreeSet::new();
        for id in model.ids() {
            assert!(seen.insert(id), "duplicate menu id: {id}");
        }
    }

    #[test]
    fn plain_desktop_menu_has_no_danger_items() {
        let m = desktop_menu(None, false);
        assert!(!m.has_danger());
        assert!(m.contains(ids::NEW_FOLDER));
        assert!(m.contains(ids::CHANGE_WALLPAPER));
        assert!(m.contains(ids::ARRANGE_ICONS));
        assert!(m.contains(ids::PROPERTIES));
        // No icon is targeted, so no Open item.
        assert!(!m.contains(ids::OPEN));
        assert_unique_ids(&m);
    }

    #[test]
    fn desktop_menu_never_has_danger_items_even_with_a_target() {
        assert!(!desktop_menu(Some(&app_icon()), false).has_danger());
        assert!(!desktop_menu(Some(&app_icon()), true).has_danger());
        assert!(!desktop_menu(None, true).has_danger());
    }

    #[test]
    fn wallpaper_item_appears_only_when_a_wallpaper_exists() {
        assert!(!desktop_menu(None, false).contains(ids::RESET_WALLPAPER));
        assert!(desktop_menu(None, true).contains(ids::RESET_WALLPAPER));
    }

    #[test]
    fn paste_is_present_but_disabled() {
        let m = desktop_menu(None, false);
        let paste = m.find(ids::PASTE).unwrap();
        assert!(!paste.enabled, "Paste must be disabled without a clipboard");
    }

    #[test]
    fn desktop_menu_offers_open_when_an_icon_is_targeted() {
        let m = desktop_menu(Some(&app_icon()), false);
        assert_eq!(m.items.first().map(|i| i.id), Some(ids::OPEN));
        assert_unique_ids(&m);
    }

    #[test]
    fn application_icon_menu_marks_open_with_disabled_and_no_danger() {
        let m = icon_menu(&app_icon());
        assert!(m.contains(ids::OPEN));
        assert!(m.contains(ids::RENAME));
        assert!(m.contains(ids::PROPERTIES));
        assert!(!m.find(ids::OPEN_WITH).unwrap().enabled);
        assert!(!m.contains(ids::EDIT_COMMAND));
        assert!(!m.has_danger());
        assert_unique_ids(&m);
    }

    #[test]
    fn file_icon_menu_enables_open_with() {
        let m = icon_menu(&icon(IconTarget::File(PathBuf::from("/home/u/a.txt"))));
        assert!(m.find(ids::OPEN_WITH).unwrap().enabled);
        assert!(!m.has_danger());
        assert_unique_ids(&m);
    }

    #[test]
    fn trash_menu_has_the_only_danger_item() {
        let m = icon_menu(&icon(IconTarget::Trash));
        let empty = m.find(ids::EMPTY_TRASH).unwrap();
        assert!(empty.danger && empty.enabled);
        // Exactly one destructive item, and no Open With for the trash.
        assert_eq!(m.items.iter().filter(|i| i.danger).count(), 1);
        assert!(!m.contains(ids::OPEN_WITH));
        assert_unique_ids(&m);
    }

    #[test]
    fn custom_command_menu_offers_edit_command() {
        let m = icon_menu(&icon(IconTarget::Custom {
            cmd: vec!["htop".into()],
        }));
        assert!(m.contains(ids::EDIT_COMMAND));
        assert!(!m.has_danger());
        assert_unique_ids(&m);
    }

    #[test]
    fn home_and_application_menus_have_no_danger() {
        assert!(!icon_menu(&icon(IconTarget::Home)).has_danger());
        assert!(!icon_menu(&app_icon()).has_danger());
    }

    #[test]
    fn every_item_has_a_namespaced_label_key() {
        let menus = [
            desktop_menu(None, true),
            desktop_menu(Some(&app_icon()), false),
            icon_menu(&app_icon()),
            icon_menu(&icon(IconTarget::Trash)),
            icon_menu(&icon(IconTarget::File(PathBuf::from("/x")))),
            icon_menu(&icon(IconTarget::Custom {
                cmd: vec!["x".into()],
            })),
        ];
        for m in &menus {
            for item in &m.items {
                assert!(
                    item.label_key.starts_with("menu."),
                    "label key not namespaced: {}",
                    item.label_key
                );
            }
        }
    }
}
