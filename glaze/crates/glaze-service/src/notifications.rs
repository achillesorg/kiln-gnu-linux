//! Freedesktop notification server **model** and the single-owner rule.
//!
//! Glaze must not double-claim `org.freedesktop.Notifications`: if another daemon
//! (GNOME Shell, dunst, mako, …) already owns the name, Glaze refuses to serve
//! notifications rather than fighting over the bus name
//! (`GLAZE.md` §4.3 — "Respect other running notification daemons and avoid
//! claiming the bus name twice").
//!
//! The **decision** ([`decide_name_claim`]) and the whole server state machine
//! ([`NotificationServer`] + [`NotificationHistory`]) are portable and
//! unit-tested on any host. Only the zbus call that actually probes/acquires the
//! bus name lives behind `#[cfg(all(unix, feature = "dbus"))]` in
//! [`crate::dbus`].

use std::collections::{BTreeSet, VecDeque};

/// The well-known bus name a notification server owns.
pub const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
/// The object path the interface is exported at.
pub const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";
/// The interface name (also the spec version Glaze implements).
pub const NOTIFICATIONS_INTERFACE: &str = "org.freedesktop.Notifications";

/// Notification urgency, as carried in the `urgency` hint (byte 0/1/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    /// Map the D-Bus `urgency` hint byte to an urgency.
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }

    /// The hint byte for this urgency.
    pub fn to_byte(self) -> u8 {
        match self {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        }
    }
}

/// The arguments of one `Notify` call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotifyRequest {
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    /// Flat `[key, label, key, label, …]` action list from the caller.
    pub actions: Vec<String>,
    pub urgency: Urgency,
    /// Milliseconds; `-1` means "server default", `0` means "never expire".
    pub expire_timeout: i32,
    /// `0` means "this is a new notification"; otherwise replace that id.
    pub replaces_id: u32,
}

/// A stored notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub urgency: Urgency,
    pub expire_timeout: i32,
    /// Unix seconds the server recorded it (injected so tests stay deterministic).
    pub received_at_unix: u64,
}

/// The `GetServerInformation` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInformation {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub spec_version: String,
}

impl Default for ServerInformation {
    fn default() -> Self {
        ServerInformation {
            name: "Glaze".into(),
            vendor: "Glaze".into(),
            version: glaze_common::VERSION.into(),
            spec_version: "1.2".into(),
        }
    }
}

/// Outcome of trying to become the notification bus-name owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameClaim {
    /// The name was free (or already ours): Glaze may serve notifications.
    Acquired,
    /// Another daemon already owns the name: Glaze must stand down.
    AlreadyOwned,
}

/// The single-owner rule, reduced to a pure, testable decision.
///
/// The caller performs the real bus probe (see [`crate::dbus`]) and passes the
/// result here. `true` (someone else owns the name) means Glaze refuses to start
/// its own server.
pub fn decide_name_claim(name_already_owned_by_another: bool) -> NameClaim {
    if name_already_owned_by_another {
        NameClaim::AlreadyOwned
    } else {
        NameClaim::Acquired
    }
}

/// A bounded ring of the most recent notifications.
#[derive(Debug, Clone)]
pub struct NotificationHistory {
    capacity: usize,
    items: VecDeque<Notification>,
}

impl Default for NotificationHistory {
    fn default() -> Self {
        Self::new(64)
    }
}

impl NotificationHistory {
    /// Create a history that keeps at most `capacity` notifications. A capacity
    /// of `0` keeps none.
    pub fn new(capacity: usize) -> Self {
        NotificationHistory {
            capacity,
            items: VecDeque::new(),
        }
    }

    /// Configured maximum.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Push a notification, evicting the oldest if the ring is full.
    pub fn push(&mut self, n: Notification) {
        if self.capacity == 0 {
            return;
        }
        self.items.push_back(n);
        while self.items.len() > self.capacity {
            self.items.pop_front();
        }
    }

    /// Look up a notification by id.
    pub fn get(&self, id: u32) -> Option<&Notification> {
        self.items.iter().find(|n| n.id == id)
    }

    /// Mutable lookup by id.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Notification> {
        self.items.iter_mut().find(|n| n.id == id)
    }

    /// Remove a notification by id. Returns whether it was present.
    pub fn remove(&mut self, id: u32) -> bool {
        if let Some(pos) = self.items.iter().position(|n| n.id == id) {
            self.items.remove(pos);
            true
        } else {
            false
        }
    }

    /// Number of stored notifications.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The most recent notification.
    pub fn newest(&self) -> Option<&Notification> {
        self.items.back()
    }

    /// Iterate oldest → newest.
    pub fn iter(&self) -> impl Iterator<Item = &Notification> {
        self.items.iter()
    }

    /// Drop everything.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// The portable notification server state machine.
///
/// Method names follow the D-Bus interface: `Notify`, `CloseNotification`,
/// `GetCapabilities`, `GetServerInformation`.
#[derive(Debug, Clone)]
pub struct NotificationServer {
    history: NotificationHistory,
    next_id: u32,
    /// Ids the user has already seen; anything in the history but not here is
    /// unread and counts toward the shell's badge.
    read: BTreeSet<u32>,
}

impl Default for NotificationServer {
    fn default() -> Self {
        Self::new(64)
    }
}

impl NotificationServer {
    /// Create a server keeping up to `capacity` notifications in its history.
    pub fn new(capacity: usize) -> Self {
        NotificationServer {
            history: NotificationHistory::new(capacity),
            // Ids are non-zero: 0 is reserved for "let the server assign".
            next_id: 1,
            read: BTreeSet::new(),
        }
    }

    /// The history ring.
    pub fn history(&self) -> &NotificationHistory {
        &self.history
    }

    /// Ids in the history the user has not seen, in arrival order.
    pub fn unread_ids(&self) -> Vec<u32> {
        self.history
            .iter()
            .map(|n| n.id)
            .filter(|id| !self.read.contains(id))
            .collect()
    }

    /// True when `id` exists and is still unread.
    pub fn is_unread(&self, id: u32) -> bool {
        self.history.get(id).is_some() && !self.read.contains(&id)
    }

    /// Mark one notification as read. Returns whether it existed.
    pub fn mark_read(&mut self, id: u32) -> bool {
        if self.history.get(id).is_none() {
            return false;
        }
        self.read.insert(id);
        true
    }

    /// Mark every stored notification as read (the badge clears).
    pub fn mark_all_read(&mut self) {
        for n in self.history.iter() {
            self.read.insert(n.id);
        }
    }

    /// `Notify`. Reuses `replaces_id` when it names an existing notification,
    /// otherwise assigns a fresh non-zero id. Returns the notification id.
    pub fn notify(&mut self, request: NotifyRequest, now_unix: u64) -> u32 {
        if request.replaces_id != 0 {
            if let Some(existing) = self.history.get_mut(request.replaces_id) {
                existing.app_name = request.app_name;
                existing.app_icon = request.app_icon;
                existing.summary = request.summary;
                existing.body = request.body;
                existing.actions = request.actions;
                existing.urgency = request.urgency;
                existing.expire_timeout = request.expire_timeout;
                existing.received_at_unix = now_unix;
                let id = existing.id;
                // Replaced content is new information: it is unread again.
                self.read.remove(&id);
                return id;
            }
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.history.push(Notification {
            id,
            app_name: request.app_name,
            app_icon: request.app_icon,
            summary: request.summary,
            body: request.body,
            actions: request.actions,
            urgency: request.urgency,
            expire_timeout: request.expire_timeout,
            received_at_unix: now_unix,
        });
        id
    }

    /// `CloseNotification`. Returns whether the id existed.
    pub fn close_notification(&mut self, id: u32) -> bool {
        self.read.remove(&id);
        self.history.remove(id)
    }

    /// `GetCapabilities`.
    pub fn get_capabilities(&self) -> Vec<&'static str> {
        // Only capabilities Glaze actually implements are advertised.
        vec!["body", "actions", "icon-static", "persistence"]
    }

    /// `GetServerInformation`.
    pub fn get_server_information(&self) -> ServerInformation {
        ServerInformation::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(summary: &str) -> NotifyRequest {
        NotifyRequest {
            app_name: "Test".into(),
            summary: summary.into(),
            body: "body".into(),
            ..Default::default()
        }
    }

    #[test]
    fn name_claim_refuses_when_already_owned() {
        assert_eq!(decide_name_claim(true), NameClaim::AlreadyOwned);
        assert_eq!(decide_name_claim(false), NameClaim::Acquired);
    }

    #[test]
    fn notify_assigns_increasing_nonzero_ids() {
        let mut s = NotificationServer::default();
        assert_eq!(s.notify(req("a"), 100), 1);
        assert_eq!(s.notify(req("b"), 101), 2);
        assert_eq!(s.history().len(), 2);
        assert_eq!(s.history().newest().unwrap().id, 2);
    }

    #[test]
    fn replaces_id_updates_in_place() {
        let mut s = NotificationServer::default();
        let id = s.notify(req("first"), 100);
        let mut replacement = req("second");
        replacement.replaces_id = id;
        let new_id = s.notify(replacement, 200);
        assert_eq!(new_id, id, "replacement keeps the original id");
        assert_eq!(s.history().len(), 1);
        let n = s.history().get(id).unwrap();
        assert_eq!(n.summary, "second");
        assert_eq!(n.received_at_unix, 200);
    }

    #[test]
    fn replacing_unknown_id_allocates_new() {
        let mut s = NotificationServer::default();
        let mut r = req("x");
        r.replaces_id = 999;
        assert_eq!(s.notify(r, 0), 1);
    }

    #[test]
    fn close_notification_removes_and_reports() {
        let mut s = NotificationServer::default();
        let id = s.notify(req("a"), 0);
        assert!(s.close_notification(id));
        assert!(!s.close_notification(id));
        assert!(s.history().is_empty());
    }

    #[test]
    fn history_ring_evicts_oldest() {
        let mut h = NotificationHistory::new(2);
        for i in 1..=3u32 {
            h.push(Notification {
                id: i,
                app_name: String::new(),
                app_icon: String::new(),
                summary: format!("n{i}"),
                body: String::new(),
                actions: vec![],
                urgency: Urgency::Normal,
                expire_timeout: 0,
                received_at_unix: 0,
            });
        }
        assert_eq!(h.len(), 2);
        assert!(h.get(1).is_none(), "oldest evicted");
        assert!(h.get(2).is_some());
        assert!(h.get(3).is_some());
    }

    #[test]
    fn zero_capacity_keeps_nothing() {
        let mut h = NotificationHistory::new(0);
        h.push(Notification {
            id: 1,
            app_name: String::new(),
            app_icon: String::new(),
            summary: "x".into(),
            body: String::new(),
            actions: vec![],
            urgency: Urgency::Normal,
            expire_timeout: 0,
            received_at_unix: 0,
        });
        assert!(h.is_empty());
    }

    #[test]
    fn urgency_maps_hint_bytes() {
        assert_eq!(Urgency::from_byte(0), Urgency::Low);
        assert_eq!(Urgency::from_byte(1), Urgency::Normal);
        assert_eq!(Urgency::from_byte(2), Urgency::Critical);
        assert_eq!(Urgency::from_byte(9), Urgency::Normal);
        assert_eq!(Urgency::Critical.to_byte(), 2);
    }

    #[test]
    fn capabilities_and_server_info_are_honest() {
        let s = NotificationServer::default();
        let caps = s.get_capabilities();
        assert!(caps.contains(&"body"));
        assert!(caps.contains(&"actions"));
        let info = s.get_server_information();
        assert_eq!(info.name, "Glaze");
        assert_eq!(info.spec_version, "1.2");
    }
}
