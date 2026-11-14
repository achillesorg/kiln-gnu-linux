//! Form-factor selection: desktop (taskbar) vs mobile (touch) shell layout.
//!
//! Pure logic, no GTK. The shell measures the primary output and its input
//! devices, builds a [`DisplayInfo`] and asks [`FormFactor::resolve`] which
//! layout to run. The user can always force a layout through
//! `shell.form_factor` in the config or the `GLAZE_FORM_FACTOR` environment
//! variable (the shell app handles the latter, see [`pref_from_env`]).

use glaze_common::config::FormFactorPref;

/// Which shell layout is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormFactor {
    /// Taskbar, start button, tray; pointer + keyboard first.
    Desktop,
    /// Bottom navigation bar, app-grid launcher, 48px+ touch targets.
    Mobile,
}

impl FormFactor {
    pub const fn slug(self) -> &'static str {
        match self {
            FormFactor::Desktop => "desktop",
            FormFactor::Mobile => "mobile",
        }
    }

    pub const fn is_mobile(self) -> bool {
        matches!(self, FormFactor::Mobile)
    }

    /// Decide the layout for `pref` on `display`.
    ///
    /// `Auto` rules (logical px = physical px / scale), first match wins:
    /// 1. shortest logical side <= [`PHONE_SHORT_SIDE_PX`] -> Mobile;
    /// 2. portrait and logical width <= [`PORTRAIT_MOBILE_MAX_WIDTH_PX`] -> Mobile;
    /// 3. touch present, no pointer, logical width <= [`TOUCH_ONLY_MAX_WIDTH_PX`]
    ///    (tablet / convertible without a mouse) -> Mobile;
    /// 4. otherwise Desktop.
    pub fn resolve(pref: FormFactorPref, display: &DisplayInfo) -> FormFactor {
        match pref {
            FormFactorPref::Desktop => FormFactor::Desktop,
            FormFactorPref::Mobile => FormFactor::Mobile,
            FormFactorPref::Auto => {
                let (w, h) = display.logical_size();
                if w.min(h) <= PHONE_SHORT_SIDE_PX
                    || (h > w && w <= PORTRAIT_MOBILE_MAX_WIDTH_PX)
                    || (display.has_touch && !display.has_pointer && w <= TOUCH_ONLY_MAX_WIDTH_PX)
                {
                    FormFactor::Mobile
                } else {
                    FormFactor::Desktop
                }
            }
        }
    }
}

/// Shortest logical side at or below which an output is treated as a phone.
pub const PHONE_SHORT_SIDE_PX: i32 = 480;
/// A portrait output up to this logical width is a phone/small tablet.
pub const PORTRAIT_MOBILE_MAX_WIDTH_PX: i32 = 820;
/// Touch-only outputs up to this logical width use the mobile layout.
pub const TOUCH_ONLY_MAX_WIDTH_PX: i32 = 1280;

/// What the shell knows about the primary output and input devices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayInfo {
    pub width_px: i32,
    pub height_px: i32,
    /// Output scale factor (`1.0` = none).
    pub scale: f64,
    pub has_touch: bool,
    pub has_pointer: bool,
}

impl DisplayInfo {
    /// Size in logical px, with the scale clamped to a sane minimum.
    pub fn logical_size(&self) -> (i32, i32) {
        let s = self.scale.max(0.5);
        (
            (self.width_px as f64 / s).round() as i32,
            (self.height_px as f64 / s).round() as i32,
        )
    }
}

/// Parse the value of `GLAZE_FORM_FACTOR` (`auto`/`desktop`/`mobile`).
pub fn pref_from_env(value: Option<&str>) -> Option<FormFactorPref> {
    value.and_then(FormFactorPref::parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(w: i32, h: i32, scale: f64, touch: bool, pointer: bool) -> DisplayInfo {
        DisplayInfo {
            width_px: w,
            height_px: h,
            scale,
            has_touch: touch,
            has_pointer: pointer,
        }
    }

    #[test]
    fn forced_preferences_ignore_the_display() {
        let phone = d(1080, 2340, 3.0, true, false);
        let monitor = d(2560, 1440, 1.0, false, true);
        assert_eq!(
            FormFactor::resolve(FormFactorPref::Desktop, &phone),
            FormFactor::Desktop
        );
        assert_eq!(
            FormFactor::resolve(FormFactorPref::Mobile, &monitor),
            FormFactor::Mobile
        );
    }

    #[test]
    fn auto_detects_phone_tablet_and_desktop() {
        let auto = FormFactorPref::Auto;
        // 1080x2340 @3x = 360x780 logical: phone.
        assert!(FormFactor::resolve(auto, &d(1080, 2340, 3.0, true, false)).is_mobile());
        // 1080x2340 at scale 1 is a tall monitor-sized panel but still portrait <= no:
        // 1080 > 820 so it is not portrait-mobile; it is Desktop without touch-only.
        assert_eq!(
            FormFactor::resolve(auto, &d(1080, 2340, 1.0, false, true)),
            FormFactor::Desktop
        );
        // Touch-only 10" tablet 1280x800.
        assert!(FormFactor::resolve(auto, &d(1280, 800, 1.0, true, false)).is_mobile());
        // Same tablet with a mouse attached: desktop.
        assert_eq!(
            FormFactor::resolve(auto, &d(1280, 800, 1.0, true, true)),
            FormFactor::Desktop
        );
        // Plain desktop monitors.
        assert_eq!(
            FormFactor::resolve(auto, &d(1920, 1080, 1.0, false, true)),
            FormFactor::Desktop
        );
        assert_eq!(
            FormFactor::resolve(auto, &d(3840, 2160, 2.0, false, true)),
            FormFactor::Desktop
        );
        // Touch-only large touchscreen kiosk stays Desktop above the cap.
        assert_eq!(
            FormFactor::resolve(auto, &d(1920, 1080, 1.0, true, false)),
            FormFactor::Desktop
        );
    }

    #[test]
    fn portrait_small_width_is_mobile() {
        assert!(
            FormFactor::resolve(FormFactorPref::Auto, &d(720, 1560, 1.0, false, true)).is_mobile()
        );
    }

    #[test]
    fn env_parsing() {
        assert_eq!(pref_from_env(Some("mobile")), Some(FormFactorPref::Mobile));
        assert_eq!(pref_from_env(Some("bogus")), None);
        assert_eq!(pref_from_env(None), None);
    }
}
