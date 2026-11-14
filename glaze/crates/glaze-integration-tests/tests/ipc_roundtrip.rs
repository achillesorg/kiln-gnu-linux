//! IPC integration tests.
//!
//! Two layers:
//!
//! 1. Portable serde round-trips of the versioned [`Request`]/[`Response`]
//!    protocol (`glaze-common::ipc`), which run on every host.
//! 2. The real server dispatch (`glaze_service::ipc_server::IpcServer::handle`)
//!    fed JSON request lines, including the protocol-mismatch path. That module
//!    is gated behind `#[cfg(unix)]` in `glaze-service` (it uses Unix domain
//!    sockets), so those tests only compile and run on Unix/Linux hosts.

use glaze_common::ipc::{ErrorCode, Request, Response, PROTOCOL_VERSION};
use glaze_common::model::SearchHit;

fn all_requests() -> Vec<Request> {
    vec![
        Request::Hello {
            protocol: PROTOCOL_VERSION,
            client: "glaze-it/0.1.0".into(),
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
        Request::ListWindows,
        Request::ListWorkspaces,
        Request::ActivateWindow { id: "42".into() },
        Request::CloseWindow { id: "42".into() },
        Request::Reload,
        Request::Status,
    ]
}

#[test]
fn every_request_round_trips_through_a_json_line() {
    for req in all_requests() {
        let line = req.to_line().unwrap();
        assert_eq!(line.matches('\n').count(), 1, "one request per line");
        assert!(line.ends_with('\n'));
        let back: Request = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(back, req);
    }
}

#[test]
fn responses_round_trip_including_errors() {
    let responses = vec![
        Response::Hello {
            protocol: PROTOCOL_VERSION,
            service_version: glaze_common::VERSION.to_string(),
        },
        Response::Pong,
        Response::Apps { apps: vec![] },
        Response::SearchResults {
            hits: vec![SearchHit {
                app_id: "firefox".into(),
                name: "Firefox".into(),
                icon: Some("firefox".into()),
                score: 1000,
                name_match_indices: vec![0, 1, 2],
            }],
        },
        Response::Windows { windows: vec![] },
        Response::Workspaces { workspaces: vec![] },
        Response::Ok,
        Response::error(ErrorCode::ProtocolMismatch, "protocol 2 != 1"),
        Response::error(ErrorCode::NotFound, "no app with id x"),
    ];

    for resp in responses {
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }
}

#[test]
fn wire_tags_and_error_codes_are_kebab_case() {
    assert_eq!(
        serde_json::to_string(&Request::ListApps).unwrap(),
        r#"{"kind":"list-apps"}"#
    );
    assert_eq!(
        serde_json::to_string(&Request::SearchApps {
            query: "x".into(),
            limit: 1
        })
        .unwrap(),
        r#"{"kind":"search-apps","query":"x","limit":1}"#
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::ProtocolMismatch).unwrap(),
        r#""protocol-mismatch""#
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::BackendUnavailable).unwrap(),
        r#""backend-unavailable""#
    );
}

#[test]
fn protocol_version_is_stable_and_positive() {
    assert_eq!(PROTOCOL_VERSION, 1);
}

/// The server-side dispatch, exercised by feeding JSON request **lines**.
#[cfg(unix)]
mod server_dispatch {
    use super::*;
    use glaze_common::ipc::SOCKET_NAME;
    use glaze_service::ipc_server::{IpcServer, ServiceState};
    use glaze_service::AppRegistry;

    fn state() -> ServiceState {
        ServiceState::new(AppRegistry::default())
    }

    /// Parse a JSON request line and dispatch it exactly like the socket server.
    fn handle_line(state: &mut ServiceState, line: &str) -> Result<Response, serde_json::Error> {
        let req: Request = serde_json::from_str(line)?;
        Ok(IpcServer::handle(state, &req))
    }

    #[test]
    fn hello_line_negotiates_the_protocol() {
        let mut st = state();
        let line =
            format!(r#"{{"kind":"hello","protocol":{PROTOCOL_VERSION},"client":"glaze-it"}}"#);
        let resp = handle_line(&mut st, &line).unwrap();
        match resp {
            Response::Hello { protocol, .. } => assert_eq!(protocol, PROTOCOL_VERSION),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn protocol_mismatch_line_reports_a_structured_error() {
        let mut st = state();
        let line = r#"{"kind":"hello","protocol":999,"client":"glaze-it"}"#;
        match handle_line(&mut st, line).unwrap() {
            Response::Error { code, .. } => assert_eq!(code, ErrorCode::ProtocolMismatch),
            other => panic!("expected ProtocolMismatch, got {other:?}"),
        }
    }

    #[test]
    fn ping_list_and_search_round_trip_as_json() {
        let mut st = state();

        assert_eq!(
            handle_line(&mut st, r#"{"kind":"ping"}"#).unwrap(),
            Response::Pong
        );

        match handle_line(&mut st, r#"{"kind":"list-apps"}"#).unwrap() {
            Response::Apps { apps } => assert!(apps.is_empty(), "empty registry"),
            other => panic!("expected Apps, got {other:?}"),
        }

        // A malformed line is a client error, not a panic.
        assert!(handle_line(&mut st, "not json").is_err());

        // Responses serialise back to a single JSON line the transport can send.
        let resp = handle_line(&mut st, r#"{"kind":"search-apps","query":"x","limit":3}"#).unwrap();
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains('\n'));
        let back: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn socket_name_is_stable() {
        assert_eq!(SOCKET_NAME, "control.sock");
    }
}
