//! IPC request dispatch (portable) plus the Unix-domain-socket transport.
//!
//! Newline-delimited JSON, one [`Request`] per line, one [`Response`] per line
//! (see [`glaze_common::ipc`]). The transport is a `SOCK_STREAM` socket at
//! `$XDG_RUNTIME_DIR/glaze/control.sock` with `0600` permissions so only the
//! owning user can talk to it.
//!
//! The request *handling* ([`ServiceState`], [`IpcServer::handle`]) is portable
//! and unit-tested on every host, including the "no backend attached" and
//! "backend real data" cases. Only [`IpcServer::run`] and [`request_once`] need
//! the Unix socket API.

use std::sync::{Arc, Mutex};

use glaze_backend::{Backend, BackendError};
use glaze_common::ipc::{ErrorCode, Request, Response, PROTOCOL_VERSION};
use glaze_common::model::ShellStatus;

use glaze_common::model::{
    NotificationInfo, NotificationUrgency, TrayCategoryInfo, TrayItemInfo, TrayStatusInfo,
};

use crate::notifications::{Notification, NotificationServer, Urgency};
use crate::registry::AppRegistry;
use crate::search::search_apps;
use crate::tray::{TrayCategory, TrayItem, TrayStatus};
use crate::tray_watcher::{SniSignal, TrayWatcher, WatcherOutcome};

/// Shared server state.
pub struct ServiceState {
    pub registry: AppRegistry,
    pub default_search_limit: usize,
    /// Optional compositor state backend. When `None`, window/workspace queries
    /// return honest empty results and focus/close return
    /// [`ErrorCode::BackendUnavailable`] — never fabricated state.
    pub backend: Option<Box<dyn Backend>>,
    /// The StatusNotifier watcher: it owns the live tray items, and the D-Bus
    /// adapter feeds it [`SniSignal`]s (empty until items register).
    pub tray: TrayWatcher,
    /// The notification history and read state.
    pub notifications: NotificationServer,
    /// Whether the notification server is in Do-Not-Disturb mode.
    pub dnd: bool,
}

impl ServiceState {
    /// State with no compositor backend attached.
    pub fn new(registry: AppRegistry) -> Self {
        ServiceState {
            registry,
            default_search_limit: 8,
            backend: None,
            tray: TrayWatcher::new(),
            notifications: NotificationServer::default(),
            dnd: false,
        }
    }

    /// State with a live compositor backend attached.
    pub fn with_backend(registry: AppRegistry, backend: Box<dyn Backend>) -> Self {
        ServiceState {
            backend: Some(backend),
            ..ServiceState::new(registry)
        }
    }

    /// Mutable access to the tray watcher (the D-Bus adapter feeds it signals).
    pub fn tray_mut(&mut self) -> &mut TrayWatcher {
        &mut self.tray
    }

    /// Feed one StatusNotifier signal into the watcher and report what changed.
    pub fn apply_tray_signal(&mut self, signal: SniSignal) -> WatcherOutcome {
        self.tray.apply(signal)
    }

    /// Mutable access to the notification server (the D-Bus adapter feeds this).
    pub fn notifications_mut(&mut self) -> &mut NotificationServer {
        &mut self.notifications
    }
}

/// Map the service's tray status onto the stable wire type.
fn wire_tray_status(status: TrayStatus) -> TrayStatusInfo {
    match status {
        TrayStatus::Passive => TrayStatusInfo::Passive,
        TrayStatus::Active => TrayStatusInfo::Active,
        TrayStatus::NeedsAttention => TrayStatusInfo::NeedsAttention,
    }
}

/// Map the service's tray category onto the stable wire type.
fn wire_tray_category(category: TrayCategory) -> TrayCategoryInfo {
    match category {
        TrayCategory::ApplicationStatus => TrayCategoryInfo::ApplicationStatus,
        TrayCategory::Communications => TrayCategoryInfo::Communications,
        TrayCategory::SystemServices => TrayCategoryInfo::SystemServices,
        TrayCategory::Hardware => TrayCategoryInfo::Hardware,
        TrayCategory::Other => TrayCategoryInfo::Other,
    }
}

/// Map the service's urgency onto the stable wire type.
fn wire_urgency(urgency: Urgency) -> NotificationUrgency {
    match urgency {
        Urgency::Low => NotificationUrgency::Low,
        Urgency::Normal => NotificationUrgency::Normal,
        Urgency::Critical => NotificationUrgency::Critical,
    }
}

/// One tray item as the wire type.
fn wire_tray_item(item: &TrayItem) -> TrayItemInfo {
    TrayItemInfo {
        id: item.id.clone(),
        title: item.title.clone(),
        icon: item.icon.clone(),
        status: wire_tray_status(item.status),
        category: wire_tray_category(item.category),
    }
}

/// One notification as the wire type.
fn wire_notification(notification: &Notification) -> NotificationInfo {
    NotificationInfo {
        id: notification.id,
        app_name: notification.app_name.clone(),
        summary: notification.summary.clone(),
        body: notification.body.clone(),
        urgency: wire_urgency(notification.urgency),
        received_at_unix: notification.received_at_unix,
    }
}

/// Map a [`BackendError`] onto a stable [`Response::Error`].
fn backend_error_response(err: BackendError) -> Response {
    let code = match err {
        BackendError::Unsupported(_) | BackendError::NotConnected(_) => {
            ErrorCode::BackendUnavailable
        }
        BackendError::NotFound(_) => ErrorCode::NotFound,
        BackendError::Protocol(_) | BackendError::Io(_) => ErrorCode::Internal,
    };
    Response::error(code, err.to_string())
}

/// A running IPC server.
pub struct IpcServer {
    state: Arc<Mutex<ServiceState>>,
}

impl IpcServer {
    /// Build a server for a registry with no compositor backend.
    pub fn new(registry: AppRegistry) -> Self {
        IpcServer {
            state: Arc::new(Mutex::new(ServiceState::new(registry))),
        }
    }

    /// Build a server with a live compositor backend attached.
    pub fn with_backend(registry: AppRegistry, backend: Box<dyn Backend>) -> Self {
        IpcServer {
            state: Arc::new(Mutex::new(ServiceState::with_backend(registry, backend))),
        }
    }

    /// Handle a single request against the current state, producing a response.
    ///
    /// Takes `&mut ServiceState` so [`Request::Reload`] can genuinely rebuild the
    /// registry rather than pretending to.
    pub fn handle(state: &mut ServiceState, request: &Request) -> Response {
        match request {
            Request::Hello { protocol, .. } => {
                if *protocol == PROTOCOL_VERSION {
                    Response::Hello {
                        protocol: PROTOCOL_VERSION,
                        service_version: glaze_common::VERSION.to_string(),
                    }
                } else {
                    Response::error(
                        ErrorCode::ProtocolMismatch,
                        format!("protocol {protocol} != {PROTOCOL_VERSION}"),
                    )
                }
            }
            Request::Ping => Response::Pong,
            Request::ListApps => Response::Apps {
                apps: state.registry.apps().to_vec(),
            },
            Request::SearchApps { query, limit } => {
                let limit = if *limit == 0 {
                    state.default_search_limit
                } else {
                    *limit
                };
                let hits = search_apps(state.registry.apps(), query, limit);
                Response::SearchResults { hits }
            }
            Request::LaunchApp { id } => match state.registry.get(id) {
                Some(app) => match crate::launch::launch(app) {
                    Ok(_child) => Response::Ok,
                    Err(e) => Response::error(ErrorCode::LauncherFailed, e.to_string()),
                },
                None => Response::error(ErrorCode::NotFound, format!("no app with id {id}")),
            },
            Request::ListTray => Response::Tray {
                items: state
                    .tray
                    .registry()
                    .items()
                    .into_iter()
                    .map(wire_tray_item)
                    .collect(),
            },
            Request::ListNotifications => Response::Notifications {
                items: state
                    .notifications
                    .history()
                    .iter()
                    .map(wire_notification)
                    .collect(),
                unread_ids: state.notifications.unread_ids(),
                dnd: state.dnd,
            },
            Request::MarkNotificationsRead => {
                state.notifications.mark_all_read();
                Response::Ok
            }
            // Served by glaze-compositor's own control socket (docs/compositor.md).
            Request::SetWindowLayout { .. } => Response::error(
                ErrorCode::BackendUnavailable,
                "window layout is handled by glaze-compositor's control socket",
            ),
            Request::Reload => {
                state.registry = AppRegistry::discover(Default::default());
                Response::Ok
            }
            Request::Status => {
                let (compositor_backend, window_count) = match state.backend.as_ref() {
                    Some(backend) => (
                        backend.name().to_string(),
                        backend.windows().map(|w| w.len()).unwrap_or(0),
                    ),
                    None => ("none".to_string(), 0),
                };
                Response::Status {
                    status: ShellStatus {
                        version: glaze_common::VERSION.to_string(),
                        theme: "clay".into(),
                        scheme: "light".into(),
                        compositor_backend,
                        app_count: state.registry.len(),
                        window_count,
                    },
                }
            }
            Request::ListWindows => match state.backend.as_ref() {
                Some(backend) => match backend.windows() {
                    Ok(windows) => Response::Windows { windows },
                    Err(e) => backend_error_response(e),
                },
                // No backend: an empty list, not a fabricated one.
                None => Response::Windows {
                    windows: Vec::new(),
                },
            },
            Request::ListWorkspaces => match state.backend.as_ref() {
                Some(backend) => match backend.workspaces() {
                    Ok(workspaces) => Response::Workspaces { workspaces },
                    Err(e) => backend_error_response(e),
                },
                None => Response::Workspaces {
                    workspaces: Vec::new(),
                },
            },
            Request::ActivateWindow { id } => match state.backend.as_ref() {
                Some(backend) => match backend.activate_window(id) {
                    Ok(()) => Response::Ok,
                    Err(e) => backend_error_response(e),
                },
                None => Response::error(
                    ErrorCode::BackendUnavailable,
                    "no compositor backend attached",
                ),
            },
            Request::CloseWindow { id } => match state.backend.as_ref() {
                Some(backend) => match backend.close_window(id) {
                    Ok(()) => Response::Ok,
                    Err(e) => backend_error_response(e),
                },
                None => Response::error(
                    ErrorCode::BackendUnavailable,
                    "no compositor backend attached",
                ),
            },
        }
    }

    /// The shared state, for embedding (e.g. to attach a backend at runtime).
    pub fn state(&self) -> Arc<Mutex<ServiceState>> {
        Arc::clone(&self.state)
    }
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    use glaze_common::paths;

    impl IpcServer {
        fn serve_stream(state: &Arc<Mutex<ServiceState>>, stream: UnixStream) {
            let peer = stream.try_clone();
            let reader = match peer {
                Ok(s) => BufReader::new(s),
                Err(_) => return,
            };
            let mut writer = stream;
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let response = match serde_json::from_str::<Request>(&line) {
                    Ok(req) => {
                        let mut guard = match state.lock() {
                            Ok(g) => g,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        Self::handle(&mut guard, &req)
                    }
                    Err(e) => Response::error(ErrorCode::InvalidRequest, e.to_string()),
                };
                let mut out = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                out.push('\n');
                if writer.write_all(out.as_bytes()).is_err() {
                    break;
                }
            }
        }

        /// Bind the socket and serve connections until `shutdown` is set.
        pub fn run(self, shutdown: Arc<AtomicBool>) -> std::io::Result<()> {
            let socket = paths::control_socket();
            if let Some(parent) = socket.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Remove a stale socket from a previous run.
            if socket.exists() {
                let _ = std::fs::remove_file(&socket);
            }
            let listener = UnixListener::bind(&socket)?;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;

            while !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let state = Arc::clone(&self.state);
                        std::thread::spawn(move || Self::serve_stream(&state, stream));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Sleep in poll(2) until a client connects (or 500 ms
                        // pass, to notice `shutdown`): no 20 Hz idle wake-ups.
                        wait_readable(&listener, 500);
                    }
                    Err(_) => break,
                }
            }
            let _ = std::fs::remove_file(&socket);
            Ok(())
        }
    }

    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }

    extern "C" {
        fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
    }

    /// Block until `listener` is readable or `timeout_ms` passes.
    fn wait_readable(listener: &UnixListener, timeout_ms: i32) {
        use std::os::unix::io::AsRawFd;
        let mut pfd = PollFd {
            fd: listener.as_raw_fd(),
            events: 0x1, // POLLIN
            revents: 0,
        };
        // SAFETY: one valid pollfd that lives for the duration of the call.
        unsafe {
            poll(&mut pfd, 1, timeout_ms);
        }
    }

    /// One-shot client: send a request and read a single response line.
    pub fn request_once(socket: &Path, request: &Request) -> std::io::Result<Response> {
        let mut stream = UnixStream::connect(socket)?;
        let mut line = request.to_line().map_err(std::io::Error::other)?;
        stream.write_all(line.as_bytes())?;
        line.clear();
        let mut reader = BufReader::new(stream);
        reader.read_line(&mut line)?;
        serde_json::from_str(line.trim_end())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(unix)]
pub use transport::request_once;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AppRegistry;
    use glaze_common::ipc::{ErrorCode, Request, PROTOCOL_VERSION};
    use glaze_common::model::{WindowInfo, WorkspaceInfo};

    fn state() -> ServiceState {
        ServiceState::new(AppRegistry::default())
    }

    /// A minimal in-memory backend: proves the IPC layer surfaces *real* backend
    /// data (and honest errors) without a compositor.
    struct MockBackend {
        connected: bool,
    }

    impl Backend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
            if self.connected {
                Ok(vec![WindowInfo {
                    id: "1".into(),
                    app_id: Some("firefox".into()),
                    title: "Firefox".into(),
                    workspace: Some(0),
                    kind: Default::default(),
                    focused: true,
                    minimized: false,
                    floating: false,
                }])
            } else {
                Err(BackendError::Unsupported("mock offline"))
            }
        }
        fn workspaces(&self) -> Result<Vec<WorkspaceInfo>, BackendError> {
            if self.connected {
                Ok(vec![WorkspaceInfo {
                    index: 0,
                    name: Some("1".into()),
                    focused: true,
                    urgent: false,
                }])
            } else {
                Err(BackendError::Unsupported("mock offline"))
            }
        }
        fn activate_window(&self, _id: &str) -> Result<(), BackendError> {
            if self.connected {
                Ok(())
            } else {
                Err(BackendError::Unsupported("mock offline"))
            }
        }
        fn close_window(&self, _id: &str) -> Result<(), BackendError> {
            if self.connected {
                Ok(())
            } else {
                Err(BackendError::Unsupported("mock offline"))
            }
        }
    }

    #[test]
    fn hello_negotiates_protocol() {
        let mut st = state();
        let ok = IpcServer::handle(
            &mut st,
            &Request::Hello {
                protocol: PROTOCOL_VERSION,
                client: "test".into(),
            },
        );
        assert!(matches!(ok, Response::Hello { .. }));

        let bad = IpcServer::handle(
            &mut st,
            &Request::Hello {
                protocol: PROTOCOL_VERSION + 99,
                client: "test".into(),
            },
        );
        match bad {
            Response::Error { code, .. } => assert_eq!(code, ErrorCode::ProtocolMismatch),
            other => panic!("expected protocol mismatch, got {other:?}"),
        }
    }

    #[test]
    fn ping_and_list_apps() {
        let mut st = state();
        assert_eq!(IpcServer::handle(&mut st, &Request::Ping), Response::Pong);
        assert!(matches!(
            IpcServer::handle(&mut st, &Request::ListApps),
            Response::Apps { .. }
        ));
    }

    #[test]
    fn launching_unknown_app_is_not_found() {
        let mut st = state();
        let resp = IpcServer::handle(&mut st, &Request::LaunchApp { id: "nope".into() });
        match resp {
            Response::Error { code, .. } => assert_eq!(code, ErrorCode::NotFound),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn windows_without_backend_is_empty_not_faked() {
        let mut st = state();
        match IpcServer::handle(&mut st, &Request::ListWindows) {
            Response::Windows { windows } => assert!(windows.is_empty()),
            other => panic!("expected empty windows, got {other:?}"),
        }
    }

    #[test]
    fn activates_and_closes_require_a_backend() {
        let mut st = state();
        for req in [
            Request::ActivateWindow { id: "1".into() },
            Request::CloseWindow { id: "1".into() },
        ] {
            match IpcServer::handle(&mut st, &req) {
                Response::Error { code, .. } => assert_eq!(code, ErrorCode::BackendUnavailable),
                other => panic!("expected BackendUnavailable, got {other:?}"),
            }
        }
    }

    #[test]
    fn attached_backend_surfaces_real_windows() {
        let mut st = ServiceState::with_backend(
            AppRegistry::default(),
            Box::new(MockBackend { connected: true }),
        );
        match IpcServer::handle(&mut st, &Request::ListWindows) {
            Response::Windows { windows } => {
                assert_eq!(windows.len(), 1);
                assert_eq!(windows[0].title, "Firefox");
                assert!(windows[0].focused);
            }
            other => panic!("expected windows, got {other:?}"),
        }
        match IpcServer::handle(&mut st, &Request::ListWorkspaces) {
            Response::Workspaces { workspaces } => assert_eq!(workspaces.len(), 1),
            other => panic!("expected workspaces, got {other:?}"),
        }
        assert_eq!(
            IpcServer::handle(&mut st, &Request::ActivateWindow { id: "1".into() }),
            Response::Ok
        );
        assert_eq!(
            IpcServer::handle(&mut st, &Request::CloseWindow { id: "1".into() }),
            Response::Ok
        );
    }

    #[test]
    fn offline_backend_reports_backend_unavailable() {
        let mut st = ServiceState::with_backend(
            AppRegistry::default(),
            Box::new(MockBackend { connected: false }),
        );
        for req in [
            Request::ListWindows,
            Request::ActivateWindow { id: "1".into() },
            Request::CloseWindow { id: "1".into() },
        ] {
            match IpcServer::handle(&mut st, &req) {
                Response::Error { code, .. } => assert_eq!(code, ErrorCode::BackendUnavailable),
                other => panic!("expected BackendUnavailable, got {other:?}"),
            }
        }
    }

    #[test]
    fn status_reports_attached_backend() {
        let mut st = ServiceState::with_backend(
            AppRegistry::default(),
            Box::new(MockBackend { connected: true }),
        );
        match IpcServer::handle(&mut st, &Request::Status) {
            Response::Status { status } => {
                assert_eq!(status.compositor_backend, "mock");
                assert_eq!(status.window_count, 1);
            }
            other => panic!("expected status, got {other:?}"),
        }
    }
}
