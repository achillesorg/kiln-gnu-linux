//! Tiling layouts: turn a work area and a window count into exact rectangles.
//!
//! `GLAZE.md` §5 P2 lists "window tiling/snapping". This module is the portable
//! layout policy behind it: given a work area, how many windows to place and the
//! requested layout, it returns the rectangles in **placement order**. The
//! rectangles of a tiling layout always
//!
//! - lie inside the work area,
//! - never overlap, and
//! - with `gap == 0`, exactly cover the work area (the last pixel of an odd
//!   division is distributed rather than lost).
//!
//! [`Layout::Floating`] is the escape hatch: floating windows keep their own
//! geometry, so the requested work area is returned once per window as the
//! nominal region and no tiling is applied.

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;

/// Smallest fraction of the width a master column may take.
pub const MIN_MASTER_RATIO: f64 = 0.1;
/// Largest fraction of the width a master column may take.
pub const MAX_MASTER_RATIO: f64 = 0.9;
/// Ratio used when [`Layout::MasterStack::master_ratio`] is not a number.
pub const DEFAULT_MASTER_RATIO: f64 = 0.5;

/// How windows are arranged inside a work area.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// Windows are not tiled; the caller keeps each window's own geometry.
    Floating,
    /// A master column of `master_count` windows beside a stack of the rest.
    MasterStack {
        master_count: u32,
        master_ratio: f64,
    },
}

/// Clamp a master ratio into `[MIN_MASTER_RATIO, MAX_MASTER_RATIO]`, treating
/// `NaN` as [`DEFAULT_MASTER_RATIO`] so a bad config never collapses the layout.
fn clamp_ratio(ratio: f64) -> f64 {
    if ratio.is_nan() {
        DEFAULT_MASTER_RATIO
    } else {
        ratio.clamp(MIN_MASTER_RATIO, MAX_MASTER_RATIO)
    }
}

/// Split `total` non-negative pixels into `parts` values that sum to exactly
/// `total`; earlier parts absorb the remainder. Returns an empty vector for zero
/// parts.
fn distribute(total: i32, parts: usize) -> Vec<i32> {
    if parts == 0 {
        return Vec::new();
    }
    let total = total.max(0);
    let parts_i32 = parts as i32;
    let base = total / parts_i32;
    let remainder = (total % parts_i32) as usize;
    (0..parts)
        .map(|i| if i < remainder { base + 1 } else { base })
        .collect()
}

/// Heights for `parts` stacked windows so that the windows plus the `gap`
/// gutters between them fill `total` exactly.
fn distribute_with_gaps(total: i32, parts: usize, gap: i32) -> Vec<i32> {
    if parts == 0 {
        return Vec::new();
    }
    let gutters = gap.max(0).saturating_mul(parts as i32 - 1);
    distribute(total - gutters, parts)
}

/// Lay out one vertical column of windows spaced by `gap`.
fn column(x: i32, y: i32, w: i32, heights: &[i32], gap: i32) -> Vec<Rect> {
    let last = heights.len().saturating_sub(1);
    let mut out = Vec::with_capacity(heights.len());
    let mut cursor_y = y;
    for (i, &h) in heights.iter().enumerate() {
        out.push(Rect::new(x, cursor_y, w, h));
        if i < last {
            cursor_y += h + gap;
        }
    }
    out
}

/// Place `count` windows inside `area` using `layout` and `gap` spacing.
///
/// `gap` separates adjacent windows (and the two MasterStack columns). It is
/// treated as an unsigned distance; negative values behave as `0`.
pub fn tile(area: Rect, count: usize, layout: Layout, gap: i32) -> Vec<Rect> {
    let area = Rect::new(area.x, area.y, area.w.max(0), area.h.max(0));
    if count == 0 {
        return Vec::new();
    }

    match layout {
        Layout::Floating => vec![area; count],
        Layout::MasterStack {
            master_count,
            master_ratio,
        } => {
            if count == 1 {
                return vec![area];
            }

            let g = gap.max(0);
            let masters = (master_count as usize).min(count);
            let stack = count - masters;

            // No masters (all-stack) or no stack (all-masters): one full-width
            // column, still spaced by `gap`.
            if masters == 0 || stack == 0 {
                let heights = distribute_with_gaps(area.h, count, g);
                return column(area.x, area.y, area.w, &heights, g);
            }

            // Two columns: master on the left, stack on the right, one gutter
            // between them. Widths always sum back to `avail`.
            let avail = (area.w - g).max(0);
            let raw = (f64::from(avail) * clamp_ratio(master_ratio)).round() as i32;
            let master_w = raw.clamp(0, avail);
            let stack_w = avail - master_w;

            let master_heights = distribute_with_gaps(area.h, masters, g);
            let stack_heights = distribute_with_gaps(area.h, stack, g);

            let mut out = column(area.x, area.y, master_w, &master_heights, g);
            out.extend(column(
                area.x + master_w + g,
                area.y,
                stack_w,
                &stack_heights,
                g,
            ));
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 1000, 600)
    }

    fn master(master_count: u32, master_ratio: f64) -> Layout {
        Layout::MasterStack {
            master_count,
            master_ratio,
        }
    }

    fn overlaps(a: &[Rect]) -> bool {
        for (i, first) in a.iter().enumerate() {
            for second in &a[i + 1..] {
                if first.intersect(second).is_some() {
                    return true;
                }
            }
        }
        false
    }

    fn sum_area(rects: &[Rect]) -> i64 {
        rects.iter().map(Rect::area).sum()
    }

    fn assert_tiles_inside(rects: &[Rect], area: Rect) {
        for r in rects {
            assert!(area.contains(r), "{r:?} escaped {area:?}");
        }
    }

    #[test]
    fn floating_returns_the_work_area_per_window() {
        let a = area();
        let rects = tile(a, 3, Layout::Floating, 12);
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| *r == a));
    }

    #[test]
    fn zero_windows_is_empty() {
        assert!(tile(area(), 0, Layout::Floating, 0).is_empty());
        assert!(tile(area(), 0, master(1, 0.5), 0).is_empty());
    }

    #[test]
    fn one_window_fills_the_area() {
        let rects = tile(area(), 1, master(1, 0.5), 16);
        assert_eq!(rects, vec![area()]);
    }

    #[test]
    fn master_stack_covers_the_area_exactly_without_gaps() {
        let a = area();
        for count in [2usize, 3, 5, 7, 13, 20] {
            let rects = tile(a, count, master(1, 0.5), 0);
            assert_eq!(rects.len(), count);
            assert!(!overlaps(&rects), "count {count} overlapped: {rects:?}");
            assert_tiles_inside(&rects, a);
            assert_eq!(
                sum_area(&rects),
                a.area(),
                "count {count} did not cover the work area"
            );
        }
    }

    #[test]
    fn master_stack_gaps_keep_tiles_inside_and_non_overlapping() {
        let a = area();
        for count in [2usize, 3, 6] {
            for gap in [4, 8, 12] {
                let rects = tile(a, count, master(1, 0.5), gap);
                assert_eq!(rects.len(), count);
                assert!(!overlaps(&rects), "count {count} gap {gap}: {rects:?}");
                assert_tiles_inside(&rects, a);
                assert!(
                    sum_area(&rects) < a.area(),
                    "a positive gap should leave gutters"
                );
            }
        }
    }

    #[test]
    fn master_ratio_controls_the_column_split() {
        let a = area();
        let narrow = tile(a, 2, master(1, 0.25), 0);
        let wide = tile(a, 2, master(1, 0.75), 0);
        assert_eq!(narrow[0].w, 250, "first (master) column follows the ratio");
        assert_eq!(wide[0].w, 750);
        assert_eq!(
            narrow[0].w + narrow[1].w,
            a.w,
            "columns still sum to the width"
        );
    }

    #[test]
    fn master_ratio_is_clamped_and_nan_is_defaulted() {
        let a = area();
        let too_big = tile(a, 2, master(1, 5.0), 0);
        let max = tile(a, 2, master(1, MAX_MASTER_RATIO), 0);
        assert_eq!(too_big, max);

        let too_small = tile(a, 2, master(1, -3.0), 0);
        let min = tile(a, 2, master(1, MIN_MASTER_RATIO), 0);
        assert_eq!(too_small, min);

        let nan = tile(a, 2, master(1, f64::NAN), 0);
        let default = tile(a, 2, master(1, DEFAULT_MASTER_RATIO), 0);
        assert_eq!(nan, default);
    }

    #[test]
    fn odd_sizes_are_distributed_without_losing_pixels() {
        let odd = Rect::new(7, 9, 1001, 601);
        let rects = tile(odd, 3, master(1, 0.5), 0);
        assert_eq!(rects.len(), 3);
        assert!(!overlaps(&rects));
        assert_tiles_inside(&rects, odd);
        assert_eq!(sum_area(&rects), odd.area(), "the odd pixel is not lost");
    }

    #[test]
    fn zero_masters_stacks_everything_in_one_column() {
        let a = area();
        let rects = tile(a, 3, master(0, 0.5), 0);
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.x == a.x && r.w == a.w));
        assert!(!overlaps(&rects));
        assert_eq!(sum_area(&rects), a.area());
    }

    #[test]
    fn more_masters_than_windows_falls_back_to_one_column() {
        let a = area();
        let rects = tile(a, 3, master(5, 0.5), 0);
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.w == a.w));
        assert_eq!(sum_area(&rects), a.area());
    }

    #[test]
    fn a_tiny_area_with_a_huge_gap_does_not_panic_or_go_negative() {
        let tiny = Rect::new(0, 0, 10, 10);
        let rects = tile(tiny, 4, master(1, 0.5), 1000);
        for r in &rects {
            assert!(r.w >= 0 && r.h >= 0, "{r:?} went negative");
        }
    }

    #[test]
    fn negative_gap_behaves_as_no_gap() {
        let a = area();
        assert_eq!(
            tile(a, 4, master(1, 0.5), -20),
            tile(a, 4, master(1, 0.5), 0)
        );
    }
}
