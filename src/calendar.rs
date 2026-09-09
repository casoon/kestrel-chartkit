//! IANA-timezone, DST-aware exchange calendars.
//!
//! [`crate::session::SessionConfig`] models a recurring session at a fixed UTC offset — correct
//! for markets that never observe daylight saving time, but wrong twice a year for any exchange
//! whose local trading hours are defined in a DST-observing timezone (its UTC offset shifts).
//! [`ExchangeCalendar`] complements it: an IANA timezone (via `chrono-tz`, which embeds the
//! timezone database rather than depending on the host OS's), a holiday set, per-date early
//! closes, and one or more session segments per trading day, all evaluated in the exchange's true
//! local wall-clock time.
//!
//! Requires the `calendar` feature (off by default): the core crate has no timezone-database
//! dependency unless a consumer opts in.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDate, NaiveTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// A single trading session segment within a trading day, in the exchange's local time.
/// `end` is exclusive, matching [`ExchangeCalendar::is_in_session`]'s half-open interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSegment {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl SessionSegment {
    pub fn new(start: NaiveTime, end: NaiveTime) -> Self {
        Self { start, end }
    }
}

/// A provider-neutral exchange calendar: an IANA timezone (DST-aware), a fixed holiday set,
/// per-date early-close overrides, and one or more regular session segments per trading day.
#[derive(Debug, Clone)]
pub struct ExchangeCalendar {
    pub timezone: Tz,
    pub sessions: Vec<SessionSegment>,
    holidays: HashSet<NaiveDate>,
    /// Date -> local close time overriding the first session segment's regular end for that date.
    early_closes: HashMap<NaiveDate, NaiveTime>,
}

impl ExchangeCalendar {
    pub fn new(timezone: Tz, sessions: Vec<SessionSegment>) -> Self {
        Self {
            timezone,
            sessions,
            holidays: HashSet::new(),
            early_closes: HashMap::new(),
        }
    }

    pub fn with_holidays(mut self, holidays: impl IntoIterator<Item = NaiveDate>) -> Self {
        self.holidays.extend(holidays);
        self
    }

    pub fn with_early_close(mut self, date: NaiveDate, close: NaiveTime) -> Self {
        self.early_closes.insert(date, close);
        self
    }

    /// Converts a Unix timestamp (seconds) to this exchange's local wall-clock date/time,
    /// correctly applying DST transitions via the IANA database.
    pub fn local_datetime(&self, unix_ts: i64) -> DateTime<Tz> {
        let utc = Utc
            .timestamp_opt(unix_ts, 0)
            .single()
            .expect("unix_ts must be a valid, unambiguous UTC instant");
        utc.with_timezone(&self.timezone)
    }

    /// The exchange's true UTC offset at `unix_ts`, in seconds — varies across DST transitions,
    /// unlike [`crate::session::SessionConfig::utc_offset_seconds`]'s fixed value.
    pub fn utc_offset_seconds(&self, unix_ts: i64) -> i32 {
        self.local_datetime(unix_ts)
            .offset()
            .fix()
            .local_minus_utc()
    }

    pub fn is_holiday(&self, unix_ts: i64) -> bool {
        self.holidays
            .contains(&self.local_datetime(unix_ts).date_naive())
    }

    /// Whether `unix_ts` falls within a regular (or early-closed) trading session segment, in
    /// exchange local time. Always `false` on a configured holiday. On a configured early-close
    /// date, only the first session segment applies, truncated at the override close time.
    pub fn is_in_session(&self, unix_ts: i64) -> bool {
        if self.is_holiday(unix_ts) {
            return false;
        }

        let local = self.local_datetime(unix_ts);
        let date = local.date_naive();
        let time = local.time();

        if let Some(&early_close) = self.early_closes.get(&date) {
            return self
                .sessions
                .first()
                .is_some_and(|s| time >= s.start && time < early_close);
        }

        self.sessions
            .iter()
            .any(|s| time >= s.start && time < s.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn new_york_regular_session() -> ExchangeCalendar {
        ExchangeCalendar::new(
            chrono_tz::America::New_York,
            vec![SessionSegment::new(
                NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
                NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            )],
        )
    }

    fn unix_ts(y: i32, m: u32, d: u32, h: u32, min: u32, tz: Tz) -> i64 {
        tz.with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .timestamp()
    }

    #[test]
    fn test_dst_transition_shifts_utc_offset() {
        let calendar = new_york_regular_session();
        // 2024-01-15: EST (UTC-5). 2024-07-15: EDT (UTC-4). Same local wall-clock time, 9:30.
        let winter = unix_ts(2024, 1, 15, 9, 30, chrono_tz::America::New_York);
        let summer = unix_ts(2024, 7, 15, 9, 30, chrono_tz::America::New_York);

        assert_eq!(calendar.utc_offset_seconds(winter), -5 * 3600);
        assert_eq!(calendar.utc_offset_seconds(summer), -4 * 3600);
        // Both are still 9:30 local -> both in session, despite the differing UTC offset.
        assert!(calendar.is_in_session(winter));
        assert!(calendar.is_in_session(summer));
    }

    #[test]
    fn test_is_in_session_respects_local_open_close() {
        let calendar = new_york_regular_session();
        let before_open = unix_ts(2024, 6, 10, 9, 0, chrono_tz::America::New_York);
        let during = unix_ts(2024, 6, 10, 12, 0, chrono_tz::America::New_York);
        let at_close = unix_ts(2024, 6, 10, 16, 0, chrono_tz::America::New_York);

        assert!(!calendar.is_in_session(before_open));
        assert!(calendar.is_in_session(during));
        assert!(!calendar.is_in_session(at_close), "end is exclusive");
    }

    #[test]
    fn test_holiday_overrides_regular_session() {
        let holiday = NaiveDate::from_ymd_opt(2024, 7, 4).unwrap();
        let calendar = new_york_regular_session().with_holidays([holiday]);

        let during_holiday = unix_ts(2024, 7, 4, 12, 0, chrono_tz::America::New_York);
        assert!(calendar.is_holiday(during_holiday));
        assert!(!calendar.is_in_session(during_holiday));
    }

    #[test]
    fn test_early_close_truncates_session() {
        let date = NaiveDate::from_ymd_opt(2024, 11, 29).unwrap();
        let early_close_time = NaiveTime::from_hms_opt(13, 0, 0).unwrap();
        let calendar = new_york_regular_session().with_early_close(date, early_close_time);

        let after_regular_open = unix_ts(2024, 11, 29, 12, 0, chrono_tz::America::New_York);
        let after_early_close = unix_ts(2024, 11, 29, 14, 0, chrono_tz::America::New_York);

        assert!(calendar.is_in_session(after_regular_open));
        assert!(!calendar.is_in_session(after_early_close));
    }
}
