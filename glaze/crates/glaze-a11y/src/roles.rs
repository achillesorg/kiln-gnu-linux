//! Accessible name/role descriptors for the shell's GTK widgets.
//!
//! `GLAZE.md` §6.5 requires "meaningful focus ring and accessible names". Every
//! interactive affordance in the panel and launcher therefore gets an
//! [`A11yNode`]: the AT-SPI/GTK role it is exposed with, a translatable name
//! key, an optional description key, and the keyboard hint the shell shows or
//! advertises. The shell (`apps/glaze-shell`) applies these with
//! `GtkAccessible::update_property`; keeping them in portable data means the
//! text and role contract is unit-tested here, on any host.

/// The accessibility role a shell affordance maps onto.
///
/// These correspond to `GtkAccessibleRole` values the shell sets on widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum A11yRole {
    /// A push button (start button, tray item, clock button).
    Button,
    /// A two-state button (running/floating task, show-desktop).
    ToggleButton,
    /// A popover / container menu (launcher, context menu).
    Menu,
    /// A single actionable menu row (pin/unpin, window action).
    MenuItem,
    /// A list container (launcher results).
    List,
    /// A single row inside a list (a launcher result).
    ListItem,
    /// A text entry used for search (launcher search box).
    SearchEntry,
    /// Non-interactive text (launcher group titles).
    Label,
    /// A progress/level indicator (volume / brightness OSD).
    ProgressBar,
    /// A non-interactive status region (tray/status area).
    Status,
}

impl A11yRole {
    /// Every role, in a stable order (used by tests for coverage).
    pub const ALL: [A11yRole; 10] = [
        A11yRole::Button,
        A11yRole::ToggleButton,
        A11yRole::Menu,
        A11yRole::MenuItem,
        A11yRole::List,
        A11yRole::ListItem,
        A11yRole::SearchEntry,
        A11yRole::Label,
        A11yRole::ProgressBar,
        A11yRole::Status,
    ];

    /// Stable lower-case slug used in reports.
    pub const fn slug(self) -> &'static str {
        match self {
            A11yRole::Button => "button",
            A11yRole::ToggleButton => "toggle-button",
            A11yRole::Menu => "menu",
            A11yRole::MenuItem => "menu-item",
            A11yRole::List => "list",
            A11yRole::ListItem => "list-item",
            A11yRole::SearchEntry => "search-box",
            A11yRole::Label => "label",
            A11yRole::ProgressBar => "progress-bar",
            A11yRole::Status => "status",
        }
    }

    /// The GTK4 `GTK_ACCESSIBLE_ROLE_*` this maps to (`apps/glaze-shell` uses it
    /// with `gtk_accessible_update_property(... GtkAccessibleProperty::Role ...)`).
    pub const fn gtk_role(self) -> &'static str {
        match self {
            A11yRole::Button => "GTK_ACCESSIBLE_ROLE_BUTTON",
            A11yRole::ToggleButton => "GTK_ACCESSIBLE_ROLE_TOGGLE_BUTTON",
            A11yRole::Menu => "GTK_ACCESSIBLE_ROLE_MENU",
            A11yRole::MenuItem => "GTK_ACCESSIBLE_ROLE_MENU_ITEM",
            A11yRole::List => "GTK_ACCESSIBLE_ROLE_LIST",
            A11yRole::ListItem => "GTK_ACCESSIBLE_ROLE_LIST_ITEM",
            A11yRole::SearchEntry => "GTK_ACCESSIBLE_ROLE_SEARCH_BOX",
            A11yRole::Label => "GTK_ACCESSIBLE_ROLE_LABEL",
            A11yRole::ProgressBar => "GTK_ACCESSIBLE_ROLE_PROGRESS_BAR",
            A11yRole::Status => "GTK_ACCESSIBLE_ROLE_STATUS",
        }
    }
}

/// One accessible affordance.
///
/// `name_key` / `description_key` are translation keys resolved by the shell's
/// message catalogue. `key_hint` is the literal keyboard affordance advertised
/// to the user (screen reader label suffix, tooltip, or hint text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct A11yNode {
    pub role: A11yRole,
    pub name_key: &'static str,
    pub description_key: Option<&'static str>,
    pub key_hint: &'static str,
}

impl A11yNode {
    /// True when this node declares a non-empty name key.
    pub fn has_name(&self) -> bool {
        !self.name_key.is_empty()
    }
}

/// The shell affordances the panel and launcher expose to assistive tech.
///
/// Order follows the panel reading order from `GLAZE.md` §6.1: start button →
/// tasks → tray/status → clock → show-desktop, then the launcher/menu surfaces.
pub static SHELL_AFFORDANCES: &[A11yNode] = &[
    A11yNode {
        role: A11yRole::Button,
        name_key: "shell.start_button.name",
        description_key: Some("shell.start_button.description"),
        key_hint: "Super",
    },
    A11yNode {
        role: A11yRole::ToggleButton,
        name_key: "shell.task_button.name",
        description_key: Some("shell.task_button.description"),
        key_hint: "Enter",
    },
    A11yNode {
        role: A11yRole::Status,
        name_key: "shell.tray_area.name",
        description_key: Some("shell.tray_area.description"),
        key_hint: "Tab",
    },
    A11yNode {
        role: A11yRole::Button,
        name_key: "shell.tray_item.name",
        description_key: Some("shell.tray_item.description"),
        key_hint: "Enter",
    },
    A11yNode {
        role: A11yRole::Button,
        name_key: "shell.clock.name",
        description_key: Some("shell.clock.description"),
        key_hint: "Enter",
    },
    A11yNode {
        role: A11yRole::ToggleButton,
        name_key: "shell.show_desktop.name",
        description_key: Some("shell.show_desktop.description"),
        key_hint: "Super+D",
    },
    A11yNode {
        role: A11yRole::Menu,
        name_key: "shell.launcher.name",
        description_key: Some("shell.launcher.description"),
        key_hint: "Super",
    },
    A11yNode {
        role: A11yRole::SearchEntry,
        name_key: "shell.launcher.search.name",
        description_key: Some("shell.launcher.search.description"),
        key_hint: "Type to filter",
    },
    A11yNode {
        role: A11yRole::List,
        name_key: "shell.launcher.list.name",
        description_key: Some("shell.launcher.list.description"),
        key_hint: "Up/Down",
    },
    A11yNode {
        role: A11yRole::ListItem,
        name_key: "shell.launcher.result.name",
        description_key: Some("shell.launcher.result.description"),
        key_hint: "Enter",
    },
    A11yNode {
        role: A11yRole::Label,
        name_key: "shell.launcher.group_title.name",
        description_key: None,
        key_hint: "",
    },
    A11yNode {
        role: A11yRole::Menu,
        name_key: "shell.context_menu.name",
        description_key: Some("shell.context_menu.description"),
        key_hint: "Shift+F10",
    },
    A11yNode {
        role: A11yRole::MenuItem,
        name_key: "shell.menu_item.name",
        description_key: None,
        key_hint: "Enter",
    },
    A11yNode {
        role: A11yRole::ProgressBar,
        name_key: "shell.osd_level.name",
        description_key: Some("shell.osd_level.description"),
        key_hint: "Esc",
    },
];

/// Look up a shell affordance by its name key.
pub fn find(name_key: &str) -> Option<&'static A11yNode> {
    SHELL_AFFORDANCES.iter().find(|n| n.name_key == name_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_has_a_non_empty_name_key() {
        for node in SHELL_AFFORDANCES {
            assert!(node.has_name(), "node {node:?} has an empty name key");
            assert!(
                node.name_key.starts_with("shell."),
                "name key '{}' should be namespaced under shell.",
                node.name_key
            );
        }
    }

    #[test]
    fn name_keys_are_unique() {
        let mut keys: Vec<&str> = SHELL_AFFORDANCES.iter().map(|n| n.name_key).collect();
        let len = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), len, "name keys must be unique");
    }

    #[test]
    fn every_role_is_exercised_by_the_table() {
        for role in A11yRole::ALL {
            assert!(
                SHELL_AFFORDANCES.iter().any(|n| n.role == role),
                "role {role:?} is never used; the descriptor table is incomplete"
            );
        }
    }

    #[test]
    fn roles_are_used_consistently() {
        // Launcher rows are list items, not buttons.
        assert_eq!(
            find("shell.launcher.result.name").unwrap().role,
            A11yRole::ListItem
        );
        // The launcher's container is a list; the popover itself is a menu.
        assert_eq!(
            find("shell.launcher.list.name").unwrap().role,
            A11yRole::List
        );
        assert_eq!(find("shell.launcher.name").unwrap().role, A11yRole::Menu);
        // Search is a search entry.
        assert_eq!(
            find("shell.launcher.search.name").unwrap().role,
            A11yRole::SearchEntry
        );
        // Start button is a plain button; stateful affordances are toggles.
        assert_eq!(
            find("shell.start_button.name").unwrap().role,
            A11yRole::Button
        );
        assert_eq!(
            find("shell.task_button.name").unwrap().role,
            A11yRole::ToggleButton
        );
        assert_eq!(
            find("shell.show_desktop.name").unwrap().role,
            A11yRole::ToggleButton
        );
        // Context menu items are menu items, group titles are labels.
        assert_eq!(
            find("shell.menu_item.name").unwrap().role,
            A11yRole::MenuItem
        );
        assert_eq!(
            find("shell.launcher.group_title.name").unwrap().role,
            A11yRole::Label
        );
    }

    #[test]
    fn interactive_nodes_advertise_a_keyboard_hint() {
        // Anything focusable should tell the user how to reach it. Pure
        // structural labels (group titles) are the only exception.
        for node in SHELL_AFFORDANCES {
            if node.role == A11yRole::Label || node.role == A11yRole::Status {
                continue;
            }
            assert!(
                !node.key_hint.is_empty(),
                "interactive node {} advertises no keyboard hint",
                node.name_key
            );
        }
    }

    #[test]
    fn find_returns_none_for_unknown_keys() {
        assert!(find("shell.does_not_exist").is_none());
    }
}
