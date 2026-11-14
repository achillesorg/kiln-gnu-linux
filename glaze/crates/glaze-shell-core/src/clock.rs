//! Deterministic clock/date formatting driven by [`PanelConfig`].
//!
//! The formatting is pure: the caller supplies a [`ClockParts`] (from the real
//! system clock in `apps/glaze-shell`), so the result is fully reproducible in
//! tests and there is no hidden dependency on the host clock (`GLAZE.md` §6.1).
//!
//! Honoured settings:
//! - `clock_24h`     — 24-hour (`14:05`) vs 12-hour (`2:05 PM`) time.
//! - `show_seconds`  — append `:SS`.
//! - `show_date`     — emit a date line at all.
//! - `date_format`   — optional template using strftime-like tokens; `None`
//!   falls back to [`DEFAULT_DATE_FORMAT`].
//!
//! ## Date tokens
//!
//! | Token | Meaning                | Example |
//! |-------|------------------------|---------|
//! | `%Y`  | year, 4 digits         | `2026`  |
//! | `%y`  | year, last 2 digits    | `26`    |
//! | `%m`  | month, 2 digits        | `11`    |
//! | `%d`  | day of month, 2 digits | `10`    |
//! | `%H`  | hour, 00–23            | `14`    |
//! | `%M`  | minute, 00–59          | `05`    |
//! | `%S`  | second, 00–59          | `09`    |
//! | `%a`  | weekday, short         | `Tue`   |
//! | `%A`  | weekday, full          | `Tuesday` |
//! | `%b`  | month, short           | `Nov`   |
//! | `%%`  | a literal `%`          | `%`     |
//!
//! An unrecognised `%x` sequence is emitted verbatim (`%x`), matching the
//! forgiving behaviour of `strftime` rather than dropping data.

use glaze_common::config::PanelConfig;

/// Default date template when the user has not set `panel.date_format`.
pub const DEFAULT_DATE_FORMAT: &str = "%a %Y-%m-%d";

const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Day of week, Monday-first. Matches `GDateTime`'s 1..=7 day numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// All weekdays in order, Monday first.
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    /// Build from a 1-based ISO / `GDateTime` day number (1 = Monday).
    pub fn from_iso(n: i32) -> Option<Weekday> {
        match n {
            1 => Some(Weekday::Monday),
            2 => Some(Weekday::Tuesday),
            3 => Some(Weekday::Wednesday),
            4 => Some(Weekday::Thursday),
            5 => Some(Weekday::Friday),
            6 => Some(Weekday::Saturday),
            7 => Some(Weekday::Sunday),
            _ => None,
        }
    }

    /// Three-letter English label, e.g. `Tue`.
    pub const fn short(self) -> &'static str {
        match self {
            Weekday::Monday => "Mon",
            Weekday::Tuesday => "Tue",
            Weekday::Wednesday => "Wed",
            Weekday::Thursday => "Thu",
            Weekday::Friday => "Fri",
            Weekday::Saturday => "Sat",
            Weekday::Sunday => "Sun",
        }
    }

    /// Full English label, e.g. `Tuesday`.
    pub const fn long(self) -> &'static str {
        match self {
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
            Weekday::Sunday => "Sunday",
        }
    }

    /// Compute the weekday for a Gregorian date (Sakamoto's algorithm).
    ///
    /// Out-of-range month/day values are clamped to a valid date first, so the
    /// function never panics on hand-built [`ClockParts`].
    pub fn from_ymd(year: i32, month: i32, day: i32) -> Weekday {
        let month = month.clamp(1, 12);
        let day = day.clamp(1, 31);
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if month < 3 { year - 1 } else { year };
        let idx = (month - 1) as usize;
        let w = (y + y / 4 - y / 100 + y / 400 + T[idx] + day).rem_euclid(7);
        // Sakamoto: 0 = Sunday .. 6 = Saturday.
        match w {
            0 => Weekday::Sunday,
            1 => Weekday::Monday,
            2 => Weekday::Tuesday,
            3 => Weekday::Wednesday,
            4 => Weekday::Thursday,
            5 => Weekday::Friday,
            _ => Weekday::Saturday,
        }
    }
}

/// Calendar fields for a single moment, as produced by the caller's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockParts {
    pub year: i32,
    /// 1–12.
    pub month: i32,
    /// 1–31.
    pub day: i32,
    /// 0–23.
    pub hour: i32,
    /// 0–59.
    pub minute: i32,
    /// 0–59.
    pub second: i32,
    pub weekday: Weekday,
}

impl ClockParts {
    /// Build from a date/time, deriving the weekday from the date.
    pub fn new(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> Self {
        ClockParts {
            year,
            month,
            day,
            hour,
            minute,
            second,
            weekday: Weekday::from_ymd(year, month, day),
        }
    }

    /// Override the weekday (e.g. from a trusted system clock).
    pub fn with_weekday(mut self, weekday: Weekday) -> Self {
        self.weekday = weekday;
        self
    }
}

/// The formatted clock text for the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockText {
    /// Always present, e.g. `14:05` or `2:05 PM`.
    pub time: String,
    /// Present only when `panel.show_date` is set.
    pub date: Option<String>,
}

/// Format a moment according to the panel's clock settings.
pub fn format_clock(parts: &ClockParts, cfg: &PanelConfig) -> ClockText {
    let time = format_time(parts, cfg.clock_24h, cfg.show_seconds);
    let date = if cfg.show_date {
        let template = cfg
            .date_format
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_DATE_FORMAT);
        Some(format_date(parts, template))
    } else {
        None
    };
    ClockText { time, date }
}

/// Format just the time field.
pub fn format_time(parts: &ClockParts, clock_24h: bool, show_seconds: bool) -> String {
    let minute = parts.minute.rem_euclid(60);
    let second = parts.second.rem_euclid(60);
    if clock_24h {
        let mut s = format!("{:02}:{:02}", parts.hour.rem_euclid(24), minute);
        if show_seconds {
            s.push_str(&format!(":{second:02}"));
        }
        s
    } else {
        let hour24 = parts.hour.rem_euclid(24);
        let hour12 = match hour24 % 12 {
            0 => 12,
            h => h,
        };
        let suffix = if hour24 < 12 { "AM" } else { "PM" };
        let mut s = format!("{hour12}:{minute:02}");
        if show_seconds {
            s.push_str(&format!(":{second:02}"));
        }
        s.push(' ');
        s.push_str(suffix);
        s
    }
}

/// Expand a `date_format` template against `parts`.
pub fn format_date(parts: &ClockParts, template: &str) -> String {
    let mut out = String::with_capacity(template.len() + 8);
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            None => out.push('%'),
            Some('Y') => out.push_str(&format!("{:04}", parts.year)),
            Some('y') => out.push_str(&format!("{:02}", parts.year.rem_euclid(100))),
            Some('m') => out.push_str(&format!("{:02}", parts.month.clamp(1, 12))),
            Some('d') => out.push_str(&format!("{:02}", parts.day.clamp(1, 31))),
            Some('H') => out.push_str(&format!("{:02}", parts.hour.rem_euclid(24))),
            Some('M') => out.push_str(&format!("{:02}", parts.minute.rem_euclid(60))),
            Some('S') => out.push_str(&format!("{:02}", parts.second.rem_euclid(60))),
            Some('a') => out.push_str(parts.weekday.short()),
            Some('A') => out.push_str(parts.weekday.long()),
            Some('b') => {
                let idx = (parts.month.clamp(1, 12) - 1) as usize;
                out.push_str(MONTH_SHORT[idx]);
            }
            Some('%') => out.push('%'),
            Some(other) => {
                // Unknown token: keep it verbatim, like strftime.
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PanelConfig {
        PanelConfig::default()
    }

    fn sample() -> ClockParts {
        // 2026-11-10 14:05:09 is a Tuesday.
        ClockParts::new(2026, 11, 10, 14, 5, 9)
    }

    #[test]
    fn weekday_is_computed_from_the_date() {
        assert_eq!(sample().weekday, Weekday::Tuesday);
        assert_eq!(Weekday::from_ymd(2000, 1, 1), Weekday::Saturday);
        assert_eq!(Weekday::from_ymd(2024, 2, 29), Weekday::Thursday);
        assert_eq!(Weekday::from_ymd(1970, 1, 1), Weekday::Thursday);
    }

    #[test]
    fn weekday_iso_round_trips() {
        for (i, w) in Weekday::ALL.iter().enumerate() {
            assert_eq!(Weekday::from_iso(i as i32 + 1), Some(*w));
        }
        assert_eq!(Weekday::from_iso(0), None);
        assert_eq!(Weekday::from_iso(8), None);
    }

    #[test]
    fn weekday_labels_are_stable() {
        assert_eq!(Weekday::Tuesday.short(), "Tue");
        assert_eq!(Weekday::Tuesday.long(), "Tuesday");
        for w in Weekday::ALL {
            assert_eq!(w.short().len(), 3);
            assert!(!w.long().is_empty());
        }
    }

    #[test]
    fn default_config_is_24h_without_seconds_and_with_date() {
        let c = cfg();
        let t = format_clock(&sample(), &c);
        assert_eq!(t.time, "14:05");
        assert_eq!(t.date.as_deref(), Some("Tue 2026-11-10"));
    }

    #[test]
    fn show_seconds_appends_seconds() {
        let mut c = cfg();
        c.show_seconds = true;
        assert_eq!(format_clock(&sample(), &c).time, "14:05:09");
    }

    #[test]
    fn twelve_hour_format_converts_afternoon_and_midnight() {
        let mut c = cfg();
        c.clock_24h = false;
        assert_eq!(format_clock(&sample(), &c).time, "2:05 PM");

        let midnight = ClockParts::new(2026, 1, 1, 0, 0, 0);
        assert_eq!(format_clock(&midnight, &c).time, "12:00 AM");

        let noon = ClockParts::new(2026, 1, 1, 12, 30, 0);
        assert_eq!(format_clock(&noon, &c).time, "12:30 PM");
    }

    #[test]
    fn show_date_false_hides_the_date_line() {
        let mut c = cfg();
        c.show_date = false;
        assert!(format_clock(&sample(), &c).date.is_none());
    }

    #[test]
    fn custom_date_format_tokens_are_expanded() {
        let mut c = cfg();
        c.date_format = Some("%Y/%m/%d %H:%M:%S %a %A %b %%".into());
        let t = format_clock(&sample(), &c);
        assert_eq!(
            t.date.as_deref(),
            Some("2026/11/10 14:05:09 Tue Tuesday Nov %")
        );
    }

    #[test]
    fn blank_date_format_falls_back_to_default() {
        let mut c = cfg();
        c.date_format = Some("   ".into());
        assert_eq!(
            format_clock(&sample(), &c).date.as_deref(),
            Some("Tue 2026-11-10")
        );
    }

    #[test]
    fn short_year_token_uses_last_two_digits() {
        assert_eq!(format_date(&sample(), "%y"), "26");
        assert_eq!(format_date(&sample(), "%Y"), "2026");
    }

    #[test]
    fn unknown_and_trailing_tokens_are_preserved() {
        assert_eq!(format_date(&sample(), "%x"), "%x");
        assert_eq!(format_date(&sample(), "end %"), "end %");
        assert_eq!(format_date(&sample(), "plain"), "plain");
    }

    #[test]
    fn out_of_range_fields_are_clamped_not_panicking() {
        let bad = ClockParts::new(2026, 13, 40, 26, 61, 61);
        // 26h -> 02, 61m -> 01 (rem_euclid 60), month clamps to Dec.
        let t = format_date(&bad, "%m-%d");
        assert_eq!(t, "12-31");
        assert_eq!(format_time(&bad, true, true), "02:01:01");
    }
}
