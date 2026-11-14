//! Versioned user configuration with forward/backward migration.
//!
//! Config is stored as TOML under `$XDG_CONFIG_HOME/glaze/config.toml`. Every
//! field has a sensible default so an older or hand-edited file still loads.
//! The `version` field lets [`GlazeConfig::migrate`] upgrade schemas without
//! discarding a user's settings (`GLAZE.md` §5 P1: "persist via versioned
//! configuration").

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::theme::ThemeTokens;

/// Current configuration schema version.
pub const CONFIG_VERSION: u32 = 3;

pub use glaze_design::kinds::{ColorScheme, ThemeKind};

/// Where the panel is docked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PanelPosition {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

impl PanelPosition {
    pub const fn slug(self) -> &'static str {
        match self {
            PanelPosition::Top => "top",
            PanelPosition::Bottom => "bottom",
            PanelPosition::Left => "left",
            PanelPosition::Right => "right",
        }
    }

    /// True for left/right (vertical) orientations.
    pub const fn is_vertical(self) -> bool {
        matches!(self, PanelPosition::Left | PanelPosition::Right)
    }
}

/// Panel auto-hide behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Autohide {
    #[default]
    Never,
    /// Hide unless the pointer touches the screen edge / a window overlaps.
    Smart,
    Always,
}

/// Which compositor backend the shell binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CompositorBackend {
    /// Auto-detect: prefer the native Glaze compositor, fall back to Sway IPC.
    #[default]
    Auto,
    /// Talk to a running Sway over its IPC socket (first runnable milestone).
    Sway,
    /// Read state from `wlr-foreign-toplevel-management` on any wlroots compositor.
    LayerShell,
    /// Native Glaze compositor (Smithay), when implemented.
    Glaze,
    /// No window-management integration (panel only).
    None,
}

/// Glyph style for app/tray icons on the taskbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IconStyle {
    /// Glaze ink-outline glyphs where one exists, the icon theme otherwise.
    #[default]
    Ink,
    /// Always the system icon theme (.desktop `Icon=`).
    System,
}

/// How the light/dark scheme is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SchemeMode {
    /// Use `appearance.scheme` as written.
    #[default]
    Manual,
    /// Light from 07:00 to 19:00 local time, dark otherwise (re-checked
    /// every minute by the shell).
    Auto,
}

impl SchemeMode {
    /// The scheme for local hour `hour` (0..=23) given the manual choice.
    pub fn resolve(self, manual: ColorScheme, hour: u32) -> ColorScheme {
        match self {
            SchemeMode::Manual => manual,
            SchemeMode::Auto if (7..19).contains(&hour) => ColorScheme::Light,
            SchemeMode::Auto => ColorScheme::Dark,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub theme: ThemeKind,
    pub scheme: ColorScheme,
    /// `manual` (use `scheme`) or `auto` (time of day).
    pub scheme_mode: SchemeMode,
    pub reduced_motion: bool,
    /// UI scale multiplier. `1.0` means "follow the toolkit / system".
    pub scale: f64,
    /// Extra font scale on top of the theme's base size.
    pub font_scale: f64,
    /// Shareable theme file: a name looked up in `~/.config/glaze/themes/`,
    /// `~/.local/share/glaze/themes/` and `$XDG_DATA_DIRS/glaze/themes/`
    /// (`<name>.toml`), or a path. Its `[meta] base` wins over `theme`.
    pub theme_file: Option<String>,
    /// Accent colour shortcut (`"#d97757"`); text on it is derived for contrast.
    pub accent: Option<String>,
    /// Control corner radius shortcut in px (0..=16).
    pub radius: Option<i32>,
    /// Hairline stroke weight shortcut in px (0..=3).
    pub stroke_weight: Option<f64>,
    /// Shadow strength 0..=2 (0 = none, 1 = theme default).
    pub shadow_strength: Option<f64>,
    pub icon_style: IconStyle,
    /// Paper texture set id (`none`, `grain`, `fibre`, `laid`).
    pub texture: String,
    /// Texture intensity 0..=1 (0 = invisible).
    pub texture_intensity: f64,
    /// Reduced effects: no textures and no shadows (also good for slow GPUs).
    pub reduced_effects: bool,
    /// Full token overrides, same shape as a theme file (applied last).
    pub overrides: Option<toml::Table>,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        AppearanceConfig {
            theme: ThemeKind::Clay,
            scheme: ColorScheme::Light,
            scheme_mode: SchemeMode::Manual,
            reduced_motion: false,
            scale: 1.0,
            font_scale: 1.0,
            theme_file: None,
            accent: None,
            radius: None,
            stroke_weight: None,
            shadow_strength: None,
            icon_style: IconStyle::Ink,
            texture: "grain".into(),
            texture_intensity: 0.5,
            reduced_effects: false,
            overrides: None,
        }
    }
}

/// Which task buttons show their text label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LabelMode {
    #[default]
    Full,
    IconOnly,
    /// Only the active (focused) task shows its title.
    ActiveOnly,
}

/// How windows of the same app share taskbar buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GroupingMode {
    /// One button per app with a window-count pill.
    #[default]
    Always,
    /// One button per window until the taskbar runs out of room, then group.
    Auto,
    /// One button per window.
    Never,
}

/// Visual style of task buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonStyle {
    /// Paper tab with a hairline border (mockup v2).
    #[default]
    Paper,
    /// No border until hovered.
    Flat,
    /// Border only, transparent fill.
    Outline,
}

/// Typeface of the Start button word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StartLabelStyle {
    /// UI sans (Inter, then Noto Sans), medium-semibold.
    #[default]
    Sans,
    /// Display serif (the clock's face).
    Serif,
}

/// Taskbar density preset (scales height, button height and spacing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Compact,
    #[default]
    Normal,
    Comfortable,
}

impl Density {
    /// Pixel delta added to the bar and button heights.
    pub const fn delta_px(self) -> i32 {
        match self {
            Density::Compact => -6,
            Density::Normal => 0,
            Density::Comfortable => 8,
        }
    }
}

/// Taskbar sections, in the order they are drawn (`panel.sections`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelSection {
    Start,
    Pinned,
    Tasks,
    Spacer,
    LayoutToggle,
    Tray,
    Clock,
    ShowDesktop,
}

impl PanelSection {
    /// Mockup v2 order.
    pub const DEFAULT_ORDER: [PanelSection; 8] = [
        PanelSection::Start,
        PanelSection::Pinned,
        PanelSection::Tasks,
        PanelSection::Spacer,
        PanelSection::LayoutToggle,
        PanelSection::Tray,
        PanelSection::Clock,
        PanelSection::ShowDesktop,
    ];
}

/// Built-in tray widgets (`panel.tray_items`). StatusNotifier app icons are
/// shown by `status-notifier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayItem {
    StatusNotifier,
    Network,
    Volume,
    Battery,
    Notifications,
}

impl TrayItem {
    pub const DEFAULT: [TrayItem; 5] = [
        TrayItem::StatusNotifier,
        TrayItem::Network,
        TrayItem::Volume,
        TrayItem::Battery,
        TrayItem::Notifications,
    ];
}

/// Whether the float/tile toggle acts on the focused workspace or all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutScope {
    #[default]
    Workspace,
    Global,
}

/// Per-monitor taskbar overrides (`[panel.monitors."HDMI-A-1"]`). Unset keys
/// inherit from `[panel]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MonitorPanelOverride {
    /// `false` hides the taskbar on this monitor.
    pub enabled: Option<bool>,
    pub position: Option<PanelPosition>,
    pub height: Option<i32>,
    pub density: Option<Density>,
    pub labels: Option<LabelMode>,
    pub grouping: Option<GroupingMode>,
    pub autohide: Option<Autohide>,
    pub sections: Option<Vec<PanelSection>>,
    pub tray_items: Option<Vec<TrayItem>>,
}

/// Motion style (`docs/decisions.md`: smooth by default; stop-motion later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MotionStyle {
    /// Critically damped springs, no overshoot.
    #[default]
    Smooth,
    /// Shorter, stiffer springs.
    Snappy,
    /// Instant state changes.
    Off,
    /// Reserved for the later stop-motion mode; behaves as `smooth` for now.
    StopMotion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionConfig {
    pub style: MotionStyle,
    /// Speed multiplier 0.25..=4 (2 = twice as fast).
    pub speed: f64,
    /// Log animation frame times to stderr (for performance checks).
    pub log_frame_times: bool,
}

impl Default for MotionConfig {
    fn default() -> Self {
        MotionConfig {
            style: MotionStyle::Smooth,
            speed: 1.0,
            log_frame_times: true,
        }
    }
}

/// The taskbar (`[panel]`). Every field is user-settable and validated by
/// `crate::config_check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelConfig {
    /// Screen edge: top, bottom, left or right.
    pub position: PanelPosition,
    /// Override panel thickness in logical px; `None` uses the theme default.
    pub height: Option<i32>,
    pub autohide: Autohide,
    /// Legacy switch (settings app): `false` forces icon-only labels.
    pub show_labels: bool,
    pub clock_24h: bool,
    pub show_seconds: bool,
    pub show_date: bool,
    /// Clock/date format fallback when not using 12/24h toggles.
    #[serde(default)]
    pub date_format: Option<String>,
    pub density: Density,
    pub button_style: ButtonStyle,
    pub labels: LabelMode,
    pub grouping: GroupingMode,
    /// Section order; omit a section to hide it.
    pub sections: Vec<PanelSection>,
    /// Tray widgets in order; omit one to hide it.
    pub tray_items: Vec<TrayItem>,
    /// Show the float/tile segmented toggle (also hideable via `sections`).
    pub show_layout_toggle: bool,
    pub layout_scope: LayoutScope,
    /// While `launcher.pinned` is empty, pin a file manager, terminal and
    /// browser found among the installed .desktop apps.
    pub default_pins: bool,
    /// Start button word typeface (`sans` default, or `serif`).
    /// `classic-bar` (flat full-width bar) or `floating` (paper tabs).
    pub style: PanelStyle,
    /// Search button next to Start (classic-bar).
    pub show_search: bool,
    /// Task view button (all windows at a glance).
    pub show_task_view: bool,
    /// Classic bar: show window titles next to the icons (off = icons only).
    pub bar_labels: bool,
    /// Start button text next to the arch mark (default "Start"; empty =
    /// mark only).
    pub start_label: String,
    pub start_label_style: StartLabelStyle,
    /// Start button word weight, 300..=800 (default 600).
    pub start_label_weight: u16,
    /// Per-monitor overrides keyed by output connector name.
    pub monitors: std::collections::BTreeMap<String, MonitorPanelOverride>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        PanelConfig {
            position: PanelPosition::Bottom,
            height: None,
            autohide: Autohide::Never,
            show_labels: true,
            clock_24h: true,
            show_seconds: false,
            show_date: true,
            date_format: None,
            density: Density::Normal,
            button_style: ButtonStyle::Paper,
            labels: LabelMode::Full,
            grouping: GroupingMode::Always,
            sections: PanelSection::DEFAULT_ORDER.to_vec(),
            tray_items: TrayItem::DEFAULT.to_vec(),
            show_layout_toggle: true,
            layout_scope: LayoutScope::Workspace,
            default_pins: true,
            style: PanelStyle::ClassicBar,
            show_search: true,
            show_task_view: true,
            bar_labels: false,
            start_label: "Start".into(),
            start_label_style: StartLabelStyle::Sans,
            start_label_weight: 600,
            monitors: Default::default(),
        }
    }
}

impl PanelConfig {
    /// Effective label mode (the legacy `show_labels = false` wins).
    pub fn label_mode(&self) -> LabelMode {
        if self.show_labels {
            self.labels
        } else {
            LabelMode::IconOnly
        }
    }

    /// The panel configuration for one output, with its overrides applied.
    pub fn for_monitor(&self, connector: &str) -> PanelConfig {
        let mut out = self.clone();
        if let Some(o) = self.monitors.get(connector) {
            if let Some(v) = o.position {
                out.position = v;
            }
            if o.height.is_some() {
                out.height = o.height;
            }
            if let Some(v) = o.density {
                out.density = v;
            }
            if let Some(v) = o.labels {
                out.labels = v;
                out.show_labels = true;
            }
            if let Some(v) = o.grouping {
                out.grouping = v;
            }
            if let Some(v) = o.autohide {
                out.autohide = v;
            }
            if let Some(v) = &o.sections {
                out.sections = v.clone();
            }
            if let Some(v) = &o.tray_items {
                out.tray_items = v.clone();
            }
        }
        out
    }

    /// False when the taskbar is disabled on `connector`.
    pub fn enabled_on(&self, connector: &str) -> bool {
        self.monitors
            .get(connector)
            .and_then(|o| o.enabled)
            .unwrap_or(true)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    pub search_limit: usize,
    /// Show recently-used apps at the top when the search box is empty.
    pub show_recent: bool,
    /// Applications pinned to the panel / launcher favourites, by desktop id.
    pub pinned: Vec<String>,
    /// Start menu card width in px (320..=900).
    pub width: i32,
    /// Start menu card height in px (360..=1000).
    pub height: i32,
    /// Columns of the pinned-apps grid (2..=8).
    pub columns: usize,
    /// Where the start menu opens: next to the start button, or centred.
    pub anchor: MenuAnchor,
    /// Show the lock/sleep/restart/shut-down row (grid style).
    pub show_power: bool,
    /// `classic` (Windows 2000 structure: brand strip, cascading menus) or
    /// `grid` (compact search + pinned grid + app list).
    pub style: MenuStyle,
    /// Classic style: show the vertical brand strip.
    pub show_strip: bool,
    /// Classic style: menu item icon size in px (16..=32).
    pub icon_size: i32,
}

/// Start-menu style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MenuStyle {
    /// Icon rail + A-Z app list + tile groups (default).
    #[default]
    TwoPane,
    /// Windows 2000 cascading single column.
    Classic,
    Grid,
}

/// Overall taskbar look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PanelStyle {
    /// Full-width flat bar with full-height icon buttons (default).
    #[default]
    ClassicBar,
    /// The earlier look: raised paper tabs, wells and pills.
    Floating,
}

/// Start-menu placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MenuAnchor {
    #[default]
    StartButton,
    Center,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        LauncherConfig {
            search_limit: 8,
            show_recent: true,
            pinned: Vec::new(),
            width: 420,
            height: 540,
            columns: 5,
            anchor: MenuAnchor::StartButton,
            show_power: true,
            style: MenuStyle::TwoPane,
            show_strip: true,
            icon_size: 22,
        }
    }
}

/// Which shell layout to use (`Auto` decides from the display, see
/// `glaze-shell-core::form_factor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FormFactorPref {
    /// Pick Desktop or Mobile from the output size and input devices.
    #[default]
    Auto,
    /// Always the taskbar layout.
    Desktop,
    /// Always the touch layout (bottom navigation bar, app grid).
    Mobile,
}

impl FormFactorPref {
    pub const ALL: [FormFactorPref; 3] = [
        FormFactorPref::Auto,
        FormFactorPref::Desktop,
        FormFactorPref::Mobile,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            FormFactorPref::Auto => "auto",
            FormFactorPref::Desktop => "desktop",
            FormFactorPref::Mobile => "mobile",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            FormFactorPref::Auto => "Automatic",
            FormFactorPref::Desktop => "Desktop",
            FormFactorPref::Mobile => "Mobile",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(FormFactorPref::Auto),
            "desktop" => Some(FormFactorPref::Desktop),
            "mobile" | "touch" | "phone" => Some(FormFactorPref::Mobile),
            _ => None,
        }
    }
}

/// Shell layout selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ShellConfig {
    pub form_factor: FormFactorPref,
}

/// Id of the wallpaper set bundled with Glaze and selected by default.
pub const DEFAULT_WALLPAPER_SET: &str = "paper-hills";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopConfig {
    /// Absolute path to a wallpaper image. Wins over `wallpaper_set`.
    pub wallpaper: Option<PathBuf>,
    /// Id of a bundled wallpaper set (see `assets/wallpapers/manifest.toml`).
    /// The light/dark and desktop/mobile variant is picked automatically.
    /// An empty string (with no `wallpaper`) uses the theme canvas colour.
    pub wallpaper_set: String,
    pub show_icons: bool,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        DesktopConfig {
            wallpaper: None,
            wallpaper_set: DEFAULT_WALLPAPER_SET.to_string(),
            show_icons: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub compositor: CompositorBackend,
    /// Restart applications that were running when the session last exited.
    pub restore_apps: bool,
    /// Enable verbose shell logging.
    pub debug: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            compositor: CompositorBackend::Auto,
            restore_apps: false,
            debug: false,
        }
    }
}

/// The complete Glaze configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlazeConfig {
    pub version: u32,
    pub appearance: AppearanceConfig,
    pub panel: PanelConfig,
    pub launcher: LauncherConfig,
    pub desktop: DesktopConfig,
    pub session: SessionConfig,
    pub shell: ShellConfig,
    pub motion: MotionConfig,
}

impl Default for GlazeConfig {
    fn default() -> Self {
        GlazeConfig {
            version: CONFIG_VERSION,
            appearance: AppearanceConfig::default(),
            panel: PanelConfig::default(),
            launcher: LauncherConfig::default(),
            desktop: DesktopConfig::default(),
            session: SessionConfig::default(),
            shell: ShellConfig::default(),
            motion: MotionConfig::default(),
        }
    }
}

/// Errors from loading or saving configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
            ConfigError::Serialize(e) => write!(f, "config serialise error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        ConfigError::Serialize(e)
    }
}

impl GlazeConfig {
    /// Parse from a TOML string and migrate to the current schema.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let mut cfg: GlazeConfig = toml::from_str(s)?;
        cfg.migrate();
        Ok(cfg)
    }

    /// Serialise to a TOML string.
    pub fn to_toml_string(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Load from `path`, or return defaults if the file does not exist.
    pub fn load_from(path: &std::path::Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_toml_str(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    /// Save to `path`, creating parent directories as needed.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml_string()?)?;
        Ok(())
    }

    /// Upgrade an older schema in place. Unknown/newer versions are clamped
    /// forward to the current version without discarding known fields.
    pub fn migrate(&mut self) {
        // v1 -> v2: classic theme was renamed and `scheme` moved from a bool.
        if self.version < 2 {
            // Nothing structurally removed in this step; just bump.
        }
        // v2 -> v3: taskbar customisation, theme files, textures and motion
        // were added; every new key has a default, `show_labels` still works.
        self.version = CONFIG_VERSION;
    }

    /// Effective theme tokens from the built-in set plus the user's inline
    /// overrides (no theme file; see `config_check::resolve_tokens` for the
    /// full load order with a theme file and validation issues).
    pub fn tokens(&self) -> ThemeTokens {
        crate::config_check::resolve_tokens(self, None).0
    }

    /// Effective panel thickness for the current theme and density.
    pub fn panel_height(&self, tokens: &ThemeTokens) -> i32 {
        self.panel
            .height
            .unwrap_or(tokens.metrics.panel_height + self.panel.density.delta_px())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_mode_auto_follows_the_clock() {
        let m = SchemeMode::Auto;
        assert_eq!(m.resolve(ColorScheme::Dark, 7), ColorScheme::Light);
        assert_eq!(m.resolve(ColorScheme::Dark, 18), ColorScheme::Light);
        assert_eq!(m.resolve(ColorScheme::Light, 19), ColorScheme::Dark);
        assert_eq!(m.resolve(ColorScheme::Light, 3), ColorScheme::Dark);
        assert_eq!(
            SchemeMode::Manual.resolve(ColorScheme::Dark, 12),
            ColorScheme::Dark
        );
        let c: GlazeConfig = toml::from_str("[appearance]\nscheme_mode = \"auto\"\n").unwrap();
        assert_eq!(c.appearance.scheme_mode, SchemeMode::Auto);
        let p: GlazeConfig =
            toml::from_str("[panel]\nstart_label_style = \"serif\"\nstart_label_weight = 500\n")
                .unwrap();
        assert_eq!(p.panel.start_label_style, StartLabelStyle::Serif);
        assert_eq!(p.panel.start_label_weight, 500);
        assert_eq!(GlazeConfig::default().panel.start_label, "Start");
    }

    #[test]
    fn defaults_are_current_and_consistent() {
        let c = GlazeConfig::default();
        assert_eq!(c.version, CONFIG_VERSION);
        assert_eq!(c.appearance.theme, ThemeKind::Clay);
        assert_eq!(c.panel.position, PanelPosition::Bottom);
        assert_eq!(c.session.compositor, CompositorBackend::Auto);
    }

    #[test]
    fn empty_toml_yields_defaults() {
        let c = GlazeConfig::from_toml_str("").unwrap();
        assert_eq!(c, GlazeConfig::default());
    }

    #[test]
    fn partial_toml_keeps_other_defaults() {
        let c = GlazeConfig::from_toml_str("[appearance]\ntheme = \"classic\"\n").unwrap();
        assert_eq!(c.appearance.theme, ThemeKind::Classic);
        assert_eq!(c.appearance.scheme, ColorScheme::Light);
        assert_eq!(c.panel.position, PanelPosition::Bottom);
    }

    #[test]
    fn round_trips_through_toml() {
        let mut c = GlazeConfig::default();
        c.appearance.theme = ThemeKind::Classic;
        c.appearance.scheme = ColorScheme::Dark;
        c.panel.clock_24h = false;
        c.launcher.pinned = vec!["org.mozilla.firefox".into(), "glaze-settings".into()];
        c.desktop.wallpaper = Some(PathBuf::from("/usr/share/backgrounds/glaze/canvas.png"));
        let s = c.to_toml_string().unwrap();
        let back = GlazeConfig::from_toml_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn shell_and_wallpaper_defaults_and_round_trip() {
        let mut c = GlazeConfig::default();
        assert_eq!(c.shell.form_factor, FormFactorPref::Auto);
        assert_eq!(c.desktop.wallpaper_set, DEFAULT_WALLPAPER_SET);
        c.shell.form_factor = FormFactorPref::Mobile;
        c.desktop.wallpaper_set = String::new();
        let back = GlazeConfig::from_toml_str(&c.to_toml_string().unwrap()).unwrap();
        assert_eq!(back.shell.form_factor, FormFactorPref::Mobile);
        assert_eq!(back.desktop.wallpaper_set, "");
        // Old configs without the new keys keep the bundled default.
        let old = GlazeConfig::from_toml_str("[desktop]\nshow_icons = true\n").unwrap();
        assert_eq!(old.desktop.wallpaper_set, DEFAULT_WALLPAPER_SET);
        assert_eq!(FormFactorPref::parse("Touch"), Some(FormFactorPref::Mobile));
    }

    #[test]
    fn migrates_old_version_to_current() {
        let c = GlazeConfig::from_toml_str("version = 1\n[appearance]\ntheme = \"clay\"\n").unwrap();
        assert_eq!(c.version, CONFIG_VERSION);
    }

    #[test]
    fn theme_slug_round_trips() {
        for t in ThemeKind::ALL {
            assert_eq!(ThemeKind::parse(t.slug()), Some(t));
        }
        for s in ColorScheme::ALL {
            assert_eq!(ColorScheme::parse(s.slug()), Some(s));
        }
        assert_eq!(ThemeKind::parse("nope"), None);
    }
}
