//! The compositor's control socket protocol (portable half).
//!
//! `glaze-compositor` listens on `$GLAZE_COMPOSITOR_SOCKET` (default
//! `$XDG_RUNTIME_DIR/glaze/compositor.sock`) and speaks the same
//! newline-delimited JSON as `glaze-service` ([`glaze_common::ipc`]): one
//! `Request` per line, one `Response` per line. It answers the window
//! requests (`list-windows`, `list-workspaces`, `activate-window`,
//! `close-window`) and the tiling toggle (`set-window-layout`), so the
//! taskbar's float/tile button works without Sway. See `docs/compositor.md`.

use std::path::PathBuf;

use glaze_common::ipc::{ErrorCode, Request, Response, PROTOCOL_VERSION};
use glaze_common::model::{WindowInfo, WorkspaceInfo};
use glaze_wm::{LayoutScope, ModeRequest};

/// Environment variable the compositor exports to its children.
pub const SOCKET_ENV: &str = "GLAZE_COMPOSITOR_SOCKET";
/// Default socket file name under `$XDG_RUNTIME_DIR/glaze/`.
pub const SOCKET_NAME: &str = "compositor.sock";

/// Default socket path.
pub fn default_socket_path() -> PathBuf {
    glaze_common::paths::runtime_dir()
        .join("glaze")
        .join(SOCKET_NAME)
}

/// What the control socket can ask of the running compositor.
pub trait ControlTarget {
    fn windows(&self) -> Vec<WindowInfo>;
    fn workspaces(&self) -> Vec<WorkspaceInfo>;
    /// Focus (and restore, and switch to the workspace of) a window.
    fn activate(&mut self, id: &str) -> bool;
    /// Ask a window to close.
    fn close(&mut self, id: &str) -> bool;
    /// Apply a layout request; returns the effective mode of the active
    /// workspace afterwards.
    fn set_layout(&mut self, scope: LayoutScope, request: ModeRequest) -> glaze_wm::LayoutMode;
    fn workspace_count(&self) -> u32;

    // --- Glaze shell extensions (M4) --------------------------------------
    fn minimize(&mut self, _id: &str) -> bool {
        false
    }
    fn restore(&mut self, id: &str) -> bool {
        self.activate(id)
    }
    /// `None` toggles.
    fn set_maximized(&mut self, _id: &str, _on: Option<bool>) -> bool {
        false
    }
    fn set_fullscreen(&mut self, _id: &str, _on: Option<bool>) -> bool {
        false
    }
    fn move_to_workspace(&mut self, _id: &str, _workspace: u32) -> bool {
        false
    }
    /// `(maximized, fullscreen, minimized)`.
    fn window_state(&self, _id: &str) -> Option<(bool, bool, bool)> {
        None
    }
    /// Effective mode of `workspace` (or the active one).
    fn layout_mode(&self, _workspace: Option<u32>) -> glaze_wm::LayoutMode {
        glaze_wm::LayoutMode::Stacking
    }
    fn set_frame_colors(&mut self, _c: &crate::frame::FrameColorsMsg) -> bool {
        false
    }
    fn exit(&mut self) {}
    fn lock(&mut self) {}
    fn is_locked(&self) -> bool {
        false
    }
    fn set_minimize_rects(&mut self, _namespace: &str, _rects: &[RectMsg]) -> bool {
        false
    }
}

/// Glaze-shell requests beyond `glaze_common::ipc` (same framing, same
/// `kind` tag). Kept here so `glaze-common` (shared with glaze-service) does
/// not change for compositor-only verbs.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExtRequest {
    MinimizeWindow {
        id: String,
    },
    RestoreWindow {
        id: String,
    },
    /// `maximized: null` (or absent) toggles.
    SetMaximized {
        id: String,
        #[serde(default)]
        maximized: Option<bool>,
    },
    SetFullscreen {
        id: String,
        #[serde(default)]
        fullscreen: Option<bool>,
    },
    /// Zero-based workspace.
    MoveToWorkspace {
        id: String,
        workspace: u32,
    },
    WindowState {
        id: String,
    },
    GetLayout {
        #[serde(default)]
        workspace: Option<u32>,
    },
    SetFrameColors(crate::frame::FrameColorsMsg),
    /// End the session (the shell's Log out).
    Exit,
    /// Start the screen locker (the shell's Lock).
    Lock,
    /// Task-button rectangles relative to the layer surface `namespace`.
    SetMinimizeRects {
        #[serde(default)]
        namespace: Option<String>,
        rects: Vec<RectMsg>,
    },
    /// Turn this connection into an event stream (`{"kind":"event",...}`
    /// lines on every window/workspace/layout change).
    Subscribe,
}

/// One task-button rectangle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RectMsg {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// What an extension request produced.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtReply {
    Json(serde_json::Value),
    Subscribe,
}

fn ok_or_missing(ok: bool, id: &str) -> serde_json::Value {
    if ok {
        serde_json::json!({"kind": "ok"})
    } else {
        serde_json::json!({"kind": "error", "code": "not-found", "message": format!("no window {id} (or nothing to do)")})
    }
}

/// Try a line as an extension request; `None` means "not one of ours".
pub fn handle_ext_line(target: &mut dyn ControlTarget, line: &str) -> Option<ExtReply> {
    let req: ExtRequest = serde_json::from_str(line.trim()).ok()?;
    Some(match req {
        ExtRequest::Subscribe => ExtReply::Subscribe,
        ExtRequest::SetMinimizeRects { namespace, rects } => {
            let ns = namespace.unwrap_or_else(|| "glaze-panel".into());
            ExtReply::Json(if target.set_minimize_rects(&ns, &rects) {
                serde_json::json!({"kind": "ok"})
            } else {
                serde_json::json!({"kind":"error","code":"not-found","message":format!("no layer surface {ns}")})
            })
        }
        ExtRequest::Exit if target.is_locked() => ExtReply::Json(
            serde_json::json!({"kind":"error","code":"invalid-request","message":"the session is locked"}),
        ),
        ExtRequest::Exit => {
            target.exit();
            ExtReply::Json(serde_json::json!({"kind": "ok"}))
        }
        ExtRequest::Lock => {
            target.lock();
            ExtReply::Json(serde_json::json!({"kind": "ok"}))
        }
        ExtRequest::MinimizeWindow { id } => ExtReply::Json(ok_or_missing(target.minimize(&id), &id)),
        ExtRequest::RestoreWindow { id } => ExtReply::Json(ok_or_missing(target.restore(&id), &id)),
        ExtRequest::SetMaximized { id, maximized } => {
            ExtReply::Json(ok_or_missing(target.set_maximized(&id, maximized), &id))
        }
        ExtRequest::SetFullscreen { id, fullscreen } => {
            ExtReply::Json(ok_or_missing(target.set_fullscreen(&id, fullscreen), &id))
        }
        ExtRequest::MoveToWorkspace { id, workspace } => {
            if workspace >= target.workspace_count() {
                ExtReply::Json(serde_json::json!({"kind":"error","code":"not-found","message":format!("no workspace {workspace}")}))
            } else {
                ExtReply::Json(ok_or_missing(target.move_to_workspace(&id, workspace), &id))
            }
        }
        ExtRequest::WindowState { id } => ExtReply::Json(match target.window_state(&id) {
            Some((maximized, fullscreen, minimized)) => serde_json::json!({
                "kind": "window-state", "id": id, "maximized": maximized,
                "fullscreen": fullscreen, "minimized": minimized
            }),
            None => ok_or_missing(false, &id),
        }),
        ExtRequest::GetLayout { workspace } => {
            let mode = target.layout_mode(workspace);
            ExtReply::Json(serde_json::json!({"kind": "layout", "mode": mode.as_str()}))
        }
        ExtRequest::SetFrameColors(c) => ExtReply::Json(if target.set_frame_colors(&c) {
            serde_json::json!({"kind": "ok"})
        } else {
            serde_json::json!({"kind":"error","code":"invalid-request","message":"focused_bg must be #rrggbb"})
        }),
    })
}

/// Handle one request line.
pub fn handle_line(target: &mut dyn ControlTarget, line: &str) -> Response {
    match serde_json::from_str::<Request>(line.trim()) {
        Ok(req) => handle(target, &req),
        Err(e) => Response::error(ErrorCode::InvalidRequest, format!("bad request: {e}")),
    }
}

/// Handle one parsed request.
pub fn handle(target: &mut dyn ControlTarget, req: &Request) -> Response {
    match req {
        Request::Hello { protocol, .. } => {
            if *protocol == PROTOCOL_VERSION {
                Response::Hello {
                    protocol: PROTOCOL_VERSION,
                    service_version: format!("glaze-compositor {}", env!("CARGO_PKG_VERSION")),
                }
            } else {
                Response::error(
                    ErrorCode::ProtocolMismatch,
                    format!("protocol {protocol} != {PROTOCOL_VERSION}"),
                )
            }
        }
        Request::Ping => Response::Pong,
        Request::ListWindows => Response::Windows {
            windows: target.windows(),
        },
        Request::ListWorkspaces => Response::Workspaces {
            workspaces: target.workspaces(),
        },
        Request::ActivateWindow { id } => {
            if target.activate(id) {
                Response::Ok
            } else {
                Response::error(ErrorCode::NotFound, format!("no window {id}"))
            }
        }
        Request::CloseWindow { id } => {
            if target.close(id) {
                Response::Ok
            } else {
                Response::error(ErrorCode::NotFound, format!("no window {id}"))
            }
        }
        Request::SetWindowLayout { mode, workspace } => {
            let Some(request) = ModeRequest::parse(mode) else {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    format!("mode must be stacking, tiling or toggle, not {mode:?}"),
                );
            };
            let scope = match workspace {
                Some(ws) if *ws >= target.workspace_count() => {
                    return Response::error(ErrorCode::NotFound, format!("no workspace {ws}"));
                }
                Some(ws) => LayoutScope::Workspace(*ws),
                None => LayoutScope::Global,
            };
            target.set_layout(scope, request);
            Response::Ok
        }
        other => Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "glaze-compositor does not serve {:?}; ask glaze-service",
                serde_json::to_value(other)
                    .ok()
                    .and_then(|v| v.get("kind").cloned())
                    .unwrap_or_default()
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wm::{Applied, WindowManager};
    use glaze_wm::{LayoutMode, Rect};

    struct Mock(WindowManager);

    impl ControlTarget for Mock {
        fn windows(&self) -> Vec<WindowInfo> {
            self.0.window_infos()
        }
        fn workspaces(&self) -> Vec<WorkspaceInfo> {
            self.0.workspace_infos()
        }
        fn activate(&mut self, id: &str) -> bool {
            self.0.focus(id)
        }
        fn close(&mut self, id: &str) -> bool {
            self.0.get(id).is_some()
        }
        fn set_layout(&mut self, scope: LayoutScope, request: ModeRequest) -> LayoutMode {
            self.0.set_layout(scope, request);
            self.0.active_mode()
        }
        fn workspace_count(&self) -> u32 {
            self.0.workspace_count()
        }
    }

    fn mock() -> Mock {
        let mut wm = WindowManager::new(Rect::new(0, 0, 1280, 800));
        wm.add_window(Some("foot".into()), "foot", (500, 300), false);
        wm.add_window(Some("foot".into()), "foot 2", (500, 300), false);
        Mock(wm)
    }

    #[test]
    fn toggle_request_changes_layout() {
        let mut m = mock();
        let line = Request::SetWindowLayout {
            mode: "toggle".into(),
            workspace: Some(0),
        }
        .to_line()
        .unwrap();
        assert!(line.contains("\"kind\":\"set-window-layout\""));
        assert_eq!(handle_line(&mut m, &line), Response::Ok);
        assert_eq!(m.0.active_mode(), LayoutMode::Tiling);
        let global = r#"{"kind":"set-window-layout","mode":"stacking","workspace":null}"#;
        assert_eq!(handle_line(&mut m, global), Response::Ok);
        assert_eq!(m.0.active_mode(), LayoutMode::Stacking);
    }

    #[test]
    fn bad_layout_requests_are_errors() {
        let mut m = mock();
        let r = handle_line(
            &mut m,
            r#"{"kind":"set-window-layout","mode":"spiral","workspace":null}"#,
        );
        assert!(r.is_error());
        let r = handle_line(
            &mut m,
            r#"{"kind":"set-window-layout","mode":"tiling","workspace":99}"#,
        );
        assert!(r.is_error());
        assert!(handle_line(&mut m, "not json").is_error());
    }

    #[test]
    fn lists_and_activates() {
        let mut m = mock();
        match handle(&mut m, &Request::ListWindows) {
            Response::Windows { windows } => assert_eq!(windows.len(), 2),
            other => panic!("{other:?}"),
        }
        let first = m.0.windows()[0].id.clone();
        assert_eq!(
            handle(&mut m, &Request::ActivateWindow { id: first.clone() }),
            Response::Ok
        );
        assert_eq!(m.0.focused(), Some(first.as_str()));
        assert!(handle(&mut m, &Request::ActivateWindow { id: "nope".into() }).is_error());
        assert!(handle(&mut m, &Request::ListApps).is_error());
        let _ = Applied::Relayout;
    }

    #[test]
    fn extension_requests_parse() {
        let mut m = mock();
        assert_eq!(
            handle_ext_line(&mut m, r#"{"kind":"subscribe"}"#),
            Some(ExtReply::Subscribe)
        );
        assert_eq!(handle_ext_line(&mut m, r#"{"kind":"ping"}"#), None);
        match handle_ext_line(&mut m, r#"{"kind":"get-layout"}"#) {
            Some(ExtReply::Json(v)) => assert_eq!(v["mode"], "stacking"),
            other => panic!("{other:?}"),
        }
        // The mock does not implement minimise: an honest error.
        match handle_ext_line(&mut m, r#"{"kind":"minimize-window","id":"x"}"#) {
            Some(ExtReply::Json(v)) => assert_eq!(v["kind"], "error"),
            other => panic!("{other:?}"),
        }
        let colors = r##"{"kind":"set-frame-colors","focused_bg":"#33302C","focused_text":"#F3EFE6","focused_border":"#D97757","other_bg":"#2B2926","other_text":"#9A9387","other_border":"#4A4640"}"##;
        assert!(matches!(handle_ext_line(&mut m, colors), Some(ExtReply::Json(_))));
    }
}
