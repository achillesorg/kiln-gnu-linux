//! GTK frame-clock animation engine.
//!
//! Every animated value is a critically damped [`Spring`]
//! (`glaze_design::motion`), stepped from the widget's frame clock (vsync
//! paced), retargetable mid-flight (it keeps its velocity) and instant under
//! reduced motion. The tick callback is installed only while something moves
//! and removed when everything has settled, so an idle shell costs nothing.
//!
//! Frame intervals of each run are logged
//! (`[glaze-shell] anim <name>: N frames, mean/p95/max ms, slow>20ms`) so
//! smoothness on the VM's software renderer is measured, not guessed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glaze_design::motion::{FrameStats, MotionSettings, Spring};
use gtk4::glib;
use gtk4::prelude::*;

type Apply = Box<dyn Fn(f64)>;
type Done = Box<dyn FnOnce()>;

struct Track {
    spring: Spring,
    apply: Apply,
    done: Option<Done>,
}

struct Inner {
    name: &'static str,
    tracks: HashMap<String, Track>,
    tick: Option<gtk4::TickCallbackId>,
    last_us: Option<i64>,
    /// Wall-clock time of the last tick (watchdog).
    last_wall: std::time::Instant,
    watchdog: bool,
    /// Frames stepped by the fallback timer in this run.
    timer_frames: u32,
    last_from_clock: Option<bool>,
    stats: FrameStats,
    settings: MotionSettings,
    log: bool,
}

/// One animator per host widget (the task strip, a popover, the panel).
#[derive(Clone)]
pub struct Animator {
    inner: Rc<RefCell<Inner>>,
    host: gtk4::Widget,
}

impl Animator {
    pub fn new(
        name: &'static str,
        host: &impl IsA<gtk4::Widget>,
        settings: MotionSettings,
        log: bool,
    ) -> Self {
        Animator {
            inner: Rc::new(RefCell::new(Inner {
                name,
                tracks: HashMap::new(),
                tick: None,
                last_us: None,
                last_wall: std::time::Instant::now(),
                watchdog: false,
                timer_frames: 0,
                last_from_clock: None,
                stats: FrameStats::default(),
                settings,
                log,
            })),
            host: host.clone().upcast(),
        }
    }

    pub fn settings(&self) -> MotionSettings {
        self.inner.borrow().settings
    }

    /// Animate `key` towards `to` over about `base_ms` (scaled by the motion
    /// settings). A running track is retargeted (velocity kept); a new one
    /// starts at `from` (or `to`). `apply` receives every intermediate value;
    /// `done` runs once when the track settles.
    pub fn animate(
        &self,
        key: &str,
        from: f64,
        to: f64,
        base_ms: f64,
        apply: impl Fn(f64) + 'static,
        done: Option<Box<dyn FnOnce()>>,
    ) {
        let settings = self.settings();
        if settings.instant() {
            // Reduced motion / off: jump, but keep the done hook contract.
            if let Some(t) = self.inner.borrow_mut().tracks.remove(key) {
                drop(t.done);
            }
            apply(to);
            if let Some(d) = done {
                d();
            }
            return;
        }
        {
            let mut inner = self.inner.borrow_mut();
            let settle = settings.settle_ms(base_ms);
            match inner.tracks.get_mut(key) {
                Some(t) => {
                    t.spring.set_settle_ms(settle);
                    t.spring.retarget(to);
                    t.apply = Box::new(apply);
                    // A retarget replaces the completion hook (e.g. a button
                    // being removed is re-added: it must not be destroyed).
                    t.done = done;
                }
                None => {
                    let mut spring = Spring::new(from, settle);
                    spring.retarget(to);
                    apply(from);
                    inner.tracks.insert(
                        key.to_string(),
                        Track {
                            spring,
                            apply: Box::new(apply),
                            done,
                        },
                    );
                }
            }
        }
        self.ensure_ticking();
    }

    fn ensure_ticking(&self) {
        if self.inner.borrow().tick.is_some() {
            return;
        }
        let inner = self.inner.clone();
        let id = self.host.add_tick_callback(move |_w, clock| {
            if step(&inner, clock.frame_time(), true) {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        {
            let mut st = self.inner.borrow_mut();
            st.tick = Some(id);
            st.last_wall = std::time::Instant::now();
        }
        self.ensure_watchdog();
    }

    /// A surface whose frame clock does not tick (e.g. a popover animated
    /// from its parent's clock, or a layer surface with nothing damaged)
    /// would freeze the animation. When no frame-clock tick arrived for
    /// ~2 frames, this 60 Hz timer steps the springs itself (wall-clock
    /// dt), so motion stays smooth either way. The log says which drove it.
    fn ensure_watchdog(&self) {
        if self.inner.borrow().watchdog {
            return;
        }
        self.inner.borrow_mut().watchdog = true;
        let inner = self.inner.clone();
        let t0 = std::time::Instant::now();
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let (empty, stalled) = {
                let st = inner.borrow();
                (
                    st.tracks.is_empty(),
                    st.last_wall.elapsed() > std::time::Duration::from_millis(34),
                )
            };
            if empty {
                let mut st = inner.borrow_mut();
                st.watchdog = false;
                if let Some(id) = st.tick.take() {
                    id.remove();
                }
                return glib::ControlFlow::Break;
            }
            if stalled {
                inner.borrow_mut().timer_frames += 1;
                let now_us = t0.elapsed().as_micros() as i64 + 1_000_000_000;
                if !step(&inner, now_us, false) {
                    let mut st = inner.borrow_mut();
                    st.watchdog = false;
                    if let Some(id) = st.tick.take() {
                        id.remove();
                    }
                    return glib::ControlFlow::Break;
                }
            }
            glib::ControlFlow::Continue
        });
    }
}

/// Advance every track to `now_us`; returns whether anything still moves.
/// `from_clock` tells frame-clock ticks from timer steps (their time bases
/// differ, so switching source restarts the dt measurement).
fn step(inner: &Rc<RefCell<Inner>>, now_us: i64, from_clock: bool) -> bool {
    let mut finished: Vec<Done> = Vec::new();
    {
        let mut st = inner.borrow_mut();
        if st.last_from_clock != Some(from_clock) {
            st.last_us = None;
            st.last_from_clock = Some(from_clock);
        }
        let dt = match st.last_us {
            Some(prev) => ((now_us - prev) as f64 / 1_000_000.0).clamp(0.0, 0.1),
            None => 1.0 / 60.0,
        };
        if let Some(prev) = st.last_us {
            st.stats.push((now_us - prev) as f64 / 1000.0);
        }
        st.last_us = Some(now_us);
        if from_clock {
            st.last_wall = std::time::Instant::now();
        }
        let mut settled = Vec::new();
        for (k, t) in st.tracks.iter_mut() {
            t.spring.step(dt);
            if t.spring.is_settled() {
                // Land exactly on the target (no 0.98 opacity leftovers).
                (t.apply)(t.spring.target);
                settled.push(k.clone());
            } else {
                (t.apply)(t.spring.value);
            }
        }
        for k in settled {
            if let Some(mut t) = st.tracks.remove(&k) {
                if let Some(d) = t.done.take() {
                    finished.push(d);
                }
            }
        }
    }
    for d in finished {
        d();
    }
    if !inner.borrow().tracks.is_empty() {
        return true;
    }
    let mut st = inner.borrow_mut();
    if from_clock {
        st.tick = None;
    }
    st.last_us = None;
    st.last_from_clock = None;
    if st.log {
        if let Some((mean, p95, max, slow)) = st.stats.summary(20.0) {
            eprintln!(
                "[glaze-shell] anim {}: {} frames ({} timer-driven), mean {:.1} ms ({:.0} fps), p95 {:.1} ms, max {:.1} ms, {} slow (>20 ms)",
                st.name,
                st.stats.frames(),
                st.timer_frames,
                mean,
                1000.0 / mean.max(0.001),
                p95,
                max,
                slow
            );
        }
    }
    st.stats.clear();
    st.timer_frames = 0;
    false
}
