//! Portable model of `wlr-foreign-toplevel-management`.
//!
//! The preferred Glaze backend observes windows through the standard
//! `wlr-foreign-toplevel-management` Wayland protocol (see
//! `docs/adr/ADR-0002-compositor-strategy.md`). That protocol delivers a
//! `zwlr_foreign_toplevel_handle_v1` per window plus a stream of small events:
//! `title`, `app_id`, `state` (`maximized`/`minimized`/`activated`/`fullscreen`),
//! `output_enter`/`output_leave`, `done` and `closed`.
//!
//! [`ToplevelRegistry`] is a faithful, transport-free reduction of those events
//! into [`glaze_common::model::WindowInfo`] values. It is compiled and
//! unit-tested on every host; the live Wayland client that feeds it events is the
//! only piece that needs a platform transport.

use std::cell::RefCell;
use std::collections::BTreeMap;

use glaze_common::model::{WindowInfo, WindowKind};

use crate::{Backend, BackendError};

/// A `zwlr_foreign_toplevel_handle_v1` identifier.
pub type HandleId = u32;

/// One event from a toplevel handle, reduced to what the taskbar needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToplevelEvent {
    /// A new handle appeared (`new_toplevel`), with its first `app_id`/`title`.
    Added {
        handle: HandleId,
        app_id: Option<String>,
        title: Option<String>,
    },
    /// `title` event.
    TitleChanged { handle: HandleId, title: String },
    /// `app_id` event.
    AppIdChanged { handle: HandleId, app_id: String },
    /// `state` event: the `activated` bit toggled.
    Activated { handle: HandleId, active: bool },
    /// `state` event: the `minimized` bit toggled.
    Minimized { handle: HandleId, minimized: bool },
    /// `closed` event.
    Closed { handle: HandleId },
}

/// The registry of live toplevels, keyed by handle id.
///
/// Focus is modelled as the `activated` state, and at most one toplevel is
/// focused at a time — matching how compositors drive the protocol.
#[derive(Debug, Default, Clone)]
pub struct ToplevelRegistry {
    by_handle: BTreeMap<HandleId, WindowInfo>,
    focused: Option<HandleId>,
}

impl ToplevelRegistry {
    /// Insert a new toplevel. Returns `false` if the handle already existed.
    pub fn add(&mut self, handle: HandleId, app_id: Option<String>, title: Option<String>) -> bool {
        if self.by_handle.contains_key(&handle) {
            return false;
        }
        self.by_handle.insert(
            handle,
            WindowInfo {
                id: handle.to_string(),
                app_id,
                title: title.unwrap_or_default(),
                workspace: None,
                kind: WindowKind::Normal,
                focused: false,
                minimized: false,
                floating: false,
            },
        );
        true
    }

    /// Remove a toplevel (`closed`). Returns the removed window, if any.
    pub fn remove(&mut self, handle: HandleId) -> Option<WindowInfo> {
        let removed = self.by_handle.remove(&handle);
        if removed.is_some() && self.focused == Some(handle) {
            self.focused = None;
        }
        removed
    }

    /// True when the handle is tracked.
    pub fn contains(&self, handle: HandleId) -> bool {
        self.by_handle.contains_key(&handle)
    }

    /// Update the title. Returns `false` if the handle is unknown.
    pub fn set_title(&mut self, handle: HandleId, title: impl Into<String>) -> bool {
        match self.by_handle.get_mut(&handle) {
            Some(w) => {
                w.title = title.into();
                true
            }
            None => false,
        }
    }

    /// Update the app id. Returns `false` if the handle is unknown.
    pub fn set_app_id(&mut self, handle: HandleId, app_id: impl Into<String>) -> bool {
        match self.by_handle.get_mut(&handle) {
            Some(w) => {
                w.app_id = Some(app_id.into());
                true
            }
            None => false,
        }
    }

    /// Set the `activated` state, moving focus to this toplevel and clearing it
    /// from every other one. Returns `false` if the handle is unknown.
    pub fn set_activated(&mut self, handle: HandleId, active: bool) -> bool {
        if !self.by_handle.contains_key(&handle) {
            return false;
        }
        if active {
            for (id, w) in self.by_handle.iter_mut() {
                w.focused = *id == handle;
            }
            self.focused = Some(handle);
        } else {
            if let Some(w) = self.by_handle.get_mut(&handle) {
                w.focused = false;
            }
            if self.focused == Some(handle) {
                self.focused = None;
            }
        }
        true
    }

    /// Set the `minimized` state. Returns `false` if the handle is unknown.
    pub fn set_minimized(&mut self, handle: HandleId, minimized: bool) -> bool {
        match self.by_handle.get_mut(&handle) {
            Some(w) => {
                w.minimized = minimized;
                true
            }
            None => false,
        }
    }

    /// The currently focused handle, if any.
    pub fn focused_handle(&self) -> Option<HandleId> {
        self.focused
    }

    /// A tracked window by handle.
    pub fn get(&self, handle: HandleId) -> Option<&WindowInfo> {
        self.by_handle.get(&handle)
    }

    /// Apply one protocol event.
    pub fn apply(&mut self, event: ToplevelEvent) {
        match event {
            ToplevelEvent::Added {
                handle,
                app_id,
                title,
            } => {
                self.add(handle, app_id, title);
            }
            ToplevelEvent::TitleChanged { handle, title } => {
                self.set_title(handle, title);
            }
            ToplevelEvent::AppIdChanged { handle, app_id } => {
                self.set_app_id(handle, app_id);
            }
            ToplevelEvent::Activated { handle, active } => {
                self.set_activated(handle, active);
            }
            ToplevelEvent::Minimized { handle, minimized } => {
                self.set_minimized(handle, minimized);
            }
            ToplevelEvent::Closed { handle } => {
                self.remove(handle);
            }
        }
    }

    /// All live windows, ordered by handle id (deterministic for the taskbar).
    pub fn windows(&self) -> Vec<WindowInfo> {
        self.by_handle.values().cloned().collect()
    }

    /// Number of tracked toplevels.
    pub fn len(&self) -> usize {
        self.by_handle.len()
    }

    /// True when no toplevel is tracked.
    pub fn is_empty(&self) -> bool {
        self.by_handle.is_empty()
    }

    /// Iterate over `(handle, window)` pairs in handle order.
    pub fn iter(&self) -> impl Iterator<Item = (HandleId, &WindowInfo)> {
        self.by_handle.iter().map(|(h, w)| (*h, w))
    }
}

/// A [`Backend`] that surfaces a [`ToplevelRegistry`].
///
/// The registry is complete and tested today; the live
/// `zwlr_foreign_toplevel_management_v1` client is a separate transport step
/// (recorded in `docs/adr/ADR-0004-system-integration.md`). Until it is linked,
/// [`ForeignToplevelBackend::connect`] returns an honest `Unsupported` error
/// rather than pretending to observe windows.
pub struct ForeignToplevelBackend {
    registry: RefCell<ToplevelRegistry>,
    connected: bool,
}

impl Default for ForeignToplevelBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ForeignToplevelBackend {
    /// A backend with an empty registry and no live transport.
    pub fn new() -> Self {
        ForeignToplevelBackend {
            registry: RefCell::new(ToplevelRegistry::default()),
            connected: false,
        }
    }

    /// A backend seeded with a registry (offline use and tests).
    pub fn from_registry(registry: ToplevelRegistry) -> Self {
        ForeignToplevelBackend {
            registry: RefCell::new(registry),
            connected: true,
        }
    }

    /// Attach to the compositor's `wlr-foreign-toplevel-management` global.
    ///
    /// The portable registry already models the full protocol; the live Wayland
    /// client is the remaining transport work, so this refuses honestly until it
    /// exists instead of fabricating an empty "connected" backend.
    #[cfg(unix)]
    pub fn connect() -> Result<Self, BackendError> {
        Err(BackendError::Unsupported(
            "live wlr-foreign-toplevel Wayland transport is not linked in this build; \
             feed events via ForeignToplevelBackend::apply_event",
        ))
    }

    /// Refer to [`Self::connect`]; on non-Unix hosts there is no Wayland at all.
    #[cfg(not(unix))]
    pub fn connect() -> Result<Self, BackendError> {
        Err(BackendError::Unsupported(
            "wlr-foreign-toplevel-management requires a Wayland session (Unix)",
        ))
    }

    /// Apply one toplevel event to the registry (transport entry point).
    pub fn apply_event(&self, event: ToplevelEvent) {
        self.registry.borrow_mut().apply(event);
    }

    /// A snapshot of the current registry contents.
    pub fn snapshot(&self) -> ToplevelRegistry {
        self.registry.borrow().clone()
    }
}

impl Backend for ForeignToplevelBackend {
    fn name(&self) -> &'static str {
        "foreign-toplevel"
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn windows(&self) -> Result<Vec<WindowInfo>, BackendError> {
        if self.connected {
            Ok(self.registry.borrow().windows())
        } else {
            Err(BackendError::Unsupported(
                "foreign-toplevel backend is not connected",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_maps_handle_to_window() {
        let mut reg = ToplevelRegistry::default();
        assert!(reg.add(7, Some("firefox".into()), Some("Mozilla".into())));
        assert!(
            !reg.add(7, Some("firefox".into()), None),
            "duplicate handle"
        );
        assert_eq!(reg.len(), 1);
        let w = reg.get(7).unwrap();
        assert_eq!(w.id, "7");
        assert_eq!(w.app_id.as_deref(), Some("firefox"));
        assert_eq!(w.title, "Mozilla");
        assert!(!w.focused);
    }

    #[test]
    fn title_and_app_id_arrive_later() {
        let mut reg = ToplevelRegistry::default();
        reg.add(1, None, None);
        assert_eq!(reg.get(1).unwrap().title, "");
        assert!(reg.get(1).unwrap().app_id.is_none());

        assert!(reg.set_title(1, "Editor"));
        assert!(reg.set_app_id(1, "gedit"));
        let w = reg.get(1).unwrap();
        assert_eq!(w.title, "Editor");
        assert_eq!(w.app_id.as_deref(), Some("gedit"));
    }

    #[test]
    fn activation_moves_focus_and_keeps_it_singular() {
        let mut reg = ToplevelRegistry::default();
        reg.add(1, Some("a".into()), None);
        reg.add(2, Some("b".into()), None);

        assert!(reg.set_activated(2, true));
        assert_eq!(reg.focused_handle(), Some(2));
        assert!(reg.get(2).unwrap().focused);
        assert!(!reg.get(1).unwrap().focused);

        // Activating another toplevel must clear the previous focus.
        assert!(reg.set_activated(1, true));
        assert_eq!(reg.focused_handle(), Some(1));
        assert!(reg.get(1).unwrap().focused);
        assert!(!reg.get(2).unwrap().focused);
    }

    #[test]
    fn deactivation_only_clears_the_named_handle() {
        let mut reg = ToplevelRegistry::default();
        reg.add(1, None, None);
        reg.add(2, None, None);
        reg.set_activated(2, true);
        assert!(reg.set_activated(2, false));
        assert_eq!(reg.focused_handle(), None);
        assert!(!reg.get(2).unwrap().focused);
    }

    #[test]
    fn minimize_state_is_tracked() {
        let mut reg = ToplevelRegistry::default();
        reg.add(1, None, None);
        assert!(reg.set_minimized(1, true));
        assert!(reg.get(1).unwrap().minimized);
        assert!(reg.set_minimized(1, false));
        assert!(!reg.get(1).unwrap().minimized);
    }

    #[test]
    fn closing_removes_and_drops_focus() {
        let mut reg = ToplevelRegistry::default();
        reg.add(3, None, None);
        reg.set_activated(3, true);
        let removed = reg.remove(3).unwrap();
        assert_eq!(removed.id, "3");
        assert_eq!(reg.focused_handle(), None);
        assert!(reg.is_empty());
    }

    #[test]
    fn unknown_handles_are_reported_honestly() {
        let mut reg = ToplevelRegistry::default();
        assert!(!reg.set_title(99, "x"));
        assert!(!reg.set_app_id(99, "x"));
        assert!(!reg.set_activated(99, true));
        assert!(!reg.set_minimized(99, true));
        assert!(reg.remove(99).is_none());
    }

    #[test]
    fn apply_dispatches_every_event() {
        let mut reg = ToplevelRegistry::default();
        reg.apply(ToplevelEvent::Added {
            handle: 5,
            app_id: Some("foot".into()),
            title: Some("stub".into()),
        });
        reg.apply(ToplevelEvent::TitleChanged {
            handle: 5,
            title: "foot".into(),
        });
        reg.apply(ToplevelEvent::AppIdChanged {
            handle: 5,
            app_id: "foot-wayland".into(),
        });
        reg.apply(ToplevelEvent::Activated {
            handle: 5,
            active: true,
        });
        reg.apply(ToplevelEvent::Minimized {
            handle: 5,
            minimized: true,
        });
        let w = reg.get(5).unwrap();
        assert_eq!(w.title, "foot");
        assert_eq!(w.app_id.as_deref(), Some("foot-wayland"));
        assert!(w.focused);
        assert!(w.minimized);

        reg.apply(ToplevelEvent::Closed { handle: 5 });
        assert!(reg.is_empty());
    }

    #[test]
    fn backend_reports_windows_when_seeded() {
        let mut reg = ToplevelRegistry::default();
        reg.add(1, Some("a".into()), Some("A".into()));
        reg.set_activated(1, true);
        let backend = ForeignToplevelBackend::from_registry(reg);
        assert!(backend.is_connected());
        assert_eq!(backend.name(), "foreign-toplevel");
        let windows = backend.windows().unwrap();
        assert_eq!(windows.len(), 1);
        assert!(windows[0].focused);
    }

    #[test]
    fn detached_backend_refuses_to_fake_windows() {
        let backend = ForeignToplevelBackend::new();
        assert!(!backend.is_connected());
        assert!(backend.windows().is_err());
        assert!(ForeignToplevelBackend::connect().is_err());
    }
}
