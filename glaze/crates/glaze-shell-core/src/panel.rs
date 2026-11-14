//! Panel composition and taskbar overflow policy.
//!
//! [`layout`] returns the canonical left-to-right (or top-to-bottom) order from
//! `GLAZE.md` §6.1:
//!
//! ```text
//! Start · Pinned · Task · [Spacer] · Tray · Clock · [ShowDesktop]
//! ```
//!
//! The zones are always returned in this relative order; a zone may be omitted
//! entirely when the configuration makes it meaningless:
//!
//! - **Task** is omitted when `session.compositor = "none"`, because with no
//!   window-management integration there can never be a real running-window
//!   list (`GLAZE.md` §4.3 forbids inventing one). Pinned items still appear.
//! - **ShowDesktop** is "optional" in §6.1 and is appended only when a desktop
//!   surface actually exists — i.e. `desktop.show_icons` is on or a wallpaper
//!   is configured.
//!
//! [`task_display_mode`] decides how running tasks degrade as space shrinks,
//! using named width constants rather than magic numbers.

use glaze_common::config::{CompositorBackend, GlazeConfig};

/// A zone in the panel's single linear layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelItem {
    /// The Glaze / start button. Always first.
    Start,
    /// Pinned launcher items.
    Pinned,
    /// Live running windows (taskbar), when a backend can report them.
    Task,
    /// Flexible space that pushes later zones to the trailing edge.
    Spacer,
    /// Tray / status area (real integrations only).
    Tray,
    /// Clock and optional date.
    Clock,
    /// Optional "show desktop" affordance.
    ShowDesktop,
}

/// Minimum width budget for one icon-only task button, in logical px.
pub const TASK_ICON_WIDTH_PX: i32 = 40;
/// Minimum width budget for one icon-plus-label task button, in logical px.
pub const TASK_LABEL_WIDTH_PX: i32 = 150;

/// How the task zone should render given the space it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskDisplayMode {
    /// Icons with window/app labels — the ideal, used when everything fits.
    IconsAndLabels,
    /// Icons only — labels dropped to avoid overflowing the panel.
    IconsOnly,
    /// Even icons do not fit; expose the tasks in a scrollable strip.
    Scrollable,
}

/// Compute the panel layout for a configuration.
pub fn layout(cfg: &GlazeConfig) -> Vec<PanelItem> {
    let mut items = vec![PanelItem::Start, PanelItem::Pinned];

    if cfg.session.compositor != CompositorBackend::None {
        items.push(PanelItem::Task);
    }

    items.push(PanelItem::Spacer);
    items.push(PanelItem::Tray);
    items.push(PanelItem::Clock);

    if desktop_surface_enabled(cfg) {
        items.push(PanelItem::ShowDesktop);
    }

    items
}

/// True when the configuration implies a desktop surface (so a "show desktop"
/// affordance makes sense).
fn desktop_surface_enabled(cfg: &GlazeConfig) -> bool {
    cfg.desktop.show_icons || cfg.desktop.wallpaper.is_some()
}

/// True when the config asks the shell to paint a desktop surface: icons, an
/// explicit wallpaper file, or a bundled wallpaper set (the default). The
/// panel's show-desktop button deliberately keys off icons / an explicit file
/// only, as before.
pub fn desktop_surface_requested(cfg: &GlazeConfig) -> bool {
    desktop_surface_enabled(cfg) || !cfg.desktop.wallpaper_set.is_empty()
}

/// Decide how to render `count` tasks in `available_px`, honouring the user's
/// label preference.
///
/// Thresholds (documented, not magic):
/// - each icon needs [`TASK_ICON_WIDTH_PX`];
/// - each icon-plus-label needs [`TASK_LABEL_WIDTH_PX`].
///
/// The most informative mode that fits wins: labels if they fit, else icons,
/// else a scrollable strip. `available_px` below zero is treated as zero.
pub fn task_display_mode(available_px: i32, count: usize, label_wanted: bool) -> TaskDisplayMode {
    let available = i64::from(available_px.max(0));
    let count = count as i64;
    let icon_total = count * i64::from(TASK_ICON_WIDTH_PX);
    let label_total = count * i64::from(TASK_LABEL_WIDTH_PX);

    if label_wanted && label_total <= available {
        TaskDisplayMode::IconsAndLabels
    } else if icon_total <= available {
        TaskDisplayMode::IconsOnly
    } else {
        TaskDisplayMode::Scrollable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::config::{GlazeConfig, PanelPosition};

    #[test]
    fn default_layout_is_the_canonical_order() {
        let cfg = GlazeConfig::default();
        assert_eq!(
            layout(&cfg),
            vec![
                PanelItem::Start,
                PanelItem::Pinned,
                PanelItem::Task,
                PanelItem::Spacer,
                PanelItem::Tray,
                PanelItem::Clock,
            ]
        );
    }

    #[test]
    fn start_is_first_and_clock_before_optional_show_desktop() {
        let cfg = GlazeConfig::default();
        let items = layout(&cfg);
        assert_eq!(items.first(), Some(&PanelItem::Start));
        assert_eq!(*items.last().unwrap(), PanelItem::Clock);
    }

    #[test]
    fn spacer_always_sits_between_task_and_tray() {
        let items = layout(&GlazeConfig::default());
        let spacer = items.iter().position(|i| *i == PanelItem::Spacer).unwrap();
        let task = items.iter().position(|i| *i == PanelItem::Task).unwrap();
        let tray = items.iter().position(|i| *i == PanelItem::Tray).unwrap();
        assert!(task < spacer, "task zone precedes the spacer");
        assert!(spacer < tray, "spacer precedes the tray");
    }

    #[test]
    fn no_compositor_removes_the_task_zone_but_keeps_pinned() {
        let mut cfg = GlazeConfig::default();
        cfg.session.compositor = CompositorBackend::None;
        let items = layout(&cfg);
        assert!(!items.contains(&PanelItem::Task));
        assert!(items.contains(&PanelItem::Pinned));
        // Ordering of the remaining zones is unchanged.
        assert_eq!(items[0], PanelItem::Start);
        assert_eq!(items[1], PanelItem::Pinned);
    }

    #[test]
    fn show_desktop_appears_only_when_a_desktop_surface_exists() {
        let mut cfg = GlazeConfig::default();
        // A bundled wallpaper set alone requests a surface but not the button.
        assert!(desktop_surface_requested(&cfg));
        cfg.desktop.wallpaper_set = String::new();
        assert!(!desktop_surface_requested(&cfg));
        assert!(!cfg.desktop.show_icons);
        assert!(cfg.desktop.wallpaper.is_none());
        assert!(!layout(&cfg).contains(&PanelItem::ShowDesktop));

        cfg.desktop.show_icons = true;
        assert!(layout(&cfg).contains(&PanelItem::ShowDesktop));

        cfg.desktop.show_icons = false;
        cfg.desktop.wallpaper = Some(std::path::PathBuf::from("/usr/share/bg.png"));
        let items = layout(&cfg);
        assert!(items.contains(&PanelItem::ShowDesktop));
        assert_eq!(*items.last().unwrap(), PanelItem::ShowDesktop);
    }

    #[test]
    fn layout_is_independent_of_orientation_and_theme() {
        let mut a = GlazeConfig::default();
        let mut b = GlazeConfig::default();
        b.panel.position = PanelPosition::Left;
        b.appearance.theme = glaze_common::config::ThemeKind::Classic;
        a.panel.position = PanelPosition::Top;
        assert_eq!(layout(&a), layout(&b));
    }

    #[test]
    fn labels_are_used_when_they_fit() {
        // 3 tasks * 150px = 450 <= 500.
        assert_eq!(
            task_display_mode(500, 3, true),
            TaskDisplayMode::IconsAndLabels
        );
    }

    #[test]
    fn falls_back_to_icons_when_labels_do_not_fit() {
        // labels: 4*150=600 > 500; icons: 4*40=160 <= 500.
        assert_eq!(task_display_mode(500, 4, true), TaskDisplayMode::IconsOnly);
    }

    #[test]
    fn becomes_scrollable_when_even_icons_do_not_fit() {
        // 20*40 = 800 > 300.
        assert_eq!(
            task_display_mode(300, 20, true),
            TaskDisplayMode::Scrollable
        );
    }

    #[test]
    fn labels_not_wanted_never_uses_icons_and_labels() {
        assert_eq!(
            task_display_mode(1000, 2, false),
            TaskDisplayMode::IconsOnly
        );
    }

    #[test]
    fn zero_tasks_never_reports_overflow() {
        assert_eq!(
            task_display_mode(0, 0, true),
            TaskDisplayMode::IconsAndLabels
        );
        assert_eq!(task_display_mode(0, 0, false), TaskDisplayMode::IconsOnly);
    }

    #[test]
    fn negative_space_is_treated_as_zero() {
        assert_eq!(
            task_display_mode(-100, 1, true),
            TaskDisplayMode::Scrollable
        );
    }
}
