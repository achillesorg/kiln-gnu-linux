//! Tray / status-item **view model** for the panel (`GLAZE.md` §3.1, §6.1).
//!
//! This is deliberately a *view* model, not the protocol: the wire details
//! (StatusNotifierItem registration, bus names, object paths) live in
//! `glaze-service::tray`. The panel only needs an ordered list of items to draw
//! plus a width-budget rule for when they no longer fit, so it can be unit
//! tested on any host without D-Bus.
//!
//! The [`TrayStatus`] / [`TrayCategory`] enums are re-used from
//! `glaze-service::tray` so there is exactly one definition of the wire values;
//! this crate never invents its own copy that could drift.
//!
//! ```text
//!  glaze-service::tray::TrayRegistry   (protocol model, D-Bus-fed)
//!            │  map each item
//!            ▼
//!  glaze-shell-core::tray::TrayModel   (ordered view model, testable)
//!            │  overflow(...)
//!            ▼
//!  apps/glaze-shell                    (GTK widgets)
//! ```

use glaze_service::tray::{TrayCategory, TrayStatus};

/// Width budget for a single tray icon in logical px.
///
/// Matches the panel's task-icon sizing so the tray and the taskbar use the
/// same optical rhythm; kept here (not in `panel`) because it is a tray concern.
pub const TRAY_ITEM_WIDTH_PX: i32 = 24;

/// One tray / status item as the panel needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    /// Stable id (the service's `service:path` id); upserts key on this.
    pub id: String,
    /// Accessible / tooltip title.
    pub title: String,
    /// Icon name or absolute path, when the item exposes one.
    pub icon: Option<String>,
    /// Whether the item is passive, active or asking for attention.
    pub status: TrayStatus,
    /// The item's functional category.
    pub category: TrayCategory,
}

impl TrayItem {
    /// A minimal item with sensible defaults (active, `Other`).
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        TrayItem {
            id: id.into(),
            title: title.into(),
            icon: None,
            status: TrayStatus::Active,
            category: TrayCategory::Other,
        }
    }

    /// True when the item wants the user's attention.
    pub fn needs_attention(&self) -> bool {
        self.status == TrayStatus::NeedsAttention
    }
}

/// The ordered set of tray items shown in the panel.
///
/// Order is **stable**: [`TrayModel::upsert`] on an existing id updates the item
/// in place without moving it, so icons never jump around when an app refreshes
/// its metadata (`GLAZE.md` §3.4: "No layout jumps when apps start/close").
#[derive(Debug, Default, Clone)]
pub struct TrayModel {
    items: Vec<TrayItem>,
}

impl TrayModel {
    /// An empty tray.
    pub fn new() -> Self {
        TrayModel { items: Vec::new() }
    }

    /// Insert a new item or update the existing one with the same id.
    ///
    /// Returns `true` when the id was already present (an update), `false` when
    /// a new item was appended.
    pub fn upsert(&mut self, item: TrayItem) -> bool {
        match self
            .items
            .iter_mut()
            .find(|existing| existing.id == item.id)
        {
            Some(existing) => {
                *existing = item;
                true
            }
            None => {
                self.items.push(item);
                false
            }
        }
    }

    /// Remove an item by id. Returns whether it was present.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        self.items.len() != before
    }

    /// An item by id.
    pub fn get(&self, id: &str) -> Option<&TrayItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Items in stable insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &TrayItem> {
        self.items.iter()
    }

    /// All items as a slice in stable order.
    pub fn items(&self) -> &[TrayItem] {
        &self.items
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when there are no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// How many items are currently asking for attention.
    pub fn needs_attention(&self) -> usize {
        self.items.iter().filter(|i| i.needs_attention()).count()
    }

    /// Discard every item (e.g. the StatusNotifier watcher went away).
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// How many tray items must be pushed into an overflow menu.
///
/// `available_px` is the width the tray zone can use and `item_px` the budget
/// for one item (see [`TRAY_ITEM_WIDTH_PX`]). The rule is intentionally the same
/// integer-division budget the taskbar uses, so overflow behaviour is
/// predictable: everything that does not fit (plus a small guard so a partial
/// icon is never drawn) goes into the `»` menu.
///
/// Degenerate inputs are handled honestly rather than panicking:
/// - no items, or a non-positive `item_px`, means nothing overflows;
/// - non-positive `available_px` means everything overflows.
pub fn overflow(item_count: usize, available_px: i32, item_px: i32) -> usize {
    if item_count == 0 || item_px <= 0 {
        return 0;
    }
    if available_px <= 0 {
        return item_count;
    }
    let fits = (i64::from(available_px) / i64::from(item_px)) as usize;
    item_count.saturating_sub(fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, title: &str) -> TrayItem {
        TrayItem::new(id, title)
    }

    #[test]
    fn starts_empty() {
        let tray = TrayModel::new();
        assert!(tray.is_empty());
        assert_eq!(tray.len(), 0);
        assert_eq!(tray.iter().count(), 0);
        assert_eq!(tray.needs_attention(), 0);
        assert!(tray.get("x").is_none());
    }

    #[test]
    fn upsert_appends_new_ids_in_order() {
        let mut tray = TrayModel::new();
        assert!(!tray.upsert(item("a", "Alpha")));
        assert!(!tray.upsert(item("b", "Beta")));
        assert_eq!(tray.len(), 2);
        let ids: Vec<&str> = tray.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn upsert_updates_in_place_and_keeps_order() {
        let mut tray = TrayModel::new();
        tray.upsert(item("a", "Alpha"));
        tray.upsert(item("b", "Beta"));
        tray.upsert(item("c", "Gamma"));

        let mut updated = item("b", "Beta (new)");
        updated.icon = Some("network-wireless".into());
        updated.status = TrayStatus::NeedsAttention;
        updated.category = TrayCategory::Hardware;

        assert!(tray.upsert(updated), "same id must report an update");
        assert_eq!(tray.len(), 3, "an update never adds an item");

        // Order is unchanged: a, b, c.
        let ids: Vec<&str> = tray.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);

        let b = tray.get("b").unwrap();
        assert_eq!(b.title, "Beta (new)");
        assert_eq!(b.icon.as_deref(), Some("network-wireless"));
        assert!(b.needs_attention());
        assert_eq!(b.category, TrayCategory::Hardware);
        assert_eq!(tray.needs_attention(), 1);
    }

    #[test]
    fn remove_reports_whether_the_item_existed() {
        let mut tray = TrayModel::new();
        tray.upsert(item("a", "Alpha"));
        assert!(tray.remove("a"));
        assert!(!tray.remove("a"));
        assert!(tray.is_empty());
    }

    #[test]
    fn clear_drops_everything() {
        let mut tray = TrayModel::new();
        tray.upsert(item("a", "Alpha"));
        tray.upsert(item("b", "Beta"));
        tray.clear();
        assert!(tray.is_empty());
    }

    #[test]
    fn overflow_none_when_everything_fits() {
        // 4 items * 24px = 96 <= 120.
        assert_eq!(overflow(4, 120, TRAY_ITEM_WIDTH_PX), 0);
        assert_eq!(overflow(5, 120, TRAY_ITEM_WIDTH_PX), 0, "exactly 5 fits");
    }

    #[test]
    fn overflow_counts_items_that_do_not_fit() {
        // 120 / 24 = 5 fit; 7 - 5 = 2 overflow.
        assert_eq!(overflow(7, 120, TRAY_ITEM_WIDTH_PX), 2);
        // 60 / 24 = 2 fit; 6 - 2 = 4 overflow.
        assert_eq!(overflow(6, 60, TRAY_ITEM_WIDTH_PX), 4);
    }

    #[test]
    fn overflow_degenerate_inputs_never_panic() {
        assert_eq!(overflow(0, 0, TRAY_ITEM_WIDTH_PX), 0);
        assert_eq!(overflow(5, 0, TRAY_ITEM_WIDTH_PX), 5);
        assert_eq!(overflow(5, -100, TRAY_ITEM_WIDTH_PX), 5);
        assert_eq!(overflow(5, 500, 0), 0, "zero item width has no meaning");
        assert_eq!(overflow(5, 500, -1), 0);
    }

    #[test]
    fn overflow_all_when_nothing_fits() {
        assert_eq!(overflow(3, 10, TRAY_ITEM_WIDTH_PX), 3);
    }
}
