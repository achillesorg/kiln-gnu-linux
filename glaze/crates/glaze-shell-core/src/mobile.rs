//! Mobile / touch shell model: bottom navigation, gestures and the app grid.
//!
//! Everything here is deterministic and GTK-free so the touch layout's rules
//! are unit-tested on any host; `apps/glaze-shell/src/mobile_ui.rs` only binds
//! it to widgets. All interactive controls are at least
//! [`MIN_TOUCH_TARGET_PX`] in both directions.

use glaze_common::config::GlazeConfig;
use glaze_common::model::{AppEntry, SearchHit};
use glaze_common::theme::{ThemeMetrics, MIN_TOUCH_TARGET_PX};
use glaze_service::search::search_apps;

/// Padding above/below the touch buttons inside the navigation bar.
pub const NAV_BAR_VERTICAL_PADDING_PX: i32 = 8;
/// Pixels from a screen edge that still count as "starting at the edge".
pub const EDGE_SWIPE_ZONE_PX: f64 = 24.0;
/// Minimum travel for a swipe to count as a gesture.
pub const MIN_SWIPE_DISTANCE_PX: f64 = 48.0;
/// A bottom-edge swipe up at least this fraction of the screen height opens
/// the app grid instead of going home.
pub const APPS_SWIPE_HEIGHT_FRACTION: f64 = 0.30;
/// Minimum width of an app tile in the grid, in logical px.
pub const MIN_TILE_WIDTH_PX: i32 = 88;
/// Height of an app tile (icon + one label line), in logical px.
pub const TILE_HEIGHT_PX: i32 = 104;

/// Buttons of the bottom navigation bar, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavAction {
    /// Go back one level: closes the app grid / search when open. With nothing
    /// to close the shell cannot navigate inside apps (that needs the
    /// compositor), so it reports [`NavEffect::BackUnhandled`].
    Back,
    /// Return to the home screen (closes the grid).
    Home,
    /// Toggle the app-grid launcher.
    Apps,
}

impl NavAction {
    /// Navigation bar order.
    pub const ALL: [NavAction; 3] = [NavAction::Back, NavAction::Home, NavAction::Apps];

    pub const fn label(self) -> &'static str {
        match self {
            NavAction::Back => "Back",
            NavAction::Home => "Home",
            NavAction::Apps => "Apps",
        }
    }

    /// Symbolic icon name (Adwaita/hicolor standard names).
    pub const fn icon_name(self) -> &'static str {
        match self {
            NavAction::Back => "go-previous-symbolic",
            NavAction::Home => "go-home-symbolic",
            NavAction::Apps => "view-app-grid-symbolic",
        }
    }
}

/// Which full-screen surface is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MobileScreen {
    /// Wallpaper only (apps are in front of it).
    #[default]
    Home,
    /// The app-grid launcher.
    AppGrid,
}

/// What the view layer must do after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavEffect {
    /// Nothing changed.
    None,
    /// Show the app grid.
    ShowGrid,
    /// Hide the app grid (back to home).
    HideGrid,
    /// "Back" with nothing of the shell's own to close: the compositor/app
    /// must handle it. The shell never pretends to have navigated.
    BackUnhandled,
}

/// A raw single-finger swipe, in screen px and milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swipe {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub duration_ms: u32,
}

/// System gesture recognised from a [`Swipe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavGesture {
    Home,
    Apps,
    Back,
}

/// Classify an edge swipe on a `screen` of `(width, height)`.
///
/// - starts in the bottom edge zone and travels up: **Home**, or **Apps** when
///   it travels at least [`APPS_SWIPE_HEIGHT_FRACTION`] of the screen height;
/// - starts in the left or right edge zone and travels inward horizontally:
///   **Back**;
/// - anything else (or too short / too diagonal) is `None`, so ordinary
///   scrolling inside apps is never hijacked.
pub fn classify_swipe(swipe: Swipe, screen: (f64, f64)) -> Option<NavGesture> {
    let (w, h) = screen;
    let dx = swipe.end.0 - swipe.start.0;
    let dy = swipe.end.1 - swipe.start.1;

    let from_bottom = swipe.start.1 >= h - EDGE_SWIPE_ZONE_PX;
    if from_bottom && -dy >= MIN_SWIPE_DISTANCE_PX && dy.abs() > dx.abs() * 1.5 {
        return Some(if -dy >= h * APPS_SWIPE_HEIGHT_FRACTION {
            NavGesture::Apps
        } else {
            NavGesture::Home
        });
    }

    let from_left = swipe.start.0 <= EDGE_SWIPE_ZONE_PX;
    let from_right = swipe.start.0 >= w - EDGE_SWIPE_ZONE_PX;
    let inward =
        (from_left && dx >= MIN_SWIPE_DISTANCE_PX) || (from_right && -dx >= MIN_SWIPE_DISTANCE_PX);
    if inward && dx.abs() > dy.abs() * 1.5 {
        return Some(NavGesture::Back);
    }
    None
}

/// Screen-level state of the mobile shell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MobileState {
    screen: MobileScreen,
    search: String,
    page: usize,
}

impl MobileState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn screen(&self) -> MobileScreen {
        self.screen
    }

    pub fn grid_open(&self) -> bool {
        self.screen == MobileScreen::AppGrid
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn page(&self) -> usize {
        self.page
    }

    /// Apply a navigation-bar button.
    pub fn nav(&mut self, action: NavAction) -> NavEffect {
        match action {
            NavAction::Apps => {
                if self.grid_open() {
                    self.close_grid();
                    NavEffect::HideGrid
                } else {
                    self.screen = MobileScreen::AppGrid;
                    self.search.clear();
                    self.page = 0;
                    NavEffect::ShowGrid
                }
            }
            NavAction::Home => {
                if self.grid_open() {
                    self.close_grid();
                    NavEffect::HideGrid
                } else {
                    NavEffect::None
                }
            }
            NavAction::Back => {
                if !self.search.is_empty() {
                    // First back clears search, second closes the grid.
                    self.search.clear();
                    self.page = 0;
                    NavEffect::None
                } else if self.grid_open() {
                    self.close_grid();
                    NavEffect::HideGrid
                } else {
                    NavEffect::BackUnhandled
                }
            }
        }
    }

    /// Apply a recognised system gesture (same semantics as the buttons).
    pub fn gesture(&mut self, g: NavGesture) -> NavEffect {
        match g {
            NavGesture::Home => self.nav(NavAction::Home),
            NavGesture::Back => self.nav(NavAction::Back),
            NavGesture::Apps => {
                if self.grid_open() {
                    NavEffect::None
                } else {
                    self.nav(NavAction::Apps)
                }
            }
        }
    }

    /// Launching an app from the grid returns to the home screen.
    pub fn app_launched(&mut self) -> NavEffect {
        if self.grid_open() {
            self.close_grid();
            NavEffect::HideGrid
        } else {
            NavEffect::None
        }
    }

    /// Update the grid search text; resets to the first page.
    pub fn set_search(&mut self, text: &str) {
        self.search = text.to_string();
        self.page = 0;
    }

    /// Go to page `page`, clamped to `0..page_count`.
    pub fn set_page(&mut self, page: usize, page_count: usize) {
        self.page = page.min(page_count.saturating_sub(1));
    }

    fn close_grid(&mut self) {
        self.screen = MobileScreen::Home;
        self.search.clear();
        self.page = 0;
    }
}

/// Geometry of the app grid for one surface size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSpec {
    pub columns: usize,
    pub rows: usize,
}

impl GridSpec {
    /// Grid for an area of `width_px` x `height_px` logical px.
    ///
    /// Columns fit tiles of at least `max(MIN_TILE_WIDTH_PX, touch_target)`,
    /// clamped to `2..=8`; rows fit [`TILE_HEIGHT_PX`] (at least 1).
    pub fn for_area(width_px: i32, height_px: i32, touch_target: i32) -> Self {
        let tile_w = MIN_TILE_WIDTH_PX.max(touch_target.max(MIN_TOUCH_TARGET_PX));
        let columns = (width_px / tile_w).clamp(2, 8) as usize;
        let rows = (height_px / TILE_HEIGHT_PX).max(1) as usize;
        GridSpec { columns, rows }
    }

    pub fn per_page(&self) -> usize {
        self.columns * self.rows
    }
}

/// Paged results for the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct GridPages {
    pub spec: GridSpec,
    pub pages: Vec<Vec<SearchHit>>,
}

impl GridPages {
    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    /// Hits of `page` (empty slice when out of range or no results).
    pub fn page(&self, page: usize) -> &[SearchHit] {
        self.pages.get(page).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn total(&self) -> usize {
        self.pages.iter().map(Vec::len).sum()
    }
}

/// Rank `apps` for `query` (all apps when empty) and split into pages.
///
/// Uses the same ranking as the desktop launcher
/// ([`glaze_service::search::search_apps`]), so results are always real
/// registry entries.
pub fn build_grid(apps: &[AppEntry], query: &str, spec: GridSpec) -> GridPages {
    let hits = search_apps(apps, query, apps.len().max(1));
    let per = spec.per_page().max(1);
    let pages = hits.chunks(per).map(<[SearchHit]>::to_vec).collect();
    GridPages { spec, pages }
}

/// Height of the bottom navigation bar for `metrics`.
pub fn nav_bar_height(metrics: &ThemeMetrics) -> i32 {
    metrics.touch_target.max(MIN_TOUCH_TARGET_PX) + 2 * NAV_BAR_VERTICAL_PADDING_PX
}

/// Whether the pinned launcher favourites should lead the grid on the first
/// page (kept as policy here so the view stays dumb): pinned apps first, in
/// pin order, then the rest alphabetically as the registry orders them.
pub fn order_with_pinned(apps: &[AppEntry], cfg: &GlazeConfig) -> Vec<AppEntry> {
    let mut out: Vec<AppEntry> = Vec::with_capacity(apps.len());
    for id in &cfg.launcher.pinned {
        if let Some(a) = apps.iter().find(|a| &a.id == id) {
            out.push(a.clone());
        }
    }
    for a in apps {
        if !out.iter().any(|o| o.id == a.id) {
            out.push(a.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_common::config::{ColorScheme, ThemeKind};
    use glaze_common::theme::ThemeTokens;

    fn app(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            comment: None,
            generic_name: None,
            exec: id.into(),
            try_exec: None,
            icon: None,
            categories: vec![],
            keywords: vec![],
            terminal: false,
            no_display: false,
            working_dir: None,
            source_path: String::new(),
        }
    }

    fn apps(n: usize) -> Vec<AppEntry> {
        (0..n)
            .map(|i| app(&format!("a{i}"), &format!("App {i:02}")))
            .collect()
    }

    #[test]
    fn nav_bar_is_touch_sized_for_every_theme() {
        for kind in ThemeKind::ALL {
            for scheme in ColorScheme::ALL {
                let m = ThemeTokens::for_kind(kind, scheme).metrics;
                assert!(
                    nav_bar_height(&m) >= MIN_TOUCH_TARGET_PX + 2 * NAV_BAR_VERTICAL_PADDING_PX
                );
                assert!(nav_bar_height(&m.scaled(0.5)) >= 64);
            }
        }
    }

    #[test]
    fn nav_buttons_drive_the_grid() {
        let mut s = MobileState::new();
        assert_eq!(s.nav(NavAction::Home), NavEffect::None);
        assert_eq!(s.nav(NavAction::Apps), NavEffect::ShowGrid);
        assert!(s.grid_open());
        assert_eq!(s.nav(NavAction::Apps), NavEffect::HideGrid);
        assert!(!s.grid_open());
        s.nav(NavAction::Apps);
        assert_eq!(s.nav(NavAction::Home), NavEffect::HideGrid);
    }

    #[test]
    fn back_clears_search_then_closes_then_reports_unhandled() {
        let mut s = MobileState::new();
        assert_eq!(s.nav(NavAction::Back), NavEffect::BackUnhandled);
        s.nav(NavAction::Apps);
        s.set_search("fire");
        assert_eq!(s.nav(NavAction::Back), NavEffect::None);
        assert_eq!(s.search(), "");
        assert!(s.grid_open());
        assert_eq!(s.nav(NavAction::Back), NavEffect::HideGrid);
        assert_eq!(s.nav(NavAction::Back), NavEffect::BackUnhandled);
    }

    #[test]
    fn launching_closes_the_grid_and_reopening_starts_clean() {
        let mut s = MobileState::new();
        s.nav(NavAction::Apps);
        s.set_search("x");
        s.set_page(3, 5);
        assert_eq!(s.app_launched(), NavEffect::HideGrid);
        s.nav(NavAction::Apps);
        assert_eq!(s.search(), "");
        assert_eq!(s.page(), 0);
        assert_eq!(s.app_launched(), NavEffect::HideGrid);
        assert_eq!(s.app_launched(), NavEffect::None);
    }

    #[test]
    fn page_is_clamped() {
        let mut s = MobileState::new();
        s.set_page(99, 3);
        assert_eq!(s.page(), 2);
        s.set_page(5, 0);
        assert_eq!(s.page(), 0);
    }

    #[test]
    fn swipe_classification() {
        let screen = (360.0, 780.0);
        let up = |dy: f64| Swipe {
            start: (180.0, 779.0),
            end: (182.0, 779.0 - dy),
            duration_ms: 200,
        };
        assert_eq!(classify_swipe(up(120.0), screen), Some(NavGesture::Home));
        assert_eq!(classify_swipe(up(300.0), screen), Some(NavGesture::Apps));
        assert_eq!(classify_swipe(up(20.0), screen), None);
        // Starting mid-screen never triggers a system gesture.
        let mid = Swipe {
            start: (180.0, 400.0),
            end: (180.0, 100.0),
            duration_ms: 200,
        };
        assert_eq!(classify_swipe(mid, screen), None);
        // Left/right edge inward = Back.
        let left = Swipe {
            start: (2.0, 400.0),
            end: (120.0, 410.0),
            duration_ms: 150,
        };
        assert_eq!(classify_swipe(left, screen), Some(NavGesture::Back));
        let right = Swipe {
            start: (359.0, 400.0),
            end: (230.0, 405.0),
            duration_ms: 150,
        };
        assert_eq!(classify_swipe(right, screen), Some(NavGesture::Back));
        // Outward or diagonal edge swipes are ignored.
        let outward = Swipe {
            start: (2.0, 400.0),
            end: (-50.0, 400.0),
            duration_ms: 100,
        };
        assert_eq!(classify_swipe(outward, screen), None);
        let diag = Swipe {
            start: (2.0, 400.0),
            end: (80.0, 700.0),
            duration_ms: 100,
        };
        assert_eq!(classify_swipe(diag, screen), None);
    }

    #[test]
    fn gestures_share_button_semantics() {
        let mut s = MobileState::new();
        assert_eq!(s.gesture(NavGesture::Apps), NavEffect::ShowGrid);
        assert_eq!(s.gesture(NavGesture::Apps), NavEffect::None);
        assert_eq!(s.gesture(NavGesture::Home), NavEffect::HideGrid);
        assert_eq!(s.gesture(NavGesture::Back), NavEffect::BackUnhandled);
    }

    #[test]
    fn grid_columns_respect_touch_width_and_clamps() {
        let phone = GridSpec::for_area(360, 600, 48);
        assert_eq!(phone.columns, 4); // 360 / 88
        assert_eq!(phone.rows, 5); // 600 / 104
        assert_eq!(
            GridSpec::for_area(100, 50, 48),
            GridSpec {
                columns: 2,
                rows: 1
            }
        );
        assert_eq!(GridSpec::for_area(4000, 4000, 48).columns, 8);
        // A larger touch target widens tiles and reduces columns.
        assert!(GridSpec::for_area(360, 600, 120).columns < phone.columns);
    }

    #[test]
    fn pagination_covers_every_app_exactly_once() {
        let list = apps(23);
        let spec = GridSpec {
            columns: 4,
            rows: 3,
        };
        let g = build_grid(&list, "", spec);
        assert_eq!(g.page_count(), 2);
        assert_eq!(g.page(0).len(), 12);
        assert_eq!(g.page(1).len(), 11);
        assert_eq!(g.total(), 23);
        assert!(g.page(9).is_empty());
        let empty = build_grid(&[], "", spec);
        assert_eq!(empty.page_count(), 1);
        assert_eq!(empty.total(), 0);
    }

    #[test]
    fn search_filters_the_grid() {
        let mut list = apps(5);
        list.push(app("org.mozilla.firefox", "Firefox"));
        let g = build_grid(
            &list,
            "fire",
            GridSpec {
                columns: 4,
                rows: 4,
            },
        );
        assert_eq!(g.total(), 1);
        assert_eq!(g.page(0)[0].app_id, "org.mozilla.firefox");
    }

    #[test]
    fn pinned_apps_lead_in_pin_order() {
        let list = apps(4);
        let mut cfg = GlazeConfig::default();
        cfg.launcher.pinned = vec!["a2".into(), "missing".into(), "a0".into()];
        let ids: Vec<_> = order_with_pinned(&list, &cfg)
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(ids, ["a2", "a0", "a1", "a3"]);
    }
}
