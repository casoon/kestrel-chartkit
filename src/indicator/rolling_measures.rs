//! Three rolling measures of the close as catalog entries: `r_squared` (how well a straight line
//! fits the window), `percent_rank` (where the close stands among the closes before it) and
//! `cycle_length` (how many bars the close stays on one side of its average). Each wraps an
//! engine that exists on its own: [`crate::regime_advanced::PredictabilityTracker`], the
//! percent-rank kernel of the smoothing module and [`crate::regime_advanced::AdaptiveCycleTracker`].

use super::smoothing::{PercentRank, Sma};
use super::{Indicator, IndicatorOutput};
use crate::model::Bar;
use crate::regime_advanced::{AdaptiveCycleTracker, PredictabilityTracker};

/// Streaming R^2 of an ordinary-least-squares line fitted to the last `period` closes against
/// their bar index: `1 - SS_res / SS_tot`, limited to `0..=1`. Close to `1` is a clean, straight
/// trend, close to `0` noise without direction. A window of equal closes (no variation to
/// explain) reads `1`. First output with the `period`-th bar. [`Indicator::reset`] clears the
/// window.
#[derive(Debug, Clone)]
pub struct RSquaredEngine {
    period: usize,
    tracker: PredictabilityTracker,
}

impl RSquaredEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            tracker: PredictabilityTracker::new(period),
        }
    }
}

impl Indicator for RSquaredEngine {
    fn name(&self) -> &str {
        "r_squared"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.tracker.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.tracker.update(bar.close).map(IndicatorOutput::new)
    }
}

/// Registry definition of `r_squared`.
pub const R_SQUARED_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "r_squared",
        aliases: &[],
        description: "R-squared of a least-squares line through the last period closes, 1 for a clean straight trend and near 0 for noise; a window of equal closes reads 1",
        params: &[super::definition::ParamSpec::count("period", 20, 2, 10000)
            .meaning("closes the line is fitted to")],
        checks: &[],
        build: |p| Ok(Box::new(RSquaredEngine::new(p.count("period")))),
    };

/// Streaming percent rank: the share, in percent, of the previous `period` closes that are at or
/// below the current close (`100 * #{previous <= close} / period`). `100` for a new high of the
/// window, near `0` for a new low. First output with the `(period + 1)`-th bar.
/// [`Indicator::reset`] clears the window.
#[derive(Debug, Clone)]
pub struct PercentRankEngine {
    period: usize,
    rank: PercentRank,
}

impl PercentRankEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            rank: PercentRank::new(period),
        }
    }
}

impl Indicator for PercentRankEngine {
    fn name(&self) -> &str {
        "percent_rank"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.rank = PercentRank::new(self.period);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.rank.update(Some(bar.close)).map(IndicatorOutput::new)
    }
}

/// Registry definition of `percent_rank`.
pub const PERCENT_RANK_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "percent_rank",
        aliases: &[],
        description: "Percent rank of the close among the previous period closes: the share, in percent, at or below the current close",
        params: &[super::definition::ParamSpec::count("period", 20, 1, 10000)
            .meaning("previous closes the current one is ranked against")],
        checks: &[],
        build: |p| Ok(Box::new(PercentRankEngine::new(p.count("period")))),
    };

/// Streaming cycle length by zero crossings: the detrended close `close - SMA(close, period)`
/// changes sign when the close crosses its average. The bars between two crossings are one swing;
/// `value` is the mean length of the last `history` completed swings, `extra["bars_since_crossing"]`
/// the bars of the swing in progress (the crossing bar is its first). A bar exactly on the average
/// belongs to the swing in progress and does not change the side. This is an empirical measure of
/// the swing length, not a spectral or Hilbert-transform estimate of a dominant cycle.
///
/// The detrended series starts with the `period`-th bar; the first output comes with the first
/// crossing after it, so the first output bar depends on the data (never before bar `period + 1`).
/// [`Indicator::reset`] starts over.
#[derive(Debug, Clone)]
pub struct CycleLengthEngine {
    period: usize,
    history: usize,
    average: Sma,
    tracker: AdaptiveCycleTracker,
}

impl CycleLengthEngine {
    pub fn new(period: usize, history: usize) -> Self {
        Self {
            period,
            history,
            average: Sma::new(period),
            tracker: AdaptiveCycleTracker::new(history),
        }
    }
}

impl Indicator for CycleLengthEngine {
    fn name(&self) -> &str {
        "cycle_length"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.average = Sma::new(self.period);
        self.tracker = AdaptiveCycleTracker::new(self.history);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let average = self.average.update(bar.close)?;
        let reading = self.tracker.update(bar.close - average);
        let length = reading.average_swing_length?;
        let mut output = IndicatorOutput::new(length);
        output.extra.insert(
            "bars_since_crossing".to_string(),
            f64::from(reading.bars_since_crossing),
        );
        Some(output)
    }
}

/// Registry definition of `cycle_length`.
pub const CYCLE_LENGTH_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "cycle_length",
        aliases: &[],
        description: "Mean bars between the close's crossings of its simple average over the last history swings (zero-crossing swing length, not a spectral cycle estimate); extra bars_since_crossing counts the swing in progress",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000)
                .meaning("bars of the simple average the close is detrended against"),
            super::definition::ParamSpec::count("history", 8, 1, 10000)
                .meaning("completed swings the mean is taken over"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(CycleLengthEngine::new(
                p.count("period"),
                p.count("history"),
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, close: f64) -> Bar {
        Bar::new(t, close, close, close, close, 100.0)
    }

    #[test]
    fn a_straight_line_has_r_squared_one() {
        let mut engine = RSquaredEngine::new(5);
        let mut last = None;
        for i in 0..8 {
            last = engine.on_bar(&bar(i, 100.0 + 2.0 * i as f64));
        }
        assert!((last.unwrap().value - 1.0).abs() < 1e-12);
    }

    #[test]
    fn percent_rank_of_a_new_high_is_one_hundred() {
        let mut engine = PercentRankEngine::new(4);
        let mut last = None;
        for (i, c) in [1.0, 2.0, 3.0, 4.0, 5.0].into_iter().enumerate() {
            last = engine.on_bar(&bar(i as i64, c));
        }
        assert_eq!(last.unwrap().value, 100.0);
    }

    #[test]
    fn cycle_length_needs_a_completed_swing() {
        let mut engine = CycleLengthEngine::new(3, 4);
        // A rise, then a fall below the average: the first output comes with the first crossing.
        let closes = [10.0, 11.0, 12.0, 13.0, 14.0, 9.0, 8.0, 7.0];
        let outputs: Vec<_> = closes
            .iter()
            .enumerate()
            .map(|(i, c)| engine.on_bar(&bar(i as i64, *c)))
            .collect();
        assert!(outputs[..5].iter().all(Option::is_none));
        assert!(outputs[5].is_some());
    }
}
