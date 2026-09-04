use crate::model::Bar;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

const DAY_SECONDS: i64 = 86_400;

/// Provider-neutral recurring trading session configuration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SessionConfig {
    pub start_hour: u8,
    pub start_minute: u8,
    pub end_hour: u8,
    pub end_minute: u8,
    pub orb_duration_mins: u32,
    /// Fixed local-time offset from UTC. Named timezones and DST belong in a calendar adapter.
    pub utc_offset_seconds: i32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            start_hour: 0,
            start_minute: 0,
            end_hour: 0,
            end_minute: 0,
            orb_duration_mins: 30,
            utc_offset_seconds: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionConfigError {
    InvalidStart,
    InvalidEnd,
    InvalidUtcOffset,
}

impl fmt::Display for SessionConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidStart => "session start must be a valid hour and minute",
            Self::InvalidEnd => "session end must be a valid hour and minute",
            Self::InvalidUtcOffset => "UTC offset must be less than 24 hours",
        })
    }
}

impl std::error::Error for SessionConfigError {}

impl SessionConfig {
    pub fn validate(&self) -> Result<(), SessionConfigError> {
        if self.start_hour > 23 || self.start_minute > 59 {
            return Err(SessionConfigError::InvalidStart);
        }
        if self.end_hour > 23 || self.end_minute > 59 {
            return Err(SessionConfigError::InvalidEnd);
        }
        if self.utc_offset_seconds.unsigned_abs() >= DAY_SECONDS as u32 {
            return Err(SessionConfigError::InvalidUtcOffset);
        }
        Ok(())
    }

    fn start_seconds(&self) -> i64 {
        i64::from(self.start_hour) * 3_600 + i64::from(self.start_minute) * 60
    }

    fn end_seconds(&self) -> i64 {
        i64::from(self.end_hour) * 3_600 + i64::from(self.end_minute) * 60
    }
}

/// Session membership and opening-range state for a stream of bars.
#[derive(Debug, Clone)]
pub struct SessionTracker {
    config: SessionConfig,
    orb_high: Option<f64>,
    orb_low: Option<f64>,
    session_open_ts: Option<i64>,
    in_session: bool,
    in_orb_window: bool,
    is_new_session_bar: bool,
}

impl SessionTracker {
    pub fn new(config: SessionConfig) -> Result<Self, SessionConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            orb_high: None,
            orb_low: None,
            session_open_ts: None,
            in_session: false,
            in_orb_window: false,
            is_new_session_bar: false,
        })
    }

    pub fn reset(&mut self) {
        self.orb_high = None;
        self.orb_low = None;
        self.session_open_ts = None;
        self.in_session = false;
        self.in_orb_window = false;
        self.is_new_session_bar = false;
    }

    /// Processes a bar using its opening timestamp.
    pub fn on_bar(&mut self, bar: &Bar) {
        let local_timestamp = bar.timestamp + i64::from(self.config.utc_offset_seconds);
        let day = local_timestamp.div_euclid(DAY_SECONDS);
        let second_of_day = local_timestamp.rem_euclid(DAY_SECONDS);
        let start = self.config.start_seconds();
        let end = self.config.end_seconds();

        let session_start_day = if start == end {
            Some(day)
        } else if start < end {
            (second_of_day >= start && second_of_day < end).then_some(day)
        } else if second_of_day >= start {
            Some(day)
        } else if second_of_day < end {
            Some(day - 1)
        } else {
            None
        };

        let Some(session_start_day) = session_start_day else {
            self.in_session = false;
            self.in_orb_window = false;
            self.is_new_session_bar = false;
            return;
        };

        let session_open_local = session_start_day * DAY_SECONDS + start;
        let session_open_utc = session_open_local - i64::from(self.config.utc_offset_seconds);
        self.in_session = true;
        self.is_new_session_bar = self.session_open_ts != Some(session_open_utc);

        if self.is_new_session_bar {
            self.session_open_ts = Some(session_open_utc);
            self.orb_high = Some(bar.high);
            self.orb_low = Some(bar.low);
        }

        let elapsed = bar.timestamp - session_open_utc;
        self.in_orb_window =
            elapsed >= 0 && elapsed < i64::from(self.config.orb_duration_mins).saturating_mul(60);
        if self.in_orb_window && !self.is_new_session_bar {
            self.orb_high = Some(self.orb_high.unwrap_or(bar.high).max(bar.high));
            self.orb_low = Some(self.orb_low.unwrap_or(bar.low).min(bar.low));
        }
    }

    pub fn is_new_session(&self) -> bool {
        self.is_new_session_bar
    }

    pub fn in_session(&self) -> bool {
        self.in_session
    }

    pub fn session_open_timestamp(&self) -> Option<i64> {
        self.session_open_ts
    }

    pub fn orb_high(&self) -> Option<f64> {
        self.orb_high
    }

    pub fn orb_low(&self) -> Option<f64> {
        self.orb_low
    }

    pub fn in_orb_window(&self) -> bool {
        self.in_orb_window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(timestamp: i64, high: f64, low: f64) -> Bar {
        Bar::new(timestamp, 100.0, high, low, 100.0, 1_000.0)
    }

    #[test]
    fn observes_end_and_orb_boundaries() {
        let mut tracker = SessionTracker::new(SessionConfig {
            start_hour: 14,
            start_minute: 30,
            end_hour: 21,
            end_minute: 0,
            orb_duration_mins: 30,
            utc_offset_seconds: 0,
        })
        .unwrap();

        tracker.on_bar(&bar(52_200, 105.0, 99.0));
        assert!(tracker.is_new_session());
        tracker.on_bar(&bar(53_100, 108.0, 98.0));
        assert_eq!(tracker.orb_high(), Some(108.0));
        tracker.on_bar(&bar(75_600, 110.0, 90.0));
        assert!(!tracker.in_session());
        assert!(!tracker.in_orb_window());
    }

    #[test]
    fn supports_overnight_sessions_and_fixed_offsets() {
        let mut tracker = SessionTracker::new(SessionConfig {
            start_hour: 22,
            start_minute: 0,
            end_hour: 2,
            end_minute: 0,
            orb_duration_mins: 60,
            utc_offset_seconds: 3_600,
        })
        .unwrap();

        tracker.on_bar(&bar(21 * 3_600, 105.0, 99.0)); // 22:00 local
        assert!(tracker.is_new_session());
        tracker.on_bar(&bar(24 * 3_600, 106.0, 98.0)); // 01:00 local next day
        assert!(tracker.in_session());
        assert!(!tracker.is_new_session());
        tracker.on_bar(&bar(25 * 3_600, 106.0, 98.0)); // 02:00 local, end-exclusive
        assert!(!tracker.in_session());
    }
}
