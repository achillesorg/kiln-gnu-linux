//! `glaze-service` — the real application registry, launch path and local IPC.
//!
//! Everything here is **portable** by design (`.desktop` parsing, the registry,
//! search ranking, `Exec` handling, the IPC request dispatch, and the
//! notification/tray models), so it is unit-tested exhaustively on any host
//! (`GLAZE.md` §9 "Launcher" + "Security"; `docs/adr/ADR-0005`). The two things
//! that are genuinely platform-bound are gated behind `#[cfg(unix)]` (the
//! control-socket transport) or behind the off-by-default `dbus` feature (the
//! live notification/StatusNotifier transports), so the root workspace always
//! builds and tests on the Windows authoring host.
//!
//! Design rules enforced by this crate:
//! - `.desktop` data is never handed to a shell — `Exec` is tokenised here and
//!   launched with an argv vector (`GLAZE.md` §4.1, §7.9).
//! - The registry never invents entries; if a directory has no `.desktop`
//!   files the result is genuinely empty.
//! - The compositor backend is attached explicitly; without one, window queries
//!   return honest empty lists and actions return `BackendUnavailable`.

#[cfg(all(unix, feature = "dbus"))]
pub mod dbus;
pub mod desktop;
pub mod ipc_server;
pub mod launch;
pub mod notifications;
pub mod registry;
pub mod search;
pub mod tray;
pub mod tray_watcher;

pub use desktop::{DesktopEntry, ExecError};
pub use ipc_server::{IpcServer, ServiceState};
pub use launch::{build_command, launch, LaunchError};
pub use notifications::{
    NameClaim, Notification, NotificationHistory, NotificationServer, Urgency,
};
pub use registry::{AppRegistry, DiscoverOptions};
pub use search::search_apps;
pub use tray::{TrayCategory, TrayItem, TrayRegistry, TrayStatus};
pub use tray_watcher::{SniSignal, TrayWatcher, WatcherOutcome};
