//! Shell status built from **real** inputs, plus an honest backend summary.
//!
//! Two related things live here:
//!
//! - [`BackendSummary`] / [`backend_summary`] describe the compositor state
//!   backend truthfully: when none is attached, or an attached one cannot report
//!   windows, that is said plainly and the window count is `0` — never a made-up
//!   number (`GLAZE.md` §4.3, §7.2).
//! - [`build_status`] assembles the [`glaze_common::model::ShellStatus`] the
//!   settings "About" page and the IPC `Status` request report, from the real
//!   configuration, registry size and backend summary.
//!
//! Everything is pure: no clock, no environment, no I/O.

use glaze_backend::Backend;
use glaze_common::config::GlazeConfig;
use glaze_common::model::ShellStatus;

/// A truthful description of the compositor state backend in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSummary {
    /// Backend name (`glaze-backend::Backend::name`), or `"none"` when absent.
    pub name: String,
    /// A backend is attached to the shell.
    pub attached: bool,
    /// The backend reports that its transport is connected.
    pub connected: bool,
    /// Live window count, or `0` when unknown.
    pub window_count: usize,
    /// `true` only when `window_count` came from a real, successful query. When
    /// `false`, `window_count` is `0` because the value is unknown, **not**
    /// because there are no windows.
    pub windows_known: bool,
    /// One-line human explanation for logs and the About page.
    pub note: String,
}

/// Summarise a backend, or the absence of one, without inventing anything.
pub fn backend_summary(backend: Option<&dyn Backend>) -> BackendSummary {
    match backend {
        None => BackendSummary {
            name: "none".to_string(),
            attached: false,
            connected: false,
            window_count: 0,
            windows_known: false,
            note: "no compositor backend attached — window state is unavailable".to_string(),
        },
        Some(backend) => {
            let connected = backend.is_connected();
            match backend.windows() {
                Ok(windows) => BackendSummary {
                    name: backend.name().to_string(),
                    attached: true,
                    connected,
                    window_count: windows.len(),
                    windows_known: true,
                    note: format!(
                        "{} reported {} live window(s)",
                        backend.name(),
                        windows.len()
                    ),
                },
                Err(err) => BackendSummary {
                    name: backend.name().to_string(),
                    attached: true,
                    connected,
                    window_count: 0,
                    windows_known: false,
                    note: format!("{} cannot report windows: {err}", backend.name()),
                },
            }
        }
    }
}

/// Build the shell status from the real configuration, registry size and backend
/// summary.
///
/// `theme`, `scheme` and `compositor_backend` come straight from the
/// configuration and the summary; `app_count` is the live registry length the
/// caller measured. No field is guessed.
pub fn build_status(
    cfg: &GlazeConfig,
    registry_len: usize,
    summary: &BackendSummary,
) -> ShellStatus {
    ShellStatus {
        version: crate::VERSION.to_string(),
        theme: cfg.appearance.theme.slug().to_string(),
        scheme: cfg.appearance.scheme.slug().to_string(),
        compositor_backend: summary.name.clone(),
        app_count: registry_len,
        window_count: summary.window_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_backend::BackendError;
    use glaze_common::config::{ColorScheme, GlazeConfig, ThemeKind};
    use glaze_common::model::{WindowInfo, WindowKind};

    fn window(id: &str, focused: bool) -> WindowInfo {
        WindowInfo {
            id: id.into(),
            app_id: Some("app".into()),
            title: "T".into(),
            workspace: Some(0),
            kind: WindowKind::Normal,
            focused,
            minimized: false,
            floating: false,
        }
    }

    struct FakeBackend {
        name: &'static str,
        connected: bool,
        windows: Result<usize, ()>,
    }

    impl Backend for FakeBackend {
        fn name(&self) -> &'static str {
            self.name
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
            match self.windows {
                Ok(n) => Ok((0..n).map(|i| window(&i.to_string(), i == 0)).collect()),
                Err(()) => Err(BackendError::Unsupported("fake cannot report windows")),
            }
        }
    }

    #[test]
    fn no_backend_is_reported_honestly() {
        let summary = backend_summary(None);
        assert_eq!(summary.name, "none");
        assert!(!summary.attached);
        assert!(!summary.connected);
        assert_eq!(summary.window_count, 0);
        assert!(!summary.windows_known, "zero is unknown, not measured");
        assert!(summary.note.contains("no compositor backend"));
    }

    #[test]
    fn attached_backend_reports_real_window_count() {
        let backend = FakeBackend {
            name: "sway",
            connected: true,
            windows: Ok(3),
        };
        let summary = backend_summary(Some(&backend));
        assert_eq!(summary.name, "sway");
        assert!(summary.attached);
        assert!(summary.connected);
        assert_eq!(summary.window_count, 3);
        assert!(summary.windows_known);
        assert!(summary.note.contains("3"));
    }

    #[test]
    fn attached_but_unsupported_backend_does_not_fabricate_windows() {
        let backend = FakeBackend {
            name: "foreign-toplevel",
            connected: false,
            windows: Err(()),
        };
        let summary = backend_summary(Some(&backend));
        assert_eq!(summary.name, "foreign-toplevel");
        assert!(summary.attached);
        assert!(!summary.connected);
        assert_eq!(summary.window_count, 0);
        assert!(!summary.windows_known, "must not claim zero real windows");
        assert!(summary.note.contains("cannot report windows"));
    }

    #[test]
    fn zero_real_windows_is_known_when_the_backend_says_so() {
        let backend = FakeBackend {
            name: "sway",
            connected: true,
            windows: Ok(0),
        };
        let summary = backend_summary(Some(&backend));
        assert_eq!(summary.window_count, 0);
        assert!(summary.windows_known, "an empty list IS a real measurement");
    }

    #[test]
    fn build_status_maps_config_and_summary_faithfully() {
        let mut cfg = GlazeConfig::default();
        cfg.appearance.theme = ThemeKind::Classic;
        cfg.appearance.scheme = ColorScheme::Dark;
        let summary = BackendSummary {
            name: "sway".into(),
            attached: true,
            connected: true,
            window_count: 2,
            windows_known: true,
            note: "sway reported 2 live window(s)".into(),
        };
        let status = build_status(&cfg, 42, &summary);
        assert_eq!(status.version, crate::VERSION);
        assert_eq!(status.theme, "classic");
        assert_eq!(status.scheme, "dark");
        assert_eq!(status.compositor_backend, "sway");
        assert_eq!(status.app_count, 42);
        assert_eq!(status.window_count, 2);
    }

    #[test]
    fn build_status_without_backend_uses_none_and_zero() {
        let cfg = GlazeConfig::default();
        let status = build_status(&cfg, 0, &backend_summary(None));
        assert_eq!(status.theme, "clay");
        assert_eq!(status.scheme, "light");
        assert_eq!(status.compositor_backend, "none");
        assert_eq!(status.app_count, 0);
        assert_eq!(status.window_count, 0);
    }
}
