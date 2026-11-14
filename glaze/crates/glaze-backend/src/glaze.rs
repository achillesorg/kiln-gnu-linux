//! The Glaze compositor backend: `glaze-compositor`'s control socket.
//!
//! `glaze-compositor` serves newline-delimited JSON on
//! `$GLAZE_COMPOSITOR_SOCKET` (default `$XDG_RUNTIME_DIR/glaze/compositor.sock`),
//! the same framing as `glaze-service` (`glaze_common::ipc`). The taskbar gets
//! everything it needs from that one channel with the compositor's own window
//! ids: the window list (minimised windows included, with workspace and
//! float/tile state), focus/minimise/restore/maximise/close, moving windows
//! between workspaces, the float/tile toggle, live frame colours, and a change
//! stream (`subscribe`). The compositor also serves the standard
//! `wlr-foreign-toplevel-management` protocol for other taskbars and tools;
//! see `docs/compositor.md` and `docs/decisions.md` (M4) for why the Glaze
//! shell uses the socket.
//!
//! Requests and replies are portable (built and unit-tested everywhere); only
//! the Unix socket transport is gated.

use glaze_common::model::{WindowInfo, WorkspaceInfo};
use serde_json::{json, Value};

use crate::{
    Backend, BackendError, BackendEvent, EventSource, FrameColors, LayoutMode, LayoutScope,
};

/// Environment variable `glaze-compositor` exports to its children.
pub const SOCKET_ENV: &str = "GLAZE_COMPOSITOR_SOCKET";

/// The control socket path from the environment, or the default.
pub fn socket_path() -> std::path::PathBuf {
    match std::env::var_os(SOCKET_ENV) {
        Some(p) if !p.is_empty() => p.into(),
        _ => glaze_common::paths::runtime_dir()
            .join("glaze")
            .join("compositor.sock"),
    }
}

/// Build one request line.
pub fn request(kind: &str, fields: Value) -> String {
    let mut v = match fields {
        Value::Object(m) => Value::Object(m),
        _ => json!({}),
    };
    v["kind"] = Value::String(kind.to_owned());
    let mut s = v.to_string();
    s.push('\n');
    s
}

/// Check a reply: `{"kind":"error",...}` becomes an error.
pub fn check_reply(reply: &Value) -> Result<(), BackendError> {
    if reply.get("kind").and_then(Value::as_str) == Some("error") {
        let code = reply.get("code").and_then(Value::as_str).unwrap_or("error");
        let msg = reply
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        return Err(if code == "not-found" {
            BackendError::NotFound(msg)
        } else {
            BackendError::Protocol(format!("{code}: {msg}"))
        });
    }
    Ok(())
}

/// Parse a `windows` reply.
pub fn parse_windows(reply: &Value) -> Result<Vec<WindowInfo>, BackendError> {
    check_reply(reply)?;
    let w = reply
        .get("windows")
        .ok_or_else(|| BackendError::Protocol("reply without windows".into()))?;
    serde_json::from_value(w.clone()).map_err(|e| BackendError::Protocol(e.to_string()))
}

/// Parse a `workspaces` reply.
pub fn parse_workspaces(reply: &Value) -> Result<Vec<WorkspaceInfo>, BackendError> {
    check_reply(reply)?;
    let w = reply
        .get("workspaces")
        .ok_or_else(|| BackendError::Protocol("reply without workspaces".into()))?;
    serde_json::from_value(w.clone()).map_err(|e| BackendError::Protocol(e.to_string()))
}

/// Parse a `layout` reply (`mode` of the active workspace).
pub fn parse_layout(reply: &Value) -> Result<LayoutMode, BackendError> {
    check_reply(reply)?;
    match reply.get("mode").and_then(Value::as_str) {
        Some("tiling") => Ok(LayoutMode::Tile),
        Some("stacking") => Ok(LayoutMode::Float),
        other => Err(BackendError::Protocol(format!("bad layout mode {other:?}"))),
    }
}

/// Parse one event-stream line.
pub fn parse_event_line(line: &str) -> Option<BackendEvent> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v.get("kind").and_then(Value::as_str) != Some("event") {
        return None;
    }
    let change = v
        .get("change")
        .and_then(Value::as_str)
        .unwrap_or("change")
        .to_owned();
    let id = v.get("id").and_then(Value::as_str).map(str::to_owned);
    Some(match v.get("scope").and_then(Value::as_str) {
        Some("workspace") => BackendEvent::Workspace { change },
        _ => BackendEvent::Window { change, id },
    })
}

/// The live backend.
pub struct GlazeBackend {
    path: std::path::PathBuf,
}

impl GlazeBackend {
    /// Connect to the running compositor (fails if nobody answers `ping`).
    pub fn connect() -> Result<Self, BackendError> {
        let b = GlazeBackend {
            path: socket_path(),
        };
        let pong = b.call("ping", json!({}))?;
        if pong.get("kind").and_then(Value::as_str) != Some("pong") {
            return Err(BackendError::Protocol(format!(
                "{} did not answer ping",
                b.path.display()
            )));
        }
        Ok(b)
    }

    fn call(&self, kind: &str, fields: Value) -> Result<Value, BackendError> {
        transport::call(&self.path, &request(kind, fields))
    }

    fn call_ok(&self, kind: &str, fields: Value) -> Result<(), BackendError> {
        check_reply(&self.call(kind, fields)?)
    }

    fn focused_workspace(&self) -> Option<u32> {
        let ws = self.workspaces().ok()?;
        ws.iter().find(|w| w.focused).map(|w| w.index)
    }
}

impl Backend for GlazeBackend {
    fn name(&self) -> &'static str {
        "glaze-compositor"
    }

    fn is_connected(&self) -> bool {
        self.call("ping", json!({})).is_ok()
    }

    fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
        parse_windows(&self.call("list-windows", json!({}))?)
    }

    fn workspaces(&self) -> Result<Vec<WorkspaceInfo>, BackendError> {
        parse_workspaces(&self.call("list-workspaces", json!({}))?)
    }

    fn activate_window(&self, id: &str) -> Result<(), BackendError> {
        self.call_ok("activate-window", json!({ "id": id }))
    }

    fn close_window(&self, id: &str) -> Result<(), BackendError> {
        self.call_ok("close-window", json!({ "id": id }))
    }

    fn minimize_window(&self, id: &str) -> Result<(), BackendError> {
        self.call_ok("minimize-window", json!({ "id": id }))
    }

    fn restore_window(&self, id: &str) -> Result<(), BackendError> {
        self.call_ok("restore-window", json!({ "id": id }))
    }

    fn is_maximized(&self, id: &str) -> bool {
        self.call("window-state", json!({ "id": id }))
            .ok()
            .and_then(|v| v.get("maximized").and_then(Value::as_bool))
            .unwrap_or(false)
    }

    fn toggle_maximize(&self, id: &str) -> Result<(), BackendError> {
        self.call_ok("set-maximized", json!({ "id": id, "maximized": null }))
    }

    fn move_to_workspace(&self, id: &str, number: u32) -> Result<(), BackendError> {
        self.call_ok(
            "move-to-workspace",
            json!({ "id": id, "workspace": number.saturating_sub(1) }),
        )
    }

    fn set_frame_colors(&self, c: &FrameColors) -> Result<(), BackendError> {
        self.call_ok(
            "set-frame-colors",
            json!({
                "focused_bg": c.focused_bg,
                "focused_text": c.focused_text,
                "focused_border": c.focused_border,
                "other_bg": c.other_bg,
                "other_text": c.other_text,
                "other_border": c.other_border,
                "urgent_bg": c.urgent_bg,
            }),
        )
    }

    fn layout_mode(&self) -> Result<LayoutMode, BackendError> {
        parse_layout(&self.call("get-layout", json!({}))?)
    }

    fn set_layout_mode(&self, mode: LayoutMode, scope: LayoutScope) -> Result<(), BackendError> {
        let workspace = match scope {
            LayoutScope::AllWorkspaces => None,
            LayoutScope::FocusedWorkspace => Some(self.focused_workspace().unwrap_or(0)),
        };
        let mode = match mode {
            LayoutMode::Float => "stacking",
            LayoutMode::Tile => "tiling",
        };
        self.call_ok(
            "set-window-layout",
            json!({ "mode": mode, "workspace": workspace }),
        )
    }

    fn set_minimize_rects(&self, rects: &[(String, i32, i32, i32, i32)]) -> Result<(), BackendError> {
        let rects: Vec<Value> = rects
            .iter()
            .map(|(id, x, y, w, h)| json!({"id": id, "x": x, "y": y, "w": w, "h": h}))
            .collect();
        self.call_ok("set-minimize-rects", json!({ "namespace": "glaze-panel", "rects": rects }))
    }

    fn subscribe(&self) -> Result<Box<dyn EventSource>, BackendError> {
        transport::subscribe(&self.path)
    }
}

#[cfg(unix)]
mod transport {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::path::Path;
    use std::time::Duration;

    use serde_json::Value;

    use crate::{BackendError, BackendEvent, EventSource};

    fn connect(path: &Path) -> Result<UnixStream, BackendError> {
        UnixStream::connect(path)
            .map_err(|e| BackendError::NotConnected(format!("{}: {e}", path.display())))
    }

    pub fn call(path: &Path, line: &str) -> Result<Value, BackendError> {
        let mut s = connect(path)?;
        let t = Some(Duration::from_secs(2));
        let _ = s.set_read_timeout(t);
        let _ = s.set_write_timeout(t);
        s.write_all(line.as_bytes())
            .map_err(|e| BackendError::Io(e.to_string()))?;
        let mut reply = String::new();
        BufReader::new(&s)
            .read_line(&mut reply)
            .map_err(|e| BackendError::Io(e.to_string()))?;
        if reply.trim().is_empty() {
            return Err(BackendError::Io("empty reply".into()));
        }
        serde_json::from_str(reply.trim()).map_err(|e| BackendError::Protocol(e.to_string()))
    }

    pub struct GlazeEvents {
        stream: UnixStream,
        buf: Vec<u8>,
    }

    pub fn subscribe(path: &Path) -> Result<Box<dyn EventSource>, BackendError> {
        let mut s = connect(path)?;
        s.write_all(super::request("subscribe", serde_json::json!({})).as_bytes())
            .map_err(|e| BackendError::Io(e.to_string()))?;
        s.set_nonblocking(true)
            .map_err(|e| BackendError::Io(e.to_string()))?;
        Ok(Box::new(GlazeEvents {
            stream: s,
            buf: Vec::new(),
        }))
    }

    impl EventSource for GlazeEvents {
        fn raw_fd(&self) -> i32 {
            self.stream.as_raw_fd()
        }

        fn read_events(&mut self) -> Result<Vec<BackendEvent>, BackendError> {
            let mut chunk = [0u8; 4096];
            loop {
                match self.stream.read(&mut chunk) {
                    Ok(0) => return Err(BackendError::Io("compositor closed the stream".into())),
                    Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(BackendError::Io(e.to_string())),
                }
            }
            let mut out = Vec::new();
            while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                if let Some(ev) = super::parse_event_line(&String::from_utf8_lossy(&line)) {
                    out.push(ev);
                }
            }
            Ok(out)
        }
    }
}

#[cfg(not(unix))]
mod transport {
    use std::path::Path;

    use serde_json::Value;

    use crate::{BackendError, EventSource};

    pub fn call(_path: &Path, _line: &str) -> Result<Value, BackendError> {
        Err(BackendError::Unsupported(
            "glaze-compositor runs on Linux only",
        ))
    }

    pub fn subscribe(_path: &Path) -> Result<Box<dyn EventSource>, BackendError> {
        Err(BackendError::Unsupported(
            "glaze-compositor runs on Linux only",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_lines_carry_kind_and_fields() {
        let line = request("minimize-window", json!({"id": "glaze-toplevel-3"}));
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["kind"], "minimize-window");
        assert_eq!(v["id"], "glaze-toplevel-3");
    }

    #[test]
    fn replies_parse_and_errors_map() {
        let w = json!({"kind":"windows","windows":[{"id":"glaze-toplevel-1","app_id":"foot","title":"t","workspace":0,"kind":"normal","focused":true,"minimized":true,"floating":true}]});
        let ws = parse_windows(&w).unwrap();
        assert_eq!(ws.len(), 1);
        assert!(ws[0].minimized);
        let e = json!({"kind":"error","code":"not-found","message":"no window x"});
        assert!(matches!(parse_windows(&e), Err(BackendError::NotFound(_))));
        assert_eq!(
            parse_layout(&json!({"kind":"layout","mode":"tiling"})).unwrap(),
            LayoutMode::Tile
        );
    }

    #[test]
    fn event_lines() {
        assert_eq!(
            parse_event_line(r#"{"kind":"event","scope":"window","change":"new","id":"glaze-toplevel-2"}"#),
            Some(BackendEvent::Window {
                change: "new".into(),
                id: Some("glaze-toplevel-2".into())
            })
        );
        assert_eq!(
            parse_event_line(r#"{"kind":"event","scope":"workspace","change":"focus"}"#),
            Some(BackendEvent::Workspace {
                change: "focus".into()
            })
        );
        assert_eq!(parse_event_line(r#"{"kind":"pong"}"#), None);
    }
}
