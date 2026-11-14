//! Portable control-protocol client.
//!
//! The shell talks to the Glaze service over a newline-delimited JSON Unix socket
//! ([`glaze_common::ipc`]). That transport only exists on Unix, but the shell's
//! *logic* around it — which requests it sends, how it reacts to each
//! [`Response`], how it distinguishes a transport failure from a structured
//! service error — should be testable everywhere. So the shell codes against the
//! [`ControlClient`] trait and two implementations:
//!
//! - [`InProcessClient`] answers using [`glaze_service::ipc_server::IpcServer`]
//!   in the same process. It works on **every** host (including Windows), which
//!   is how the shell's request/response handling is unit-tested with no socket.
//! - [`UnixSocketClient`] (`#[cfg(unix)]`) is the real client: one request, one
//!   response line over `$XDG_RUNTIME_DIR/glaze/control.sock`.
//!
//! Error discipline: a [`ClientError::Transport`] means the message never got a
//! complete reply (connection lost, malformed JSON). A service-level failure is
//! *not* a transport error — the call succeeded and the service said "no" — so
//! [`ControlClient::request`] returns it as `Ok(Response::Error { .. })`, exactly
//! as it arrived on the wire. [`ControlClient::request_expect_ok`] is the
//! convenience that turns that into a [`ClientError::Service`].

use std::sync::{Arc, Mutex};

use glaze_backend::Backend;
use glaze_common::ipc::{ErrorCode, Request, Response};
use glaze_service::ipc_server::{IpcServer, ServiceState};
use glaze_service::registry::AppRegistry;

/// Something went wrong talking to the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// The transport failed: no socket, I/O error, or an unreadable reply.
    Transport(String),
    /// A reply arrived but did not fit the protocol.
    Protocol(String),
    /// The service answered with a structured [`Response::Error`].
    Service {
        /// Stable error code from the service.
        code: ErrorCode,
        /// Human-readable detail.
        message: String,
    },
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Transport(m) => write!(f, "transport error: {m}"),
            ClientError::Protocol(m) => write!(f, "protocol error: {m}"),
            ClientError::Service { code, message } => {
                write!(f, "service error ({code:?}): {message}")
            }
        }
    }
}

impl std::error::Error for ClientError {}

/// A client that can send a [`Request`] and get a [`Response`].
///
/// Implementations are stateful (`&mut self`) so a real socket client can keep a
/// persistent connection.
pub trait ControlClient {
    /// Send one request and return the reply.
    ///
    /// A service-level [`Response::Error`] is returned as `Ok` (the exchange
    /// itself worked); use [`ControlClient::request_expect_ok`] to treat it as an
    /// error.
    fn request(&mut self, request: &Request) -> Result<Response, ClientError>;

    /// Like [`ControlClient::request`], but map a [`Response::Error`] into
    /// [`ClientError::Service`]. Handy for code that only cares about success.
    fn request_expect_ok(&mut self, request: &Request) -> Result<Response, ClientError> {
        match self.request(request)? {
            Response::Error { code, message } => Err(ClientError::Service { code, message }),
            ok => Ok(ok),
        }
    }

    /// Convenience: send [`Request::Hello`] for this crate's protocol version and
    /// report a [`ClientError::Protocol`] on a version mismatch.
    fn hello(&mut self, client: &str) -> Result<u32, ClientError> {
        match self.request(&Request::Hello {
            protocol: glaze_common::ipc::PROTOCOL_VERSION,
            client: client.to_string(),
        })? {
            Response::Hello { protocol, .. } => Ok(protocol),
            Response::Error { code, message } => Err(ClientError::Service { code, message }),
            other => Err(ClientError::Protocol(format!(
                "expected Hello, got {other:?}"
            ))),
        }
    }
}

/// A [`ControlClient`] that answers in-process through [`IpcServer::handle`].
///
/// This is the portable implementation: it runs the *real* request handling
/// (registry lookups, search, launch planning, backend dispatch) with no socket,
/// so it works on every host and makes the shell's protocol logic testable.
pub struct InProcessClient {
    state: Arc<Mutex<ServiceState>>,
}

impl InProcessClient {
    /// A client over a fresh service state with no compositor backend.
    pub fn new(registry: AppRegistry) -> Self {
        InProcessClient {
            state: IpcServer::new(registry).state(),
        }
    }

    /// A client over a fresh service state with a live compositor backend.
    pub fn with_backend(registry: AppRegistry, backend: Box<dyn Backend>) -> Self {
        InProcessClient {
            state: IpcServer::with_backend(registry, backend).state(),
        }
    }

    /// Share the state of an existing [`IpcServer`] (e.g. one also serving a
    /// socket).
    pub fn from_server(server: &IpcServer) -> Self {
        InProcessClient {
            state: server.state(),
        }
    }

    /// The underlying service state, so a caller (or a test) can seed real
    /// registry/tray/notification data and then exercise the client against it.
    pub fn state(&self) -> Arc<Mutex<ServiceState>> {
        Arc::clone(&self.state)
    }
}

impl ControlClient for InProcessClient {
    fn request(&mut self, request: &Request) -> Result<Response, ClientError> {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            // A poisoned lock still holds valid state; recover it like the server.
            Err(poisoned) => poisoned.into_inner(),
        };
        Ok(IpcServer::handle(&mut state, request))
    }
}

/// The real client: one request / one response line over the Unix control socket.
///
/// Kept behind `#[cfg(unix)]` because the socket API does not exist elsewhere;
/// the rest of the shell never sees this type directly, only [`ControlClient`].
#[cfg(unix)]
pub struct UnixSocketClient {
    socket: std::path::PathBuf,
}

#[cfg(unix)]
impl UnixSocketClient {
    /// A client for the default socket (`$XDG_RUNTIME_DIR/glaze/control.sock`).
    pub fn new() -> Self {
        UnixSocketClient {
            socket: glaze_common::paths::control_socket(),
        }
    }

    /// A client for an explicit socket path (tests, custom service instances).
    pub fn at(socket: impl Into<std::path::PathBuf>) -> Self {
        UnixSocketClient {
            socket: socket.into(),
        }
    }

    /// The socket this client talks to.
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket
    }
}

#[cfg(unix)]
impl Default for UnixSocketClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
impl ControlClient for UnixSocketClient {
    fn request(&mut self, request: &Request) -> Result<Response, ClientError> {
        glaze_service::ipc_server::request_once(&self.socket, request)
            .map_err(|e| ClientError::Transport(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_backend::BackendError;
    use glaze_common::model::{AppEntry, WindowInfo};

    fn app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: None,
            generic_name: None,
            exec: format!("/usr/bin/{id}"),
            try_exec: None,
            icon: None,
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: format!("/usr/share/applications/{id}.desktop"),
        }
    }

    fn registry() -> AppRegistry {
        AppRegistry::from_entries(vec![
            app("org.mozilla.firefox", "Firefox"),
            app("org.gnome.Terminal", "Terminal"),
        ])
    }

    struct WireBackend;

    impl Backend for WireBackend {
        fn name(&self) -> &'static str {
            "wire"
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
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
        }
    }

    #[test]
    fn in_process_ping_returns_pong() {
        let mut client = InProcessClient::new(registry());
        assert_eq!(client.request(&Request::Ping).unwrap(), Response::Pong);
    }

    #[test]
    fn in_process_list_apps_returns_the_real_registry() {
        let mut client = InProcessClient::new(registry());
        match client.request(&Request::ListApps).unwrap() {
            Response::Apps { apps } => {
                assert_eq!(apps.len(), 2);
                assert_eq!(apps[0].id, "org.mozilla.firefox");
            }
            other => panic!("expected Apps, got {other:?}"),
        }
    }

    #[test]
    fn in_process_search_uses_the_real_ranker() {
        let mut client = InProcessClient::new(registry());
        match client
            .request(&Request::SearchApps {
                query: "term".into(),
                limit: 5,
            })
            .unwrap()
        {
            Response::SearchResults { hits } => {
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].app_id, "org.gnome.Terminal");
            }
            other => panic!("expected SearchResults, got {other:?}"),
        }
    }

    #[test]
    fn service_error_is_returned_ok_by_request() {
        let mut client = InProcessClient::new(registry());
        let resp = client
            .request(&Request::LaunchApp { id: "nope".into() })
            .unwrap();
        match resp {
            Response::Error { code, .. } => assert_eq!(code, ErrorCode::NotFound),
            other => panic!("expected an Error response, got {other:?}"),
        }
    }

    #[test]
    fn request_expect_ok_maps_service_errors() {
        let mut client = InProcessClient::new(registry());
        let err = client
            .request_expect_ok(&Request::LaunchApp { id: "nope".into() })
            .unwrap_err();
        match err {
            ClientError::Service { code, .. } => assert_eq!(code, ErrorCode::NotFound),
            other => panic!("expected ClientError::Service, got {other:?}"),
        }
        // A successful call still comes back Ok.
        assert_eq!(
            client.request_expect_ok(&Request::Ping).unwrap(),
            Response::Pong
        );
    }

    #[test]
    fn hello_negotiates_the_protocol_version() {
        let mut client = InProcessClient::new(registry());
        assert_eq!(
            client.hello("glaze-shell/test").unwrap(),
            glaze_common::ipc::PROTOCOL_VERSION
        );
    }

    #[test]
    fn windows_without_backend_are_empty_not_faked() {
        let mut client = InProcessClient::new(registry());
        match client.request(&Request::ListWindows).unwrap() {
            Response::Windows { windows } => assert!(windows.is_empty()),
            other => panic!("expected empty Windows, got {other:?}"),
        }
    }

    #[test]
    fn attached_backend_is_surfaced_through_the_client() {
        let mut client = InProcessClient::with_backend(registry(), Box::new(WireBackend));
        match client.request(&Request::ListWindows).unwrap() {
            Response::Windows { windows } => {
                assert_eq!(windows.len(), 1);
                assert_eq!(windows[0].title, "Firefox");
            }
            other => panic!("expected Windows, got {other:?}"),
        }
        match client.request(&Request::Status).unwrap() {
            Response::Status { status } => {
                assert_eq!(status.compositor_backend, "wire");
                assert_eq!(status.app_count, 2);
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn from_server_shares_the_server_state() {
        let server = IpcServer::new(registry());
        let mut client = InProcessClient::from_server(&server);
        assert_eq!(client.request(&Request::Ping).unwrap(), Response::Pong);
    }

    #[test]
    fn works_behind_a_trait_object() {
        // The shell holds a `Box<dyn ControlClient>`; prove it is object-safe.
        let mut client: Box<dyn ControlClient> = Box::new(InProcessClient::new(registry()));
        assert_eq!(client.request(&Request::Ping).unwrap(), Response::Pong);
        assert_eq!(client.hello("test").unwrap(), 1);
    }

    #[test]
    fn client_error_displays_clearly() {
        let e = ClientError::Service {
            code: ErrorCode::BackendUnavailable,
            message: "no backend".into(),
        };
        let text = e.to_string();
        assert!(text.contains("BackendUnavailable"));
        assert!(text.contains("no backend"));
        assert!(ClientError::Transport("closed".into())
            .to_string()
            .contains("transport error"));
    }
}
