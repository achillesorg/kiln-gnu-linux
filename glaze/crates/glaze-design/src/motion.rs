//! Motion maths shared by the shell (and later the compositor).
//!
//! Smooth motion is a top priority (`docs/decisions.md`): every animated value
//! is a **critically damped spring** solved analytically, so it is
//! frame-rate independent (a dropped frame on llvmpipe just takes a bigger
//! exact step), never overshoots, and can be **retargeted** mid-flight while
//! keeping its velocity (interruptible: hover in/out, a button that is added
//! and then removed, a reorder during a reorder).
//!
//! Reduced motion (Glaze setting or the toolkit's) and `style = off` make every
//! transition instant.

/// The user's motion settings, resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSettings {
    /// Duration multiplier: `1 / speed`. 0 = instant.
    pub duration_scale: f64,
    /// Snappy style shortens every transition further.
    pub snappy: bool,
}

impl MotionSettings {
    /// From config values. `reduced` or `off` → instant.
    pub fn new(speed: f64, snappy: bool, off: bool, reduced: bool) -> Self {
        let duration_scale = if off || reduced {
            0.0
        } else {
            1.0 / speed.clamp(0.25, 4.0)
        };
        MotionSettings {
            duration_scale,
            snappy,
        }
    }

    pub fn instant(&self) -> bool {
        self.duration_scale <= 0.0
    }

    /// Settle time for a transition whose theme duration is `base_ms`.
    pub fn settle_ms(&self, base_ms: f64) -> f64 {
        let s = if self.snappy { 0.65 } else { 1.0 };
        base_ms * self.duration_scale * s
    }

    /// A spring for a transition of `base_ms`, starting at `value`.
    pub fn spring(&self, base_ms: f64, value: f64) -> Spring {
        Spring::new(value, self.settle_ms(base_ms))
    }
}

impl Default for MotionSettings {
    fn default() -> Self {
        MotionSettings::new(1.0, false, false, false)
    }
}

/// `ω·t` at which a critically damped spring released from rest is within
/// 0.1 % of its target: (1 + x)·e^(−x) = 0.001 → x ≈ 9.23.
const SETTLE_OMEGA_T: f64 = 9.23;

/// A critically damped spring towards `target`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    /// Natural frequency (rad/s); `INFINITY` = instant.
    omega: f64,
}

impl Spring {
    /// A spring at rest at `value` that settles in about `settle_ms`.
    pub fn new(value: f64, settle_ms: f64) -> Self {
        let omega = if settle_ms <= 0.0 {
            f64::INFINITY
        } else {
            SETTLE_OMEGA_T / (settle_ms / 1000.0)
        };
        Spring {
            value,
            velocity: 0.0,
            target: value,
            omega,
        }
    }

    /// Change the settle time without disturbing position/velocity.
    pub fn set_settle_ms(&mut self, settle_ms: f64) {
        self.omega = Spring::new(0.0, settle_ms).omega;
    }

    /// Aim at a new target; position and velocity are kept (interruptible).
    pub fn retarget(&mut self, target: f64) {
        self.target = target;
        if self.omega.is_infinite() {
            self.snap();
        }
    }

    /// Jump to the target and stop.
    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    /// Advance by `dt` seconds (exact solution; any `dt` is stable).
    pub fn step(&mut self, dt: f64) {
        if self.omega.is_infinite() || dt <= 0.0 && self.is_settled() {
            self.snap();
            return;
        }
        let w = self.omega;
        let x0 = self.value - self.target;
        let v0 = self.velocity;
        let e = (-w * dt).exp();
        let c = v0 + w * x0;
        let x = (x0 + c * dt) * e;
        let v = (v0 - w * c * dt) * e;
        self.value = self.target + x;
        self.velocity = v;
        if self.is_settled() {
            self.snap();
        }
    }

    /// Close enough to stop ticking (sub-pixel and nearly still).
    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < 0.01 && self.velocity.abs() < 0.5
    }
}

/// Frame-time statistics for one animation run (logged by the shell).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameStats {
    intervals_ms: Vec<f64>,
}

impl FrameStats {
    pub fn push(&mut self, interval_ms: f64) {
        if interval_ms.is_finite() && interval_ms > 0.0 {
            self.intervals_ms.push(interval_ms);
        }
    }

    pub fn frames(&self) -> usize {
        self.intervals_ms.len()
    }

    pub fn clear(&mut self) {
        self.intervals_ms.clear();
    }

    /// (mean, p95, max, count over `slow_ms`).
    pub fn summary(&self, slow_ms: f64) -> Option<(f64, f64, f64, usize)> {
        if self.intervals_ms.is_empty() {
            return None;
        }
        let mut v = self.intervals_ms.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).clamp(1, v.len()) - 1];
        let max = *v.last().unwrap_or(&0.0);
        let slow = v.iter().filter(|x| **x > slow_ms).count();
        Some((mean, p95, max, slow))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Spring, dt: f64, secs: f64) -> (f64, f64) {
        let mut max = f64::MIN;
        let mut min = f64::MAX;
        let steps = (secs / dt).round() as usize;
        for _ in 0..steps {
            s.step(dt);
            max = max.max(s.value);
            min = min.min(s.value);
        }
        (min, max)
    }

    #[test]
    fn settles_in_about_the_requested_time_without_overshoot() {
        let mut s = Spring::new(0.0, 200.0);
        s.retarget(100.0);
        let (_, max) = run(&mut s, 1.0 / 60.0, 0.21);
        assert!(max <= 100.0 + 1e-9, "overshoot {max}");
        assert!(
            (s.value - 100.0).abs() < 0.2,
            "not settled at 210 ms: {}",
            s.value
        );
        let mut early = Spring::new(0.0, 200.0);
        early.retarget(100.0);
        run(&mut early, 1.0 / 60.0, 0.05);
        assert!(early.value > 20.0 && early.value < 95.0, "{}", early.value);
    }

    #[test]
    fn frame_rate_independent() {
        let mut a = Spring::new(0.0, 250.0);
        let mut b = a;
        a.retarget(1.0);
        b.retarget(1.0);
        run(&mut a, 1.0 / 60.0, 0.1);
        run(&mut b, 1.0 / 20.0, 0.1);
        assert!(
            (a.value - b.value).abs() < 0.02,
            "{} vs {}",
            a.value,
            b.value
        );
    }

    #[test]
    fn retarget_keeps_velocity_and_is_continuous() {
        let mut s = Spring::new(0.0, 300.0);
        s.retarget(100.0);
        run(&mut s, 1.0 / 60.0, 0.06);
        let (pos, vel) = (s.value, s.velocity);
        assert!(vel > 0.0);
        s.retarget(0.0);
        assert_eq!(s.value, pos, "no jump on retarget");
        assert_eq!(s.velocity, vel, "velocity carried over");
        s.step(1.0 / 60.0);
        assert!(s.value > pos - 5.0, "turns smoothly");
        run(&mut s, 1.0 / 60.0, 0.6);
        assert!(s.is_settled() && s.value == 0.0);
    }

    #[test]
    fn huge_dt_is_stable() {
        let mut s = Spring::new(0.0, 100.0);
        s.retarget(50.0);
        s.step(5.0);
        assert_eq!(s.value, 50.0);
    }

    #[test]
    fn reduced_motion_and_off_are_instant() {
        for m in [
            MotionSettings::new(1.0, false, false, true),
            MotionSettings::new(1.0, false, true, false),
        ] {
            assert!(m.instant());
            let mut s = m.spring(200.0, 0.0);
            s.retarget(10.0);
            assert_eq!(s.value, 10.0);
        }
        let fast = MotionSettings::new(2.0, false, false, false);
        assert!((fast.settle_ms(200.0) - 100.0).abs() < 1e-9);
        let snappy = MotionSettings::new(1.0, true, false, false);
        assert!(snappy.settle_ms(200.0) < 200.0);
    }

    #[test]
    fn frame_stats_summary() {
        let mut f = FrameStats::default();
        assert!(f.summary(20.0).is_none());
        for v in [16.0, 17.0, 16.5, 40.0] {
            f.push(v);
        }
        let (mean, p95, max, slow) = f.summary(20.0).unwrap();
        assert!((mean - 22.375).abs() < 1e-9);
        assert_eq!(p95, 40.0);
        assert_eq!(max, 40.0);
        assert_eq!(slow, 1);
    }
}
