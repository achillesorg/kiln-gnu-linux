//! Compositor-side animation plumbing: interruptible, retargetable tweens
//! driven by the render clock.
//!
//! The render loop samples every running animation once per presented frame
//! (vsync-paced by the backend's frame clock), so motion is time-based, never
//! frame-count based: a slow llvmpipe frame skips ahead instead of slowing the
//! animation down. A window with a running animation is re-rendered every
//! frame; everything else is left to damage tracking.
//!
//! Policy (see `docs/decisions.md`): motion is smooth by default, speed is a
//! user multiplier, and **reduced motion always wins** (durations collapse to
//! zero, so every transition jumps to its end state on the next frame).

use std::time::Duration;

use glaze_wm::Rect;

/// Easing curves. Flat paper does not bounce: no overshoot by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    /// Fast start, gentle settle: the default for things arriving.
    OutCubic,
    /// Slow start, fast exit: things leaving.
    InCubic,
    /// Symmetric, for moves between two resting places.
    InOutCubic,
    /// Critically damped spring (no overshoot), normalised to end at 1.
    Spring,
}

impl Easing {
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::OutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::InCubic => t * t * t,
            Easing::Spring => {
                const K: f64 = 7.0;
                let f = |x: f64| 1.0 - (1.0 + K * x) * (-K * x).exp();
                (f(t) / f(1.0)).min(1.0)
            }
            Easing::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

/// How much window motion there is (`[motion] intensity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Intensity {
    /// No animations (like reduced motion).
    Off,
    /// Shorter, smaller moves.
    Low,
    #[default]
    Normal,
    /// Longer moves, larger offsets, a subtle scale on open.
    High,
}

impl Intensity {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Intensity::Off,
            "low" | "subtle" => Intensity::Low,
            "normal" | "default" => Intensity::Normal,
            "high" | "full" => Intensity::High,
            _ => return None,
        })
    }

    /// Duration multiplier.
    pub fn time(self) -> f64 {
        match self {
            Intensity::Off => 0.0,
            Intensity::Low => 0.6,
            Intensity::Normal => 1.0,
            Intensity::High => 1.25,
        }
    }

    /// Open/close and popup offset, logical px.
    pub fn slide(self) -> f64 {
        match self {
            Intensity::Off => 0.0,
            Intensity::Low => 5.0,
            Intensity::Normal => OPEN_SLIDE,
            Intensity::High => 16.0,
        }
    }
}

/// Motion settings (from `[appearance]` / `[motion]` in the Glaze config).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionConfig {
    /// Multiplier on every duration: 1.0 = default, 2.0 = twice as slow,
    /// 0.5 = twice as fast. Clamped to `0.1..=5.0`.
    pub speed: f64,
    /// Reduced motion: every duration becomes zero.
    pub reduced: bool,
    pub intensity: Intensity,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            reduced: false,
            intensity: Intensity::Normal,
        }
    }
}

impl MotionConfig {
    /// Scale a base duration by the user's settings.
    pub fn duration(&self, base_ms: u64) -> Duration {
        if self.reduced || self.intensity == Intensity::Off {
            return Duration::ZERO;
        }
        let speed = if self.speed.is_finite() {
            self.speed.clamp(0.1, 5.0)
        } else {
            1.0
        };
        Duration::from_secs_f64(base_ms as f64 / 1000.0 * speed * self.intensity.time())
    }
}

/// Base durations (ms) before the speed multiplier.
pub mod durations {
    pub const OPEN: u64 = 180;
    pub const CLOSE: u64 = 150;
    pub const MINIMIZE: u64 = 220;
    pub const RESTORE: u64 = 220;
    pub const MOVE: u64 = 220;
    pub const WORKSPACE: u64 = 260;
    pub const FOCUS: u64 = 160;
    pub const POPUP: u64 = 140;
    pub const SWITCHER: u64 = 120;
    pub const LOCK: u64 = 220;
    pub const LOGOUT: u64 = 350;
    pub const HOVER: u64 = 90;
}

/// A scalar tween from `from` to `to`. `now` is any monotonic clock in
/// [`Duration`] since an arbitrary epoch (the compositor's start).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    pub from: f64,
    pub to: f64,
    pub start: Duration,
    pub duration: Duration,
    pub easing: Easing,
}

impl Tween {
    pub fn new(from: f64, to: f64, start: Duration, duration: Duration, easing: Easing) -> Self {
        Self {
            from,
            to,
            start,
            duration,
            easing,
        }
    }

    /// Linear progress `0..=1`.
    pub fn progress(&self, now: Duration) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_sub(self.start).as_secs_f64();
        (elapsed / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    pub fn value(&self, now: Duration) -> f64 {
        let p = self.easing.apply(self.progress(now));
        self.from + (self.to - self.from) * p
    }

    pub fn is_done(&self, now: Duration) -> bool {
        self.progress(now) >= 1.0
    }

    /// Interrupt and head somewhere else, starting from wherever we are now
    /// (no jump). The new leg gets the remaining fraction of `duration` scaled
    /// by distance, so a quick reversal is quick.
    pub fn retarget(&mut self, now: Duration, to: f64, duration: Duration) {
        let current = self.value(now);
        let span = (self.to - self.from).abs();
        let new_span = (to - current).abs();
        let scaled = if span > f64::EPSILON {
            duration.mul_f64((new_span / span).clamp(0.0, 1.0))
        } else {
            duration
        };
        *self = Tween::new(current, to, now, scaled, self.easing);
    }
}

/// What a window animation is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimKind {
    Open,
    Close,
    Minimize,
    Restore,
    /// Geometry change between two resting rects (tiling toggle, snap,
    /// maximise, fullscreen). With a frozen snapshot the window morphs
    /// (size + position); without one it glides at its final size.
    Move,
    /// Workspace switch: slides in from the side (full size, opaque).
    SlideIn,
    /// Workspace switch: slides out to the side, then is hidden.
    SlideOut,
}

/// The visual state of a window for one frame: where its outer frame is drawn
/// (possibly not its real geometry) and its opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Visual {
    pub rect: RectF,
    pub alpha: f32,
}

/// Float rectangle for in-between frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<Rect> for RectF {
    fn from(r: Rect) -> Self {
        Self {
            x: f64::from(r.x),
            y: f64::from(r.y),
            w: f64::from(r.w),
            h: f64::from(r.h),
        }
    }
}

impl RectF {
    pub fn lerp(a: RectF, b: RectF, t: f64) -> RectF {
        RectF {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
            w: a.w + (b.w - a.w) * t,
            h: a.h + (b.h - a.h) * t,
        }
    }

    /// Scale about the centre.
    pub fn shifted(self, dx: f64, dy: f64) -> RectF {
        RectF {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    pub fn scaled(self, s: f64) -> RectF {
        let w = self.w * s;
        let h = self.h * s;
        RectF {
            x: self.x + (self.w - w) / 2.0,
            y: self.y + (self.h - h) / 2.0,
            w,
            h,
        }
    }

    /// Map a point inside `from` to the same relative place inside `self`.
    pub fn map_from(&self, from: &RectF, x: f64, y: f64) -> (f64, f64) {
        let sx = if from.w > 0.0 { self.w / from.w } else { 1.0 };
        let sy = if from.h > 0.0 { self.h / from.h } else { 1.0 };
        (self.x + (x - from.x) * sx, self.y + (y - from.y) * sy)
    }
}

/// Starting scale of an opening window (and ending scale of a closing one).
pub const OPEN_SCALE: f64 = 0.94;
/// How far a window rises while opening (sinks while closing), logical px.
pub const OPEN_SLIDE: f64 = 10.0;

/// One running window animation. `progress` runs 0 -> 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowAnim {
    pub kind: AnimKind,
    pub progress: Tween,
    /// Rest rect at the start (Move) or the "far" end (minimise target, the
    /// taskbar button) for Minimize/Restore.
    pub from: RectF,
    /// The rect the window settles at (its real geometry), or where it was
    /// for Close/Minimize.
    pub to: RectF,
}

impl WindowAnim {
    /// Window appears: rises [`OPEN_SLIDE`] px into place and fades in. No
    /// scaling: llvmpipe samples 1:1 textures several times faster than
    /// scaled ones (open/close frames went from ~100-200 ms to drag cost).
    pub fn open(rect: Rect, now: Duration, motion: &MotionConfig) -> Self {
        let r = RectF::from(rect);
        Self {
            kind: AnimKind::Open,
            progress: Tween::new(
                0.0,
                1.0,
                now,
                motion.duration(durations::OPEN),
                Easing::Spring,
            ),
            from: {
                let f = r.shifted(0.0, motion.intensity.slide());
                if motion.intensity == Intensity::High {
                    f.scaled(0.97)
                } else {
                    f
                }
            },
            to: r,
        }
    }

    /// Window goes away (closed or destroyed): sinks [`OPEN_SLIDE`] px and
    /// fades out (no scaling, see [`WindowAnim::open`]).
    pub fn close(rect: Rect, now: Duration, motion: &MotionConfig) -> Self {
        let r = RectF::from(rect);
        Self {
            kind: AnimKind::Close,
            progress: Tween::new(
                0.0,
                1.0,
                now,
                motion.duration(durations::CLOSE),
                Easing::InCubic,
            ),
            from: r,
            to: r.shifted(0.0, motion.intensity.slide()),
        }
    }

    /// Window flies into `target` (its taskbar button, if the taskbar told us
    /// via `set_rectangle`) and fades.
    pub fn minimize(rect: Rect, target: Rect, now: Duration, motion: &MotionConfig) -> Self {
        Self {
            kind: AnimKind::Minimize,
            progress: Tween::new(
                0.0,
                1.0,
                now,
                motion.duration(durations::MINIMIZE),
                Easing::InOutCubic,
            ),
            from: rect.into(),
            to: target.into(),
        }
    }

    /// The reverse of [`WindowAnim::minimize`].
    pub fn restore(rect: Rect, source: Rect, now: Duration, motion: &MotionConfig) -> Self {
        Self {
            kind: AnimKind::Restore,
            progress: Tween::new(
                0.0,
                1.0,
                now,
                motion.duration(durations::RESTORE),
                Easing::OutCubic,
            ),
            from: source.into(),
            to: rect.into(),
        }
    }

    /// Glide between two resting rects.
    pub fn moving(from: RectF, to: Rect, now: Duration, motion: &MotionConfig) -> Self {
        Self {
            kind: AnimKind::Move,
            progress: Tween::new(
                0.0,
                1.0,
                now,
                motion.duration(durations::MOVE),
                Easing::Spring,
            ),
            from,
            to: to.into(),
        }
    }

    /// Workspace switch: from `from` (off to the side) into `to`.
    pub fn slide_in(from: RectF, to: Rect, now: Duration, motion: &MotionConfig) -> Self {
        Self {
            kind: AnimKind::SlideIn,
            progress: Tween::new(0.0, 1.0, now, motion.duration(durations::WORKSPACE), Easing::Spring),
            from,
            to: to.into(),
        }
    }

    /// Workspace switch: from where it rests out to `to` (off to the side).
    pub fn slide_out(from: Rect, to: RectF, now: Duration, motion: &MotionConfig) -> Self {
        Self {
            kind: AnimKind::SlideOut,
            progress: Tween::new(0.0, 1.0, now, motion.duration(durations::WORKSPACE), Easing::Spring),
            from: from.into(),
            to,
        }
    }

    pub fn is_done(&self, now: Duration) -> bool {
        self.progress.is_done(now)
    }

    /// The visual for this frame.
    pub fn sample(&self, now: Duration) -> Visual {
        let t = self.progress.value(now);
        let mut rect = RectF::lerp(self.from, self.to, t);
        // Open/close (unless High intensity's open scale is in play) and
        // workspace slides: translate only, at full size, on whole pixels, so
        // the live window texture stays 1:1 (fast and crisp on llvmpipe).
        // Moves keep the interpolated size: the renderer morphs a frozen
        // snapshot, or glides the live window at its final size.
        let scaled_open = self.kind == AnimKind::Open && (self.from.w - self.to.w).abs() > 0.5;
        if matches!(self.kind, AnimKind::Open | AnimKind::Close | AnimKind::SlideIn | AnimKind::SlideOut)
            && !scaled_open
        {
            let size = if matches!(self.kind, AnimKind::Close | AnimKind::SlideOut) {
                self.from
            } else {
                self.to
            };
            rect = RectF {
                x: rect.x.round(),
                y: rect.y.round(),
                w: size.w,
                h: size.h,
            };
        }
        let alpha = match self.kind {
            AnimKind::Open | AnimKind::Restore => t,
            AnimKind::Close => 1.0 - t,
            // Stay mostly visible while flying, fade in the last third.
            AnimKind::Minimize => (1.0 - ((t - 0.66) / 0.34).clamp(0.0, 1.0)).max(0.0),
            AnimKind::Move | AnimKind::SlideIn | AnimKind::SlideOut => 1.0,
        };
        Visual {
            rect,
            alpha: alpha.clamp(0.0, 1.0) as f32,
        }
    }

    /// Interrupt: a minimise that is reversed mid-flight turns into a restore
    /// from the current visual rect, with no jump.
    pub fn reversed_to_restore(&self, rest: Rect, now: Duration, motion: &MotionConfig) -> Self {
        let current = self.sample(now).rect;
        let mut anim = Self {
            kind: AnimKind::Restore,
            progress: Tween::new(
                0.0,
                1.0,
                now,
                motion.duration(durations::RESTORE),
                Easing::OutCubic,
            ),
            from: current,
            to: rest.into(),
        };
        // Shorten by how far we had travelled.
        let travelled = self.progress.progress(now);
        anim.progress.duration = anim.progress.duration.mul_f64(travelled.clamp(0.05, 1.0));
        anim
    }
}

/// Frame-time statistics for honest logging (`frame_ms min/avg/p95/max`).
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    samples: Vec<f64>,
}

impl FrameStats {
    pub fn push(&mut self, ms: f64) {
        self.samples.push(ms);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// `(min, avg, p95, max)` in ms, then clears.
    pub fn take_summary(&mut self) -> Option<(f64, f64, f64, f64)> {
        if self.samples.is_empty() {
            return None;
        }
        let mut s = std::mem::take(&mut self.samples);
        s.sort_by(|a, b| a.total_cmp(b));
        let n = s.len();
        let avg = s.iter().sum::<f64>() / n as f64;
        let p95 = s[((n as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(n - 1)];
        Some((s[0], avg, p95, s[n - 1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn easing_endpoints() {
        for e in [
            Easing::Linear,
            Easing::OutCubic,
            Easing::InCubic,
            Easing::InOutCubic,
        ] {
            assert_eq!(e.apply(0.0), 0.0);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9);
            assert!(e.apply(0.5) > 0.0 && e.apply(0.5) < 1.0);
        }
        assert!(Easing::OutCubic.apply(0.3) > 0.3, "out-cubic leads");
    }

    #[test]
    fn tween_is_time_based() {
        let t = Tween::new(0.0, 100.0, ms(1000), ms(200), Easing::Linear);
        assert_eq!(t.value(ms(900)), 0.0, "before start");
        assert_eq!(t.value(ms(1100)), 50.0);
        assert_eq!(t.value(ms(5000)), 100.0);
        assert!(t.is_done(ms(1200)));
        assert!(!t.is_done(ms(1199)));
    }

    #[test]
    fn retarget_has_no_jump_and_shortens() {
        let mut t = Tween::new(0.0, 100.0, ms(0), ms(200), Easing::Linear);
        let at = t.value(ms(100));
        t.retarget(ms(100), 0.0, ms(200));
        assert_eq!(t.value(ms(100)), at);
        assert_eq!(t.duration, ms(100), "half the distance, half the time");
        assert_eq!(t.value(ms(200)), 0.0);
    }

    #[test]
    fn spring_settles_without_overshoot_and_intensity_scales() {
        let mut last = 0.0;
        for i in 0..=100 {
            let v = Easing::Spring.apply(f64::from(i) / 100.0);
            assert!(v >= last - 1e-9 && v <= 1.0, "monotonic, no overshoot");
            last = v;
        }
        assert_eq!(Easing::Spring.apply(1.0), 1.0);
        let off = MotionConfig { intensity: Intensity::Off, ..MotionConfig::default() };
        assert_eq!(off.duration(200), Duration::ZERO);
        let high = MotionConfig { intensity: Intensity::High, ..MotionConfig::default() };
        assert!(high.duration(200) > MotionConfig::default().duration(200));
        assert_eq!(Intensity::parse("LOW"), Some(Intensity::Low));
        // A workspace slide-out ends a full width to the side, at full size.
        let r = Rect::new(100, 50, 400, 300);
        let a = WindowAnim::slide_out(r, RectF::from(r).shifted(-1280.0, 0.0), ms(0), &MotionConfig::default());
        let end = a.sample(ms(5000));
        assert_eq!((end.rect.x, end.rect.w), (-1180.0, 400.0));
    }

    #[test]
    fn reduced_motion_wins() {
        let m = MotionConfig {
            speed: 3.0,
            reduced: true,
            ..MotionConfig::default()
        };
        assert_eq!(m.duration(500), Duration::ZERO);
        let a = WindowAnim::open(Rect::new(0, 0, 100, 100), ms(10), &m);
        assert!(a.is_done(ms(10)));
        assert_eq!(a.sample(ms(10)).alpha, 1.0);
    }

    #[test]
    fn speed_scales_and_clamps() {
        let m = MotionConfig {
            speed: 2.0,
            reduced: false,
            ..MotionConfig::default()
        };
        assert_eq!(m.duration(100), ms(200));
        let silly = MotionConfig {
            speed: f64::NAN,
            reduced: false,
            ..MotionConfig::default()
        };
        assert_eq!(silly.duration(100), ms(100));
    }

    #[test]
    fn open_rises_and_fades_in_at_full_size() {
        let m = MotionConfig::default();
        let r = Rect::new(100, 100, 400, 300);
        let a = WindowAnim::open(r, ms(0), &m);
        let first = a.sample(ms(0));
        assert_eq!(first.alpha, 0.0);
        // 1:1 size (no scaling), starting OPEN_SLIDE px lower, whole pixels.
        assert_eq!((first.rect.w, first.rect.h), (400.0, 300.0));
        assert_eq!(first.rect.y, 100.0 + OPEN_SLIDE);
        assert_eq!(first.rect.x.fract(), 0.0);
        let mid = a.sample(ms(90));
        assert!(mid.alpha > 0.0 && mid.alpha < 1.0);
        let end = a.sample(ms(1000));
        assert_eq!(end.rect, RectF::from(r));
        assert_eq!(end.alpha, 1.0);
    }

    #[test]
    fn minimize_flies_to_target_and_reversal_is_continuous() {
        let m = MotionConfig::default();
        let r = Rect::new(100, 100, 400, 300);
        let target = Rect::new(200, 760, 128, 28);
        let a = WindowAnim::minimize(r, target, ms(0), &m);
        assert_eq!(a.sample(ms(1000)).rect, RectF::from(target));
        assert!(a.sample(ms(1000)).alpha < 1e-6);
        let mid = a.sample(ms(110));
        let back = a.reversed_to_restore(r, ms(110), &m);
        assert_eq!(back.sample(ms(110)).rect, mid.rect, "no jump on reversal");
        assert_eq!(back.sample(ms(10_000)).rect, RectF::from(r));
    }

    #[test]
    fn map_from_preserves_relative_position() {
        let a = RectF::from(Rect::new(0, 0, 100, 100));
        let b = RectF::from(Rect::new(50, 50, 50, 50));
        assert_eq!(b.map_from(&a, 50.0, 50.0), (75.0, 75.0));
    }

    #[test]
    fn frame_stats_summary() {
        let mut s = FrameStats::default();
        for v in 1..=100 {
            s.push(v as f64);
        }
        let (min, avg, p95, max) = s.take_summary().unwrap();
        assert_eq!((min, max), (1.0, 100.0));
        assert_eq!(avg, 50.5);
        assert_eq!(p95, 95.0);
        assert!(s.is_empty());
    }
}
