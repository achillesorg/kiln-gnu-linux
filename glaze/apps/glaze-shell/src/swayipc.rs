//! A tiny second Sway IPC connection for the shell's own window effects.
//!
//! `glaze_backend::SwayBackend` owns the taskbar's connection and its
//! snapshot model; the title-bar controls and the minimise/restore/close
//! animations need a few things that model does not carry (border kind,
//! stacking order, visible workspaces, full-screen state) and must send
//! many small commands (`opacity`, `move position`) per animation frame.
//! They share this connection instead. Every request has a short timeout
//! (Sway on an inactive VT may not answer) and reconnects once on failure,
//! so it can never wedge the GTK main loop.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::Value;

const MAGIC: &[u8; 6] = b"i3-ipc";
const RUN_COMMAND: u32 = 0;
const GET_TREE: u32 = 4;
const TIMEOUT: Duration = Duration::from_millis(400);

thread_local! {
    static CONN: RefCell<Option<UnixStream>> = const { RefCell::new(None) };
}

fn socket_path() -> Option<String> {
    std::env::var("SWAYSOCK").ok().filter(|s| !s.is_empty())
}

/// True when the shell runs inside a Sway session.
pub fn available() -> bool {
    socket_path().is_some()
}

fn connect() -> std::io::Result<UnixStream> {
    let path = socket_path().ok_or_else(|| std::io::Error::other("SWAYSOCK is not set"))?;
    let s = UnixStream::connect(path)?;
    s.set_read_timeout(Some(TIMEOUT))?;
    s.set_write_timeout(Some(TIMEOUT))?;
    Ok(s)
}

fn round_trip(s: &mut UnixStream, msg: u32, payload: &[u8]) -> std::io::Result<String> {
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.extend_from_slice(MAGIC);
    frame.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
    frame.extend_from_slice(&msg.to_ne_bytes());
    frame.extend_from_slice(payload);
    s.write_all(&frame)?;
    let mut header = [0u8; 14];
    s.read_exact(&mut header)?;
    if &header[..6] != MAGIC {
        return Err(std::io::Error::other("bad i3-ipc magic"));
    }
    let len = u32::from_ne_bytes([header[6], header[7], header[8], header[9]]) as usize;
    let mut body = vec![0u8; len];
    s.read_exact(&mut body)?;
    String::from_utf8(body).map_err(std::io::Error::other)
}

fn request(msg: u32, payload: &[u8]) -> Option<String> {
    CONN.with(|c| {
        let mut c = c.borrow_mut();
        for attempt in 0..2 {
            if c.is_none() {
                match connect() {
                    Ok(s) => *c = Some(s),
                    Err(e) => {
                        if attempt == 1 {
                            eprintln!("[glaze-shell] sway ipc (effects): {e}");
                        }
                        return None;
                    }
                }
            }
            let s = c.as_mut()?;
            match round_trip(s, msg, payload) {
                Ok(r) => return Some(r),
                Err(e) => {
                    // A timed-out reply would desynchronise the stream:
                    // drop the connection and try once on a fresh one.
                    *c = None;
                    if attempt == 1 {
                        eprintln!("[glaze-shell] sway ipc (effects): {e}");
                    }
                }
            }
        }
        None
    })
}

/// Run one Sway command list; true when every command succeeded.
pub fn command(cmd: &str) -> bool {
    let Some(reply) = request(RUN_COMMAND, cmd.as_bytes()) else {
        return false;
    };
    match serde_json::from_str::<Value>(&reply) {
        Ok(Value::Array(results)) => results
            .iter()
            .all(|r| r.get("success").and_then(Value::as_bool) == Some(true)),
        _ => false,
    }
}

/// One toplevel view as the effects need it (screen coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub id: i64,
    /// Container rect below the title bar, borders included.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Height of the Sway-drawn title bar (0: none, e.g. client-side
    /// decorations, `border pixel` or `none`).
    pub title_height: i32,
    /// Side border width.
    pub border_width: i32,
    pub floating: bool,
    pub focused: bool,
    pub fullscreen: bool,
    /// On a workspace currently shown on an output.
    pub visible: bool,
    /// Sits in a tabbed/stacked parent (its title bar is a tab).
    pub tabbed: bool,
    /// Workspace name (stacking only matters within one workspace).
    pub workspace: String,
    /// The workspace's usable area (output minus exclusive zones).
    pub area: (i32, i32, i32, i32),
    /// Being animated by the shell (hidden mark `_glaze_anim`).
    pub animating: bool,
    /// Client process id (0 when Sway does not know it).
    pub pid: i64,
}

impl View {
    /// Outer frame (title bar included).
    pub fn frame(&self) -> (i32, i32, i32, i32) {
        (
            self.x,
            self.y - self.title_height,
            self.width,
            self.height + self.title_height,
        )
    }

    /// A real server-side title bar the shell can put buttons on.
    pub fn has_titlebar(&self) -> bool {
        self.title_height >= 16 && !self.fullscreen && !self.tabbed
    }

    /// Fills its work area (maximised by any path: buttons, keys, menus).
    pub fn maximised(&self) -> bool {
        let (x, y, w, h) = self.frame();
        let (ax, ay, aw, ah) = self.area;
        (x - ax).abs() <= 2 && (y - ay).abs() <= 2 && (w - aw).abs() <= 4 && (h - ah).abs() <= 4
    }
}

fn int(v: &Value, k: &str) -> i32 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0) as i32
}

fn is_view(n: &Value) -> bool {
    n.get("app_id").is_some_and(|a| !a.is_null())
        || n.get("window").is_some_and(|w| !w.is_null())
        || n.get("window_properties").is_some()
}

/// Every view on a real workspace (the scratchpad is skipped), in stacking
/// order per workspace: tiled first, then floating bottom to top.
pub fn parse_views(tree: &Value) -> Vec<View> {
    struct Ws<'a> {
        name: &'a str,
        visible: bool,
        area: (i32, i32, i32, i32),
    }
    fn walk(n: &Value, out: &mut Vec<View>, ws: &Ws, floating: bool, tabbed: bool) {
        if is_view(n) {
            let rect = n.get("rect").cloned().unwrap_or(Value::Null);
            let deco = n.get("deco_rect").cloned().unwrap_or(Value::Null);
            let border = n.get("border").and_then(Value::as_str).unwrap_or("");
            out.push(View {
                id: n.get("id").and_then(Value::as_i64).unwrap_or(-1),
                x: int(&rect, "x"),
                y: int(&rect, "y"),
                width: int(&rect, "width"),
                height: int(&rect, "height"),
                title_height: if border == "normal" {
                    int(&deco, "height").max(0)
                } else {
                    0
                },
                border_width: n
                    .get("current_border_width")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0) as i32,
                floating,
                focused: n.get("focused").and_then(Value::as_bool) == Some(true),
                fullscreen: n
                    .get("fullscreen_mode")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    != 0,
                visible: ws.visible,
                tabbed,
                workspace: ws.name.to_string(),
                area: ws.area,
                animating: n
                    .get("marks")
                    .and_then(Value::as_array)
                    .is_some_and(|m| m.iter().any(|m| m.as_str() == Some("_glaze_anim"))),
                pid: n.get("pid").and_then(Value::as_i64).unwrap_or(0),
            });
        }
        let layout = n.get("layout").and_then(Value::as_str).unwrap_or("");
        let child_tabbed = matches!(layout, "tabbed" | "stacked");
        let empty = Vec::new();
        let nodes = n.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
        for c in nodes {
            walk(c, out, ws, floating, child_tabbed);
        }
        let fl = n
            .get("floating_nodes")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for c in fl {
            walk(c, out, ws, true, false);
        }
    }
    let mut out = Vec::new();
    let empty = Vec::new();
    let outputs = tree
        .get("nodes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    for output in outputs {
        let current = output
            .get("current_workspace")
            .and_then(Value::as_str)
            .unwrap_or("");
        let wss = output
            .get("nodes")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for ws in wss {
            let name = ws.get("name").and_then(Value::as_str).unwrap_or("");
            if name == "__i3_scratch" || ws.get("type").and_then(Value::as_str) != Some("workspace")
            {
                continue;
            }
            let r = ws.get("rect").cloned().unwrap_or(Value::Null);
            let info = Ws {
                name,
                visible: name == current,
                area: (
                    int(&r, "x"),
                    int(&r, "y"),
                    int(&r, "width"),
                    int(&r, "height"),
                ),
            };
            walk(ws, &mut out, &info, false, false);
        }
    }
    out
}

/// Every view on a real workspace (see [`parse_views`]), or `None` when
/// Sway does not answer.
pub fn views() -> Option<Vec<View>> {
    let json = request(GET_TREE, b"")?;
    let tree: Value = serde_json::from_str(&json).ok()?;
    Some(parse_views(&tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = r#"{"id":1,"type":"root","nodes":[
      {"id":3,"type":"output","name":"__i3","nodes":[
        {"id":4,"type":"workspace","name":"__i3_scratch","nodes":[],"floating_nodes":[
          {"id":9,"type":"floating_con","app_id":"hidden","rect":{"x":0,"y":0,"width":10,"height":10},"deco_rect":{"x":0,"y":0,"width":10,"height":25},"border":"normal"}]}]},
      {"id":2,"type":"output","name":"Virtual-1","current_workspace":"1","nodes":[
        {"id":5,"type":"workspace","name":"1","rect":{"x":0,"y":0,"width":1280,"height":766},
         "nodes":[{"id":6,"type":"con","app_id":"foot","rect":{"x":0,"y":25,"width":640,"height":741},"deco_rect":{"x":0,"y":0,"width":640,"height":25},"border":"normal","current_border_width":6,"fullscreen_mode":0}],
         "floating_nodes":[
          {"id":7,"type":"floating_con","app_id":"thunar","focused":true,"rect":{"x":315,"y":25,"width":650,"height":741},"deco_rect":{"x":315,"y":0,"width":650,"height":25},"border":"normal","current_border_width":6},
          {"id":8,"type":"floating_con","app_id":"org.gnome.TextEditor","rect":{"x":320,"y":183,"width":640,"height":400},"deco_rect":{"x":0,"y":0,"width":0,"height":0},"border":"csd"}]},
        {"id":10,"type":"workspace","name":"2","nodes":[{"id":11,"type":"con","app_id":"x","rect":{"x":0,"y":25,"width":5,"height":5},"deco_rect":{"x":0,"y":0,"width":5,"height":25},"border":"normal"}],"floating_nodes":[]}]}]}"#;

    #[test]
    fn views_keep_stacking_order_and_skip_the_scratchpad() {
        let v = parse_views(&serde_json::from_str(TREE).unwrap());
        let ids: Vec<i64> = v.iter().map(|v| v.id).collect();
        assert_eq!(ids, vec![6, 7, 8, 11]);
        assert!(!v[0].floating && v[1].floating && v[2].floating);
        assert!(v[1].focused && v[1].visible && !v[3].visible);
        assert_eq!(v[1].frame(), (315, 0, 650, 766));
        assert_eq!(v[1].border_width, 6);
        assert_eq!(v[1].area, (0, 0, 1280, 766));
        // 315,0 650x766 is full height but not full width.
        assert!(!v[1].maximised());
    }

    #[test]
    fn only_server_side_title_bars_take_buttons() {
        let v = parse_views(&serde_json::from_str(TREE).unwrap());
        assert!(v[0].has_titlebar());
        assert!(v[1].has_titlebar());
        // Client-side decorations draw their own buttons: never doubled.
        assert!(!v[2].has_titlebar());
    }
}
