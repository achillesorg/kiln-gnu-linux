//! Portable, unit-tested output formatting for the `glaze` command.
//!
//! No terminal detection or colour here on purpose: the plain text is stable,
//! pipeable and identical on every host.

use glaze_common::ipc::{ErrorCode, Response};
use glaze_common::model::{
    AppEntry, NotificationInfo, SearchHit, ShellStatus, TrayItemInfo, WindowInfo, WorkspaceInfo,
};
use serde::Serialize;

/// Render a left-aligned table with two spaces between columns.
///
/// Column widths are computed from the widest cell (header or data). The last
/// column is never right-padded so there is no trailing whitespace.
pub fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return String::new();
    }
    let mut widths = vec![0usize; cols];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = cell_width(h);
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < cols {
                widths[i] = widths[i].max(cell_width(cell));
            }
        }
    }

    let mut out = String::new();
    let header_cells: Vec<String> = headers.iter().map(|s| s.to_string()).collect();
    push_row(&mut out, &header_cells, &widths, false);
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    push_row(&mut out, &sep, &widths, true);
    for row in rows {
        push_row(&mut out, row, &widths, false);
    }
    out
}

fn cell_width(s: &str) -> usize {
    s.chars().count()
}

fn push_row(out: &mut String, cells: &[String], widths: &[usize], pad_last: bool) {
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        let cell = cells.get(i).map(String::as_str).unwrap_or("");
        if pad_last || i + 1 < widths.len() {
            out.push_str(&format!("{cell:<w$}"));
        } else {
            out.push_str(cell);
        }
    }
    out.push('\n');
}

/// Pretty-print any serialisable value as JSON. Never panics.
pub fn to_pretty_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("<json serialisation failed: {e}>"))
}

/// Compact single-line JSON. Never panics.
pub fn to_line_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| format!("<json serialisation failed: {e}>"))
}

/// `yes`/`no` for booleans — stable and locale-independent.
pub fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Aligned table of applications: id, name, terminal flag.
pub fn render_apps_table(apps: &[AppEntry]) -> String {
    let rows: Vec<Vec<String>> = apps
        .iter()
        .map(|a| vec![a.id.clone(), a.name.clone(), yes_no(a.terminal).to_string()])
        .collect();
    render_table(&["id", "name", "terminal"], &rows)
}

/// Aligned table of ranked search hits: score, app id, name.
pub fn render_hits_table(hits: &[SearchHit]) -> String {
    let rows: Vec<Vec<String>> = hits
        .iter()
        .map(|h| vec![h.score.to_string(), h.app_id.clone(), h.name.clone()])
        .collect();
    render_table(&["score", "app id", "name"], &rows)
}

/// Aligned table of live windows: id, app, title, focus.
pub fn render_windows_table(windows: &[WindowInfo]) -> String {
    let rows: Vec<Vec<String>> = windows
        .iter()
        .map(|w| {
            vec![
                w.id.clone(),
                w.app_id.clone().unwrap_or_else(|| "-".to_string()),
                w.title.clone(),
                yes_no(w.focused).to_string(),
            ]
        })
        .collect();
    render_table(&["id", "app", "title", "focus"], &rows)
}

/// Aligned table of workspaces: index, name, focus, urgent.
pub fn render_workspaces_table(workspaces: &[WorkspaceInfo]) -> String {
    let rows: Vec<Vec<String>> = workspaces
        .iter()
        .map(|w| {
            vec![
                w.index.to_string(),
                w.name.clone().unwrap_or_else(|| "-".to_string()),
                yes_no(w.focused).to_string(),
                yes_no(w.urgent).to_string(),
            ]
        })
        .collect();
    render_table(&["index", "name", "focus", "urgent"], &rows)
}

/// Multi-line human summary of the service `Status` response.
pub fn render_shell_status(status: &ShellStatus) -> String {
    format!(
        "version           : {}\ntheme             : {}\nscheme            : {}\ncompositor backend: {}\napps              : {}\nwindows           : {}",
        status.version,
        status.theme,
        status.scheme,
        status.compositor_backend,
        status.app_count,
        status.window_count
    )
}

fn error_code_str(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::ProtocolMismatch => "protocol-mismatch",
        ErrorCode::NotFound => "not-found",
        ErrorCode::LauncherFailed => "launcher-failed",
        ErrorCode::BackendUnavailable => "backend-unavailable",
        ErrorCode::InvalidRequest => "invalid-request",
        ErrorCode::Internal => "internal",
    }
}

/// Render an IPC [`Response`] for a human reader.
pub fn render_response(resp: &Response) -> String {
    match resp {
        Response::Hello {
            protocol,
            service_version,
        } => format!("hello: protocol {protocol}, service {service_version}"),
        Response::Pong => "pong".to_string(),
        Response::Ok => "ok".to_string(),
        Response::Apps { apps } => render_apps_table(apps),
        Response::SearchResults { hits } => render_hits_table(hits),
        Response::Windows { windows } => render_windows_table(windows),
        Response::Workspaces { workspaces } => render_workspaces_table(workspaces),
        Response::Tray { items } => render_tray_table(items),
        Response::Notifications {
            items,
            unread_ids,
            dnd,
        } => render_notifications_table(items, unread_ids, *dnd),
        Response::Status { status } => render_shell_status(status),
        Response::Error { code, message } => {
            format!("error [{}]: {message}", error_code_str(*code))
        }
    }
}

/// One row per tray item.
pub fn render_tray_table(items: &[TrayItemInfo]) -> String {
    if items.is_empty() {
        return "no tray items".to_string();
    }
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|item| {
            vec![
                item.id.clone(),
                item.title.clone(),
                format!("{:?}", item.category),
                format!("{:?}", item.status),
            ]
        })
        .collect();
    render_table(&["id", "title", "category", "status"], &rows)
}

/// One row per notification, with an unread marker and the DND state.
pub fn render_notifications_table(
    items: &[NotificationInfo],
    unread_ids: &[u32],
    dnd: bool,
) -> String {
    let header = format!(
        "{} notification(s), {} unread{}",
        items.len(),
        unread_ids.len(),
        if dnd { ", do not disturb" } else { "" }
    );
    if items.is_empty() {
        return header;
    }
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|n| {
            vec![
                n.id.to_string(),
                if unread_ids.contains(&n.id) {
                    "unread"
                } else {
                    ""
                }
                .to_string(),
                n.app_name.clone(),
                n.summary.clone(),
            ]
        })
        .collect();
    format!(
        "{header}\n{}",
        render_table(&["id", "state", "app", "summary"], &rows)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::model::AppEntry;

    #[test]
    fn table_aligns_columns_without_trailing_space() {
        let rows = vec![
            vec!["a".to_string(), "Alpha".to_string()],
            vec!["long-id".to_string(), "B".to_string()],
        ];
        let out = render_table(&["id", "name"], &rows);
        let expected = "id       name\n\
                        -------  -----\n\
                        a        Alpha\n\
                        long-id  B\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn table_with_no_rows_still_has_header_and_separator() {
        let out = render_table(&["id", "name"], &[]);
        assert_eq!(out, "id  name\n--  ----\n");
    }

    #[test]
    fn table_row_longer_than_headers_is_kept() {
        let rows = vec![vec!["x".to_string(), "y".to_string(), "z".to_string()]];
        let out = render_table(&["a"], &rows);
        assert!(out.contains("z"));
        assert!(!out.ends_with(' '));
    }

    #[test]
    fn pretty_and_line_json() {
        let v = vec!["a", "b"];
        assert_eq!(to_line_json(&v), "[\"a\",\"b\"]");
        assert!(to_pretty_json(&v).contains("  \"a\""));
    }

    fn app(id: &str, name: &str, terminal: bool) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            comment: None,
            generic_name: None,
            exec: "app".to_string(),
            try_exec: None,
            icon: None,
            categories: vec![],
            keywords: vec![],
            terminal,
            no_display: false,
            working_dir: None,
            source_path: "/tmp/x.desktop".to_string(),
        }
    }

    #[test]
    fn apps_table_has_expected_columns() {
        let apps = vec![
            app("a.desktop", "Alpha", false),
            app("t.desktop", "Term", true),
        ];
        let out = render_apps_table(&apps);
        assert!(out.starts_with("id"));
        assert!(out.contains("terminal"));
        assert!(out.contains("no"));
        assert!(out.contains("yes"));
    }

    #[test]
    fn renders_pong_ok_and_error() {
        assert_eq!(render_response(&Response::Pong), "pong");
        assert_eq!(render_response(&Response::Ok), "ok");
        let err = Response::error(ErrorCode::NotFound, "no such app");
        assert_eq!(render_response(&err), "error [not-found]: no such app");
    }

    #[test]
    fn renders_status_lines() {
        let s = ShellStatus {
            version: "0.1.0".into(),
            theme: "clay".into(),
            scheme: "light".into(),
            compositor_backend: "none".into(),
            app_count: 3,
            window_count: 0,
        };
        let out = render_response(&Response::Status { status: s });
        assert!(out.contains("version           : 0.1.0"));
        assert!(out.contains("apps              : 3"));
    }
}
