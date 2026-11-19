//! The settings catalogue: pages (Control-Panel-like categories) and every
//! searchable setting with its page, group, title and search keywords.
//!
//! The UI builds one row per entry and registers the row widget under the
//! entry `id`, so the search box can list matches from every page and jump to
//! the exact row. `id`s that start with a `glaze-common` section name
//! (`panel.`, `launcher.`, `appearance.`, `desktop.`, `motion.`) are keys of
//! `config.toml`; the rest are pseudo-ids for sidecar keys, live system
//! controls or read-only information.

use crate::keys;
use crate::store::{Change, Doc};

/// Sidebar groups (Windows 2000 Control Panel structure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Personalise,
    Desktop,
    System,
}

impl Category {
    pub const ALL: [Category; 3] = [Category::Personalise, Category::Desktop, Category::System];

    pub const fn title(self) -> &'static str {
        match self {
            Category::Personalise => "Personalise",
            Category::Desktop => "Desktop",
            Category::System => "System",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Appearance,
    Wallpaper,
    Animation,
    Taskbar,
    StartMenu,
    Windows,
    Displays,
    Sound,
    Network,
    Power,
    DefaultApps,
    About,
}

impl Page {
    pub const ALL: [Page; 12] = [
        Page::Appearance,
        Page::Wallpaper,
        Page::Animation,
        Page::Taskbar,
        Page::StartMenu,
        Page::Windows,
        Page::Displays,
        Page::Sound,
        Page::Network,
        Page::Power,
        Page::DefaultApps,
        Page::About,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Page::Appearance => "appearance",
            Page::Wallpaper => "wallpaper",
            Page::Animation => "animation",
            Page::Taskbar => "taskbar",
            Page::StartMenu => "start-menu",
            Page::Windows => "windows",
            Page::Displays => "displays",
            Page::Sound => "sound",
            Page::Network => "network",
            Page::Power => "power",
            Page::DefaultApps => "default-apps",
            Page::About => "about",
        }
    }

    pub fn from_slug(s: &str) -> Option<Page> {
        let s = s.trim().to_ascii_lowercase().replace('_', "-");
        Page::ALL.into_iter().find(|p| p.slug() == s)
    }

    pub const fn title(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Wallpaper => "Wallpaper",
            Page::Animation => "Animation",
            Page::Taskbar => "Taskbar",
            Page::StartMenu => "Start menu",
            Page::Windows => "Windows",
            Page::Displays => "Displays",
            Page::Sound => "Sound",
            Page::Network => "Network",
            Page::Power => "Power",
            Page::DefaultApps => "Default apps",
            Page::About => "About",
        }
    }

    pub const fn blurb(self) -> &'static str {
        match self {
            Page::Appearance => "Theme, colour, shape, paper and type for every Glaze surface.",
            Page::Wallpaper => "The picture on the desktop, per scheme and per monitor.",
            Page::Animation => "How quickly and how smoothly things move.",
            Page::Taskbar => "Edge, size, buttons, sections, tray and clock.",
            Page::StartMenu => "Menu style, size and the apps pinned to it.",
            Page::Windows => "Stacking or tiling, gaps, focus and keyboard shortcuts.",
            Page::Displays => "Resolution, scale, rotation and arrangement of outputs.",
            Page::Sound => "Output volume and mute.",
            Page::Network => "Connections and addresses.",
            Page::Power => "Battery, power supply and energy-saving options.",
            Page::DefaultApps => "Which application opens links, folders and files.",
            Page::About => "Your system, Glaze, this computer and licences.",
        }
    }

    /// Ink glyph name (see `assets.rs`).
    pub const fn icon(self) -> &'static str {
        match self {
            Page::Appearance => "appearance",
            Page::Wallpaper => "wallpaper",
            Page::Animation => "animation",
            Page::Taskbar => "taskbar",
            Page::StartMenu => "start",
            Page::Windows => "layout-tile",
            Page::Displays => "display",
            Page::Sound => "volume-high",
            Page::Network => "network-wired",
            Page::Power => "power",
            Page::DefaultApps => "apps",
            Page::About => "about",
        }
    }

    pub const fn category(self) -> Category {
        match self {
            Page::Appearance | Page::Wallpaper | Page::Animation => Category::Personalise,
            Page::Taskbar | Page::StartMenu | Page::Windows => Category::Desktop,
            _ => Category::System,
        }
    }

    /// The changes "Reset section" applies (empty: nothing to reset).
    pub fn reset_changes(self) -> Vec<Change> {
        let m = |p: &str| Change::Remove(Doc::Main, p.to_string());
        let x = |p: &str| Change::Remove(Doc::Extra, p.to_string());
        match self {
            Page::Taskbar => vec![m("panel")],
            Page::StartMenu => vec![m("launcher")],
            Page::Appearance => [
                "appearance.theme",
                "appearance.scheme",
                "appearance.amoled",
                "appearance.theme_file",
                "appearance.accent",
                "appearance.radius",
                "appearance.stroke_weight",
                "appearance.shadow_strength",
                "appearance.icon_style",
                "appearance.texture",
                "appearance.texture_intensity",
                "appearance.reduced_effects",
                "appearance.overrides",
                "appearance.scale",
                "appearance.font_scale",
            ]
            .into_iter()
            .map(m)
            .chain([x("appearance")])
            .collect(),
            Page::Wallpaper => vec![
                m("desktop.wallpaper"),
                m("desktop.wallpaper_set"),
                m("desktop.wallpaper_dim"),
                m("desktop.show_icons"),
                x("wallpaper"),
            ],
            Page::Animation => vec![m("motion"), m("appearance.reduced_motion")],
            Page::Windows => vec![x("windows"), x("keybindings")],
            Page::DefaultApps => vec![x("default_apps")],
            _ => Vec::new(),
        }
    }
}

/// How a setting takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Written to `config.toml`; the running shell reloads it (inotify).
    Shell,
    /// Applied to the running session right away (swaymsg, wpctl, ...).
    Immediate,
    /// Saved to the settings sidecar; no Glaze component reads it yet.
    Pending,
    /// Read-only information.
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: &'static str,
    pub page: Page,
    pub group: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub effect: Effect,
}

const fn e(
    id: &'static str,
    page: Page,
    group: &'static str,
    title: &'static str,
    keywords: &'static str,
    effect: Effect,
) -> Entry {
    Entry {
        id,
        page,
        group,
        title,
        keywords,
        effect,
    }
}

use Effect::{Immediate as Imm, Info, Pending as Pend, Shell};
use Page::*;

/// Every static setting, in page order.
pub const STATIC: &[Entry] = &[
    // Appearance
    e(
        "appearance.theme",
        Appearance,
        "Theme",
        "Theme family",
        "clay classic windows 2000 style look",
        Shell,
    ),
    e(
        "appearance.scheme",
        Appearance,
        "Theme",
        "Colour scheme",
        "light dark automatic auto night mode color",
        Shell,
    ),
    e(
        "appearance.theme_file",
        Appearance,
        "Theme",
        "Theme file",
        "import export shared custom toml",
        Shell,
    ),
    e(
        "appearance.amoled",
        Appearance,
        "Theme",
        "AMOLED black",
        "amoled oled true black pure dark battery contrast night",
        Shell,
    ),
    e(
        "appearance.accent",
        Appearance,
        "Colour",
        "Accent colour",
        "color terracotta highlight contrast picker",
        Shell,
    ),
    e(
        "appearance.radius",
        Appearance,
        "Shape",
        "Corner radius",
        "rounded corners square roundness",
        Shell,
    ),
    e(
        "appearance.stroke_weight",
        Appearance,
        "Shape",
        "Stroke weight",
        "border line hairline outline thickness",
        Shell,
    ),
    e(
        "appearance.shadow_strength",
        Appearance,
        "Shape",
        "Shadow strength",
        "shadows elevation depth offset",
        Shell,
    ),
    e(
        "appearance.texture",
        Appearance,
        "Paper",
        "Paper texture",
        "grain fibre fiber laid none surface",
        Shell,
    ),
    e(
        "appearance.texture_intensity",
        Appearance,
        "Paper",
        "Texture intensity",
        "strength amount grain",
        Shell,
    ),
    e(
        "appearance.reduced_effects",
        Appearance,
        "Paper",
        "Reduced effects",
        "performance flat no shadows no texture battery",
        Shell,
    ),
    e(
        "appearance.icon_style",
        Appearance,
        "Icons",
        "Icon style",
        "ink system theme glyphs",
        Shell,
    ),
    e(
        "fonts.ui",
        Appearance,
        "Fonts",
        "Interface font",
        "typeface family sans text font",
        Shell,
    ),
    e(
        "fonts.display",
        Appearance,
        "Fonts",
        "Heading font",
        "serif display clock title typeface",
        Shell,
    ),
    e(
        "fonts.size",
        Appearance,
        "Fonts",
        "Text size",
        "font size points bigger smaller",
        Shell,
    ),
    e(
        "appearance.scale",
        Appearance,
        "Fonts",
        "Interface scale",
        "hidpi zoom size scaling",
        Shell,
    ),
    // Wallpaper
    e(
        "desktop.wallpaper_set",
        Wallpaper,
        "Picture",
        "Wallpaper",
        "background picture image set kiln hills paper",
        Shell,
    ),
    e(
        "desktop.wallpaper",
        Wallpaper,
        "Picture",
        "Custom image",
        "own file photo browse",
        Shell,
    ),
    e(
        "desktop.wallpaper_dim",
        Wallpaper,
        "Picture",
        "Dim wallpaper",
        "darken dim brightness amoled night black",
        Shell,
    ),
    e(
        "wallpaper.variant",
        Wallpaper,
        "Picture",
        "Light or dark variant",
        "scheme follow night day",
        Pend,
    ),
    e(
        "wallpaper.fit",
        Wallpaper,
        "Picture",
        "Fit",
        "fill fit centre center tile stretch zoom",
        Pend,
    ),
    e(
        "wallpaper.monitors",
        Wallpaper,
        "Monitors",
        "Per-monitor wallpaper",
        "multi monitor output second screen display",
        Pend,
    ),
    e(
        "desktop.show_icons",
        Wallpaper,
        "Desktop",
        "Desktop icons",
        "shortcuts icons on desktop",
        Shell,
    ),
    // Animation
    e(
        "motion.style",
        Animation,
        "Motion",
        "Animation style",
        "smooth snappy off springs",
        Shell,
    ),
    e(
        "motion.speed",
        Animation,
        "Motion",
        "Animation speed",
        "fast slow duration quick",
        Shell,
    ),
    e(
        "motion.stop_motion",
        Animation,
        "Motion",
        "Stop-motion mode",
        "stepped frames stop motion later",
        Info,
    ),
    e(
        "appearance.reduced_motion",
        Animation,
        "Accessibility",
        "Reduce motion",
        "accessibility vestibular instant no animation",
        Shell,
    ),
    e(
        "motion.log_frame_times",
        Animation,
        "Diagnostics",
        "Log frame times",
        "performance fps frame time debug",
        Shell,
    ),
    // Taskbar
    e(
        "panel.position",
        Taskbar,
        "Layout",
        "Screen edge",
        "position top bottom left right dock edge vertical",
        Shell,
    ),
    e(
        "panel.density",
        Taskbar,
        "Layout",
        "Density",
        "compact comfortable normal size",
        Shell,
    ),
    e(
        "panel.height",
        Taskbar,
        "Layout",
        "Height",
        "thickness pixels size custom",
        Shell,
    ),
    e(
        "panel.autohide",
        Taskbar,
        "Layout",
        "Auto-hide",
        "hide slide away autohide",
        Shell,
    ),
    e(
        "panel.button_style",
        Taskbar,
        "Buttons",
        "Button style",
        "paper flat outline task buttons",
        Shell,
    ),
    e(
        "panel.labels",
        Taskbar,
        "Buttons",
        "Window labels",
        "icon only text titles label mode",
        Shell,
    ),
    e(
        "panel.grouping",
        Taskbar,
        "Buttons",
        "Grouping",
        "combine group windows count pill",
        Shell,
    ),
    e(
        "panel.default_pins",
        Taskbar,
        "Buttons",
        "Suggested pinned apps",
        "default pins file manager terminal browser",
        Shell,
    ),
    e(
        "panel.sections",
        Taskbar,
        "Sections",
        "Section order",
        "reorder arrange start pinned tasks tray clock show desktop spacer",
        Shell,
    ),
    e(
        "panel.tray_items",
        Taskbar,
        "Tray",
        "Tray items",
        "notification area network volume battery bell status icons",
        Shell,
    ),
    e(
        "panel.clock_24h",
        Taskbar,
        "Clock",
        "24-hour clock",
        "time format 12 hour am pm",
        Shell,
    ),
    e(
        "panel.show_seconds",
        Taskbar,
        "Clock",
        "Show seconds",
        "time seconds",
        Shell,
    ),
    e(
        "panel.show_date",
        Taskbar,
        "Clock",
        "Show date",
        "date day",
        Shell,
    ),
    e(
        "panel.date_format",
        Taskbar,
        "Clock",
        "Date format",
        "strftime pattern custom date",
        Shell,
    ),
    e(
        "panel.show_layout_toggle",
        Taskbar,
        "Float/tile toggle",
        "Show float/tile toggle",
        "tiling stacking floating button",
        Shell,
    ),
    e(
        "panel.layout_scope",
        Taskbar,
        "Float/tile toggle",
        "Toggle applies to",
        "workspace global all",
        Shell,
    ),
    e(
        "panel.monitors",
        Taskbar,
        "Monitors",
        "Per-monitor taskbar",
        "multi monitor output display second screen",
        Shell,
    ),
    // Start menu
    e(
        "launcher.style",
        StartMenu,
        "Style",
        "Menu style",
        "classic cascading grid compact",
        Shell,
    ),
    e(
        "launcher.show_strip",
        StartMenu,
        "Style",
        "Brand strip",
        "kiln vertical banner sidebar strip",
        Shell,
    ),
    e(
        "launcher.icon_size",
        StartMenu,
        "Style",
        "Item icon size",
        "icons size",
        Shell,
    ),
    e(
        "launcher.anchor",
        StartMenu,
        "Placement",
        "Open position",
        "centre center start button anchor",
        Shell,
    ),
    e(
        "launcher.width",
        StartMenu,
        "Placement",
        "Menu width",
        "size",
        Shell,
    ),
    e(
        "launcher.height",
        StartMenu,
        "Placement",
        "Menu height",
        "size",
        Shell,
    ),
    e(
        "launcher.pinned",
        StartMenu,
        "Pinned apps",
        "Pinned apps",
        "favourites favorites pin apps editor",
        Shell,
    ),
    e(
        "launcher.columns",
        StartMenu,
        "Pinned apps",
        "Grid columns",
        "columns pinned grid",
        Shell,
    ),
    e(
        "launcher.show_recent",
        StartMenu,
        "Content",
        "Recent apps",
        "recently used history",
        Shell,
    ),
    e(
        "launcher.search_limit",
        StartMenu,
        "Content",
        "Search results",
        "limit number results",
        Shell,
    ),
    e(
        "launcher.show_power",
        StartMenu,
        "Content",
        "Power buttons",
        "shut down restart log out lock sleep",
        Shell,
    ),
    // Windows
    e(
        "windows.default_layout",
        Windows,
        "Layout",
        "Default layout",
        "stacking tiling floating mode",
        Pend,
    ),
    e(
        "windows.workspace_layouts",
        Windows,
        "Layout",
        "Per-workspace layout",
        "workspace override tiling stacking",
        Pend,
    ),
    e(
        "windows.gaps",
        Windows,
        "Layout",
        "Tiling gaps",
        "gap spacing margin",
        Pend,
    ),
    e(
        "windows.focus_follows_mouse",
        Windows,
        "Focus",
        "Focus follows mouse",
        "hover focus pointer sloppy",
        Pend,
    ),
    e(
        "windows.mouse_warping",
        Windows,
        "Focus",
        "Pointer follows focus",
        "mouse warping cursor jump",
        Pend,
    ),
    // Displays
    e(
        "displays.resolution",
        Displays,
        "Output",
        "Resolution",
        "mode size refresh rate hz",
        Imm,
    ),
    e(
        "displays.scale",
        Displays,
        "Output",
        "Scale",
        "hidpi zoom",
        Imm,
    ),
    e(
        "displays.rotation",
        Displays,
        "Output",
        "Rotation",
        "orientation portrait landscape transform",
        Imm,
    ),
    e(
        "displays.position",
        Displays,
        "Arrangement",
        "Position",
        "arrangement layout x y left right",
        Imm,
    ),
    // Sound
    e(
        "sound.volume",
        Sound,
        "Output",
        "Volume",
        "loudness speaker audio",
        Imm,
    ),
    e("sound.mute", Sound, "Output", "Mute", "silence audio", Imm),
    // Network
    e(
        "network.status",
        Network,
        "Connections",
        "Network status",
        "internet ethernet wifi ip address connection",
        Info,
    ),
    e(
        "network.wifi",
        Network,
        "Connections",
        "Wi-Fi",
        "wireless wlan radio",
        Imm,
    ),
    // Power
    e(
        "power.battery",
        Power,
        "Power source",
        "Battery",
        "charge percent ac adapter",
        Info,
    ),
    e(
        "power.saving",
        Power,
        "Energy saving",
        "Reduced effects",
        "power saving battery performance",
        Shell,
    ),
    // Default apps
    e(
        "default_apps.browser",
        DefaultApps,
        "Applications",
        "Web browser",
        "internet http links",
        Imm,
    ),
    e(
        "default_apps.files",
        DefaultApps,
        "Applications",
        "File manager",
        "folders directory",
        Imm,
    ),
    e(
        "default_apps.terminal",
        DefaultApps,
        "Applications",
        "Terminal",
        "console shell",
        Pend,
    ),
    e(
        "default_apps.editor",
        DefaultApps,
        "Applications",
        "Text editor",
        "text plain notes",
        Imm,
    ),
    e(
        "default_apps.images",
        DefaultApps,
        "Applications",
        "Image viewer",
        "pictures photos png jpeg",
        Imm,
    ),
    e(
        "default_apps.pdf",
        DefaultApps,
        "Applications",
        "Document viewer",
        "pdf documents",
        Imm,
    ),
    e(
        "default_apps.mail",
        DefaultApps,
        "Applications",
        "Email",
        "mail mailto",
        Imm,
    ),
    // About
    e(
        "about.system",
        About,
        "This computer",
        "System information",
        "version kernel hardware cpu memory os release",
        Info,
    ),
    e(
        "about.glaze",
        About,
        "Glaze",
        "Glaze version",
        "desktop version build",
        Info,
    ),
    e(
        "branding.logo",
        About,
        "Branding",
        "Logo",
        "brand logo distro kiln glaze mark symbol icon",
        Shell,
    ),
    e(
        "branding.name",
        About,
        "Branding",
        "System name",
        "brand name distro os label",
        Shell,
    ),
    e(
        "about.licences",
        About,
        "Licences",
        "Licences",
        "license licence open source mit apache ofl",
        Info,
    ),
    e(
        "about.files",
        About,
        "Configuration",
        "Configuration files",
        "config backup path toml",
        Info,
    ),
    e(
        "about.reset_all",
        About,
        "Configuration",
        "Reset all settings",
        "factory defaults restore",
        Shell,
    ),
];

/// Every searchable entry: the static catalogue plus one per keybinding.
pub fn all() -> Vec<Entry> {
    let mut v: Vec<Entry> = STATIC.to_vec();
    for a in keys::ACTIONS {
        v.push(Entry {
            id: a.entry_id,
            page: Page::Windows,
            group: "Keyboard shortcuts",
            title: a.title,
            keywords: a.keywords,
            effect: Effect::Pending,
        });
    }
    v
}

pub fn find(id: &str) -> Option<Entry> {
    all().into_iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_pages_are_covered() {
        let all = all();
        let mut seen = HashSet::new();
        for e in &all {
            assert!(seen.insert(e.id), "duplicate id {}", e.id);
        }
        for p in Page::ALL {
            assert!(all.iter().any(|e| e.page == p), "{p:?} has no entries");
            assert_eq!(Page::from_slug(p.slug()), Some(p));
        }
        assert_eq!(
            all.iter()
                .filter(|e| e.group == "Keyboard shortcuts")
                .count(),
            20
        );
    }

    #[test]
    fn config_ids_are_real_config_keys() {
        // Every id under a glaze-common section must be a key the current
        // schema knows (serialised defaults or documented optional keys).
        let defaults = toml::Value::try_from(glaze_common::config::GlazeConfig::default()).unwrap();
        let optional = [
            "panel.height",
            "panel.date_format",
            "panel.monitors",
            "desktop.wallpaper",
            "appearance.theme_file",
            "appearance.accent",
            "appearance.radius",
            "appearance.stroke_weight",
            "appearance.shadow_strength",
        ];
        for e in STATIC {
            let section = e.id.split('.').next().unwrap();
            if ["panel", "launcher", "appearance", "desktop", "motion", "branding"].contains(&section)
                && e.effect == Effect::Shell
            {
                let t = defaults.as_table().unwrap();
                assert!(
                    crate::store::lookup(t, e.id).is_some() || optional.contains(&e.id),
                    "{} is not a config.toml key",
                    e.id
                );
            }
        }
    }

    #[test]
    fn resets_cover_their_pages() {
        assert!(!Page::Taskbar.reset_changes().is_empty());
        assert!(Page::About.reset_changes().is_empty());
        assert!(Page::Displays.reset_changes().is_empty());
    }
}
