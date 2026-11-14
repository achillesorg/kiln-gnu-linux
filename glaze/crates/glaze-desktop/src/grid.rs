//! The desktop icon grid: a fixed-column pack, as in the classic desktop model.
//!
//! `GLAZE.md` §3.1 asks for "desktop background with launchable shortcuts" laid
//! out like a familiar desktop: icons fill a column from top to bottom, then the
//! next column starts to the right. This module is the pure geometry of that
//! decision, so the packing rules can be unit-tested on any host and the GTK
//! view merely positions widgets from the returned [`Rect`]s.
//!
//! The engine never overflows the work area: when `count` icons do not fit, the
//! result is truncated to the number of cells the area actually holds. Callers
//! can therefore place every returned rect without clipping.

/// An axis-aligned rectangle in logical pixels, relative to the desktop area
/// origin (top-left = `0,0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Rect {
    /// Exclusive right edge (`x + w`).
    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    /// Exclusive bottom edge (`y + h`).
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// True when the rect is fully inside a `w × h` area anchored at the origin.
    pub fn fits_within(&self, area_w: i32, area_h: i32) -> bool {
        self.x >= 0 && self.y >= 0 && self.right() <= area_w && self.bottom() <= area_h
    }
}

/// Metrics of one desktop icon cell.
///
/// A cell holds the icon artwork ([`GridConfig::icon_size`]) plus room for the
/// label ([`GridConfig::label_lines`]); `cell_w`/`cell_h` are the total slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridConfig {
    /// Width of one icon cell (slot) in logical px.
    pub cell_w: i32,
    /// Height of one icon cell (slot) in logical px.
    pub cell_h: i32,
    /// Space between neighbouring cells, both axes.
    pub gap: i32,
    /// Rendered size of the icon artwork inside the cell.
    pub icon_size: i32,
    /// Number of label text lines reserved under the icon.
    pub label_lines: u32,
}

impl Default for GridConfig {
    /// Glaze Clay desktop defaults: 88 × 88 slots, tight 8 px gutter around a
    /// 48 px icon with room for two label lines.
    fn default() -> Self {
        GridConfig {
            cell_w: 88,
            cell_h: 88,
            gap: 8,
            icon_size: 48,
            label_lines: 2,
        }
    }
}

/// How many cells fit along one axis of `area` given `cell` size and `gap`.
///
/// This is the standard "n cells with n-1 gutters" count:
/// `n = floor((area + gap) / (cell + gap))`. Non-positive cell size or area
/// yields `0`, and a negative `gap` is treated as `0`, so the result is always
/// safe to use directly.
fn count_along(area: i32, cell: i32, gap: i32) -> usize {
    if area <= 0 || cell <= 0 {
        return 0;
    }
    let gap = gap.max(0);
    let total = area + gap;
    let stride = cell + gap;
    (total / stride).max(0) as usize
}

/// Number of icon cells that fit in a column of the given area height.
pub fn cells_per_column(area_h: i32, cfg: &GridConfig) -> usize {
    count_along(area_h, cfg.cell_h, cfg.gap)
}

/// Number of icon columns that fit in an area width.
fn cells_per_row(area_w: i32, cfg: &GridConfig) -> usize {
    count_along(area_w, cfg.cell_w, cfg.gap)
}

/// Lay out `count` icons as a fixed-column pack inside `area_w × area_h`.
///
/// Icons fill the first column top-to-bottom, then the second column, and so on.
/// At most `cells_per_column(area_h) * columns(area_w)` rects are produced; any
/// surplus icons are dropped rather than allowed to overflow the area. Every
/// returned [`Rect`] satisfies [`Rect::fits_within`] for the given area (when
/// the area is non-empty), and the slots are contiguous with `cfg.gap` between
/// them.
pub fn grid_positions(count: usize, area_w: i32, area_h: i32, cfg: &GridConfig) -> Vec<Rect> {
    let rows = cells_per_column(area_h, cfg);
    let cols = cells_per_row(area_w, cfg);
    let capacity = rows.saturating_mul(cols);
    let n = count.min(capacity);
    if n == 0 {
        return Vec::new();
    }
    let gap = cfg.gap.max(0);
    let stride_x = cfg.cell_w + gap;
    let stride_y = cfg.cell_h + gap;
    (0..n)
        .map(|i| {
            let col = i / rows;
            let row = i % rows;
            Rect {
                x: col as i32 * stride_x,
                y: row as i32 * stride_y,
                w: cfg.cell_w,
                h: cfg.cell_h,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GridConfig {
        // 64×64 slots, 8 px gap, easily reasoned about by hand.
        GridConfig {
            cell_w: 64,
            cell_h: 64,
            gap: 8,
            icon_size: 48,
            label_lines: 2,
        }
    }

    #[test]
    fn default_config_is_sane() {
        let d = GridConfig::default();
        assert!(d.cell_w > 0 && d.cell_h > 0);
        assert!(d.icon_size <= d.cell_w.min(d.cell_h));
        assert!(d.label_lines > 0);
    }

    #[test]
    fn cells_per_column_maths() {
        let c = cfg();
        // (1000 + 8) / (64 + 8) = 1008 / 72 = 14
        assert_eq!(cells_per_column(1000, &c), 14);
        // Exactly one cell: 64 + a 7px remainder.
        assert_eq!(cells_per_column(64, &c), 1);
        assert_eq!(cells_per_column(63, &c), 0);
    }

    #[test]
    fn exact_count_when_everything_fits() {
        let c = cfg();
        // 200x600 → cols = (200+8)/72 = 2, rows = (600+8)/72 = 8 → capacity 16.
        let rects = grid_positions(6, 200, 600, &c);
        assert_eq!(rects.len(), 6);
        for r in &rects {
            assert!(r.fits_within(200, 600), "{r:?} overflows a 200x600 area");
        }
    }

    #[test]
    fn first_column_fills_top_to_bottom_before_the_second() {
        let c = cfg();
        // rows = (600+8)/72 = 8. With 9 icons the 9th must start column 2, top.
        let rects = grid_positions(9, 600, 600, &c);
        assert_eq!(rects.len(), 9);
        // Fixture count depends on rows; recompute the row count for clarity.
        let rows = cells_per_column(600, &c);
        assert_eq!(rows, 8);
        assert_eq!(
            rects[0],
            Rect {
                x: 0,
                y: 0,
                w: 64,
                h: 64
            }
        );
        assert_eq!(rects[1].x, 0);
        assert_eq!(rects[1].y, 72);
        assert_eq!(
            rects[rows],
            Rect {
                x: 72,
                y: 0,
                w: 64,
                h: 64
            }
        );
    }

    #[test]
    fn never_overflows_the_area() {
        // A deliberately awkward area: 2 full columns + a too-narrow 3rd.
        let c = cfg();
        let (aw, ah) = (150, 200);
        let rects = grid_positions(50, aw, ah, &c);
        assert!(!rects.is_empty());
        for r in &rects {
            assert!(r.fits_within(aw, ah), "{r:?} overflows {aw}x{ah}");
        }
        // 150 fits one column (72*2 = 144 <= 150 gives 2? check): (150+8)/72 = 2
        assert_eq!(cells_per_column(200, &c), 2);
    }

    #[test]
    fn tiny_area_yields_no_positions() {
        let c = cfg();
        assert!(grid_positions(10, 10, 10, &c).is_empty());
        assert!(grid_positions(10, 0, 0, &c).is_empty());
        assert!(grid_positions(10, -5, -5, &c).is_empty());
        // Even one cell for a big count is refused when there is no space.
        assert!(grid_positions(1000, 20, 20, &c).is_empty());
    }

    #[test]
    fn zero_gap_packs_contiguously() {
        let c = GridConfig {
            cell_w: 50,
            cell_h: 25,
            gap: 0,
            icon_size: 24,
            label_lines: 1,
        };
        // 100 height / 25 = 4 rows, exact.
        assert_eq!(cells_per_column(100, &c), 4);
        let rects = grid_positions(5, 100, 100, &c);
        assert_eq!(rects.len(), 5);
        assert_eq!(
            rects[3],
            Rect {
                x: 0,
                y: 75,
                w: 50,
                h: 25
            }
        );
        assert_eq!(
            rects[4],
            Rect {
                x: 50,
                y: 0,
                w: 50,
                h: 25
            }
        );
    }

    #[test]
    fn large_counts_are_capped_at_capacity() {
        let c = cfg();
        let rows = cells_per_column(720, &c); // (720+8)/72 = 10
        assert_eq!(rows, 10);
        let capacity = rows * 5;
        let rects = grid_positions(100_000, 400, 720, &c);
        assert_eq!(rects.len(), capacity);
        for r in &rects {
            assert!(r.fits_within(400, 720));
        }
    }

    #[test]
    fn zero_count_is_empty_and_negative_gap_is_clamped() {
        let c = cfg();
        assert!(grid_positions(0, 500, 500, &c).is_empty());
        let neg = GridConfig { gap: -4, ..c };
        // Behaves exactly like gap = 0.
        assert_eq!(
            grid_positions(5, 500, 500, &neg),
            grid_positions(5, 500, 500, &GridConfig { gap: 0, ..c })
        );
    }

    #[test]
    fn invalid_cell_size_is_refused() {
        let c = cfg();
        // A zero-width cell means no columns fit, so nothing is placed — but
        // the (valid) height still admits rows on its own.
        let bad = GridConfig { cell_w: 0, ..c };
        assert!(grid_positions(5, 500, 500, &bad).is_empty());
        assert_eq!(cells_per_column(500, &bad), 7);

        // A non-positive height row count is zero, so nothing is placed.
        let bad = GridConfig { cell_h: -10, ..c };
        assert!(grid_positions(5, 500, 500, &bad).is_empty());
        assert_eq!(cells_per_column(500, &bad), 0);
    }

    #[test]
    fn layout_is_deterministic() {
        let c = cfg();
        let a = grid_positions(23, 333, 777, &c);
        let b = grid_positions(23, 333, 777, &c);
        assert_eq!(a, b);
    }
}
