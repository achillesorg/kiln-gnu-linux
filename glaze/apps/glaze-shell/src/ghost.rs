//! Minimise / restore / close motion for real windows in the Sway session.
//!
//! Sway animates nothing. Earlier the shell painted a paper outline on a
//! full-screen overlay surface, which read as a blank sheet over the desktop.
//! Now the window itself moves: every frame sends one Sway command that sets
//! the container's `opacity` and (for floating windows) nudges its position
//! towards or away from its taskbar button. What you see is always the live
//! window, translucent while it travels; no extra surface is mapped at all.
//!
//! * minimise: fade out while sliding part of the way to the task button,
//!   then park in the scratchpad and put position and opacity back (both
//!   invisible at that point), so a later restore finds the old geometry;
//! * restore: show at opacity 0, then slide in from the button and fade in;
//! * close: fade out with a short drop, then `kill`. An app that refuses to
//!   close (unsaved changes) fades back in.
//!
//! Each motion runs on its own short-lived thread with its own Sway IPC
//! connection, so it keeps its frame rate while the GTK main loop is busy
//! (the taskbar repaints on the same tick, and on llvmpipe a repaint can
//! take longer than a whole animation frame).
//!
//! Sway's scratchpad forgets a floating window's size and place when it
//! comes back (`floating enable` re-centres it at its natural size), so the
//! frame from before a minimise is remembered and put back on restore.
//!
//! Windows being animated carry the hidden Sway mark `_glaze_anim`, so a
//! shell restart in the middle of an animation can put their opacity back.
//! Reduced motion / motion off: no animation, the plain command runs.
//! The Glaze compositor animates windows itself and never uses this.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use glaze_backend::BackendError;
use glaze_design::motion::MotionSettings;
use gtk4::gdk;
use gtk4::prelude::*;

use crate::panel_ui::Ctx;
use crate::swayipc;

/// Screen rect `(x, y, w, h)`.
pub type Rect = (f64, f64, f64, f64);
pub type ButtonLookup = Box<dyn Fn(&str) -> Option<Rect>>;
/// Integer frame `(x, y, w, h)`.
type Frame = (i32, i32, i32, i32);

const MARK: &str = "_glaze_anim";
/// Base durations (ms), scaled by the motion settings.
const MINIMISE_MS: f64 = 230.0;
const RESTORE_MS: f64 = 250.0;
const CLOSE_MS: f64 = 160.0;
/// Share of the way to the task button a floating window travels.
const TRAVEL: f64 = 0.45;
/// Frame period of the driver threads (~60 Hz).
const FRAME: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Minimise,
    Restore,
    Close,
}

#[derive(Clone, Copy)]
struct Run {
    kind: Kind,
    started: Instant,
    ms: f64,
    /// Floating window: (x0, y0) is its real position, (dx, dy) the offset
    /// at the "away" end of the motion.
    pos: Option<(i32, i32, f64, f64)>,
}

struct Driver {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

pub struct Ghost {
    settings: MotionSettings,
    buttons: ButtonLookup,
    runs: RefCell<HashMap<i64, Driver>>,
    alive: Cell<bool>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<Ghost>>> = const { RefCell::new(None) };
    /// Floating frames of minimised windows, by con id.
    static PARKED: RefCell<HashMap<i64, Frame>> = RefCell::new(HashMap::new());
}

fn active() -> Option<Rc<Ghost>> {
    ACTIVE.with(|a| a.borrow().clone())
}

fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in(t: f64) -> f64 {
    t * t * t
}

/// How far (0 = in place and opaque, 1 = faded and shifted) a window is at
/// progress `t` of a motion of `kind`.
fn away(kind: Kind, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        Kind::Minimise | Kind::Close => ease_in(t),
        Kind::Restore => 1.0 - ease_out(t),
    }
}

/// The Sway command for progress `t` of `run` on con `id`, and the
/// (opacity %, x, y) it sets (to skip identical frames).
fn frame_command(id: i64, run: &Run, t: f64) -> (String, (i32, i32, i32)) {
    let a = away(run.kind, t);
    let opacity = ((1.0 - a) * 100.0).round() as i32;
    let (x, y) = match run.pos {
        Some((x0, y0, dx, dy)) => (x0 + (dx * a).round() as i32, y0 + (dy * a).round() as i32),
        None => (0, 0),
    };
    let alpha = opacity as f64 / 100.0;
    let cmd = if run.pos.is_some() {
        format!("[con_id={id}] opacity {alpha:.2}, move position {x} {y}")
    } else {
        format!("[con_id={id}] opacity {alpha:.2}")
    };
    (cmd, (opacity, x, y))
}

/// Put run `id` in its final state (driver thread).
fn finish_commands(id: i64, run: &Run) {
    let back = run
        .pos
        .map(|(x0, y0, _, _)| format!(", move position {x0} {y0}"))
        .unwrap_or_default();
    match run.kind {
        Kind::Minimise => {
            // Invisible at this point: put the geometry back, park it,
            // then make it opaque again inside the scratchpad.
            swayipc::command(&format!("[con_id={id}] opacity 0{back}, move scratchpad"));
            swayipc::command(&format!("[con_id={id}] opacity 1, unmark {MARK}"));
        }
        Kind::Restore => {
            swayipc::command(&format!("[con_id={id}] opacity 1{back}, unmark {MARK}"));
        }
        Kind::Close => {
            swayipc::command(&format!("[con_id={id}] opacity 0{back}"));
            swayipc::command(&format!("[con_id={id}] kill"));
            // Still there a moment later (a "save changes?" prompt, or an
            // app that ignores close): bring it back. Detached, so ending
            // this run never waits for it.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(700));
                let still = swayipc::views()
                    .map(|v| v.iter().any(|w| w.id == id))
                    .unwrap_or(false);
                if still {
                    swayipc::command(&format!("[con_id={id}] opacity 1, unmark {MARK}"));
                }
            });
        }
    }
}

/// Drive one motion to its end (or until `stop`), then finish it.
fn drive(id: i64, run: Run, stop: Arc<AtomicBool>) {
    let mut last = None;
    let mut frames = 0u32;
    loop {
        let tick = Instant::now();
        let t = tick.duration_since(run.started).as_secs_f64() * 1000.0 / run.ms;
        if t >= 1.0 || stop.load(Ordering::Relaxed) {
            break;
        }
        let (cmd, key) = frame_command(id, &run, t);
        if last != Some(key) {
            last = Some(key);
            frames += 1;
            swayipc::command(&cmd);
        }
        if let Some(rest) = FRAME.checked_sub(tick.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    finish_commands(id, &run);
    eprintln!(
        "[glaze-shell] anim window {:?} con {id}: {frames} frames in {:.0} ms",
        run.kind,
        run.started.elapsed().as_secs_f64() * 1000.0
    );
}

impl Ghost {
    /// Install the animator for the Sway session. `buttons` maps a window id
    /// to its task button's screen rect (the minimise target).
    pub fn new(ctx: &Ctx, buttons: ButtonLookup) -> Option<Rc<Self>> {
        if !swayipc::available() {
            return None;
        }
        // A previous shell may have died mid-animation: never leave a
        // window invisible. (No match is not an error worth logging.)
        swayipc::command(&format!("[con_mark=\"{MARK}\"] opacity 1, unmark {MARK}"));
        let g = Rc::new(Ghost {
            settings: ctx.motion,
            buttons,
            runs: RefCell::new(HashMap::new()),
            alive: Cell::new(true),
        });
        ACTIVE.with(|a| *a.borrow_mut() = Some(g.clone()));
        Some(g)
    }

    /// Finish every running animation at once and uninstall.
    pub fn destroy(&self) {
        self.alive.set(false);
        let ids: Vec<i64> = self.runs.borrow().keys().copied().collect();
        for id in ids {
            self.finish(id);
        }
        ACTIVE.with(|a| {
            let mine = a
                .borrow()
                .as_ref()
                .is_some_and(|g| std::ptr::eq(g.as_ref(), self));
            if mine {
                *a.borrow_mut() = None;
            }
        });
    }

    fn usable(&self) -> bool {
        self.alive.get() && !self.settings.instant()
    }

    /// Offset towards the task button of window `id` for a window at `frame`.
    fn offset(&self, id: &str, frame: Frame) -> (f64, f64) {
        let (fx, fy, fw, fh) = (
            frame.0 as f64,
            frame.1 as f64,
            frame.2 as f64,
            frame.3 as f64,
        );
        let (cx, cy) = (fx + fw / 2.0, fy + fh / 2.0);
        let (tx, ty) = match (self.buttons)(id) {
            Some((bx, by, bw, bh)) => (bx + bw / 2.0, by + bh / 2.0),
            None => {
                let (mw, mh) = screen_size();
                (mw / 2.0, mh)
            }
        };
        ((tx - cx) * TRAVEL, (ty - cy) * TRAVEL)
    }

    fn start(&self, id: i64, run: Run) {
        // Forget drivers that are done.
        self.runs
            .borrow_mut()
            .retain(|_, d| !d.thread.is_finished());
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("glaze-anim".into())
            .spawn(move || drive(id, run, s));
        match spawned {
            Ok(thread) => {
                self.runs.borrow_mut().insert(id, Driver { stop, thread });
            }
            Err(e) => {
                // No thread: land in the final state at once.
                eprintln!("[glaze-shell] anim window con {id}: {e}");
                finish_commands(id, &run);
            }
        }
    }

    /// End run `id` in its final state (waits at most about one frame).
    fn finish(&self, id: i64) {
        let Some(d) = self.runs.borrow_mut().remove(&id) else {
            return;
        };
        d.stop.store(true, Ordering::Relaxed);
        let _ = d.thread.join();
    }

    /// The visible view `id` (ending any motion it is in), when animating
    /// is allowed.
    fn target(&self, id: &str) -> Option<(i64, swayipc::View)> {
        if !self.usable() {
            return None;
        }
        let num: i64 = id.parse().ok()?;
        // A new request for the same window ends the old motion first.
        self.finish(num);
        let view = swayipc::views()?.into_iter().find(|v| v.id == num)?;
        view.visible.then_some((num, view))
    }

    fn run_for(&self, id: &str, kind: Kind, view: &swayipc::View, base_ms: f64) -> Run {
        let frame = view.frame();
        let pos = view.floating.then(|| {
            let (dx, dy) = match kind {
                // A short drop, not a trip to the taskbar.
                Kind::Close => (0.0, 18.0),
                _ => self.offset(id, frame),
            };
            (frame.0, frame.1, dx, dy)
        });
        Run {
            kind,
            started: Instant::now(),
            ms: self.settings.settle_ms(base_ms).max(1.0),
            pos,
        }
    }
}

fn screen_size() -> (f64, f64) {
    gdk::Display::default()
        .and_then(|d| d.monitors().item(0))
        .and_then(|o| o.downcast::<gdk::Monitor>().ok())
        .map(|m| {
            let g = m.geometry();
            (g.width() as f64, g.height() as f64)
        })
        .unwrap_or((1280.0, 800.0))
}

/// Remember the floating frame of `id` before it is minimised (animated or
/// not), for [`put_back`].
pub fn remember(id: &str) {
    let Ok(num) = id.parse::<i64>() else { return };
    // A motion in flight would report a shifted position: end it first.
    if let Some(g) = active() {
        g.finish(num);
    }
    let Some(view) = swayipc::views().and_then(|v| v.into_iter().find(|v| v.id == num)) else {
        return;
    };
    PARKED.with(|p| {
        let mut p = p.borrow_mut();
        if view.floating && !view.fullscreen {
            p.insert(num, view.frame());
        } else {
            p.remove(&num);
        }
    });
}

/// Give a window that just left the scratchpad its remembered frame back
/// (kept inside its work area). Tiled windows are left to the layout.
pub fn put_back(id: &str) {
    let Ok(num) = id.parse::<i64>() else { return };
    let Some(saved) = PARKED.with(|p| p.borrow_mut().remove(&num)) else {
        return;
    };
    let Some(view) = swayipc::views().and_then(|v| v.into_iter().find(|v| v.id == num)) else {
        return;
    };
    if !view.floating {
        return;
    }
    let (x, y, w, h) = fit(saved, view.area);
    swayipc::command(&format!(
        "[con_id={num}] resize set {w} {h}, move position {x} {y}"
    ));
}

/// `f` moved (and if need be shrunk) to lie inside `area`.
fn fit(f: Frame, area: Frame) -> Frame {
    let (ax, ay, aw, ah) = area;
    if aw <= 0 || ah <= 0 {
        return f;
    }
    let w = f.2.min(aw);
    let h = f.3.min(ah);
    let x = f.0.clamp(ax, ax + aw - w);
    let y = f.1.clamp(ay, ay + ah - h);
    (x, y, w, h)
}

/// Minimise `id` with motion. `None`: not animated (reduced motion, no
/// Sway, window not visible); the caller minimises it directly.
pub fn minimise(id: &str) -> Option<Result<(), BackendError>> {
    let g = active()?;
    let (num, view) = g.target(id)?;
    if !swayipc::command(&format!("[con_id={num}] mark --add {MARK}")) {
        return None;
    }
    let run = g.run_for(id, Kind::Minimise, &view, MINIMISE_MS);
    g.start(num, run);
    Some(Ok(()))
}

/// Restore `id` with motion: `show` brings it out of the scratchpad (the
/// backend's own restore). `None`: not animated and `show` not called.
pub fn restore(
    id: &str,
    show: impl FnOnce() -> Result<(), BackendError>,
) -> Option<Result<(), BackendError>> {
    let g = active()?;
    if !g.usable() {
        return None;
    }
    let num: i64 = id.parse().ok()?;
    g.finish(num);
    // Invisible before it appears, so the first frame is not a flash.
    if !swayipc::command(&format!("[con_id={num}] opacity 0, mark --add {MARK}")) {
        return None;
    }
    if let Err(e) = show() {
        swayipc::command(&format!("[con_id={num}] opacity 1, unmark {MARK}"));
        return Some(Err(e));
    }
    let view = swayipc::views().and_then(|v| v.into_iter().find(|v| v.id == num));
    let Some(view) = view.filter(|v| v.visible) else {
        swayipc::command(&format!("[con_id={num}] opacity 1, unmark {MARK}"));
        return Some(Ok(()));
    };
    let run = g.run_for(id, Kind::Restore, &view, RESTORE_MS);
    g.start(num, run);
    Some(Ok(()))
}

/// Close `id` with a fade. `None`: not animated; the caller closes it.
pub fn close(id: &str) -> Option<Result<(), BackendError>> {
    let g = active()?;
    let (num, view) = g.target(id)?;
    if !swayipc::command(&format!("[con_id={num}] mark --add {MARK}")) {
        return None;
    }
    let run = g.run_for(id, Kind::Close, &view, CLOSE_MS);
    g.start(num, run);
    Some(Ok(()))
}

/// A monitor's size (logical px) for the panel window `w`, used to turn
/// panel-local coordinates into screen coordinates.
pub fn monitor_size(w: &gtk4::Window) -> Option<(i32, i32)> {
    let surface = w.surface()?;
    let mon = gdk::Display::default()?.monitor_at_surface(&surface)?;
    let g = mon.geometry();
    Some((g.width(), g.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_ends_where_it_should() {
        // Minimise and close start in place and end fully away.
        assert_eq!(away(Kind::Minimise, 0.0), 0.0);
        assert_eq!(away(Kind::Minimise, 1.0), 1.0);
        assert_eq!(away(Kind::Close, 1.0), 1.0);
        // Restore starts away and lands in place.
        assert_eq!(away(Kind::Restore, 0.0), 1.0);
        assert_eq!(away(Kind::Restore, 1.0), 0.0);
        // Monotonic.
        let mut prev = away(Kind::Restore, 0.0);
        for i in 1..=20 {
            let v = away(Kind::Restore, i as f64 / 20.0);
            assert!(v <= prev);
            prev = v;
        }
    }

    #[test]
    fn frames_fade_and_travel_together() {
        let run = Run {
            kind: Kind::Minimise,
            started: Instant::now(),
            ms: 200.0,
            pos: Some((100, 50, 200.0, 300.0)),
        };
        let (cmd, key) = frame_command(7, &run, 0.0);
        assert_eq!(key, (100, 100, 50));
        assert_eq!(cmd, "[con_id=7] opacity 1.00, move position 100 50");
        let (_, key) = frame_command(7, &run, 1.0);
        assert_eq!(key, (0, 300, 350));
        let tiled = Run { pos: None, ..run };
        assert_eq!(frame_command(7, &tiled, 1.0).0, "[con_id=7] opacity 0.00");
    }

    #[test]
    fn restored_frames_stay_in_the_work_area() {
        let area = (0, 0, 1280, 766);
        assert_eq!(fit((640, 0, 640, 383), area), (640, 0, 640, 383));
        // Title bar above the top, and hanging off the right.
        assert_eq!(fit((900, -12, 640, 400), area), (640, 0, 640, 400));
        // Bigger than the area: shrunk to it.
        assert_eq!(fit((0, 0, 1400, 900), area), (0, 0, 1280, 766));
    }
}
