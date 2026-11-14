//! Shared data models: applications, windows, workspaces and search hits.

use serde::{Deserialize, Serialize};

/// A launchable application discovered from an XDG `.desktop` entry.
///
/// `id` is the stable desktop-file id (relative path with `/` replaced by `-`,
/// per the XDG Desktop Entry Specification) and is what callers should use to
/// refer to the application; `exec` is already validated for shell safety by
/// the service layer before anything is launched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    /// Stable desktop-file id, e.g. `org.mozilla.firefox` or `glaze-settings`.
    pub id: String,
    /// Localised display name (`Name=`).
    pub name: String,
    /// Optional short description (`Comment=`).
    #[serde(default)]
    pub comment: Option<String>,
    /// Generic name (`GenericName=`), e.g. "Web Browser".
    #[serde(default)]
    pub generic_name: Option<String>,
    /// Raw `Exec=` value (field codes preserved, never shell-interpreted).
    pub exec: String,
    /// `TryExec=` binary that must exist for the entry to be considered valid.
    #[serde(default)]
    pub try_exec: Option<String>,
    /// Icon name or absolute path (`Icon=`).
    #[serde(default)]
    pub icon: Option<String>,
    /// `Categories=` split on `;`.
    #[serde(default)]
    pub categories: Vec<String>,
    /// `Keywords=` split on `;`, used to improve search ranking.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Whether the app runs in a terminal (`Terminal=true`).
    #[serde(default)]
    pub terminal: bool,
    /// Hidden from menus (`NoDisplay=true` or `Hidden=true`).
    #[serde(default)]
    pub no_display: bool,
    /// Working directory from `Path=`.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Absolute path of the `.desktop` file this entry came from.
    pub source_path: String,
}

impl AppEntry {
    /// The lowercased haystack used by search ranking.
    pub fn search_blob(&self) -> String {
        let mut s = String::with_capacity(48);
        s.push_str(&self.name);
        if let Some(g) = &self.generic_name {
            s.push(' ');
            s.push_str(g);
        }
        if let Some(c) = &self.comment {
            s.push(' ');
            s.push_str(c);
        }
        for k in &self.keywords {
            s.push(' ');
            s.push_str(k);
        }
        s.push_str(&self.id);
        s.to_lowercase()
    }
}

/// The kind of window as reported by the compositor backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WindowKind {
    #[default]
    Normal,
    Dialog,
    Utility,
    Splash,
    /// Not a user-facing toplevel (e.g. layer-shell surface we own).
    Internal,
}

/// A live toplevel window as observed from the compositor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Backend-specific window identifier (compositor con id / handle string).
    pub id: String,
    /// Wayland app_id or X11 WM_CLASS.
    #[serde(default)]
    pub app_id: Option<String>,
    /// Current window title.
    pub title: String,
    /// Zero-based workspace index the window lives on, if the backend reports it.
    #[serde(default)]
    pub workspace: Option<u32>,
    #[serde(default)]
    pub kind: WindowKind,
    /// True when this is the focused toplevel.
    #[serde(default)]
    pub focused: bool,
    /// True when minimised.
    #[serde(default)]
    pub minimized: bool,
    /// True when floating (not tiled), where the backend distinguishes this.
    #[serde(default)]
    pub floating: bool,
}

/// A workspace / virtual desktop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub index: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub urgent: bool,
}

/// Tray/status-item status **as it crosses the IPC boundary**.
///
/// Kept separate from the service's internal model so the protocol is a stable
/// contract: the service maps its own types onto these, and the shell maps these
/// onto its view model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TrayStatusInfo {
    /// Not currently showing anything.
    Passive,
    /// Normal, visible status item.
    #[default]
    Active,
    /// Wants the user's attention.
    NeedsAttention,
}

impl TrayStatusInfo {
    /// True when the item is asking for attention.
    pub const fn needs_attention(self) -> bool {
        matches!(self, TrayStatusInfo::NeedsAttention)
    }
}

/// Tray/status-item category as it crosses the IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TrayCategoryInfo {
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
    #[default]
    Other,
}

/// One tray/status item on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayItemInfo {
    /// Stable id (usually `service:path`); upserts key on this.
    pub id: String,
    /// Accessible / tooltip title.
    pub title: String,
    /// Icon name or absolute path, when the item exposes one.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub status: TrayStatusInfo,
    #[serde(default)]
    pub category: TrayCategoryInfo,
}

/// Notification urgency as it crosses the IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
}

/// One stored notification on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationInfo {
    /// Server-assigned id, used to replace or close a notification.
    pub id: u32,
    /// Sending application's name.
    pub app_name: String,
    /// Short summary line.
    pub summary: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub urgency: NotificationUrgency,
    /// Unix seconds the server recorded it.
    #[serde(default)]
    pub received_at_unix: u64,
}

/// A ranked search result over the application registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub app_id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Higher is a better match.
    pub score: i32,
    /// Matched character indices in `name`, for highlighting in the launcher UI.
    #[serde(default)]
    pub name_match_indices: Vec<usize>,
}

/// Overall health/telemetry reported by the shell for the settings "About" page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellStatus {
    pub version: String,
    pub theme: String,
    pub scheme: String,
    pub compositor_backend: String,
    pub app_count: usize,
    pub window_count: usize,
}
