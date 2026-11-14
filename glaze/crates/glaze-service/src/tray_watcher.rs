//! StatusNotifierItem **watcher**: the portable signal-handling half.
//!
//! A `org.kde.StatusNotifierWatcher` receives signals from status items (and
//! from the host) and keeps a live set of them. All of the mapping logic lives
//! here — portable and unit-tested on any host — and the D-Bus connection that
//! actually delivers the signals lives behind
//! `#[cfg(all(unix, feature = "dbus"))]` in [`crate::dbus`].
//!
//! Nothing is invented: a metadata signal for an id the registry does not know
//! is reported as [`WatcherOutcome::Ignored`], never created.

use crate::tray::{
    StatusNotifierHostRegistration, StatusNotifierItemRegistration, TrayCategory, TrayRegistry,
    TrayStatus,
};

/// A signal the watcher must handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SniSignal {
    /// `RegisterStatusNotifierItem(service_or_path)` from `sender`.
    ///
    /// The spec allows either a bus name or an object path; a path means "the
    /// sender's own unique name owns it" (`GLAZE.md` §4.3 — real items only).
    RegisterItem {
        service_or_path: String,
        sender: Option<String>,
    },
    /// The item's well-known name vanished, or it asked to be removed.
    UnregisterItem {
        service_or_path: String,
        sender: Option<String>,
    },
    /// `NewTitle` — the item's tooltip/title text.
    NewTitle { id: String, title: String },
    /// `NewIcon` — an icon name or absolute path.
    NewIcon { id: String, icon: Option<String> },
    /// `NewStatus` — `passive` / `active` / `needsattention`.
    NewStatus { id: String, status: TrayStatus },
    /// `NewCategory` — the item's functional category.
    NewCategory { id: String, category: TrayCategory },
    /// `RegisterStatusNotifierHost(service)`.
    RegisterHost { service: String },
    /// `UnregisterStatusNotifierHost(service)`.
    UnregisterHost { service: String },
}

/// Map an **item-side** StatusNotifier signal onto the watcher signal that
/// applies to `id`.
///
/// A StatusNotifierItem emits `NewTitle`, `NewIcon`, `NewStatus` and
/// `NewCategory` on its own interface; the host listens and forwards them here.
/// `member` is the D-Bus signal name and `text` is its first string argument
/// where it has one.
///
/// Signals Glaze does not model return `None`, so the caller ignores them rather
/// than guessing:
/// - `NewToolTip` carries a structured `(icon, icon-pixmap, title, text)` tuple;
///   mapping only its title would be lossy, so it is deliberately unmapped.
/// - any unknown member, or a text-carrying signal with no text, is unmapped.
pub fn item_signal(id: &str, member: &str, text: Option<&str>) -> Option<SniSignal> {
    match member {
        "NewTitle" => text.map(|title| SniSignal::NewTitle {
            id: id.to_string(),
            title: title.to_string(),
        }),
        "NewIcon" => Some(SniSignal::NewIcon {
            id: id.to_string(),
            icon: text.filter(|icon| !icon.is_empty()).map(str::to_string),
        }),
        "NewStatus" => text.map(|status| SniSignal::NewStatus {
            id: id.to_string(),
            status: TrayStatus::from_wire(status),
        }),
        "NewCategory" => text.map(|category| SniSignal::NewCategory {
            id: id.to_string(),
            category: TrayCategory::from_wire(category),
        }),
        _ => None,
    }
}

/// What applying a signal did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherOutcome {
    /// An item appeared; carries its stable registry id.
    Registered(String),
    /// An item disappeared.
    Removed(String),
    /// An item's metadata changed.
    Updated(String),
    /// A host registered (or was already known).
    HostRegistered { service: String, newly: bool },
    /// A host went away (or was unknown).
    HostRemoved { service: String, existed: bool },
    /// The signal could not be applied (unknown id) — nothing was invented.
    Ignored { reason: &'static str },
}

/// The watcher: owns the tray registry and applies item/host signals to it.
#[derive(Debug, Default)]
pub struct TrayWatcher {
    registry: TrayRegistry,
}

impl TrayWatcher {
    /// A watcher with no items.
    pub fn new() -> Self {
        Self::default()
    }

    /// The registry the shell reads through the IPC `ListTray` request.
    pub fn registry(&self) -> &TrayRegistry {
        &self.registry
    }

    /// Mutable registry access (seed data, tests, non-signal updates).
    pub fn registry_mut(&mut self) -> &mut TrayRegistry {
        &mut self.registry
    }

    /// Consume the watcher, yielding its registry.
    pub fn into_registry(self) -> TrayRegistry {
        self.registry
    }

    /// The stable id a registration signal maps to.
    fn id_for(service_or_path: &str, sender: Option<&str>) -> String {
        StatusNotifierItemRegistration::resolve(service_or_path, sender).id()
    }

    /// Apply one signal.
    pub fn apply(&mut self, signal: SniSignal) -> WatcherOutcome {
        match signal {
            SniSignal::RegisterItem {
                service_or_path,
                sender,
            } => {
                let registration =
                    StatusNotifierItemRegistration::resolve(&service_or_path, sender.as_deref());
                let id = self.registry.register_item(registration);
                WatcherOutcome::Registered(id)
            }
            SniSignal::UnregisterItem {
                service_or_path,
                sender,
            } => {
                let id = Self::id_for(&service_or_path, sender.as_deref());
                if self.registry.unregister_item(&id) {
                    WatcherOutcome::Removed(id)
                } else {
                    WatcherOutcome::Ignored {
                        reason: "unknown item",
                    }
                }
            }
            SniSignal::NewTitle { id, title } => self.update(&id, |item| item.title = title),
            SniSignal::NewIcon { id, icon } => self.update(&id, |item| item.icon = icon),
            SniSignal::NewStatus { id, status } => self.update(&id, |item| item.status = status),
            SniSignal::NewCategory { id, category } => {
                self.update(&id, |item| item.category = category)
            }
            SniSignal::RegisterHost { service } => {
                let newly = self.registry.register_host(StatusNotifierHostRegistration {
                    service: service.clone(),
                });
                WatcherOutcome::HostRegistered { service, newly }
            }
            SniSignal::UnregisterHost { service } => {
                let existed = self.registry.unregister_host(&service);
                WatcherOutcome::HostRemoved { service, existed }
            }
        }
    }

    /// Apply a one-field metadata change, reporting an unknown id honestly.
    fn update<F>(&mut self, id: &str, f: F) -> WatcherOutcome
    where
        F: FnOnce(&mut crate::tray::TrayItem),
    {
        if self.registry.update(id, f) {
            WatcherOutcome::Updated(id.to_string())
        } else {
            WatcherOutcome::Ignored {
                reason: "unknown item",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watcher_with_item() -> (TrayWatcher, String) {
        let mut watcher = TrayWatcher::new();
        let outcome = watcher.apply(SniSignal::RegisterItem {
            service_or_path: "org.example.Volume".into(),
            sender: None,
        });
        let WatcherOutcome::Registered(id) = outcome else {
            panic!("expected Registered, got {outcome:?}");
        };
        (watcher, id)
    }

    #[test]
    fn registering_an_item_creates_it_with_sane_defaults() {
        let (watcher, id) = watcher_with_item();
        assert_eq!(id, "org.example.Volume:/StatusNotifierItem");
        let item = watcher.registry().get(&id).expect("item exists");
        assert_eq!(item.service, "org.example.Volume");
        assert_eq!(item.status, TrayStatus::Active);
        assert_eq!(item.category, TrayCategory::Other);
        assert!(item.title.is_empty());
    }

    #[test]
    fn a_path_registration_uses_the_sender_as_the_service() {
        let mut watcher = TrayWatcher::new();
        let outcome = watcher.apply(SniSignal::RegisterItem {
            service_or_path: "/StatusNotifierItem".into(),
            sender: Some(":1.42".into()),
        });
        assert_eq!(
            outcome,
            WatcherOutcome::Registered(":1.42:/StatusNotifierItem".into())
        );
    }

    #[test]
    fn metadata_signals_update_in_place_and_keep_the_position() {
        let (mut watcher, id) = watcher_with_item();
        let register_second = watcher.apply(SniSignal::RegisterItem {
            service_or_path: "org.example.Network".into(),
            sender: None,
        });
        assert!(matches!(register_second, WatcherOutcome::Registered(_)));

        assert_eq!(
            watcher.apply(SniSignal::NewTitle {
                id: id.clone(),
                title: "Volume 42%".into()
            }),
            WatcherOutcome::Updated(id.clone())
        );
        watcher.apply(SniSignal::NewIcon {
            id: id.clone(),
            icon: Some("audio-volume-high".into()),
        });
        watcher.apply(SniSignal::NewStatus {
            id: id.clone(),
            status: TrayStatus::NeedsAttention,
        });
        watcher.apply(SniSignal::NewCategory {
            id: id.clone(),
            category: TrayCategory::Hardware,
        });

        let item = watcher.registry().get(&id).expect("item exists");
        assert_eq!(item.title, "Volume 42%");
        assert_eq!(item.icon.as_deref(), Some("audio-volume-high"));
        assert_eq!(item.status, TrayStatus::NeedsAttention);
        assert_eq!(item.category, TrayCategory::Hardware);

        // The registry keeps items in a BTreeMap, so `items()` is deterministic
        // and ordered by id — an update never changes an item's key, so nothing
        // jumps when an application refreshes its metadata.
        let ids: Vec<&str> = watcher
            .registry()
            .items()
            .iter()
            .map(|i| i.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "org.example.Network:/StatusNotifierItem",
                "org.example.Volume:/StatusNotifierItem"
            ]
        );
    }

    #[test]
    fn unregistering_removes_the_item() {
        let (mut watcher, id) = watcher_with_item();
        assert_eq!(
            watcher.apply(SniSignal::UnregisterItem {
                service_or_path: "org.example.Volume".into(),
                sender: None,
            }),
            WatcherOutcome::Removed(id)
        );
        assert!(watcher.registry().is_empty());
    }

    #[test]
    fn metadata_for_an_unknown_item_is_ignored_not_invented() {
        let mut watcher = TrayWatcher::new();
        assert_eq!(
            watcher.apply(SniSignal::NewTitle {
                id: "ghost:/StatusNotifierItem".into(),
                title: "Ghost".into()
            }),
            WatcherOutcome::Ignored {
                reason: "unknown item"
            }
        );
        assert!(watcher.registry().is_empty(), "nothing may be invented");
    }

    #[test]
    fn unregistering_an_unknown_item_is_ignored() {
        let mut watcher = TrayWatcher::new();
        assert_eq!(
            watcher.apply(SniSignal::UnregisterItem {
                service_or_path: "org.example.Gone".into(),
                sender: None,
            }),
            WatcherOutcome::Ignored {
                reason: "unknown item"
            }
        );
    }

    #[test]
    fn item_side_signals_map_onto_watcher_signals() {
        let id = "org.example.Volume:/StatusNotifierItem";
        assert_eq!(
            item_signal(id, "NewTitle", Some("Volume 42%")),
            Some(SniSignal::NewTitle {
                id: id.into(),
                title: "Volume 42%".into()
            })
        );
        assert_eq!(
            item_signal(id, "NewIcon", Some("audio-volume-high")),
            Some(SniSignal::NewIcon {
                id: id.into(),
                icon: Some("audio-volume-high".into())
            })
        );
        // An empty icon name clears the icon instead of pointing at "".
        assert_eq!(
            item_signal(id, "NewIcon", Some("")),
            Some(SniSignal::NewIcon {
                id: id.into(),
                icon: None
            })
        );
        // The spec sends PascalCase; lowercase (sent by some implementations)
        // must still be understood rather than silently treated as Active.
        for status in ["NeedsAttention", "needsattention"] {
            assert_eq!(
                item_signal(id, "NewStatus", Some(status)),
                Some(SniSignal::NewStatus {
                    id: id.into(),
                    status: TrayStatus::NeedsAttention
                }),
                "status {status:?} must map to NeedsAttention"
            );
        }
        for category in ["Hardware", "hardware"] {
            assert_eq!(
                item_signal(id, "NewCategory", Some(category)),
                Some(SniSignal::NewCategory {
                    id: id.into(),
                    category: TrayCategory::Hardware
                }),
                "category {category:?} must map to Hardware"
            );
        }
    }

    #[test]
    fn unmapped_item_signals_are_ignored_not_guessed() {
        let id = "org.example.Volume:/StatusNotifierItem";
        // NewToolTip carries a structured tuple; mapping only its title is lossy.
        assert_eq!(item_signal(id, "NewToolTip", Some("Tooltip")), None);
        assert_eq!(item_signal(id, "NewAttentionIcon", Some("icon")), None);
        // A text-carrying signal with no text cannot invent one.
        assert_eq!(item_signal(id, "NewTitle", None), None);
        assert_eq!(item_signal(id, "NewStatus", None), None);
    }

    /// The host path end to end: an item-side signal updates the registered item.
    #[test]
    fn item_side_signals_update_a_registered_item() {
        let (mut watcher, id) = watcher_with_item();
        for (member, text) in [
            ("NewTitle", Some("Volume 42%")),
            ("NewIcon", Some("audio-volume-high")),
            ("NewStatus", Some("needsattention")),
            ("NewCategory", Some("hardware")),
        ] {
            let signal = item_signal(&id, member, text).expect("mapped");
            assert_eq!(watcher.apply(signal), WatcherOutcome::Updated(id.clone()));
        }
        let item = watcher.registry().get(&id).expect("item exists");
        assert_eq!(item.title, "Volume 42%");
        assert_eq!(item.icon.as_deref(), Some("audio-volume-high"));
        assert_eq!(item.status, TrayStatus::NeedsAttention);
        assert_eq!(item.category, TrayCategory::Hardware);
    }

    #[test]
    fn host_registration_is_tracked() {
        let mut watcher = TrayWatcher::new();
        assert_eq!(
            watcher.apply(SniSignal::RegisterHost {
                service: "org.glaze.Shell".into()
            }),
            WatcherOutcome::HostRegistered {
                service: "org.glaze.Shell".into(),
                newly: true
            }
        );
        // Registering twice is not an error, just not new.
        assert_eq!(
            watcher.apply(SniSignal::RegisterHost {
                service: "org.glaze.Shell".into()
            }),
            WatcherOutcome::HostRegistered {
                service: "org.glaze.Shell".into(),
                newly: false
            }
        );
        assert!(watcher.registry().has_host());
        assert_eq!(
            watcher.apply(SniSignal::UnregisterHost {
                service: "org.glaze.Shell".into()
            }),
            WatcherOutcome::HostRemoved {
                service: "org.glaze.Shell".into(),
                existed: true
            }
        );
        assert!(!watcher.registry().has_host());
    }
}
