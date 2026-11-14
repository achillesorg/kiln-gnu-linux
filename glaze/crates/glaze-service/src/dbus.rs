//! Real D-Bus transports (compiled only with `--features dbus` on Unix).
//!
//! This is the *only* place a session bus is touched. Everything decision-shaped
//! lives in the portable [`crate::notifications`] / [`crate::tray_watcher`]
//! models; this module is a thin adapter over `zbus`:
//!
//! - [`start_notification_server`] probes/claims `org.freedesktop.Notifications`
//!   and **refuses to start** if another daemon already owns it
//!   ([`crate::notifications::decide_name_claim`]).
//! - [`start_status_notifier_watcher`] claims `org.kde.StatusNotifierWatcher`
//!   and feeds the shared tray watcher.
//!
//! **One state, not two.** Both exported objects hold the *same*
//! `Arc<Mutex<ServiceState>>` the IPC control socket serves. A notification that
//! arrives on the bus is therefore visible to the shell's `ListNotifications`
//! query immediately, and a tray item that registers is visible to `ListTray` —
//! there is no second copy to drift out of sync.
//!
//! # Verification status
//!
//! The `dbus` feature is **off by default** and this module is not compiled on
//! the Windows authoring host. It is type-checked and linted for
//! `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`
//! (`cargo clippy -p glaze-service --features dbus --target … -D warnings`), but
//! it has **not** been run against a real session bus. That is recorded in
//! `docs/adr/ADR-0004-system-integration.md` and `PROJECT_STATUS.md`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::message::Header;

use crate::ipc_server::ServiceState;
use crate::notifications::{
    decide_name_claim, NameClaim, NotifyRequest, Urgency, NOTIFICATIONS_BUS_NAME,
    NOTIFICATIONS_OBJECT_PATH,
};
use crate::tray::{StatusNotifierItemRegistration, WATCHER_BUS_NAME, WATCHER_OBJECT_PATH};
use crate::tray_watcher::{item_signal, SniSignal};

/// The StatusNotifierItem interface carrying the item-side metadata signals.
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";

/// Errors from the D-Bus adapter.
#[derive(Debug)]
pub enum DbusError {
    /// The session bus connection failed.
    Bus(String),
    /// The well-known name is already owned by another peer.
    NameTaken(String),
    /// Exporting the object failed.
    Export(String),
}

impl std::fmt::Display for DbusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbusError::Bus(m) => write!(f, "session bus error: {m}"),
            DbusError::NameTaken(m) => write!(f, "bus name already owned: {m}"),
            DbusError::Export(m) => write!(f, "failed to export object: {m}"),
        }
    }
}

impl std::error::Error for DbusError {}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Lock the shared service state, recovering from a poisoned mutex.
fn lock_state(state: &Arc<Mutex<ServiceState>>) -> std::sync::MutexGuard<'_, ServiceState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Try to claim `name` without queueing. Returns `Ok(false)` when another peer
/// already owns it (single-owner rule).
fn claim_exclusive(conn: &Connection, name: &str) -> Result<bool, DbusError> {
    let reply = conn
        // zbus 5 exposes `RequestNameFlags` as an enum; the call takes a
        // `BitFlags<RequestNameFlags>`, hence `.into()`.
        .request_name_with_flags(name, RequestNameFlags::DoNotQueue.into())
        .map_err(|e| DbusError::Bus(e.to_string()))?;
    let already_owned = matches!(reply, RequestNameReply::Exists | RequestNameReply::InQueue);
    Ok(decide_name_claim(already_owned) == NameClaim::Acquired)
}

/// Read the `urgency` hint (a byte: 0 low, 1 normal, 2 critical).
///
/// The hint is optional and may be sent with any integer width, so a failure to
/// read it falls back to the spec default (`normal`) rather than dropping the
/// notification.
fn urgency_from_hints(hints: &HashMap<String, zbus::zvariant::Value<'_>>) -> Urgency {
    hints
        .get("urgency")
        .and_then(|value| u8::try_from(value.clone()).ok())
        .map(Urgency::from_byte)
        .unwrap_or_default()
}

/// The exported `org.freedesktop.Notifications` object.
struct Notifications {
    /// The *same* state the IPC server serves, so the shell sees what the bus
    /// delivered.
    state: Arc<Mutex<ServiceState>>,
}

#[allow(clippy::too_many_arguments)]
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    /// `Notify`.
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let request = NotifyRequest {
            app_name: app_name.to_string(),
            app_icon: app_icon.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions,
            urgency: urgency_from_hints(&hints),
            expire_timeout,
            replaces_id,
        };
        lock_state(&self.state)
            .notifications_mut()
            .notify(request, now_unix())
    }

    /// `CloseNotification`.
    fn close_notification(&self, id: u32) {
        lock_state(&self.state)
            .notifications_mut()
            .close_notification(id);
    }

    /// `GetCapabilities`.
    fn get_capabilities(&self) -> Vec<String> {
        lock_state(&self.state)
            .notifications
            .get_capabilities()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// `GetServerInformation` → `(name, vendor, version, spec_version)`.
    fn get_server_information(&self) -> (String, String, String, String) {
        let info = lock_state(&self.state)
            .notifications
            .get_server_information();
        (info.name, info.vendor, info.version, info.spec_version)
    }
}

/// Start the notification server, or return [`DbusError::NameTaken`] if another
/// daemon already owns `org.freedesktop.Notifications`.
///
/// The history is the one already inside `state` (default capacity 64), so the
/// adapter adds no second store.
pub fn start_notification_server(state: Arc<Mutex<ServiceState>>) -> Result<Connection, DbusError> {
    let conn = Connection::session().map_err(|e| DbusError::Bus(e.to_string()))?;
    if !claim_exclusive(&conn, NOTIFICATIONS_BUS_NAME)? {
        return Err(DbusError::NameTaken(format!(
            "{NOTIFICATIONS_BUS_NAME} is already owned by another daemon; \
             Glaze will not double-claim it"
        )));
    }
    conn.object_server()
        .at(NOTIFICATIONS_OBJECT_PATH, Notifications { state })
        .map_err(|e| DbusError::Export(e.to_string()))?;
    Ok(conn)
}

/// The exported `org.kde.StatusNotifierWatcher` object.
struct StatusNotifierWatcher {
    /// Feeds the same watcher the IPC `ListTray` request reads.
    state: Arc<Mutex<ServiceState>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    /// `RegisterStatusNotifierItem`.
    ///
    /// The spec allows either a bus name or an object path; for a path the item
    /// is addressed by the **caller's unique name**, which the message header
    /// provides. Registering an item also subscribes to its own metadata signals
    /// (`NewTitle`/`NewIcon`/`NewStatus`/`NewCategory`) so the tray stays live.
    fn register_status_notifier_item(&self, #[zbus(header)] header: Header<'_>, service: &str) {
        let sender = header.sender().map(|name| name.to_string());
        let registration = StatusNotifierItemRegistration::resolve(service, sender.as_deref());
        let (item_service, item_path) = (registration.service.clone(), registration.path.clone());

        lock_state(&self.state).apply_tray_signal(SniSignal::RegisterItem {
            service_or_path: service.to_string(),
            sender,
        });
        spawn_item_listener(Arc::clone(&self.state), item_service, item_path);
    }

    /// `RegisterStatusNotifierHost`.
    fn register_status_notifier_host(&self, service: &str) {
        lock_state(&self.state).apply_tray_signal(SniSignal::RegisterHost {
            service: service.to_string(),
        });
    }

    /// `RegisteredStatusNotifierItems` property.
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        lock_state(&self.state)
            .tray
            .registry()
            .items()
            .into_iter()
            .map(|item| item.id.clone())
            .collect()
    }

    /// `IsStatusNotifierHostRegistered` property.
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        lock_state(&self.state).tray.registry().has_host()
    }

    /// `ProtocolVersion` property (0 — the baseline SNI protocol).
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

/// Subscribe to one item's metadata signals and feed them to the shared watcher.
///
/// A StatusNotifierItem publishes its state as both properties and signals; the
/// host side must listen for the signals or the tray icon goes stale when the
/// application updates itself. Each item gets its own blocking signal iterator on
/// its own connection, so a misbehaving item cannot stall the others.
///
/// The listener is best-effort: a failure to connect or subscribe is logged with
/// the item id and the thread simply ends, leaving the item registered (its
/// registration-time state is still correct).
pub fn spawn_item_listener(state: Arc<Mutex<ServiceState>>, service: String, path: String) {
    std::thread::spawn(move || {
        let id = format!("{service}:{path}");

        let connection = match Connection::session() {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("glaze-service: tray listener for {id}: session bus unavailable: {error}");
                return;
            }
        };
        let proxy = match zbus::blocking::Proxy::new(
            &connection,
            service.as_str(),
            path.as_str(),
            ITEM_INTERFACE,
        ) {
            Ok(proxy) => proxy,
            Err(error) => {
                eprintln!("glaze-service: tray listener for {id}: cannot subscribe: {error}");
                return;
            }
        };
        let signals = match proxy.receive_all_signals() {
            Ok(signals) => signals,
            Err(error) => {
                eprintln!("glaze-service: tray listener for {id}: cannot listen: {error}");
                return;
            }
        };

        for message in signals {
            // `header()` is infallible for a received message; only the member
            // name is optional. Bind it so `member` does not outlive a temporary.
            let header = message.header();
            let Some(member) = header.member() else {
                continue;
            };
            // Every modelled item signal carries exactly one string argument.
            let text = message
                .body()
                .deserialize::<(String,)>()
                .ok()
                .map(|(first,)| first);
            if let Some(signal) = item_signal(&id, member.as_str(), text.as_deref()) {
                lock_state(&state).apply_tray_signal(signal);
            }
        }
        eprintln!("glaze-service: tray item {id} signal stream ended");
    });
}

/// Start the StatusNotifier watcher, or return [`DbusError::NameTaken`] if a
/// watcher is already running.
pub fn start_status_notifier_watcher(
    state: Arc<Mutex<ServiceState>>,
) -> Result<Connection, DbusError> {
    let conn = Connection::session().map_err(|e| DbusError::Bus(e.to_string()))?;
    if !claim_exclusive(&conn, WATCHER_BUS_NAME)? {
        return Err(DbusError::NameTaken(format!(
            "{WATCHER_BUS_NAME} is already owned; another tray host is running"
        )));
    }
    conn.object_server()
        .at(WATCHER_OBJECT_PATH, StatusNotifierWatcher { state })
        .map_err(|e| DbusError::Export(e.to_string()))?;
    Ok(conn)
}
