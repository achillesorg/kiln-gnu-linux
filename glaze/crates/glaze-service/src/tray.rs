//! StatusNotifierItem / AppIndicator **model** for the Glaze system tray.
//!
//! The real tray is the freedesktop `org.kde.StatusNotifierWatcher` +
//! `org.kde.StatusNotifierItem` protocol (the modern KDE/AppIndicator mechanism,
//! `GLAZE.md` §4.3 — "no fake icons"). Items register with a running watcher over
//! `RegisterStatusNotifierItem`; hosts register with `RegisterStatusNotifierHost`.
//!
//! Every struct and the whole [`TrayRegistry`] are portable and unit-tested
//! everywhere. Only the zbus watcher that owns the bus name and receives those
//! calls lives behind `#[cfg(all(unix, feature = "dbus"))]` in [`crate::dbus`].

use std::collections::{BTreeMap, BTreeSet};

/// The watcher's well-known bus name.
pub const WATCHER_BUS_NAME: &str = "org.kde.StatusNotifierWatcher";
/// The watcher's object path.
pub const WATCHER_OBJECT_PATH: &str = "/StatusNotifierWatcher";
/// Default object path an item exposes when only a bus name is given.
pub const DEFAULT_ITEM_PATH: &str = "/StatusNotifierItem";

/// `StatusNotifierItem.Status` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayStatus {
    /// The item is not showing anything (e.g. dropped its icon).
    Passive,
    /// The item is showing normally.
    #[default]
    Active,
    /// The item wants attention.
    NeedsAttention,
}

impl TrayStatus {
    /// Parse the wire string.
    ///
    /// The StatusNotifierItem spec sends `"Passive"`, `"Active"` and
    /// `"NeedsAttention"`, but some AppIndicator implementations send them
    /// lower-cased, so matching is **case-insensitive**. Unknown values fall back
    /// to [`TrayStatus::Active`], the spec default — an item stays visible rather
    /// than being dropped.
    pub fn from_wire(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("Passive") {
            TrayStatus::Passive
        } else if s.eq_ignore_ascii_case("NeedsAttention") {
            TrayStatus::NeedsAttention
        } else {
            TrayStatus::Active
        }
    }

    /// The wire string for this status.
    pub fn as_wire(self) -> &'static str {
        match self {
            TrayStatus::Passive => "Passive",
            TrayStatus::Active => "Active",
            TrayStatus::NeedsAttention => "NeedsAttention",
        }
    }
}

/// `StatusNotifierItem.Category` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayCategory {
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
    #[default]
    Other,
}

impl TrayCategory {
    /// Parse the wire string (case-insensitively; the spec sends PascalCase but
    /// implementations vary). Unknown values fall back to
    /// [`TrayCategory::Other`].
    pub fn from_wire(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("ApplicationStatus") {
            TrayCategory::ApplicationStatus
        } else if s.eq_ignore_ascii_case("Communications") {
            TrayCategory::Communications
        } else if s.eq_ignore_ascii_case("SystemServices") {
            TrayCategory::SystemServices
        } else if s.eq_ignore_ascii_case("Hardware") {
            TrayCategory::Hardware
        } else {
            TrayCategory::Other
        }
    }

    /// The wire string for this category.
    pub fn as_wire(self) -> &'static str {
        match self {
            TrayCategory::ApplicationStatus => "ApplicationStatus",
            TrayCategory::Communications => "Communications",
            TrayCategory::SystemServices => "SystemServices",
            TrayCategory::Hardware => "Hardware",
            TrayCategory::Other => "Other",
        }
    }
}

/// Arguments of `RegisterStatusNotifierItem`.
///
/// Per the spec the argument is either a bus name or an object path; when it is a
/// path the item's bus name is the caller's unique name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusNotifierItemRegistration {
    pub service: String,
    pub path: String,
}

impl StatusNotifierItemRegistration {
    /// Resolve the spec's "service *or* path" argument into a service + path.
    pub fn resolve(service_or_path: &str, sender_unique_name: Option<&str>) -> Self {
        if service_or_path.starts_with('/') {
            StatusNotifierItemRegistration {
                service: sender_unique_name.unwrap_or_default().to_string(),
                path: service_or_path.to_string(),
            }
        } else {
            StatusNotifierItemRegistration {
                service: service_or_path.to_string(),
                path: DEFAULT_ITEM_PATH.to_string(),
            }
        }
    }

    /// Stable registry id for this registration.
    pub fn id(&self) -> String {
        format!("{}:{}", self.service, self.path)
    }
}

/// Arguments of `RegisterStatusNotifierHost`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusNotifierHostRegistration {
    pub service: String,
}

/// Metadata for one tray item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    pub id: String,
    pub service: String,
    pub path: String,
    pub title: String,
    pub icon: Option<String>,
    pub status: TrayStatus,
    pub category: TrayCategory,
}

/// The live set of registered tray items and hosts.
#[derive(Debug, Default, Clone)]
pub struct TrayRegistry {
    items: BTreeMap<String, TrayItem>,
    hosts: BTreeSet<String>,
}

impl TrayRegistry {
    /// Register (or refresh) an item. Returns its stable id.
    pub fn register_item(&mut self, reg: StatusNotifierItemRegistration) -> String {
        let id = reg.id();
        self.items.entry(id.clone()).or_insert(TrayItem {
            id: id.clone(),
            service: reg.service,
            path: reg.path,
            title: String::new(),
            icon: None,
            status: TrayStatus::Active,
            category: TrayCategory::Other,
        });
        id
    }

    /// Insert or replace an item keyed by its id.
    ///
    /// This is the watcher's upsert path: a StatusNotifierItem often sends its
    /// metadata in several signals (title, then icon, then status), and each one
    /// must update the same entry in place without moving it.
    pub fn upsert(&mut self, item: TrayItem) {
        self.items.insert(item.id.clone(), item);
    }

    /// Mutate an item in place. Returns whether it existed.
    ///
    /// The signal-driven updates (title, icon, status, category) each change one
    /// field; in-place mutation keeps the item's position stable so tray icons
    /// never jump when an application refreshes.
    pub fn update<F>(&mut self, id: &str, f: F) -> bool
    where
        F: FnOnce(&mut TrayItem),
    {
        match self.items.get_mut(id) {
            Some(item) => {
                f(item);
                true
            }
            None => false,
        }
    }

    /// Remove an item. Returns whether it was present.
    pub fn unregister_item(&mut self, id: &str) -> bool {
        self.items.remove(id).is_some()
    }

    /// Update an item's metadata. Returns whether the item existed.
    pub fn update_metadata(
        &mut self,
        id: &str,
        title: &str,
        icon: Option<String>,
        status: TrayStatus,
        category: TrayCategory,
    ) -> bool {
        match self.items.get_mut(id) {
            Some(item) => {
                item.title = title.to_string();
                item.icon = icon;
                item.status = status;
                item.category = category;
                true
            }
            None => false,
        }
    }

    /// Register a host. Returns whether it was newly added.
    pub fn register_host(&mut self, reg: StatusNotifierHostRegistration) -> bool {
        self.hosts.insert(reg.service)
    }

    /// Remove a host. Returns whether it was present.
    pub fn unregister_host(&mut self, service: &str) -> bool {
        self.hosts.remove(service)
    }

    /// True when at least one host is registered.
    pub fn has_host(&self) -> bool {
        !self.hosts.is_empty()
    }

    /// Registered hosts, in stable order.
    pub fn hosts(&self) -> Vec<&str> {
        self.hosts.iter().map(String::as_str).collect()
    }

    /// An item by id.
    pub fn get(&self, id: &str) -> Option<&TrayItem> {
        self.items.get(id)
    }

    /// All items, ordered by id (deterministic for the panel).
    pub fn items(&self) -> Vec<&TrayItem> {
        self.items.values().collect()
    }

    /// Number of registered items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when no items are registered.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bus_name_and_object_path_arguments() {
        let by_name = StatusNotifierItemRegistration::resolve(":1.42", None);
        assert_eq!(by_name.service, ":1.42");
        assert_eq!(by_name.path, DEFAULT_ITEM_PATH);

        let by_path = StatusNotifierItemRegistration::resolve("/StatusNotifierItem", Some(":1.7"));
        assert_eq!(by_path.service, ":1.7");
        assert_eq!(by_path.path, "/StatusNotifierItem");

        assert_eq!(by_name.id(), ":1.42:/StatusNotifierItem");
        assert_ne!(by_name.id(), by_path.id());
    }

    #[test]
    fn registers_items_idempotently() {
        let mut reg = TrayRegistry::default();
        let r = StatusNotifierItemRegistration::resolve("org.kde.bluez", None);
        let id1 = reg.register_item(r.clone());
        let id2 = reg.register_item(r);
        assert_eq!(id1, id2);
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn updates_metadata() {
        let mut reg = TrayRegistry::default();
        let id = reg.register_item(StatusNotifierItemRegistration::resolve("app", None));
        assert!(reg.update_metadata(
            &id,
            "Volume",
            Some("audio-volume-high".into()),
            TrayStatus::NeedsAttention,
            TrayCategory::Hardware,
        ));
        let item = reg.get(&id).unwrap();
        assert_eq!(item.title, "Volume");
        assert_eq!(item.icon.as_deref(), Some("audio-volume-high"));
        assert_eq!(item.status, TrayStatus::NeedsAttention);
        assert_eq!(item.category, TrayCategory::Hardware);
        assert!(!reg.update_metadata(
            "missing",
            "x",
            None,
            TrayStatus::Active,
            TrayCategory::Other
        ));
    }

    #[test]
    fn unregisters_items() {
        let mut reg = TrayRegistry::default();
        let id = reg.register_item(StatusNotifierItemRegistration::resolve("app", None));
        assert!(reg.unregister_item(&id));
        assert!(!reg.unregister_item(&id));
        assert!(reg.is_empty());
    }

    #[test]
    fn tracks_hosts() {
        let mut reg = TrayRegistry::default();
        assert!(!reg.has_host());
        assert!(reg.register_host(StatusNotifierHostRegistration {
            service: ":1.5".into()
        }));
        assert!(!reg.register_host(StatusNotifierHostRegistration {
            service: ":1.5".into()
        }));
        assert!(reg.has_host());
        assert_eq!(reg.hosts(), vec![":1.5"]);
        assert!(reg.unregister_host(":1.5"));
        assert!(!reg.has_host());
    }

    #[test]
    fn status_and_category_round_trip_on_wire() {
        for s in [
            TrayStatus::Passive,
            TrayStatus::Active,
            TrayStatus::NeedsAttention,
        ] {
            assert_eq!(TrayStatus::from_wire(s.as_wire()), s);
        }
        assert_eq!(TrayStatus::from_wire("nonsense"), TrayStatus::Active);
        for c in [
            TrayCategory::ApplicationStatus,
            TrayCategory::Communications,
            TrayCategory::SystemServices,
            TrayCategory::Hardware,
            TrayCategory::Other,
        ] {
            assert_eq!(TrayCategory::from_wire(c.as_wire()), c);
        }
        assert_eq!(TrayCategory::from_wire("nonsense"), TrayCategory::Other);
    }

    #[test]
    fn items_are_ordered_by_id() {
        let mut reg = TrayRegistry::default();
        reg.register_item(StatusNotifierItemRegistration::resolve(":1.9", None));
        reg.register_item(StatusNotifierItemRegistration::resolve(":1.2", None));
        let ids: Vec<&str> = reg.items().iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![":1.2:/StatusNotifierItem", ":1.9:/StatusNotifierItem"]
        );
    }
}
