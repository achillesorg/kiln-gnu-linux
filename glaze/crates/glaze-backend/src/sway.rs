//! Sway/i3 IPC message framing and `GET_TREE` tree parsing.
//!
//! Sway speaks the i3 IPC protocol: every message is a fixed 14-byte header —
//! the ASCII magic `i3-ipc`, a native-endian `u32` payload length and a
//! native-endian `u32` message type — followed by a UTF-8 JSON payload
//! ([i3 IPC docs](https://i3wm.org/docs/ipc.html#_message_types)).
//!
//! Framing ([`encode`], [`decode_header`], [`parse_frame`]) and tree parsing
//! ([`parse_tree`]) are **portable and unit-tested** on every host. Only the
//! Unix-domain-socket transport lives behind `#[cfg(unix)]` in
//! [`crate::sway_transport`], so a Windows host can still prove the wire format
//! and the model mapping without a compositor present.

use std::cell::RefCell;

use glaze_common::model::{WindowInfo, WindowKind, WorkspaceInfo};
use serde::Deserialize;

use std::collections::HashMap;

use crate::{
    Backend, BackendError, BackendEvent, EventSource, Frame, FrameColors, LayoutMode, LayoutScope,
};

/// The 6-byte i3 IPC magic string that prefixes every frame.
pub const MAGIC: &[u8; 6] = b"i3-ipc";
/// Fixed header size: magic (6) + payload length (4) + message type (4).
pub const HEADER_LEN: usize = 14;

/// `RUN_COMMAND` — run a sway command (used to focus/close windows).
pub const MSG_RUN_COMMAND: u32 = 0;
/// `GET_WORKSPACES` — list workspaces directly.
pub const MSG_GET_WORKSPACES: u32 = 1;
/// `SUBSCRIBE` — subscribe to an event stream.
pub const MSG_SUBSCRIBE: u32 = 2;
/// `GET_TREE` — the container tree Glaze parses for windows and workspaces.
pub const MSG_GET_TREE: u32 = 4;
/// `GET_VERSION` — compositor version handshake.
pub const MSG_GET_VERSION: u32 = 7;
/// Event message types have the high bit set; low bits name the stream.
pub const EVENT_BIT: u32 = 0x8000_0000;
/// `workspace` event stream.
pub const EVENT_WORKSPACE: u32 = EVENT_BIT;
/// `window` event stream.
pub const EVENT_WINDOW: u32 = EVENT_BIT | 3;
/// Name of sway's hidden scratchpad workspace (Glaze's "minimised" bin).
pub const SCRATCH_WORKSPACE: &str = "__i3_scratch";

/// Errors from framing or tree parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwayError {
    /// The buffer did not start with [`MAGIC`].
    BadMagic,
    /// The buffer was shorter than a complete header/payload.
    ShortFrame,
    /// The payload was not the expected JSON shape.
    Json(String),
}

impl std::fmt::Display for SwayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwayError::BadMagic => write!(f, "not an i3-ipc frame (bad magic)"),
            SwayError::ShortFrame => write!(f, "frame shorter than the {HEADER_LEN}-byte header"),
            SwayError::Json(m) => write!(f, "invalid tree JSON: {m}"),
        }
    }
}

impl std::error::Error for SwayError {}

/// Encode one i3 IPC frame: magic, native-endian payload length, native-endian
/// message type, then the raw payload bytes.
pub fn encode(msg_type: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
    out.extend_from_slice(&msg_type.to_ne_bytes());
    out.extend_from_slice(payload);
    out
}

/// True when `header` begins with the i3 IPC [`MAGIC`].
pub fn has_magic(header: &[u8]) -> bool {
    header.len() >= MAGIC.len() && &header[..MAGIC.len()] == MAGIC
}

/// Decode the fixed 14-byte header into `(message_type, payload_length)`.
///
/// The i3 protocol carries both integers in the host's **native** byte order,
/// so this must not be normalised to little- or big-endian.
pub fn decode_header(header: &[u8; HEADER_LEN]) -> (u32, u32) {
    let len = u32::from_ne_bytes([header[6], header[7], header[8], header[9]]);
    let typ = u32::from_ne_bytes([header[10], header[11], header[12], header[13]]);
    (typ, len)
}

/// Parse a complete frame into `(message_type, payload)`, validating the magic
/// and that the whole advertised payload is present.
pub fn parse_frame(buf: &[u8]) -> Result<(u32, &[u8]), SwayError> {
    if buf.len() < HEADER_LEN {
        return Err(SwayError::ShortFrame);
    }
    if !has_magic(buf) {
        return Err(SwayError::BadMagic);
    }
    let header: [u8; HEADER_LEN] = buf[..HEADER_LEN].try_into().expect("checked length");
    let (msg_type, len) = decode_header(&header);
    let end = HEADER_LEN + len as usize;
    if buf.len() < end {
        return Err(SwayError::ShortFrame);
    }
    Ok((msg_type, &buf[HEADER_LEN..end]))
}

/// A compositor snapshot parsed from one `GET_TREE` reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwaySnapshot {
    pub windows: Vec<WindowInfo>,
    pub workspaces: Vec<WorkspaceInfo>,
    /// Geometry of each window (same order as `windows`).
    pub frames: Vec<Frame>,
    /// Usable area of each workspace (excludes layer-shell exclusive zones,
    /// i.e. the taskbar), by workspace index.
    pub work_areas: Vec<(u32, Rect)>,
}

/// A rectangle in output-layout pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Rect {
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
}

/// The subset of an i3/Sway container node Glaze reads. Unknown fields are
/// ignored so a future sway version cannot break parsing.
#[derive(Debug, Deserialize)]
struct RawNode {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    rect: Option<Rect>,
    #[serde(default)]
    deco_rect: Option<Rect>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    node_type: Option<String>,
    /// Wayland `app_id`.
    #[serde(default)]
    app_id: Option<String>,
    /// X11 window id (present for XWayland clients).
    #[serde(default)]
    window: Option<i64>,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    urgent: bool,
    /// X11 properties, only present for XWayland windows.
    #[serde(default)]
    window_properties: Option<WindowProperties>,
    #[serde(default)]
    nodes: Vec<RawNode>,
    #[serde(default)]
    floating_nodes: Vec<RawNode>,
}

/// X11 window properties for an XWayland client in the sway tree.
#[derive(Debug, Deserialize)]
struct WindowProperties {
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    instance: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

/// Parse a `GET_TREE` JSON payload into windows and workspaces.
///
/// Non-toplevel containers (root, outputs, splitting containers, the
/// `floating_con` wrappers) are skipped; only nodes that actually carry a view
/// (a Wayland `app_id`, an X11 `window` id or `window_properties`) become a
/// [`WindowInfo`]. Claims are never invented: an empty tree yields empty lists.
pub fn parse_tree(json: &str) -> Result<SwaySnapshot, SwayError> {
    let root: RawNode = serde_json::from_str(json).map_err(|e| SwayError::Json(e.to_string()))?;
    let mut snapshot = SwaySnapshot::default();
    // Workspaces get a stable, contiguous, zero-based index in tree order. The
    // raw `num` is deliberately *not* used: sway reports `-1` for named
    // workspaces, and the shared model wants a backend-agnostic index.
    let mut next_ws: u32 = 0;
    walk(&root, false, None, &mut next_ws, &mut snapshot);
    Ok(snapshot)
}

/// True when a node is an actual toplevel view rather than a container.
fn is_view(node: &RawNode) -> bool {
    node.app_id.is_some() || node.window.is_some() || node.window_properties.is_some()
}

/// Depth-first walk of the container tree, mapping nodes to the shared model.
fn walk(
    node: &RawNode,
    floating: bool,
    workspace: Option<u32>,
    next_ws: &mut u32,
    out: &mut SwaySnapshot,
) {
    if node.node_type.as_deref() == Some("workspace")
        && node.name.as_deref() == Some(SCRATCH_WORKSPACE)
    {
        // Scratchpad: every window here is minimised, on no workspace.
        let before = out.windows.len();
        for child in node.nodes.iter().chain(node.floating_nodes.iter()) {
            walk(child, true, None, next_ws, out);
        }
        for w in &mut out.windows[before..] {
            w.minimized = true;
        }
        return;
    }
    if node.node_type.as_deref() == Some("workspace") {
        let index = *next_ws;
        *next_ws += 1;
        out.workspaces.push(WorkspaceInfo {
            index,
            name: node.name.clone(),
            focused: node.focused,
            urgent: node.urgent,
        });
        if let Some(r) = node.rect {
            out.work_areas.push((index, r));
        }
        for child in &node.nodes {
            walk(child, false, Some(index), next_ws, out);
        }
        for child in &node.floating_nodes {
            walk(child, true, Some(index), next_ws, out);
        }
        return;
    }

    if is_view(node) {
        out.windows
            .push(window_from_node(node, floating, workspace));
        let rect = node.rect.unwrap_or_default();
        let deco_h = node.deco_rect.map(|d| d.height).unwrap_or(0);
        out.frames.push(Frame {
            id: node.id.map(|i| i.to_string()).unwrap_or_default(),
            x: rect.x,
            y: rect.y - deco_h,
            width: rect.width,
            height: rect.height + deco_h,
            title_height: deco_h,
        });
    }

    for child in &node.nodes {
        walk(child, floating, workspace, next_ws, out);
    }
    // Anything reached through `floating_nodes` is floating by definition.
    for child in &node.floating_nodes {
        walk(child, true, workspace, next_ws, out);
    }
}

/// Map a single view node to a [`WindowInfo`].
fn window_from_node(node: &RawNode, floating: bool, workspace: Option<u32>) -> WindowInfo {
    let props = node.window_properties.as_ref();
    // Wayland `app_id` first; otherwise the X11 `WM_CLASS` (class, then instance).
    let app_id = node.app_id.clone().or_else(|| {
        props
            .and_then(|p| p.class.clone())
            .or_else(|| props.and_then(|p| p.instance.clone()))
    });
    let title = node
        .name
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| props.and_then(|p| p.title.clone()))
        .unwrap_or_default();
    let id = node
        .id
        .map(|i| i.to_string())
        .unwrap_or_else(|| format!("sway:{title}"));

    WindowInfo {
        id,
        app_id,
        title,
        workspace,
        kind: WindowKind::Normal,
        focused: node.focused,
        // Minimised = in the scratchpad (set by the scratchpad branch of
        // `walk`). A window on a hidden workspace is not minimised.
        minimized: false,
        floating: floating || node.node_type.as_deref() == Some("floating_con"),
    }
}

/// Build the `RUN_COMMAND` payload that focuses a window by con id.
pub fn focus_command(id: &str) -> String {
    format!("[con_id={id}] focus")
}

/// Build the `RUN_COMMAND` payload that politely closes a window by con id.
pub fn close_command(id: &str) -> String {
    format!("[con_id={id}] kill")
}

/// Maximise a floating window to the work area (the taskbar stays visible).
/// `area` is the workspace's usable rect; `title_h` the title bar height.
pub fn maximize_command(id: &str, area: Rect, _title_h: i32) -> String {
    // `resize set` sizes the whole container (title bar included) and
    // `move position` places its top-left corner (verified on sway 1.10.1).
    format!(
        "[con_id={id}] floating enable, resize set {} {}, move position {} {}",
        area.width, area.height, area.x, area.y
    )
}

/// Fit a new floating window into the work area: Sway's only size clamp is
/// the output (`floating_maximum_size 0 x 0` in Glaze's sway.conf, so users
/// can resize windows up to the screen), so a window that asks for more than
/// the area above the taskbar is shrunk and centred. `None` when it fits.
pub fn fit_command(id: &str, f: &Frame, area: Rect) -> Option<String> {
    let fits = f.x >= area.x
        && f.y >= area.y
        && f.x + f.width <= area.x + area.width
        && f.y + f.height <= area.y + area.height;
    if fits {
        return None;
    }
    let w = f.width.min(area.width);
    let h = f.height.min(area.height);
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Some(format!(
        "[con_id={id}] resize set {w} {h}, move position {x} {y}"
    ))
}

pub fn unmaximize_command(id: &str, f: &Frame) -> String {
    format!(
        "[con_id={id}] resize set {} {}, move position {} {}",
        f.width, f.height, f.x, f.y
    )
}

/// Sway `client.*` colour commands for the Glaze tokens.
pub fn frame_color_command(c: &FrameColors) -> String {
    // Colours only: border widths, padding and the title font belong to
    // sway.conf (6 px borders double as resize hitboxes).
    format!(
        "client.focused {fb} {fbg} {ft} {fb} {ff}; \
         client.focused_inactive {ob} {obg} {ot} {obg} {of}; \
         client.unfocused {ob} {obg} {ot} {obg} {of}; \
         client.urgent {fb} {ubg} {ft} {ubg} {ubg}; \
         client.background {of}",
        fb = c.focused_border,
        fbg = c.focused_bg,
        ft = c.focused_text,
        ob = c.other_border,
        obg = c.other_bg,
        ot = c.other_text,
        ubg = c.urgent_bg,
        ff = c.focused_frame,
        of = c.other_frame,
    )
}

/// Minimise: move the window to the scratchpad (verified on sway 1.10.1).
pub fn minimize_command(id: &str) -> String {
    format!("[con_id={id}] move scratchpad")
}

/// Restore a scratchpad window onto the current workspace in `mode`. The
/// `floating disable` takes it out of the scratchpad for good; in float mode
/// `floating enable` then puts it back with its saved floating geometry.
pub fn restore_command(id: &str, mode: LayoutMode) -> String {
    match mode {
        LayoutMode::Float => {
            format!("[con_id={id}] scratchpad show, floating disable, floating enable, focus")
        }
        LayoutMode::Tile => format!("[con_id={id}] scratchpad show, floating disable, focus"),
    }
}

/// One `RUN_COMMAND` payload that sets `mode` on each window id.
pub fn layout_command(ids: &[String], mode: LayoutMode) -> Option<String> {
    let verb = match mode {
        LayoutMode::Float => "floating enable",
        LayoutMode::Tile => "floating disable",
    };
    (!ids.is_empty()).then(|| {
        ids.iter()
            .map(|id| format!("[con_id={id}] {verb}"))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

/// Parse one event frame (type + JSON payload) into a [`BackendEvent`].
pub fn parse_event(msg_type: u32, payload: &str) -> BackendEvent {
    #[derive(Deserialize)]
    struct Ev {
        #[serde(default)]
        change: String,
        #[serde(default)]
        container: Option<RawId>,
    }
    #[derive(Deserialize)]
    struct RawId {
        #[serde(default)]
        id: Option<i64>,
    }
    let ev: Option<Ev> = serde_json::from_str(payload).ok();
    match (msg_type, ev) {
        (EVENT_WINDOW, Some(ev)) => BackendEvent::Window {
            change: ev.change,
            id: ev.container.and_then(|c| c.id).map(|i| i.to_string()),
        },
        (EVENT_WORKSPACE, Some(ev)) => BackendEvent::Workspace { change: ev.change },
        _ => BackendEvent::Other,
    }
}

/// Mode a workspace is in, judged from its live windows: tile only when it
/// has visible windows and none of them floats.
pub fn infer_layout(windows: &[WindowInfo], workspace: u32) -> LayoutMode {
    let mut any = false;
    for w in windows
        .iter()
        .filter(|w| w.workspace == Some(workspace) && !w.minimized)
    {
        any = true;
        if w.floating {
            return LayoutMode::Float;
        }
    }
    if any {
        LayoutMode::Tile
    } else {
        LayoutMode::Float
    }
}

/// Incremental frame splitter for the event socket.
#[derive(Debug, Default)]
pub struct FrameBuffer {
    buf: Vec<u8>,
}

impl FrameBuffer {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pop every complete frame as `(type, payload)`.
    pub fn frames(&mut self) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        loop {
            match parse_frame(&self.buf) {
                Ok((t, payload)) => {
                    let used = HEADER_LEN + payload.len();
                    out.push((t, String::from_utf8_lossy(payload).into_owned()));
                    self.buf.drain(..used);
                }
                Err(SwayError::BadMagic) => {
                    // Resynchronise: drop garbage up to the next magic.
                    let skip = self.buf[1..]
                        .windows(MAGIC.len())
                        .position(|w| w == MAGIC)
                        .map(|p| p + 1)
                        .unwrap_or(self.buf.len());
                    self.buf.drain(..skip);
                }
                Err(_) => break,
            }
        }
        out
    }
}

/// Internal mutable state of a [`SwayBackend`], shared behind a `RefCell` so the
/// `Backend` trait can stay `&self` (the IPC server holds boxed backends behind
/// its own mutex).
#[derive(Default)]
struct SwayInner {
    connected: bool,
    snapshot: SwaySnapshot,
    transport: Option<SwayTransport>,
    /// Socket path, kept to open the event stream.
    socket: Option<std::path::PathBuf>,
    /// Geometry before maximise, by window id.
    restore: HashMap<String, Frame>,
    /// Explicit float/tile choices by workspace name (`None` key = global).
    layouts: HashMap<String, LayoutMode>,
    global_layout: Option<LayoutMode>,
}

/// A compositor state backend backed by the Sway/i3 IPC socket.
///
/// The portable parts — framing, parsing and command construction — are always
/// available. The socket transport is compiled only on Unix; on other hosts the
/// backend reports an honest [`BackendError::Unsupported`] instead of pretending
/// to be connected.
pub struct SwayBackend {
    inner: RefCell<SwayInner>,
}

impl Default for SwayBackend {
    fn default() -> Self {
        Self::detached()
    }
}

impl SwayBackend {
    /// A backend with no transport. `is_connected()` is `false` and every query
    /// returns [`BackendError::Unsupported`] (Windows) or
    /// [`BackendError::NotConnected`] (Unix, no socket yet).
    pub fn detached() -> Self {
        SwayBackend {
            inner: RefCell::new(SwayInner::default()),
        }
    }

    /// A backend seeded with an already-parsed snapshot (offline use and tests).
    /// This is real parsed data, never fabricated.
    pub fn with_snapshot(snapshot: SwaySnapshot) -> Self {
        SwayBackend {
            inner: RefCell::new(SwayInner {
                connected: true,
                snapshot,
                ..SwayInner::default()
            }),
        }
    }

    /// Connect to the Sway IPC socket at `socket`.
    #[cfg(unix)]
    pub fn connect(socket: impl Into<std::path::PathBuf>) -> Result<Self, BackendError> {
        let socket = socket.into();
        let transport = SwayTransport::connect(&socket)?;
        Ok(SwayBackend {
            inner: RefCell::new(SwayInner {
                connected: true,
                transport: Some(transport),
                socket: Some(socket),
                ..SwayInner::default()
            }),
        })
    }

    /// The effective layout mode of a workspace (explicit choice, else the
    /// global choice, else inferred from its windows).
    fn mode_for(inner: &SwayInner, ws_index: u32) -> LayoutMode {
        let name = inner
            .snapshot
            .workspaces
            .iter()
            .find(|w| w.index == ws_index)
            .and_then(|w| w.name.clone())
            .unwrap_or_default();
        inner
            .layouts
            .get(&name)
            .copied()
            .or(inner.global_layout)
            .unwrap_or_else(|| infer_layout(&inner.snapshot.windows, ws_index))
    }

    fn focused_ws(inner: &SwayInner) -> Option<(u32, String)> {
        // The focused workspace is the one holding the focused window, else
        // the workspace node flagged focused.
        inner
            .snapshot
            .windows
            .iter()
            .find(|w| w.focused)
            .and_then(|w| w.workspace)
            .or_else(|| {
                inner
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.focused)
                    .map(|w| w.index)
            })
            .or_else(|| inner.snapshot.workspaces.first().map(|w| w.index))
            .map(|i| {
                let name = inner
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.index == i)
                    .and_then(|w| w.name.clone())
                    .unwrap_or_default();
                (i, name)
            })
    }

    /// Connect using `$SWAYSOCK` (the socket sway advertises in the session
    /// environment).
    #[cfg(unix)]
    pub fn connect_from_env() -> Result<Self, BackendError> {
        let path = std::env::var_os("SWAYSOCK").ok_or_else(|| {
            BackendError::NotConnected("SWAYSOCK is not set; is this a Sway session?".into())
        })?;
        Self::connect(std::path::PathBuf::from(path))
    }

    /// The most recent parsed snapshot (may be empty until [`Self::refresh`]).
    pub fn snapshot(&self) -> SwaySnapshot {
        self.inner.borrow().snapshot.clone()
    }

    /// Fetch and store a fresh `GET_TREE` snapshot.
    pub fn refresh(&self) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::refresh_inner(&mut inner)
    }

    /// One request; on an I/O or framing failure (timeout, desynchronised
    /// stream, sway restarted) reconnect once and retry, so a single bad
    /// reply never leaves the backend wedged.
    #[cfg(unix)]
    fn request(inner: &mut SwayInner, msg: u32, payload: &[u8]) -> Result<String, BackendError> {
        let first = match inner.transport.as_mut() {
            Some(t) => t.request(msg, payload),
            None => Err(BackendError::Io("no transport".into())),
        };
        match first {
            Ok(r) => Ok(r),
            Err(e @ (BackendError::Io(_) | BackendError::Protocol(_))) => {
                let Some(socket) = inner.socket.clone() else {
                    return Err(e);
                };
                eprintln!("[glaze-backend] sway ipc request failed ({e}); reconnecting");
                inner.transport = None;
                let mut t = SwayTransport::connect(&socket)?;
                let r = t.request(msg, payload);
                inner.transport = Some(t);
                r
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(unix)]
    fn refresh_inner(inner: &mut SwayInner) -> Result<(), BackendError> {
        if inner.transport.is_none() && inner.socket.is_none() {
            return Err(Self::unavailable());
        }
        let json = Self::request(inner, MSG_GET_TREE, b"")?;
        inner.snapshot = parse_tree(&json).map_err(|e| BackendError::Protocol(e.to_string()))?;
        inner.connected = true;
        Ok(())
    }

    #[cfg(not(unix))]
    fn refresh_inner(_inner: &mut SwayInner) -> Result<(), BackendError> {
        Err(Self::unavailable())
    }

    /// Send a `RUN_COMMAND` payload over the transport.
    #[cfg(unix)]
    fn run_command(inner: &mut SwayInner, command: &str) -> Result<(), BackendError> {
        if inner.transport.is_none() && inner.socket.is_none() {
            return Err(Self::unavailable());
        }
        Self::request(inner, MSG_RUN_COMMAND, command.as_bytes())?;
        Ok(())
    }

    #[cfg(not(unix))]
    fn run_command(_inner: &mut SwayInner, _command: &str) -> Result<(), BackendError> {
        Err(Self::unavailable())
    }

    #[cfg(unix)]
    fn unavailable() -> BackendError {
        BackendError::NotConnected("Sway IPC backend is not connected".into())
    }

    #[cfg(not(unix))]
    fn unavailable() -> BackendError {
        BackendError::Unsupported("Sway IPC requires a Unix-domain socket")
    }

    /// Make sure the snapshot is fresh when a live transport is attached.
    fn ensure_fresh(inner: &mut SwayInner) -> Result<(), BackendError> {
        if inner.transport.is_some() || inner.socket.is_some() {
            Self::refresh_inner(inner)?;
        }
        if inner.connected {
            Ok(())
        } else {
            Err(Self::unavailable())
        }
    }
}

impl Backend for SwayBackend {
    fn name(&self) -> &'static str {
        "sway-ipc"
    }

    fn is_connected(&self) -> bool {
        self.inner.borrow().connected
    }

    fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        Ok(inner.snapshot.windows.clone())
    }

    fn workspaces(&self) -> Result<Vec<WorkspaceInfo>, BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        Ok(inner.snapshot.workspaces.clone())
    }

    fn activate_window(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::run_command(&mut inner, &focus_command(id))
    }

    fn close_window(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::run_command(&mut inner, &close_command(id))
    }

    fn refresh(&self) -> Result<(), BackendError> {
        SwayBackend::refresh(self)
    }

    fn frames(&self) -> Result<Vec<Frame>, BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        let mut v = inner.snapshot.frames.clone();
        for f in v.iter_mut() {
            f.title_height = f.title_height.max(0);
        }
        Ok(v)
    }

    fn set_frame_colors(&self, c: &FrameColors) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::run_command(&mut inner, &frame_color_command(c))
    }

    /// Maximised = the window (title bar included) fills its workspace's
    /// work area, however it got there (taskbar, glaze-snap, Super+Up).
    fn is_maximized(&self, id: &str) -> bool {
        let inner = self.inner.borrow();
        if inner.restore.contains_key(id) {
            return true;
        }
        let snap = &inner.snapshot;
        let Some(pos) = snap.windows.iter().position(|w| w.id == id) else {
            return false;
        };
        let ws = snap.windows[pos].workspace;
        let (Some(f), Some(area)) = (
            snap.frames.get(pos),
            snap.work_areas
                .iter()
                .find(|(i, _)| Some(*i) == ws)
                .map(|(_, r)| *r),
        ) else {
            return false;
        };
        (f.x - area.x).abs() <= 2
            && (f.y - area.y).abs() <= 2
            && (f.width - area.width).abs() <= 4
            && (f.height - area.height).abs() <= 4
    }

    fn toggle_maximize(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        if let Some(prev) = inner.restore.remove(id) {
            return Self::run_command(&mut inner, &unmaximize_command(id, &prev));
        }
        let Some(pos) = inner.snapshot.windows.iter().position(|w| w.id == id) else {
            return Err(BackendError::NotFound(id.to_string()));
        };
        let ws = inner.snapshot.windows[pos].workspace;
        let frame = inner.snapshot.frames.get(pos).cloned().unwrap_or_default();
        let area = inner
            .snapshot
            .work_areas
            .iter()
            .find(|(i, _)| Some(*i) == ws)
            .map(|(_, r)| *r)
            .ok_or_else(|| BackendError::NotFound(format!("workspace of {id}")))?;
        inner.restore.insert(id.to_string(), frame.clone());
        let cmd = maximize_command(id, area, frame.title_height.max(0));
        Self::run_command(&mut inner, &cmd)
    }

    fn minimize_window(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::run_command(&mut inner, &minimize_command(id))
    }

    fn move_to_workspace(&self, id: &str, number: u32) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::run_command(
            &mut inner,
            &format!("[con_id={id}] move container to workspace number {number}"),
        )
    }

    fn restore_window(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        let mode = Self::focused_ws(&inner)
            .map(|(i, _)| Self::mode_for(&inner, i))
            .unwrap_or_default();
        Self::run_command(&mut inner, &restore_command(id, mode))
    }

    fn layout_mode(&self) -> Result<LayoutMode, BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        Ok(Self::focused_ws(&inner)
            .map(|(i, _)| Self::mode_for(&inner, i))
            .unwrap_or_default())
    }

    fn set_layout_mode(&self, mode: LayoutMode, scope: LayoutScope) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        let targets: Vec<String> = match scope {
            LayoutScope::FocusedWorkspace => {
                let Some((index, name)) = Self::focused_ws(&inner) else {
                    return Ok(());
                };
                inner.layouts.insert(name, mode);
                inner
                    .snapshot
                    .windows
                    .iter()
                    .filter(|w| w.workspace == Some(index) && !w.minimized)
                    .map(|w| w.id.clone())
                    .collect()
            }
            LayoutScope::AllWorkspaces => {
                inner.layouts.clear();
                inner.global_layout = Some(mode);
                inner
                    .snapshot
                    .windows
                    .iter()
                    .filter(|w| !w.minimized)
                    .map(|w| w.id.clone())
                    .collect()
            }
        };
        match layout_command(&targets, mode) {
            Some(cmd) => Self::run_command(&mut inner, &cmd),
            None => Ok(()),
        }
    }

    fn window_opened(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.inner.borrow_mut();
        Self::ensure_fresh(&mut inner)?;
        let Some(win) = inner.snapshot.windows.iter().find(|w| w.id == id) else {
            return Ok(());
        };
        let Some(ws) = win.workspace else {
            return Ok(());
        };
        // sway.conf floats every new window; only tile workspaces need a fix.
        if Self::mode_for(&inner, ws) == LayoutMode::Tile && win.floating {
            let cmd = layout_command(&[id.to_string()], LayoutMode::Tile).unwrap_or_default();
            Self::run_command(&mut inner, &cmd)?;
        } else if win.floating {
            let pos = inner.snapshot.windows.iter().position(|w| w.id == id);
            let frame = pos.and_then(|p| inner.snapshot.frames.get(p).cloned());
            let area = inner
                .snapshot
                .work_areas
                .iter()
                .find(|(i, _)| *i == ws)
                .map(|(_, r)| *r);
            if let (Some(f), Some(a)) = (frame, area) {
                if let Some(cmd) = fit_command(id, &f, a) {
                    Self::run_command(&mut inner, &cmd)?;
                }
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    fn subscribe(&self) -> Result<Box<dyn EventSource>, BackendError> {
        let inner = self.inner.borrow();
        let Some(socket) = inner.socket.clone() else {
            return Err(Self::unavailable());
        };
        Ok(Box::new(crate::sway_transport::SwayEvents::connect(
            &socket,
        )?))
    }
}

#[cfg(unix)]
pub(crate) use crate::sway_transport::SwayTransport;

/// Non-Unix placeholder so the `SwayBackend` field type is always nameable.
/// It is never constructed, and `refresh_inner` returns `Unsupported`.
#[cfg(not(unix))]
pub(crate) struct SwayTransport;

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TREE: &str = r#"{
      "id": 1, "name": "root", "type": "root", "focused": false, "urgent": false,
      "nodes": [
        {
          "id": 2, "name": "eDP-1", "type": "output", "focused": false,
          "nodes": [
            {
              "id": 3, "name": "1", "type": "workspace", "num": 1, "focused": true,
              "urgent": false,
              "nodes": [
                { "id": 4, "name": "Firefox", "type": "con", "app_id": "firefox",
                  "focused": true, "visible": true },
                { "id": 5, "name": null, "type": "con", "layout": "splith",
                  "focused": false,
                  "nodes": [
                    { "id": 6, "name": "Terminal", "type": "con", "app_id": "foot",
                      "focused": false, "visible": true },
                    { "id": 7, "name": "Editor", "type": "con", "app_id": "gedit",
                      "focused": false, "visible": true }
                  ] }
              ],
              "floating_nodes": [
                { "id": 8, "name": null, "type": "floating_con", "focused": false,
                  "nodes": [
                    { "id": 9, "name": "GIMP", "type": "con",
                      "window_properties": {
                        "class": "Gimp", "instance": "gimp", "title": "GIMP" },
                      "focused": false, "visible": false }
                  ] }
              ]
            },
            {
              "id": 10, "name": "2", "type": "workspace", "num": 2,
              "focused": false, "urgent": true, "nodes": [], "floating_nodes": []
            },
            {
              "id": 11, "name": "web", "type": "workspace", "num": -1,
              "focused": false, "urgent": false, "nodes": [], "floating_nodes": []
            }
          ]
        }
      ]
    }"#;

    #[test]
    fn frame_round_trips_through_encode_and_parse() {
        let frame = encode(MSG_GET_TREE, b"{}");
        assert_eq!(frame.len(), HEADER_LEN + 2);
        assert!(has_magic(&frame));
        let header: [u8; HEADER_LEN] = frame[..HEADER_LEN].try_into().unwrap();
        assert_eq!(decode_header(&header), (MSG_GET_TREE, 2));
        let (msg_type, payload) = parse_frame(&frame).unwrap();
        assert_eq!(msg_type, MSG_GET_TREE);
        assert_eq!(payload, b"{}");
    }

    #[test]
    fn decode_header_uses_native_endian() {
        // A 258-byte payload of type 4, encoded the way sway would on x86_64/arm64.
        let frame = encode(4, &vec![0u8; 258]);
        let header: [u8; HEADER_LEN] = frame[..HEADER_LEN].try_into().unwrap();
        assert_eq!(decode_header(&header), (4, 258));
    }

    #[test]
    fn rejects_bad_magic_and_short_frames() {
        assert_eq!(parse_frame(b"nope"), Err(SwayError::ShortFrame));
        let mut frame = encode(4, b"[]");
        frame[0] = b'x';
        assert_eq!(parse_frame(&frame), Err(SwayError::BadMagic));
        // Header present but payload truncated.
        let frame = encode(4, b"[1,2,3]");
        assert_eq!(
            parse_frame(&frame[..HEADER_LEN + 3]),
            Err(SwayError::ShortFrame)
        );
    }

    #[test]
    fn parses_windows_workspaces_and_skips_containers() {
        let snap = parse_tree(SAMPLE_TREE).unwrap();

        // Workspaces: "1" (focused), "2" (urgent), "web" (named), zero-based order.
        assert_eq!(snap.workspaces.len(), 3);
        assert_eq!(snap.workspaces[0].index, 0);
        assert_eq!(snap.workspaces[0].name.as_deref(), Some("1"));
        assert!(snap.workspaces[0].focused);
        assert!(!snap.workspaces[0].urgent);
        assert_eq!(snap.workspaces[1].index, 1);
        assert!(snap.workspaces[1].urgent);
        assert_eq!(snap.workspaces[2].index, 2);
        assert_eq!(snap.workspaces[2].name.as_deref(), Some("web"));

        // Only the four views become windows; the split and floating containers
        // (ids 5 and 8) are skipped.
        let titles: Vec<&str> = snap.windows.iter().map(|w| w.title.as_str()).collect();
        assert_eq!(titles, vec!["Firefox", "Terminal", "Editor", "GIMP"]);
        assert_eq!(snap.windows.len(), 4);
    }

    #[test]
    fn maps_focus_workspace_and_floating() {
        let snap = parse_tree(SAMPLE_TREE).unwrap();
        let firefox = snap.windows.iter().find(|w| w.title == "Firefox").unwrap();
        assert_eq!(firefox.app_id.as_deref(), Some("firefox"));
        assert!(firefox.focused);
        assert!(!firefox.floating);
        assert_eq!(firefox.workspace, Some(0));
        assert_eq!(firefox.id, "4");

        let gimp = snap.windows.iter().find(|w| w.title == "GIMP").unwrap();
        assert!(
            gimp.floating,
            "window inside floating_nodes must be floating"
        );
        assert!(
            !gimp.minimized,
            "hidden is not minimised; only the scratchpad is"
        );
        assert_eq!(gimp.workspace, Some(0));
    }

    #[test]
    fn x11_node_without_app_id_uses_window_class() {
        let snap = parse_tree(SAMPLE_TREE).unwrap();
        let gimp = snap.windows.iter().find(|w| w.title == "GIMP").unwrap();
        // No `app_id`: the X11 WM_CLASS "Gimp" is used.
        assert_eq!(gimp.app_id.as_deref(), Some("Gimp"));
    }

    #[test]
    fn empty_tree_yields_no_fabricated_state() {
        let snap = parse_tree(r#"{"id":1,"type":"root","nodes":[]}"#).unwrap();
        assert!(snap.windows.is_empty());
        assert!(snap.workspaces.is_empty());
    }

    const SCRATCH_TREE: &str = r#"{
      "id": 1, "type": "root", "nodes": [
        { "id": 2147483647, "name": "__i3", "type": "output", "nodes": [
          { "id": 2147483646, "name": "__i3_scratch", "type": "workspace", "nodes": [],
            "floating_nodes": [
              { "id": 5, "name": "foot", "type": "floating_con", "app_id": "foot",
                "visible": false, "focused": false }
            ] } ] },
        { "id": 3, "name": "Virtual-1", "type": "output", "nodes": [
          { "id": 4, "name": "1", "type": "workspace", "focused": false, "nodes": [
              { "id": 7, "name": "Editor", "type": "con", "app_id": "gedit",
                "visible": true, "focused": true } ],
            "floating_nodes": [
              { "id": 8, "name": "foot 2", "type": "floating_con", "app_id": "foot",
                "visible": true, "focused": false } ] } ] }
      ]
    }"#;

    #[test]
    fn scratchpad_windows_are_minimised_and_have_no_workspace() {
        let snap = parse_tree(SCRATCH_TREE).unwrap();
        assert_eq!(snap.workspaces.len(), 1, "scratchpad is not a workspace");
        let min = snap.windows.iter().find(|w| w.id == "5").unwrap();
        assert!(min.minimized);
        assert_eq!(min.workspace, None);
        let other = snap.windows.iter().find(|w| w.id == "8").unwrap();
        assert!(!other.minimized && other.floating);
        assert_eq!(infer_layout(&snap.windows, 0), LayoutMode::Float);
    }

    #[test]
    fn infer_layout_tiles_only_all_tiled_workspaces() {
        let mut snap = parse_tree(SCRATCH_TREE).unwrap();
        snap.windows.retain(|w| w.id != "8");
        assert_eq!(infer_layout(&snap.windows, 0), LayoutMode::Tile);
        assert_eq!(infer_layout(&[], 0), LayoutMode::Float);
    }

    #[test]
    fn minimise_restore_and_layout_commands() {
        assert_eq!(minimize_command("5"), "[con_id=5] move scratchpad");
        assert_eq!(
            restore_command("5", LayoutMode::Float),
            "[con_id=5] scratchpad show, floating disable, floating enable, focus"
        );
        assert_eq!(
            restore_command("5", LayoutMode::Tile),
            "[con_id=5] scratchpad show, floating disable, focus"
        );
        assert_eq!(
            layout_command(&["1".into(), "2".into()], LayoutMode::Tile).unwrap(),
            "[con_id=1] floating disable; [con_id=2] floating disable"
        );
        assert_eq!(layout_command(&[], LayoutMode::Float), None);
    }

    #[test]
    fn frames_include_the_title_bar_and_work_areas_parse() {
        let tree = r#"{"id":1,"type":"root","nodes":[{"id":3,"type":"output","name":"V","nodes":[
          {"id":4,"type":"workspace","name":"1","rect":{"x":0,"y":0,"width":1280,"height":760},
           "nodes":[],"floating_nodes":[
             {"id":7,"type":"floating_con","app_id":"foot","name":"foot",
              "rect":{"x":203,"y":104,"width":700,"height":496},
              "deco_rect":{"x":203,"y":79,"width":700,"height":25}}]}]}]}"#;
        let snap = parse_tree(tree).unwrap();
        assert_eq!(
            snap.frames[0],
            Frame {
                id: "7".into(),
                x: 203,
                y: 79,
                width: 700,
                height: 521,
                title_height: 25
            }
        );
        assert_eq!(
            snap.work_areas,
            vec![(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 760
                }
            )]
        );
        assert!(maximize_command("7", snap.work_areas[0].1, 25)
            .contains("[con_id=7] floating enable, resize set 1280 760, move position 0 0"));
        let big = Frame {
            id: "7".into(),
            x: 0,
            y: 0,
            width: 1400,
            height: 900,
            title_height: 25,
        };
        assert_eq!(
            fit_command("7", &big, snap.work_areas[0].1).as_deref(),
            Some("[con_id=7] resize set 1280 760, move position 0 0")
        );
        assert!(fit_command("7", &snap.frames[0], snap.work_areas[0].1).is_none());
        assert_eq!(
            unmaximize_command("7", &snap.frames[0]),
            "[con_id=7] resize set 700 521, move position 203 79"
        );
    }

    #[test]
    fn events_parse_and_frames_split() {
        let ev = parse_event(
            EVENT_WINDOW,
            r#"{"change":"new","container":{"id":42,"app_id":"foot"}}"#,
        );
        assert_eq!(
            ev,
            BackendEvent::Window {
                change: "new".into(),
                id: Some("42".into())
            }
        );
        assert_eq!(
            parse_event(EVENT_WORKSPACE, r#"{"change":"focus"}"#),
            BackendEvent::Workspace {
                change: "focus".into()
            }
        );
        assert_eq!(parse_event(EVENT_BIT | 7, "{}"), BackendEvent::Other);

        let mut fb = FrameBuffer::default();
        let a = encode(EVENT_WINDOW, br#"{"change":"focus"}"#);
        let b = encode(EVENT_WORKSPACE, br#"{"change":"init"}"#);
        let mut all = a.clone();
        all.extend_from_slice(&b);
        fb.push(&all[..5]);
        assert!(fb.frames().is_empty(), "partial header");
        fb.push(&all[5..a.len() + 3]);
        assert_eq!(fb.frames().len(), 1);
        fb.push(&all[a.len() + 3..]);
        let rest = fb.frames();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].0, EVENT_WORKSPACE);
    }

    #[test]
    fn command_builders_target_con_id() {
        assert_eq!(focus_command("42"), "[con_id=42] focus");
        assert_eq!(close_command("42"), "[con_id=42] kill");
    }

    #[test]
    fn detached_backend_is_honest_about_no_transport() {
        let backend = SwayBackend::detached();
        assert!(!backend.is_connected());
        assert_eq!(backend.name(), "sway-ipc");
        let err = backend.windows().unwrap_err();
        #[cfg(not(unix))]
        assert!(matches!(err, BackendError::Unsupported(_)));
        #[cfg(unix)]
        assert!(matches!(err, BackendError::NotConnected(_)));
    }

    #[test]
    fn snapshot_backend_returns_parsed_windows() {
        let snap = parse_tree(SAMPLE_TREE).unwrap();
        let backend = SwayBackend::with_snapshot(snap);
        assert!(backend.is_connected());
        assert_eq!(backend.windows().unwrap().len(), 4);
        assert_eq!(backend.workspaces().unwrap().len(), 3);
    }
}
