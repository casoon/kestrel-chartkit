//! Unified price-level candidates: Opening Ranges, period midpoints, round numbers, and swing
//! Fibonacci retracements/extensions all produce the same [`PriceLevel`] shape, so a consumer
//! (e.g. a zone/confluence step) can merge and rank them uniformly instead of handling four
//! bespoke, differently-shaped outputs.

use crate::model::Bar;
use crate::session::{SessionConfig, SessionConfigError, SessionTracker};
use crate::timeframe::{Timeframe, TimeframeError};

/// Which family a [`PriceLevel`] candidate comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceLevelKind {
    OpeningRangeHigh,
    OpeningRangeLow,
    OpeningRangeMid,
    PeriodMidpoint,
    RoundNumber,
    SwingFibonacci,
}

/// A single, provider-neutral price-level candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceLevel {
    pub kind: PriceLevelKind,
    pub price: f64,
    /// Human-readable qualifier (e.g. a Fibonacci ratio `"0.618"`, or the round increment
    /// `"100"`). Empty for kinds that need no further qualification.
    pub label: String,
}

impl PriceLevel {
    fn new(kind: PriceLevelKind, price: f64, label: impl Into<String>) -> Self {
        Self {
            kind,
            price,
            label: label.into(),
        }
    }
}

/// The three canonical Opening Range levels once the ORB window has closed.
pub fn opening_range_levels(orb_high: f64, orb_low: f64) -> Vec<PriceLevel> {
    vec![
        PriceLevel::new(PriceLevelKind::OpeningRangeHigh, orb_high, ""),
        PriceLevel::new(PriceLevelKind::OpeningRangeLow, orb_low, ""),
        PriceLevel::new(
            PriceLevelKind::OpeningRangeMid,
            (orb_high + orb_low) / 2.0,
            "",
        ),
    ]
}

/// Round-number levels nearest `price`: `levels_each_side` multiples of `increment` above and
/// below (plus the multiple `price` itself falls between), e.g. `increment = 100.0` for
/// round-hundred levels on an equity index, or `0.0050` for a 50-pip FX grid.
pub fn round_number_levels(price: f64, increment: f64, levels_each_side: usize) -> Vec<PriceLevel> {
    if !increment.is_finite() || increment <= 0.0 || !price.is_finite() {
        return Vec::new();
    }
    let base = (price / increment).floor() * increment;
    let mut levels = Vec::with_capacity(levels_each_side * 2 + 2);
    for i in 0..=(levels_each_side as i64 + 1) {
        let level = base + i as f64 * increment;
        levels.push(PriceLevel::new(
            PriceLevelKind::RoundNumber,
            level,
            format!("{increment}"),
        ));
    }
    for i in 1..=levels_each_side as i64 {
        let level = base - i as f64 * increment;
        levels.push(PriceLevel::new(
            PriceLevelKind::RoundNumber,
            level,
            format!("{increment}"),
        ));
    }
    levels
}

/// Standard retracement/extension ratios for [`swing_fibonacci_levels`].
pub const FIBONACCI_RATIOS: [f64; 8] = [0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.272, 1.618];

/// Fibonacci levels between a confirmed swing high and low (from e.g.
/// [`super::zigzag_advanced::AdvancedZigZagEngine::nodes`]'s last two opposite-type confirmed
/// nodes). `is_uptrend`: `true` if the swing ran low-to-high (retracements measured down from the
/// high), `false` if it ran high-to-low (retracements measured up from the low).
pub fn swing_fibonacci_levels(
    swing_high: f64,
    swing_low: f64,
    is_uptrend: bool,
) -> Vec<PriceLevel> {
    let range = swing_high - swing_low;
    FIBONACCI_RATIOS
        .iter()
        .map(|&ratio| {
            let price = if is_uptrend {
                swing_high - range * ratio
            } else {
                swing_low + range * ratio
            };
            PriceLevel::new(PriceLevelKind::SwingFibonacci, price, format!("{ratio}"))
        })
        .collect()
}

/// Tracks period open/high/low/close on a configurable [`Timeframe`] and yields the prior
/// (confirmed) period's midpoint once a new period starts.
pub struct PeriodMidpointTracker {
    period_tf: Timeframe,
    utc_offset_seconds: i32,
    current_period_start: Option<i64>,
    curr_high: f64,
    curr_low: f64,
}

impl PeriodMidpointTracker {
    pub fn new(period_tf: Timeframe) -> Result<Self, TimeframeError> {
        Self::with_utc_offset(period_tf, 0)
    }

    pub fn with_utc_offset(
        period_tf: Timeframe,
        utc_offset_seconds: i32,
    ) -> Result<Self, TimeframeError> {
        Ok(Self {
            period_tf: period_tf.validate()?,
            utc_offset_seconds,
            current_period_start: None,
            curr_high: f64::MIN,
            curr_low: f64::MAX,
        })
    }

    pub fn reset(&mut self) {
        self.current_period_start = None;
        self.curr_high = f64::MIN;
        self.curr_low = f64::MAX;
    }

    /// Returns `Some(midpoint)` only on the bar that closes a period (the same "confirmed value
    /// becomes available one bar later" convention as [`crate::timeframe::BarResampler`]).
    pub fn on_bar(&mut self, bar: &Bar) -> Option<PriceLevel> {
        let period_start = self
            .period_tf
            .bucket_start(bar.timestamp, self.utc_offset_seconds);
        let mut completed = None;

        match self.current_period_start {
            Some(start) if start != period_start => {
                completed = Some(PriceLevel::new(
                    PriceLevelKind::PeriodMidpoint,
                    (self.curr_high + self.curr_low) / 2.0,
                    "",
                ));
                self.current_period_start = Some(period_start);
                self.curr_high = bar.high;
                self.curr_low = bar.low;
            }
            Some(_) => {
                self.curr_high = self.curr_high.max(bar.high);
                self.curr_low = self.curr_low.min(bar.low);
            }
            None => {
                self.current_period_start = Some(period_start);
                self.curr_high = bar.high;
                self.curr_low = bar.low;
            }
        }

        completed
    }
}

/// Combines Opening Range and period-midpoint levels (both stateful/time-driven) plus round
/// numbers (stateless, derived from the current price) into one merged candidate list per bar.
/// Swing Fibonacci levels are intentionally not included here: they need an externally supplied
/// confirmed swing (see [`swing_fibonacci_levels`]), which this aggregator has no opinion on.
pub struct PriceLevelAggregator {
    session: SessionTracker,
    period: PeriodMidpointTracker,
    round_increment: f64,
    round_levels_each_side: usize,
}

impl PriceLevelAggregator {
    pub fn new(
        session_config: SessionConfig,
        period_tf: Timeframe,
        round_increment: f64,
        round_levels_each_side: usize,
    ) -> Result<Self, PriceLevelAggregatorError> {
        Ok(Self {
            session: SessionTracker::new(session_config)
                .map_err(PriceLevelAggregatorError::Session)?,
            period: PeriodMidpointTracker::new(period_tf)
                .map_err(PriceLevelAggregatorError::Timeframe)?,
            round_increment,
            round_levels_each_side,
        })
    }

    pub fn reset(&mut self) {
        self.session.reset();
        self.period.reset();
    }

    pub fn on_bar(&mut self, bar: &Bar) -> Vec<PriceLevel> {
        self.session.on_bar(bar);
        let period_level = self.period.on_bar(bar);

        let mut levels = Vec::new();
        if let (Some(h), Some(l)) = (self.session.orb_high(), self.session.orb_low()) {
            if !self.session.in_orb_window() {
                levels.extend(opening_range_levels(h, l));
            }
        }
        if let Some(level) = period_level {
            levels.push(level);
        }
        levels.extend(round_number_levels(
            bar.close,
            self.round_increment,
            self.round_levels_each_side,
        ));
        levels
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PriceLevelAggregatorError {
    Session(SessionConfigError),
    Timeframe(TimeframeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opening_range_levels() {
        let levels = opening_range_levels(110.0, 100.0);
        assert_eq!(levels.len(), 3);
        assert!(levels
            .iter()
            .any(|l| l.kind == PriceLevelKind::OpeningRangeMid && (l.price - 105.0).abs() < 1e-9));
    }

    #[test]
    fn test_round_number_levels_bracket_price() {
        let levels = round_number_levels(1234.0, 100.0, 1);
        let prices: Vec<f64> = levels.iter().map(|l| l.price).collect();
        assert!(prices.contains(&1200.0));
        assert!(prices.contains(&1300.0));
        assert!(prices.contains(&1100.0));
        assert!(prices.contains(&1400.0));
    }

    #[test]
    fn test_round_number_levels_rejects_degenerate_input() {
        assert!(round_number_levels(100.0, 0.0, 3).is_empty());
        assert!(round_number_levels(f64::NAN, 10.0, 3).is_empty());
    }

    #[test]
    fn test_swing_fibonacci_uptrend_retraces_down_from_high() {
        let levels = swing_fibonacci_levels(200.0, 100.0, true);
        assert_eq!(levels.len(), FIBONACCI_RATIOS.len());
        let half = levels.iter().find(|l| l.label == "0.5").unwrap();
        assert!((half.price - 150.0).abs() < 1e-9);
        let full_ext = levels.iter().find(|l| l.label == "1.618").unwrap();
        assert!(
            full_ext.price < 100.0,
            "1.618 extension in an uptrend must project below the swing low"
        );
    }

    #[test]
    fn test_swing_fibonacci_downtrend_retraces_up_from_low() {
        let levels = swing_fibonacci_levels(200.0, 100.0, false);
        let half = levels.iter().find(|l| l.label == "0.5").unwrap();
        assert!((half.price - 150.0).abs() < 1e-9);
        let full_ext = levels.iter().find(|l| l.label == "1.618").unwrap();
        assert!(
            full_ext.price > 200.0,
            "1.618 extension in a downtrend must project above the swing high"
        );
    }

    #[test]
    fn test_period_midpoint_tracker_yields_prior_period_confirmed() {
        let mut tracker = PeriodMidpointTracker::new(Timeframe::Minute(5)).unwrap();
        for i in 0..5 {
            let out = tracker.on_bar(&Bar::new(i * 60, 100.0, 110.0, 90.0, 100.0, 10.0));
            assert!(out.is_none());
        }
        let level = tracker
            .on_bar(&Bar::new(300, 100.0, 101.0, 99.0, 100.0, 10.0))
            .unwrap();
        assert!((level.price - 100.0).abs() < 1e-9); // (110+90)/2
    }

    #[test]
    fn test_aggregator_merges_round_numbers_and_opening_range() {
        let session = SessionConfig {
            start_hour: 0,
            start_minute: 0,
            end_hour: 23,
            end_minute: 59,
            orb_duration_mins: 1,
            utc_offset_seconds: 0,
        };
        let mut aggregator =
            PriceLevelAggregator::new(session, Timeframe::Day(1), 10.0, 1).unwrap();

        // First bar starts the ORB window (still forming), later bars are past it.
        aggregator.on_bar(&Bar::new(0, 100.0, 101.0, 99.0, 100.0, 10.0));
        let levels = aggregator.on_bar(&Bar::new(120, 105.0, 106.0, 104.0, 105.0, 10.0));

        assert!(levels.iter().any(|l| l.kind == PriceLevelKind::RoundNumber));
        assert!(levels
            .iter()
            .any(|l| l.kind == PriceLevelKind::OpeningRangeHigh));
    }
}
