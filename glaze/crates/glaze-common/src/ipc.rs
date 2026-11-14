//! Versioned local IPC protocol between the Glaze shell and the Glaze service.
//!
//! The transport is a Unix domain socket at `$XDG_RUNTIME_DIR/glaze/control.sock`
//! speaking newline-delimited JSON (one [`Request`] per line, one [`Response`]
//! per line). The protocol is explicitly versioned; a mismatch is reported
//! rather than silently ignored. No request can execute arbitrary code —
//! launching an app is limited to registry ids that were already validated.

use crate::model::{
    AppEntry, NotificationInfo, SearchHit, ShellStatus, TrayItemInfo, WindowInfo, WorkspaceInfo,
};
use serde::{Deserialize, Serialize};

/// Bumped whenever a request/response shape changes incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default socket file name under the runtime directory.
pub const SOCKET_NAME: &str = "control.sock";

/// A request from the shell (or CLI) to the service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Request {
    /// Handshake. Returns [`Response::Hello`] or a version error.
    Hello { protocol: u32, client: String },
    /// Liveness check.
    Ping,
    /// Full application registry snapshot.
    ListApps,
    /// Launch a registered application by its desktop id.
    LaunchApp { id: String },
    /// Search the registry; `limit == 0` means "use the service default".
    SearchApps { query: String, limit: usize },
    /// List live toplevel windows from the compositor backend.
    ListWindows,
    /// List workspaces.
    ListWorkspaces,
    /// Focus a window by backend id.
    ActivateWindow { id: String },
    /// Close a window by backend id (asks the client politely).
    CloseWindow { id: String },
    /// Real StatusNotifier tray items the service knows about.
    ListTray,
    /// The notification history plus which ids are still unread.
    ListNotifications,
    /// Mark every stored notification as read (badge clears).
    MarkNotificationsRead,
    /// Ask the service to reload its registry / configuration.
    Reload,
    /// Aggregate health/telemetry for the settings "About" page.
    Status,
    // --- glaze-compositor (agent B, M3): stacking <-> tiling toggle --------
    // Served by glaze-compositor's own socket ($GLAZE_COMPOSITOR_SOCKET, default
    // $XDG_RUNTIME_DIR/glaze/compositor.sock), not by glaze-service. See
    // docs/compositor.md. `mode`: "stacking" | "tiling" | "toggle".
    // `workspace`: zero-based index, or None for the global mode (which clears
    // per-workspace overrides). Answered with `Ok` or `Error`.
    SetWindowLayout {
        mode: String,
        workspace: Option<u32>,
    },
}

impl Request {
    /// Serialise to a single-line JSON request.
    pub fn to_line(&self) -> Result<String, serde_json::Error> {
        let mut s = serde_json::to_string(self)?;
        s.push('\n');
        Ok(s)
    }
}

/// A response from the service to the shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Response {
    /// Successful handshake with the negotiated version.
    Hello {
        protocol: u32,
        service_version: String,
    },
    Pong,
    Apps {
        apps: Vec<AppEntry>,
    },
    SearchResults {
        hits: Vec<SearchHit>,
    },
    Windows {
        windows: Vec<WindowInfo>,
    },
    Workspaces {
        workspaces: Vec<WorkspaceInfo>,
    },
    /// Tray items, in stable order.
    Tray {
        items: Vec<TrayItemInfo>,
    },
    /// Notification history (`items`) with the ids still unread, and whether the
    /// server is in Do-Not-Disturb mode.
    Notifications {
        items: Vec<NotificationInfo>,
        unread_ids: Vec<u32>,
        dnd: bool,
    },
    /// Generic acknowledgement for fire-and-forget actions.
    Ok,
    Status {
        status: ShellStatus,
    },
    /// A structured failure. `code` is stable; `message` is for humans/logs.
    Error {
        code: ErrorCode,
        message: String,
    },
}

/// Stable error codes so the shell can react without string matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    ProtocolMismatch,
    NotFound,
    LauncherFailed,
    BackendUnavailable,
    InvalidRequest,
    Internal,
}

impl Response {
    /// Convenience constructor for [`Response::Error`].
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Response::Error {
            code,
            message: message.into(),
        }
    }

    /// True when this is an error response.
    pub fn is_error(&self) -> bool {
        matches!(self, Response::Error { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_through_json_line() {
        let reqs = vec![
            Request::Hello {
                protocol: PROTOCOL_VERSION,
                client: "glaze-shell/0.1.0".into(),
            },
            Request::Ping,
            Request::ListApps,
            Request::LaunchApp {
                id: "org.mozilla.firefox".into(),
            },
            Request::SearchApps {
                query: "fire".into(),
                limit: 5,
            },
            Request::ActivateWindow { id: "42".into() },
            Request::Status,
        ];
        for req in reqs {
            let line = req.to_line().unwrap();
            assert!(line.ends_with('\n'));
            assert_eq!(line.matches('\n').count(), 1, "must be a single line");
            let back: Request = serde_json::from_str(line.trim_end()).unwrap();
            assert_eq!(back, req);
        }
    }

    #[test]
    fn response_round_trips() {
        let resp = Response::error(ErrorCode::NotFound, "no such app");
        let s = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&s).unwrap();
        assert_eq!(back, resp);
        assert!(back.is_error());
    }

    #[test]
    fn tags_are_kebab_case_on_the_wire() {
        let s = serde_json::to_string(&Request::ListApps).unwrap();
        assert!(s.contains("\"kind\":\"list-apps\""));
    }
}
