use crate::model::Bar;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

const SECONDS_PER_DAY: i64 = 86_400;

/// Provider-neutral chart timeframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Timeframe {
    Second(u32),
    Minute(u32),
    Hour(u32),
    Day(u32),
    Week(u32),
    Month(u32),
}

/// Invalid timeframe configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeframeError;

impl fmt::Display for TimeframeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("timeframe multiplier must be greater than zero")
    }
}

impl std::error::Error for TimeframeError {}

impl Timeframe {
    pub fn validate(self) -> Result<Self, TimeframeError> {
        let value = match self {
            Self::Second(v)
            | Self::Minute(v)
            | Self::Hour(v)
            | Self::Day(v)
            | Self::Week(v)
            | Self::Month(v) => v,
        };
        (value > 0).then_some(self).ok_or(TimeframeError)
    }

    /// The bucket's close timestamp (exclusive upper bound), given the bucket's open timestamp
    /// (as returned in `Bar::timestamp` for a completed bar from [`BarResampler`]/
    /// [`ConfirmedResampler`]). Calendar months have no fixed width — use `bucket_start` on the
    /// next bar to find the following bucket's boundary instead.
    pub fn bucket_close(self, bucket_open_ts: i64) -> Option<i64> {
        self.fixed_seconds()
            .map(|secs| bucket_open_ts + secs as i64)
    }

    /// Returns a fixed duration. Calendar months intentionally return `None`.
    pub fn fixed_seconds(self) -> Option<u64> {
        match self {
            Self::Second(v) => Some(v as u64),
            Self::Minute(v) => Some(v as u64 * 60),
            Self::Hour(v) => Some(v as u64 * 3_600),
            Self::Day(v) => Some(v as u64 * 86_400),
            Self::Week(v) => Some(v as u64 * 604_800),
            Self::Month(_) => None,
        }
    }

    /// Parses strings such as `30s`, `15m`, `4h`, `1d`, `1w`, and `1M`.
    pub fn parse_str(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        let unit = trimmed.chars().last()?;
        let number = &trimmed[..trimmed.len().checked_sub(unit.len_utf8())?];
        let multiplier = if number.is_empty() {
            1
        } else {
            number.parse().ok()?
        };
        let timeframe = match unit {
            's' | 'S' => Self::Second(multiplier),
            'm' => Self::Minute(multiplier),
            'h' | 'H' => Self::Hour(multiplier),
            'd' | 'D' => Self::Day(multiplier),
            'w' | 'W' => Self::Week(multiplier),
            'M' => Self::Month(multiplier),
            _ => return None,
        };
        timeframe.validate().ok()
    }

    pub(crate) fn bucket_start(self, timestamp: i64, utc_offset_seconds: i32) -> i64 {
        let local = timestamp + i64::from(utc_offset_seconds);
        let start_local = match self {
            Self::Second(v) => fixed_bucket(local, i64::from(v)),
            Self::Minute(v) => fixed_bucket(local, i64::from(v) * 60),
            Self::Hour(v) => fixed_bucket(local, i64::from(v) * 3_600),
            Self::Day(v) => fixed_bucket(local, i64::from(v) * SECONDS_PER_DAY),
            Self::Week(v) => {
                let width = i64::from(v) * 7;
                let day = local.div_euclid(SECONDS_PER_DAY);
                let start_day = (day + 3).div_euclid(width) * width - 3;
                start_day * SECONDS_PER_DAY
            }
            Self::Month(v) => {
                let day = local.div_euclid(SECONDS_PER_DAY);
                let (year, month, _) = civil_from_days(day);
                let month_index = i64::from(year) * 12 + i64::from(month) - 1;
                let width = i64::from(v);
                let start_index = month_index.div_euclid(width) * width;
                let start_year = i32::try_from(start_index.div_euclid(12)).unwrap_or(1970);
                let start_month = u32::try_from(start_index.rem_euclid(12)).unwrap_or(0) + 1;
                days_from_civil(start_year, start_month, 1) * SECONDS_PER_DAY
            }
        };
        start_local - i64::from(utc_offset_seconds)
    }
}

fn fixed_bucket(timestamp: i64, width: i64) -> i64 {
    timestamp.div_euclid(width) * width
}

// Civil-date conversions by Howard Hinnant, adapted to Unix epoch day zero.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (
        i32::try_from(year).unwrap_or(1970),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let adjusted_year = i64::from(year) - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let yoe = adjusted_year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl fmt::Display for Timeframe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Second(v) => write!(f, "{v}s"),
            Self::Minute(v) => write!(f, "{v}m"),
            Self::Hour(v) => write!(f, "{v}h"),
            Self::Day(v) => write!(f, "{v}d"),
            Self::Week(v) => write!(f, "{v}w"),
            Self::Month(v) => write!(f, "{v}M"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResamplerOutput {
    /// The just-closed target-timeframe bar, if this input bar started a new bucket.
    /// `completed_bar.timestamp` is the bucket's **open** timestamp (see
    /// [`Timeframe::bucket_close`] for the corresponding close timestamp) — the same convention
    /// [`Bar::timestamp`] uses everywhere else in this crate.
    pub completed_bar: Option<Bar>,
    /// The still-forming bucket as of this input bar. Reading this is a deliberate lookahead/
    /// repaint risk (Pine's `barstate.isconfirmed == false` case): its OHLC will keep changing
    /// until the bucket closes. Prefer [`ConfirmedResampler`] when only confirmed HTF values may
    /// be consumed.
    pub current_unconfirmed: Bar,
    /// Count of whole target-timeframe buckets with no incoming bars between the previous
    /// completed bucket and this one (0 = no gap, i.e. contiguous). Always 0 for
    /// [`Timeframe::Month`], whose bucket width is not a fixed duration (see
    /// [`Timeframe::fixed_seconds`]), and for the very first bucket (no prior bucket to compare
    /// against). Assumes bars are fed in non-decreasing timestamp order, as the whole resampler
    /// does.
    pub gap_buckets: u32,
}

/// Aggregates OHLCV bars into calendar-aligned target bars.
#[derive(Debug, Clone)]
pub struct BarResampler {
    target_tf: Timeframe,
    utc_offset_seconds: i32,
    current_bucket: Option<Bar>,
    bucket_start_ts: i64,
}

impl BarResampler {
    pub fn new(target_tf: Timeframe) -> Result<Self, TimeframeError> {
        Self::with_utc_offset(target_tf, 0)
    }

    pub fn with_utc_offset(
        target_tf: Timeframe,
        utc_offset_seconds: i32,
    ) -> Result<Self, TimeframeError> {
        Ok(Self {
            target_tf: target_tf.validate()?,
            utc_offset_seconds,
            current_bucket: None,
            bucket_start_ts: 0,
        })
    }

    pub fn reset(&mut self) {
        self.current_bucket = None;
        self.bucket_start_ts = 0;
    }

    pub fn on_bar(&mut self, bar: &Bar) -> ResamplerOutput {
        let bucket_start = self
            .target_tf
            .bucket_start(bar.timestamp, self.utc_offset_seconds);
        let mut completed_bar = None;
        let mut gap_buckets = 0u32;

        if let Some(mut current) = self.current_bucket.take() {
            if bucket_start != self.bucket_start_ts {
                completed_bar = Some(current);
                gap_buckets = self.gap_buckets_between(self.bucket_start_ts, bucket_start);
                self.bucket_start_ts = bucket_start;
                self.current_bucket = Some(start_bucket(bucket_start, bar));
            } else {
                current.high = current.high.max(bar.high);
                current.low = current.low.min(bar.low);
                current.close = bar.close;
                current.volume += bar.volume;
                self.current_bucket = Some(current);
            }
        } else {
            self.bucket_start_ts = bucket_start;
            self.current_bucket = Some(start_bucket(bucket_start, bar));
        }

        ResamplerOutput {
            completed_bar,
            current_unconfirmed: self.current_bucket.clone().expect("bucket was initialized"),
            gap_buckets,
        }
    }

    /// Whole buckets skipped between the previous bucket's open (`prev_start`) and the new
    /// bucket's open (`next_start`), 0 if contiguous or if the target timeframe has no fixed
    /// width (`Timeframe::Month`).
    fn gap_buckets_between(&self, prev_start: i64, next_start: i64) -> u32 {
        match self.target_tf.fixed_seconds() {
            Some(width) if width > 0 => {
                let delta_buckets = (next_start - prev_start) / (width as i64);
                delta_buckets.saturating_sub(1).max(0) as u32
            }
            _ => 0,
        }
    }
}

/// A lookahead-safe view over a [`BarResampler`]: only ever yields a bar once its target-
/// timeframe bucket is confirmed closed, so it is structurally impossible to read a still-forming
/// (repainting) higher-timeframe value through it — the equivalent of Pine's
/// `request.security(..., lookahead = barmerge.lookahead_off)` combined with confirmed-only
/// consumption.
#[derive(Debug, Clone)]
pub struct ConfirmedResampler {
    inner: BarResampler,
}

impl ConfirmedResampler {
    pub fn new(target_tf: Timeframe) -> Result<Self, TimeframeError> {
        Ok(Self {
            inner: BarResampler::new(target_tf)?,
        })
    }

    pub fn with_utc_offset(
        target_tf: Timeframe,
        utc_offset_seconds: i32,
    ) -> Result<Self, TimeframeError> {
        Ok(Self {
            inner: BarResampler::with_utc_offset(target_tf, utc_offset_seconds)?,
        })
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Returns `Some(bar)` only when this input bar closes a target-timeframe bucket; `None`
    /// while the current bucket is still forming.
    pub fn on_bar(&mut self, bar: &Bar) -> Option<Bar> {
        self.inner.on_bar(bar).completed_bar
    }
}

fn start_bucket(timestamp: i64, bar: &Bar) -> Bar {
    Bar::new(
        timestamp, bar.open, bar.high, bar.low, bar.close, bar.volume,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_rejects_zero_and_supports_seconds() {
        assert_eq!(Timeframe::parse_str("30s"), Some(Timeframe::Second(30)));
        assert_eq!(Timeframe::parse_str("15m"), Some(Timeframe::Minute(15)));
        assert_eq!(Timeframe::parse_str("1M"), Some(Timeframe::Month(1)));
        assert_eq!(Timeframe::parse_str("0m"), None);
    }

    #[test]
    fn resamples_fixed_intervals() {
        let mut resampler = BarResampler::new(Timeframe::Minute(5)).unwrap();
        for i in 0..5 {
            let out = resampler.on_bar(&Bar::new(
                i * 60,
                100.0,
                105.0,
                95.0,
                100.0 + i as f64,
                100.0,
            ));
            assert!(out.completed_bar.is_none());
        }
        let out = resampler.on_bar(&Bar::new(300, 105.0, 110.0, 104.0, 108.0, 100.0));
        let completed = out.completed_bar.unwrap();
        assert_eq!(completed.timestamp, 0);
        assert_eq!(completed.close, 104.0);
        assert_eq!(completed.volume, 500.0);
    }

    #[test]
    fn month_boundaries_are_calendar_aligned() {
        let february = Timeframe::Month(1).bucket_start(1_706_745_600, 0);
        let march = Timeframe::Month(1).bucket_start(1_709_251_200, 0);
        assert_eq!(february, 1_706_745_600);
        assert_eq!(march, 1_709_251_200);
        assert_ne!(march - february, 30 * SECONDS_PER_DAY);
    }

    #[test]
    fn negative_timestamps_use_euclidean_buckets() {
        assert_eq!(Timeframe::Day(1).bucket_start(-1, 0), -86_400);
    }

    #[test]
    fn bucket_close_matches_fixed_width_and_is_none_for_month() {
        assert_eq!(Timeframe::Minute(5).bucket_close(0), Some(300));
        assert_eq!(Timeframe::Day(1).bucket_close(0), Some(SECONDS_PER_DAY));
        assert_eq!(Timeframe::Month(1).bucket_close(0), None);
    }

    #[test]
    fn gap_buckets_is_zero_for_contiguous_bars() {
        let mut resampler = BarResampler::new(Timeframe::Minute(5)).unwrap();
        resampler.on_bar(&Bar::new(0, 100.0, 101.0, 99.0, 100.0, 10.0));
        let out = resampler.on_bar(&Bar::new(300, 100.0, 101.0, 99.0, 100.0, 10.0));
        assert_eq!(out.gap_buckets, 0);
    }

    #[test]
    fn gap_buckets_reports_skipped_htf_buckets() {
        let mut resampler = BarResampler::new(Timeframe::Minute(5)).unwrap();
        resampler.on_bar(&Bar::new(0, 100.0, 101.0, 99.0, 100.0, 10.0));
        // Next bar arrives 3 buckets later (900s = 3 * 300s): two whole 5m buckets had no data.
        let out = resampler.on_bar(&Bar::new(900, 100.0, 101.0, 99.0, 100.0, 10.0));
        assert!(out.completed_bar.is_some());
        assert_eq!(out.gap_buckets, 2);
    }

    #[test]
    fn confirmed_resampler_never_exposes_unconfirmed_bucket() {
        let mut confirmed = ConfirmedResampler::new(Timeframe::Minute(5)).unwrap();
        for i in 0..5 {
            let out = confirmed.on_bar(&Bar::new(
                i * 60,
                100.0,
                105.0,
                95.0,
                100.0 + i as f64,
                100.0,
            ));
            assert!(
                out.is_none(),
                "bucket must not repaint through ConfirmedResampler"
            );
        }
        let closed = confirmed.on_bar(&Bar::new(300, 105.0, 110.0, 104.0, 108.0, 100.0));
        let bar = closed.unwrap();
        assert_eq!(bar.timestamp, 0);
        assert_eq!(bar.close, 104.0);
    }
}
