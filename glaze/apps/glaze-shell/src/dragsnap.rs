//! Drag-to-edge snapping for floating windows in the Sway session.
//!
//! Sway moves a dragged floating window itself and tells no client about
//! the pointer, so the shell watches window geometry instead (the same
//! poll that keeps the title-bar buttons on their windows). A window that
//! moves over several polls at a constant size is being dragged. When it
//! comes to rest pushed past a screen edge, which only happens while the
//! pointer is at that edge, it snaps through `glaze-snap` like the
//! Super+arrow keys:
//!
//! * title bar pushed above the top: maximise;
//! * past the left / right edge: that half;
//! * past a side and the top, or with its title bar down at the bottom:
//!   that quarter.
//!
//! Keyboard snaps and the shell's own animations never trigger it: a
//! snapped geometry always lies inside the work area, and windows being
//! animated are skipped. The geometry from before the drag is saved as the
//! window's restore geometry, so Super+Down / the restore button bring the
//! old size and place back. The Glaze compositor snaps windows itself.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::swayipc::View;

/// Outer frame `(x, y, w, h)`, title bar included.
type Frame = (i32, i32, i32, i32);

/// How far past a side edge a window must be pushed.
const SIDE_PUSH: i32 = 16;
/// Rest time after the last move before the snap happens.
const SETTLE: Duration = Duration::from_millis(350);
/// Moves seen before a motion counts as a drag (a scripted move is one).
const MIN_MOVES: u32 = 2;

struct Drag {
    start: Frame,
    moves: u32,
    since: Instant,
}

#[derive(Default)]
pub struct DragSnap {
    frames: HashMap<i64, Frame>,
    drags: HashMap<i64, Drag>,
}

/// One snap to perform: window, `glaze-snap` mode, pre-drag frame, pid.
pub struct Snap {
    pub id: i64,
    pub mode: &'static str,
    pub start: Frame,
    pub pid: i64,
}

/// The snap zone for a window at rest at `f` in work area `area`.
pub fn zone(f: Frame, title_h: i32, area: Frame) -> Option<&'static str> {
    let (x, y, w, _) = f;
    let (ax, ay, aw, ah) = area;
    let left = x <= ax - SIDE_PUSH;
    let right = x + w >= ax + aw + SIDE_PUSH;
    let top = y < ay;
    let bottom = y + title_h.max(1) >= ay + ah;
    match (top, bottom, left, right) {
        (true, _, true, _) => Some("topleft"),
        (true, _, _, true) => Some("topright"),
        (true, _, _, _) => Some("maximise"),
        (_, true, true, _) => Some("bottomleft"),
        (_, true, _, true) => Some("bottomright"),
        (_, _, true, _) => Some("left"),
        (_, _, _, true) => Some("right"),
        _ => None,
    }
}

impl DragSnap {
    /// Feed the current views; returns (any floating window moved, snaps
    /// to perform now).
    pub fn update(&mut self, views: &[View], now: Instant) -> (bool, Vec<Snap>) {
        let mut moved = false;
        let mut snaps = Vec::new();
        let mut seen = Vec::new();
        for v in views
            .iter()
            .filter(|v| v.visible && v.floating && !v.fullscreen && !v.animating)
        {
            seen.push(v.id);
            let f = v.frame();
            match self.frames.insert(v.id, f) {
                Some(p) if p != f => {
                    moved = true;
                    if (p.2, p.3) == (f.2, f.3) {
                        let d = self.drags.entry(v.id).or_insert(Drag {
                            start: p,
                            moves: 0,
                            since: now,
                        });
                        d.moves += 1;
                        d.since = now;
                    } else {
                        // Resized (by the user or a snap): not a drag.
                        self.drags.remove(&v.id);
                    }
                }
                Some(_) => {
                    let rested = self
                        .drags
                        .get(&v.id)
                        .is_some_and(|d| now.duration_since(d.since) >= SETTLE);
                    if rested {
                        if let Some(d) = self.drags.remove(&v.id) {
                            let mode = zone(f, v.title_height, v.area);
                            if let Some(mode) = mode.filter(|_| d.moves >= MIN_MOVES) {
                                snaps.push(Snap {
                                    id: v.id,
                                    mode,
                                    start: d.start,
                                    pid: v.pid,
                                });
                            }
                        }
                    }
                }
                None => {}
            }
        }
        self.frames.retain(|id, _| seen.contains(id));
        self.drags.retain(|id, _| seen.contains(id));
        (moved, snaps)
    }

    /// A floating window is on screen (worth polling for drags).
    pub fn watching(&self) -> bool {
        !self.frames.is_empty()
    }

    /// A drag is in progress or settling.
    pub fn busy(&self) -> bool {
        !self.drags.is_empty()
    }
}

/// Snap `s` with `glaze-snap`, recording the pre-drag frame as its restore
/// geometry first (unless it already has one from an earlier snap).
pub fn perform(s: &Snap) {
    // Same place glaze-snap keeps it ($XDG_RUNTIME_DIR is always set in a
    // Sway session; without it only the restore memory is skipped).
    let dir =
        std::env::var_os("XDG_RUNTIME_DIR").map(|d| std::path::PathBuf::from(d).join("glaze-snap"));
    let path = dir.as_ref().map(|d| d.join(format!("{}.json", s.id)));
    if let (Some(dir), Some(path)) = (dir, path.filter(|p| !p.exists())) {
        let (x, y, w, h) = s.start;
        let json = serde_json::json!({ "x": x, "y": y, "w": w, "h": h, "pid": s.pid });
        if let Err(e) =
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, json.to_string()))
        {
            eprintln!("[glaze-shell] drag snap: restore geometry not saved: {e}");
        }
    }
    eprintln!("[glaze-shell] drag snap: con {} -> {}", s.id, s.mode);
    crate::panel_ui::snap(&s.id.to_string(), s.mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Frame = (0, 0, 1280, 766);

    fn view(id: i64, f: Frame) -> View {
        View {
            id,
            x: f.0,
            y: f.1 + 25,
            width: f.2,
            height: f.3 - 25,
            title_height: 25,
            border_width: 6,
            floating: true,
            focused: true,
            fullscreen: false,
            visible: true,
            tabbed: false,
            workspace: "1".into(),
            area: AREA,
            animating: false,
            pid: 42,
        }
    }

    #[test]
    fn zones_follow_the_edge_the_window_was_pushed_past() {
        assert_eq!(zone((300, -12, 640, 766), 25, AREA), Some("maximise"));
        assert_eq!(zone((-110, 300, 640, 500), 25, AREA), Some("left"));
        assert_eq!(zone((900, 300, 640, 500), 25, AREA), Some("right"));
        assert_eq!(zone((-50, -5, 640, 500), 25, AREA), Some("topleft"));
        assert_eq!(zone((900, -5, 640, 500), 25, AREA), Some("topright"));
        assert_eq!(zone((-50, 770, 640, 500), 25, AREA), Some("bottomleft"));
        assert_eq!(zone((900, 770, 640, 500), 25, AREA), Some("bottomright"));
        // Keyboard snaps land exactly inside the work area: no zone.
        for f in [
            (0, 0, 640, 766),
            (640, 0, 640, 383),
            (0, 0, 1280, 766),
            (640, 383, 640, 383),
        ] {
            assert_eq!(zone(f, 25, AREA), None, "{f:?}");
        }
        // Dragged to the bottom edge alone: nothing.
        assert_eq!(zone((300, 770, 640, 500), 25, AREA), None);
    }

    #[test]
    fn a_drag_that_rests_past_an_edge_snaps_once() {
        let mut d = DragSnap::default();
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        assert!(!d.update(&[view(1, (320, 0, 640, 766))], ms(0)).0);
        assert!(d.update(&[view(1, (200, 100, 640, 766))], ms(50)).0);
        assert!(d.update(&[view(1, (-120, 200, 640, 766))], ms(100)).0);
        assert!(d.busy());
        // Still settling.
        assert!(d
            .update(&[view(1, (-120, 200, 640, 766))], ms(200))
            .1
            .is_empty());
        let (_, snaps) = d.update(&[view(1, (-120, 200, 640, 766))], ms(500));
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].mode, "left");
        assert_eq!(snaps[0].start, (320, 0, 640, 766));
        assert!(!d.busy());
        assert!(d
            .update(&[view(1, (-120, 200, 640, 766))], ms(900))
            .1
            .is_empty());
    }

    #[test]
    fn single_jumps_resizes_and_animations_never_snap() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        // One jump (a scripted move) past the edge.
        let mut d = DragSnap::default();
        d.update(&[view(1, (320, 0, 640, 766))], ms(0));
        d.update(&[view(1, (-200, 0, 640, 766))], ms(50));
        assert!(d
            .update(&[view(1, (-200, 0, 640, 766))], ms(600))
            .1
            .is_empty());
        // A resize ends the drag.
        let mut d = DragSnap::default();
        d.update(&[view(1, (320, 0, 640, 766))], ms(0));
        d.update(&[view(1, (100, 0, 640, 766))], ms(50));
        d.update(&[view(1, (-200, 0, 640, 766))], ms(100));
        d.update(&[view(1, (-200, 0, 500, 766))], ms(150));
        assert!(d
            .update(&[view(1, (-200, 0, 500, 766))], ms(700))
            .1
            .is_empty());
        // Animated windows are not watched at all.
        let mut d = DragSnap::default();
        let mut v = view(1, (320, 0, 640, 766));
        v.animating = true;
        d.update(&[v], ms(0));
        assert!(!d.watching());
    }
}
