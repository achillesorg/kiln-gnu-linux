//! The stacking <-> tiling toggle: which layout mode a workspace uses, and
//! where every window goes under it.
//!
//! Glaze windows **stack** (Win2K-style overlapping windows) by default. The
//! user can switch to **tiling** globally or per workspace (`docs/decisions.md`,
//! "Tiling toggle is required"). This module is the portable policy behind that
//! switch; the compositor (`glaze-compositor`) only applies its output to real
//! surfaces, and the taskbar's float/tile toggle only sends a request.
//!
//! Semantics (recorded in `docs/decisions.md`, "Compositor defaults"):
//!
//! - There is one **global** mode (default [`LayoutMode::Stacking`]) and an
//!   optional **per-workspace override**.
//! - Toggling a workspace sets an override on that workspace only.
//! - Toggling (or setting) the global mode clears every override, so "make
//!   everything tile" means everything, not "everything except the workspaces I
//!   touched last week".
//! - A window keeps its **stacking geometry** while tiled. Switching back to
//!   stacking puts every window where it was, so the toggle is lossless.
//! - A window can be pinned floating ([`ArrangeInput::pinned_floating`]); it
//!   keeps its stacking geometry even on a tiled workspace (dialogs, pickers).
//!
//! The tiling algorithm is a simple **dwindle** tree by default (each new window
//! splits the previous one's space, alternating vertical/horizontal splits,
//! like bspwm/Hyprland's dwindle) or the existing master/stack layout from
//! [`crate::tiling`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::tiling::{tile, Layout};

/// How windows are arranged on a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutMode {
    /// Overlapping windows the user moves and resizes freely (the default).
    #[default]
    Stacking,
    /// Non-overlapping windows filling the work area.
    Tiling,
}

impl LayoutMode {
    /// The other mode.
    pub const fn toggled(self) -> Self {
        match self {
            LayoutMode::Stacking => LayoutMode::Tiling,
            LayoutMode::Tiling => LayoutMode::Stacking,
        }
    }

    /// Stable wire name (`"stacking"` / `"tiling"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            LayoutMode::Stacking => "stacking",
            LayoutMode::Tiling => "tiling",
        }
    }
}

/// A requested change: an explicit mode or a flip of the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModeRequest {
    Set(LayoutMode),
    Toggle,
}

impl ModeRequest {
    /// Parse the wire form used by the control socket: `"stacking"`,
    /// `"tiling"` (aliases `"floating"`, `"float"`, `"tile"`) or `"toggle"`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stacking" | "stack" | "floating" | "float" => {
                Some(ModeRequest::Set(LayoutMode::Stacking))
            }
            "tiling" | "tile" | "tiled" => Some(ModeRequest::Set(LayoutMode::Tiling)),
            "toggle" => Some(ModeRequest::Toggle),
            _ => None,
        }
    }
}

/// Which workspaces a change applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutScope {
    /// Every workspace (clears per-workspace overrides).
    Global,
    /// One workspace, by zero-based index.
    Workspace(u32),
}

/// The tiling algorithm used when a workspace is in [`LayoutMode::Tiling`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TilingAlgorithm {
    /// Recursive halving, alternating split direction (the default).
    Dwindle { ratio: f64 },
    /// One master column beside a stack (see [`crate::tiling`]).
    MasterStack {
        master_count: u32,
        master_ratio: f64,
    },
}

impl Default for TilingAlgorithm {
    fn default() -> Self {
        TilingAlgorithm::Dwindle { ratio: 0.5 }
    }
}

/// Default gap between tiled windows, in logical px.
pub const DEFAULT_TILE_GAP: i32 = 8;

/// Global + per-workspace layout modes and the tiling parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutPolicy {
    global: LayoutMode,
    overrides: BTreeMap<u32, LayoutMode>,
    pub algorithm: TilingAlgorithm,
    pub gap: i32,
}

impl Default for LayoutPolicy {
    fn default() -> Self {
        Self::new(LayoutMode::Stacking)
    }
}

impl LayoutPolicy {
    /// A policy whose global mode is `global` and with no overrides.
    pub fn new(global: LayoutMode) -> Self {
        Self {
            global,
            overrides: BTreeMap::new(),
            algorithm: TilingAlgorithm::default(),
            gap: DEFAULT_TILE_GAP,
        }
    }

    /// The global mode.
    pub fn global(&self) -> LayoutMode {
        self.global
    }

    /// The effective mode of `workspace`.
    pub fn mode_for(&self, workspace: u32) -> LayoutMode {
        self.overrides
            .get(&workspace)
            .copied()
            .unwrap_or(self.global)
    }

    /// Whether `workspace` has its own override.
    pub fn has_override(&self, workspace: u32) -> bool {
        self.overrides.contains_key(&workspace)
    }

    /// Apply a request. Returns the workspaces whose effective mode changed
    /// (`known_workspaces` lists the indices that exist, so a global change can
    /// report them all).
    pub fn apply(
        &mut self,
        scope: LayoutScope,
        request: ModeRequest,
        known_workspaces: &[u32],
    ) -> Vec<u32> {
        let before: Vec<(u32, LayoutMode)> = known_workspaces
            .iter()
            .map(|&ws| (ws, self.mode_for(ws)))
            .collect();
        match scope {
            LayoutScope::Global => {
                self.global = match request {
                    ModeRequest::Set(mode) => mode,
                    ModeRequest::Toggle => self.global.toggled(),
                };
                self.overrides.clear();
            }
            LayoutScope::Workspace(ws) => {
                let mode = match request {
                    ModeRequest::Set(mode) => mode,
                    ModeRequest::Toggle => self.mode_for(ws).toggled(),
                };
                if mode == self.global {
                    self.overrides.remove(&ws);
                } else {
                    self.overrides.insert(ws, mode);
                }
            }
        }
        let mut changed: Vec<u32> = before
            .into_iter()
            .filter(|&(ws, mode)| self.mode_for(ws) != mode)
            .map(|(ws, _)| ws)
            .collect();
        if let LayoutScope::Workspace(ws) = scope {
            if !known_workspaces.contains(&ws) && !changed.contains(&ws) {
                changed.push(ws);
            }
        }
        changed
    }
}

/// One window as the arranger sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrangeInput {
    pub id: String,
    /// The window's remembered stacking geometry.
    pub stacking: Rect,
    /// Keep floating even when the workspace tiles (dialogs, fixed-size).
    pub pinned_floating: bool,
}

/// Target geometry for every input window, in input order.
///
/// Stacking returns each window's own stacking rect. Tiling lays out the
/// non-pinned windows inside `area` with the policy's algorithm and gap;
/// pinned windows keep their stacking rect.
pub fn arrange(
    mode: LayoutMode,
    algorithm: TilingAlgorithm,
    area: Rect,
    gap: i32,
    windows: &[ArrangeInput],
) -> Vec<(String, Rect)> {
    if mode == LayoutMode::Stacking {
        return windows.iter().map(|w| (w.id.clone(), w.stacking)).collect();
    }
    let tiled: Vec<usize> = windows
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.pinned_floating)
        .map(|(i, _)| i)
        .collect();
    let rects = match algorithm {
        TilingAlgorithm::Dwindle { ratio } => dwindle(area, tiled.len(), gap, ratio),
        TilingAlgorithm::MasterStack {
            master_count,
            master_ratio,
        } => tile(
            area,
            tiled.len(),
            Layout::MasterStack {
                master_count,
                master_ratio,
            },
            gap,
        ),
    };
    let mut out: Vec<(String, Rect)> = windows.iter().map(|w| (w.id.clone(), w.stacking)).collect();
    for (slot, rect) in tiled.into_iter().zip(rects) {
        out[slot].1 = rect;
    }
    out
}

/// Dwindle layout: window *i* takes `ratio` of the remaining space, split
/// vertically then horizontally alternately (the longer side is split first
/// so very wide outputs start with side-by-side windows). The last window
/// takes whatever is left. Rects never overlap, lie inside `area` and, with
/// `gap == 0`, exactly cover it.
pub fn dwindle(area: Rect, count: usize, gap: i32, ratio: f64) -> Vec<Rect> {
    let ratio = if ratio.is_nan() {
        0.5
    } else {
        ratio.clamp(0.1, 0.9)
    };
    let g = gap.max(0);
    let mut out = Vec::with_capacity(count);
    let mut rest = Rect::new(area.x, area.y, area.w.max(0), area.h.max(0));
    for i in 0..count {
        if i + 1 == count {
            out.push(rest);
            break;
        }
        // Split the longer side; ties split vertically (side by side).
        let split_vertical = rest.w >= rest.h;
        if split_vertical {
            let avail = (rest.w - g).max(0);
            let first = ((f64::from(avail) * ratio).round() as i32).clamp(0, avail);
            out.push(Rect::new(rest.x, rest.y, first, rest.h));
            rest = Rect::new(rest.x + first + g, rest.y, avail - first, rest.h);
        } else {
            let avail = (rest.h - g).max(0);
            let first = ((f64::from(avail) * ratio).round() as i32).clamp(0, avail);
            out.push(Rect::new(rest.x, rest.y, rest.w, first));
            rest = Rect::new(rest.x, rest.y + first + g, rest.w, avail - first);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 1280, 760)
    }

    fn overlaps(rects: &[Rect]) -> bool {
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                if a.intersect(b).is_some_and(|r| r.area() > 0) {
                    return true;
                }
            }
        }
        false
    }

    fn inputs(n: usize) -> Vec<ArrangeInput> {
        (0..n)
            .map(|i| ArrangeInput {
                id: format!("w{i}"),
                stacking: Rect::new(40 * i as i32, 30 * i as i32, 640, 480),
                pinned_floating: false,
            })
            .collect()
    }

    #[test]
    fn default_is_stacking_everywhere() {
        let p = LayoutPolicy::default();
        assert_eq!(p.global(), LayoutMode::Stacking);
        for ws in 0..9 {
            assert_eq!(p.mode_for(ws), LayoutMode::Stacking);
        }
    }

    #[test]
    fn workspace_toggle_only_changes_that_workspace() {
        let mut p = LayoutPolicy::default();
        let changed = p.apply(LayoutScope::Workspace(2), ModeRequest::Toggle, &[0, 1, 2]);
        assert_eq!(changed, vec![2]);
        assert_eq!(p.mode_for(2), LayoutMode::Tiling);
        assert_eq!(p.mode_for(0), LayoutMode::Stacking);
        assert!(p.has_override(2));
        // Toggling back removes the override instead of storing a redundant one.
        p.apply(LayoutScope::Workspace(2), ModeRequest::Toggle, &[0, 1, 2]);
        assert_eq!(p.mode_for(2), LayoutMode::Stacking);
        assert!(!p.has_override(2));
    }

    #[test]
    fn global_toggle_flips_everything_and_clears_overrides() {
        let mut p = LayoutPolicy::default();
        p.apply(
            LayoutScope::Workspace(1),
            ModeRequest::Set(LayoutMode::Tiling),
            &[0, 1],
        );
        let changed = p.apply(LayoutScope::Global, ModeRequest::Toggle, &[0, 1]);
        // ws1 was already tiling, so only ws0 changed effective mode.
        assert_eq!(changed, vec![0]);
        assert_eq!(p.global(), LayoutMode::Tiling);
        assert!(!p.has_override(1));
        assert_eq!(p.mode_for(5), LayoutMode::Tiling);
        p.apply(
            LayoutScope::Global,
            ModeRequest::Set(LayoutMode::Stacking),
            &[0, 1],
        );
        assert_eq!(p.mode_for(1), LayoutMode::Stacking);
    }

    #[test]
    fn workspace_override_survives_until_global_change() {
        let mut p = LayoutPolicy::new(LayoutMode::Tiling);
        p.apply(
            LayoutScope::Workspace(0),
            ModeRequest::Set(LayoutMode::Stacking),
            &[0],
        );
        assert_eq!(p.mode_for(0), LayoutMode::Stacking);
        assert_eq!(p.mode_for(1), LayoutMode::Tiling);
    }

    #[test]
    fn mode_request_parses_wire_names() {
        assert_eq!(ModeRequest::parse("toggle"), Some(ModeRequest::Toggle));
        assert_eq!(
            ModeRequest::parse(" Tiling "),
            Some(ModeRequest::Set(LayoutMode::Tiling))
        );
        assert_eq!(
            ModeRequest::parse("float"),
            Some(ModeRequest::Set(LayoutMode::Stacking))
        );
        assert_eq!(ModeRequest::parse("spiral"), None);
        assert_eq!(LayoutMode::Tiling.as_str(), "tiling");
    }

    #[test]
    fn stacking_returns_own_geometry() {
        let w = inputs(3);
        let out = arrange(
            LayoutMode::Stacking,
            TilingAlgorithm::default(),
            area(),
            8,
            &w,
        );
        for (i, (id, r)) in out.iter().enumerate() {
            assert_eq!(id, &w[i].id);
            assert_eq!(*r, w[i].stacking);
        }
    }

    #[test]
    fn tiling_is_non_overlapping_and_inside() {
        for n in 1..=7 {
            let w = inputs(n);
            let out = arrange(
                LayoutMode::Tiling,
                TilingAlgorithm::default(),
                area(),
                8,
                &w,
            );
            let rects: Vec<Rect> = out.iter().map(|(_, r)| *r).collect();
            assert!(!overlaps(&rects), "{n} windows overlap: {rects:?}");
            for r in &rects {
                assert!(area().contains(r), "{r:?} outside");
                assert!(r.w > 0 && r.h > 0);
            }
        }
    }

    #[test]
    fn dwindle_without_gap_covers_area_exactly() {
        for n in 1..=6 {
            let rects = dwindle(area(), n, 0, 0.5);
            let total: i64 = rects.iter().map(Rect::area).sum();
            assert_eq!(total, area().area(), "{n} windows");
        }
    }

    #[test]
    fn dwindle_shapes() {
        let r = dwindle(area(), 2, 0, 0.5);
        assert_eq!(r[0], Rect::new(0, 0, 640, 760));
        assert_eq!(r[1], Rect::new(640, 0, 640, 760));
        let r = dwindle(area(), 3, 0, 0.5);
        assert_eq!(r[1], Rect::new(640, 0, 640, 380));
        assert_eq!(r[2], Rect::new(640, 380, 640, 380));
    }

    #[test]
    fn pinned_windows_keep_stacking_geometry_while_tiled() {
        let mut w = inputs(3);
        w[1].pinned_floating = true;
        let out = arrange(
            LayoutMode::Tiling,
            TilingAlgorithm::default(),
            area(),
            0,
            &w,
        );
        assert_eq!(out[1].1, w[1].stacking);
        assert_eq!(out[0].1, Rect::new(0, 0, 640, 760));
        assert_eq!(out[2].1, Rect::new(640, 0, 640, 760));
    }

    #[test]
    fn toggle_round_trip_is_lossless() {
        let w = inputs(4);
        let tiled = arrange(
            LayoutMode::Tiling,
            TilingAlgorithm::default(),
            area(),
            8,
            &w,
        );
        assert_ne!(tiled[0].1, w[0].stacking);
        let back = arrange(
            LayoutMode::Stacking,
            TilingAlgorithm::default(),
            area(),
            8,
            &w,
        );
        for (i, (_, r)) in back.iter().enumerate() {
            assert_eq!(*r, w[i].stacking);
        }
    }

    #[test]
    fn master_stack_algorithm_is_available() {
        let w = inputs(3);
        let algo = TilingAlgorithm::MasterStack {
            master_count: 1,
            master_ratio: 0.5,
        };
        let out = arrange(LayoutMode::Tiling, algo, area(), 0, &w);
        assert_eq!(out[0].1.h, 760);
        assert_eq!(out[1].1.x, out[2].1.x);
    }
}
