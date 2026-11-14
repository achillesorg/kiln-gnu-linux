//! Portable window geometry: rectangles, snapping zones and edge detection.
//!
//! Everything here is plain integer arithmetic on logical desktop coordinates,
//! so it is identical on x86_64 and aarch64 and can be unit-tested on the
//! Windows authoring host. The compositor and the shell both consume these
//! types: the compositor to place windows, the shell/overlay to *preview* where
//! a drag will land (`GLAZE.md` §6.3).
//!
//! The snapping model follows the familiar pointer-push convention: dragging to
//! a screen edge halves the window, dragging into a corner quarters it, and a
//! [`SNAP_EDGE_THRESHOLD`] region near each edge (with a larger
//! [`SNAP_CORNER_SIZE`] region at the corners) triggers the preview.

use serde::{Deserialize, Serialize};

/// An axis-aligned rectangle in logical desktop coordinates.
///
/// `w` / `h` are allowed to be zero (an empty rectangle, e.g. a fully clamped
/// window); they are never negative in a rectangle produced by this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Build a rectangle from an origin and a size.
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Right edge (`x + w`).
    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    /// Bottom edge (`y + h`).
    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// Signed area in logical square pixels. Uses `i64` so a full coverage sum
    /// over a large multi-monitor canvas cannot overflow.
    pub fn area(&self) -> i64 {
        i64::from(self.w.max(0)) * i64::from(self.h.max(0))
    }

    /// Half-open point containment: the top/left edges are inside, the
    /// bottom/right edges are not. Matches the Wayland surface convention used
    /// by `glaze-compositor`'s hit-testing.
    pub const fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    /// True when `other` lies entirely within `self` (edges may touch).
    pub const fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.w <= self.x + self.w
            && other.y + other.h <= self.y + self.h
    }

    /// The overlapping rectangle, or `None` when the two do not overlap.
    ///
    /// Rectangles that merely share an edge do not "overlap" (the intersection
    /// would have zero area and is reported as `None`).
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());
        if x2 <= x1 || y2 <= y1 {
            None
        } else {
            Some(Rect::new(x1, y1, x2 - x1, y2 - y1))
        }
    }

    /// Move and shrink `self` so that it lies inside `bounds`.
    ///
    /// The size is clamped down to the bounds first, then the origin is clamped
    /// so the whole rectangle stays inside. A rectangle larger than `bounds`
    /// becomes `bounds`-sized and pinned to its top-left. Sizes are never made
    /// negative.
    pub fn clamp_to(&self, bounds: &Rect) -> Rect {
        let w = self.w.min(bounds.w).max(0);
        let h = self.h.min(bounds.h).max(0);
        let max_x = (bounds.x + bounds.w - w).max(bounds.x);
        let max_y = (bounds.y + bounds.h - h).max(bounds.y);
        Rect::new(
            self.x.clamp(bounds.x, max_x),
            self.y.clamp(bounds.y, max_y),
            w,
            h,
        )
    }

    /// Inset every edge by `by` pixels, clamping the resulting size to zero.
    pub fn shrink(&self, by: i32) -> Rect {
        let b = by.max(0);
        Rect::new(
            self.x + b,
            self.y + b,
            (self.w - 2 * b).max(0),
            (self.h - 2 * b).max(0),
        )
    }
}

/// A snap target reachable by dragging a window to an edge or corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapZone {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Maximize,
    Center,
}

impl SnapZone {
    /// Every zone, in a stable order — useful for overlays and exhaustive tests.
    pub const ALL: [SnapZone; 10] = [
        SnapZone::LeftHalf,
        SnapZone::RightHalf,
        SnapZone::TopHalf,
        SnapZone::BottomHalf,
        SnapZone::TopLeft,
        SnapZone::TopRight,
        SnapZone::BottomLeft,
        SnapZone::BottomRight,
        SnapZone::Maximize,
        SnapZone::Center,
    ];
}

/// Distance from a work-area edge (logical px) at which an edge snap begins.
pub const SNAP_EDGE_THRESHOLD: i32 = 24;
/// Distance from a work-area corner (logical px) at which a corner snap begins.
///
/// Corners are deliberately larger than plain edges so the four quarter snaps
/// stay reachable with a quick flick.
pub const SNAP_CORNER_SIZE: i32 = 64;

/// Half of a track, with the first half receiving the odd extra pixel.
const fn split_low(total: i32, gap: i32) -> i32 {
    let avail = total - gap;
    if avail <= 0 {
        0
    } else {
        avail / 2
    }
}

/// The remaining half, so the two halves plus one gap fill `total` exactly.
const fn split_high(total: i32, gap: i32) -> i32 {
    let avail = total - gap;
    let low = if avail <= 0 { 0 } else { avail / 2 };
    if avail <= 0 {
        0
    } else {
        avail - low
    }
}

/// Compute the rectangle a window should occupy for `zone`.
///
/// `work_area` is the usable desktop region (output minus panel/struts). `gap`
/// is the spacing left around the window and between halves: the result is
/// inset from every work-area edge by `gap`, and two adjoining halves are
/// separated by one `gap`. A `gap` large relative to the area degrades safely
/// to zero-sized or clipped rectangles rather than negative ones.
pub fn snap_zone(zone: SnapZone, work_area: Rect, gap: i32) -> Rect {
    let g = gap.max(0);
    let u = work_area.shrink(g);
    let lw = split_low(u.w, g);
    let rw = split_high(u.w, g);
    let th = split_low(u.h, g);
    let bh = split_high(u.h, g);

    match zone {
        SnapZone::Maximize => u,
        SnapZone::LeftHalf => Rect::new(u.x, u.y, lw, u.h),
        SnapZone::RightHalf => Rect::new(u.x + lw + g, u.y, rw, u.h),
        SnapZone::TopHalf => Rect::new(u.x, u.y, u.w, th),
        SnapZone::BottomHalf => Rect::new(u.x, u.y + th + g, u.w, bh),
        SnapZone::TopLeft => Rect::new(u.x, u.y, lw, th),
        SnapZone::TopRight => Rect::new(u.x + lw + g, u.y, rw, th),
        SnapZone::BottomLeft => Rect::new(u.x, u.y + th + g, lw, bh),
        SnapZone::BottomRight => Rect::new(u.x + lw + g, u.y + th + g, rw, bh),
        SnapZone::Center => {
            let cw = u.w / 2;
            let ch = u.h / 2;
            Rect::new(u.x + (u.w - cw) / 2, u.y + (u.h - ch) / 2, cw, ch)
        }
    }
}

/// Detect which snap zone a pointer position targets.
///
/// The pointer must be inside the work area expanded by [`SNAP_EDGE_THRESHOLD`]
/// (so pushing just past an edge still snaps). Corners win over edges, edges win
/// over nothing. Returns `None` in the middle of the desktop, where a drag means
/// "move freely".
pub fn detect_zone(pointer: (i32, i32), work_area: Rect) -> Option<SnapZone> {
    let (x, y) = pointer;
    let slack = SNAP_EDGE_THRESHOLD;
    let within = x >= work_area.x - slack
        && x < work_area.right() + slack
        && y >= work_area.y - slack
        && y < work_area.bottom() + slack;
    if !within {
        return None;
    }

    let dl = x - work_area.x;
    let dr = work_area.right() - x;
    let dt = y - work_area.y;
    let db = work_area.bottom() - y;

    let in_corner_l = dl <= SNAP_CORNER_SIZE;
    let in_corner_r = dr <= SNAP_CORNER_SIZE;
    let in_corner_t = dt <= SNAP_CORNER_SIZE;
    let in_corner_b = db <= SNAP_CORNER_SIZE;

    if in_corner_l && in_corner_t {
        return Some(SnapZone::TopLeft);
    }
    if in_corner_r && in_corner_t {
        return Some(SnapZone::TopRight);
    }
    if in_corner_l && in_corner_b {
        return Some(SnapZone::BottomLeft);
    }
    if in_corner_r && in_corner_b {
        return Some(SnapZone::BottomRight);
    }

    let near_l = dl <= SNAP_EDGE_THRESHOLD;
    let near_r = dr <= SNAP_EDGE_THRESHOLD;
    let near_t = dt <= SNAP_EDGE_THRESHOLD;
    let near_b = db <= SNAP_EDGE_THRESHOLD;

    if near_l {
        Some(SnapZone::LeftHalf)
    } else if near_r {
        Some(SnapZone::RightHalf)
    } else if near_t {
        Some(SnapZone::TopHalf)
    } else if near_b {
        Some(SnapZone::BottomHalf)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wa() -> Rect {
        Rect::new(0, 0, 1000, 600)
    }

    #[test]
    fn intersect_reports_the_overlap_or_none() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 100, 100);
        assert_eq!(a.intersect(&b), Some(Rect::new(50, 50, 50, 50)));
        assert_eq!(a.intersect(&b), b.intersect(&a));

        let touching = Rect::new(100, 0, 10, 10);
        assert_eq!(a.intersect(&touching), None, "shared edges do not overlap");

        let disjoint = Rect::new(200, 200, 10, 10);
        assert_eq!(a.intersect(&disjoint), None);

        let empty = Rect::new(10, 10, 0, 5);
        assert_eq!(a.intersect(&empty), None);
    }

    #[test]
    fn contains_is_rect_containment_and_point_containment_is_half_open() {
        let outer = Rect::new(0, 0, 100, 100);
        assert!(
            outer.contains(&Rect::new(0, 0, 100, 100)),
            "self contains self"
        );
        assert!(outer.contains(&Rect::new(10, 10, 90, 90)));
        assert!(
            !outer.contains(&Rect::new(10, 10, 91, 90)),
            "overflows right"
        );
        assert!(!outer.contains(&Rect::new(-1, 0, 10, 10)));

        assert!(outer.contains_point(0, 0));
        assert!(outer.contains_point(99, 99));
        assert!(!outer.contains_point(100, 99), "right edge is outside");
        assert!(!outer.contains_point(99, 100), "bottom edge is outside");
    }

    #[test]
    fn area_matches_multiplication_and_never_goes_negative() {
        assert_eq!(Rect::new(0, 0, 3, 4).area(), 12);
        assert_eq!(Rect::new(10, 10, 0, 50).area(), 0);
        assert_eq!(
            Rect::new(0, 0, -5, 5).area(),
            0,
            "negative sizes clamp to 0"
        );
    }

    #[test]
    fn clamp_to_keeps_a_rect_inside_its_bounds() {
        let bounds = Rect::new(0, 0, 100, 100);

        let inside = Rect::new(10, 10, 20, 20);
        assert_eq!(inside.clamp_to(&bounds), inside);

        let too_far = Rect::new(95, 95, 20, 20);
        assert_eq!(too_far.clamp_to(&bounds), Rect::new(80, 80, 20, 20));

        let too_big = Rect::new(-40, -40, 400, 400);
        assert_eq!(too_big.clamp_to(&bounds), Rect::new(0, 0, 100, 100));

        let negative = Rect::new(10, 10, -5, -5);
        assert_eq!(negative.clamp_to(&bounds), Rect::new(10, 10, 0, 0));
    }

    #[test]
    fn shrink_insets_all_edges() {
        assert_eq!(wa().shrink(10), Rect::new(10, 10, 980, 580));
        assert_eq!(wa().shrink(0), wa());
        assert_eq!(wa().shrink(10000), Rect::new(10000, 10000, 0, 0));
        assert_eq!(wa().shrink(-5), wa(), "negative insets are ignored");
    }

    #[test]
    fn snap_zones_are_exact_without_a_gap() {
        let a = wa();
        assert_eq!(
            snap_zone(SnapZone::LeftHalf, a, 0),
            Rect::new(0, 0, 500, 600)
        );
        assert_eq!(
            snap_zone(SnapZone::RightHalf, a, 0),
            Rect::new(500, 0, 500, 600)
        );
        assert_eq!(
            snap_zone(SnapZone::TopHalf, a, 0),
            Rect::new(0, 0, 1000, 300)
        );
        assert_eq!(
            snap_zone(SnapZone::BottomHalf, a, 0),
            Rect::new(0, 300, 1000, 300)
        );
        assert_eq!(
            snap_zone(SnapZone::TopLeft, a, 0),
            Rect::new(0, 0, 500, 300)
        );
        assert_eq!(
            snap_zone(SnapZone::TopRight, a, 0),
            Rect::new(500, 0, 500, 300)
        );
        assert_eq!(
            snap_zone(SnapZone::BottomLeft, a, 0),
            Rect::new(0, 300, 500, 300)
        );
        assert_eq!(
            snap_zone(SnapZone::BottomRight, a, 0),
            Rect::new(500, 300, 500, 300)
        );
        assert_eq!(snap_zone(SnapZone::Maximize, a, 0), a);
        assert_eq!(
            snap_zone(SnapZone::Center, a, 0),
            Rect::new(250, 150, 500, 300)
        );
    }

    #[test]
    fn snap_zones_account_for_the_gap_on_every_shared_edge() {
        let a = wa();
        let g = 10;
        // Outer inset of 10 on every side, and one 10px gutter between halves.
        assert_eq!(
            snap_zone(SnapZone::LeftHalf, a, g),
            Rect::new(10, 10, 485, 580)
        );
        assert_eq!(
            snap_zone(SnapZone::RightHalf, a, g),
            Rect::new(505, 10, 485, 580)
        );
        assert_eq!(
            snap_zone(SnapZone::TopHalf, a, g),
            Rect::new(10, 10, 980, 285)
        );
        assert_eq!(
            snap_zone(SnapZone::BottomHalf, a, g),
            Rect::new(10, 305, 980, 285)
        );
        assert_eq!(
            snap_zone(SnapZone::Maximize, a, g),
            Rect::new(10, 10, 980, 580)
        );
        assert_eq!(
            snap_zone(SnapZone::Center, a, g),
            Rect::new(255, 155, 490, 290)
        );

        // The two halves plus the gutter fill the inset area exactly.
        let l = snap_zone(SnapZone::LeftHalf, a, g);
        let r = snap_zone(SnapZone::RightHalf, a, g);
        assert_eq!(l.right() + g, r.x);
        assert_eq!(r.right(), a.right() - g);
        assert_eq!(
            l.area() + r.area(),
            i64::from(980 * 580) - l.h as i64 * g as i64
        );
    }

    #[test]
    fn snap_zones_distribute_odd_sizes_deterministically() {
        let odd = Rect::new(0, 0, 1001, 601);
        let l = snap_zone(SnapZone::LeftHalf, odd, 0);
        let r = snap_zone(SnapZone::RightHalf, odd, 0);
        assert_eq!(l, Rect::new(0, 0, 500, 601));
        assert_eq!(r, Rect::new(500, 0, 501, 601));
        assert_eq!(l.w + r.w, 1001, "the odd column is fully covered");
        assert_eq!(r.right(), odd.right());

        let t = snap_zone(SnapZone::TopHalf, odd, 0);
        let b = snap_zone(SnapZone::BottomHalf, odd, 0);
        assert_eq!(t.h + b.h, 601);
        assert_eq!(b.bottom(), odd.bottom());
    }

    #[test]
    fn every_snap_zone_fits_inside_the_work_area() {
        let a = wa();
        for zone in SnapZone::ALL {
            let r = snap_zone(zone, a, 8);
            assert!(a.contains(&r), "{zone:?} escaped the work area: {r:?}");
            assert!(r.w >= 0 && r.h >= 0, "{zone:?} has a negative size: {r:?}");
        }
    }

    #[test]
    fn detect_zone_finds_edges_and_corners() {
        let a = wa();
        assert_eq!(detect_zone((5, 300), a), Some(SnapZone::LeftHalf));
        assert_eq!(detect_zone((995, 300), a), Some(SnapZone::RightHalf));
        assert_eq!(detect_zone((500, 5), a), Some(SnapZone::TopHalf));
        assert_eq!(detect_zone((500, 595), a), Some(SnapZone::BottomHalf));

        assert_eq!(detect_zone((5, 5), a), Some(SnapZone::TopLeft));
        assert_eq!(detect_zone((995, 5), a), Some(SnapZone::TopRight));
        assert_eq!(detect_zone((5, 595), a), Some(SnapZone::BottomLeft));
        assert_eq!(detect_zone((995, 595), a), Some(SnapZone::BottomRight));
    }

    #[test]
    fn detect_zone_returns_none_in_the_middle_and_far_outside() {
        let a = wa();
        assert_eq!(detect_zone((500, 300), a), None, "centre is free-drag");
        assert_eq!(detect_zone((40, 300), a), None, "outside the edge band");
        assert_eq!(detect_zone((500, 700), a), None, "far below the work area");
        assert_eq!(detect_zone((-100, 300), a), None, "far past the left edge");
    }

    #[test]
    fn detect_zone_still_snaps_just_past_an_edge() {
        let a = wa();
        assert_eq!(detect_zone((-10, 300), a), Some(SnapZone::LeftHalf));
        assert_eq!(detect_zone((1005, 300), a), Some(SnapZone::RightHalf));
        assert_eq!(detect_zone((500, -10), a), Some(SnapZone::TopHalf));
        assert_eq!(detect_zone((500, 610), a), Some(SnapZone::BottomHalf));
    }

    #[test]
    fn detect_zone_corner_band_is_larger_than_the_edge_band() {
        let a = wa();
        // 60px in from the top-left is inside the corner band but not the edge band.
        assert_eq!(detect_zone((60, 60), a), Some(SnapZone::TopLeft));
        // The same distance along an edge, away from the corner, is not a snap.
        assert_eq!(detect_zone((300, 60), a), None);
    }
}
