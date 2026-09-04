//! Batch and replay execution over a full bar history.
//!
//! [`Indicator::on_bar`](crate::indicator::Indicator::on_bar) processes one bar and returns the
//! latest value; consumers wanting a full, timestamp-aligned output series over a backfill
//! (charting, backtesting, golden-fixture generation) previously had to hand-roll the loop. These
//! helpers standardize it: deterministic (always start from [`Indicator::reset`]), timestamp-
//! aligned (one entry per input bar, in order, `None` during warmup), and reproducible (pure
//! function of the indicator + bar slice, safe to call repeatedly for replay).

use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::{Bar, BarValidationError};

/// One entry of a batch/replay output series: the source bar's timestamp paired with the
/// indicator's output for that bar (`None` while still inside the warmup period).
#[derive(Debug, Clone, PartialEq)]
pub struct TimestampedOutput {
    pub timestamp: i64,
    pub output: Option<IndicatorOutput>,
}

/// Resets `indicator`, then feeds `bars` through it in order, collecting one [`TimestampedOutput`]
/// per bar. Two calls with the same indicator type and `bars` slice always produce identical
/// results (deterministic backfill / reproducible replay).
pub fn run_batch<I: Indicator + ?Sized>(indicator: &mut I, bars: &[Bar]) -> Vec<TimestampedOutput> {
    indicator.reset();
    bars.iter()
        .map(|bar| TimestampedOutput {
            timestamp: bar.timestamp,
            output: indicator.on_bar(bar),
        })
        .collect()
}

/// Like [`run_batch`], but validates each bar via
/// [`Indicator::on_checked_bar`] and stops at the
/// first invalid bar, returning the entries collected so far plus the validation error.
pub fn run_batch_checked<I: Indicator + ?Sized>(
    indicator: &mut I,
    bars: &[Bar],
) -> Result<Vec<TimestampedOutput>, (Vec<TimestampedOutput>, BarValidationError)> {
    indicator.reset();
    let mut series = Vec::with_capacity(bars.len());
    for bar in bars {
        match indicator.on_checked_bar(bar) {
            Ok(output) => series.push(TimestampedOutput {
                timestamp: bar.timestamp,
                output,
            }),
            Err(err) => return Err((series, err)),
        }
    }
    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::moving_averages::SmaEngine;

    /// Builds valid bars around each close (offset so `low = close + 100.0 - 1.0` stays positive).
    fn sample_bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let c = c + 100.0;
                Bar::new((i as i64) * 60, c, c + 1.0, c - 1.0, c, 100.0)
            })
            .collect()
    }

    #[test]
    fn test_run_batch_is_timestamp_aligned_and_warmup_aware() {
        let bars = sample_bars(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut sma = SmaEngine::new(3);
        let series = run_batch(&mut sma, &bars);

        assert_eq!(series.len(), bars.len());
        for (entry, bar) in series.iter().zip(&bars) {
            assert_eq!(entry.timestamp, bar.timestamp);
        }
        // Warmup: SmaEngine needs 3 bars before it emits a value.
        assert!(series[0].output.is_none());
        assert!(series[1].output.is_none());
        assert!(series[2].output.is_some());
        assert_eq!(series[2].output.as_ref().unwrap().value, 102.0);
        assert_eq!(series[4].output.as_ref().unwrap().value, 104.0);
    }

    #[test]
    fn test_run_batch_is_deterministic_and_resets_prior_state() {
        let bars = sample_bars(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut sma = SmaEngine::new(3);

        let first = run_batch(&mut sma, &bars);
        // Re-running over the same indicator instance must reset first, so replay is idempotent.
        let second = run_batch(&mut sma, &bars);
        assert_eq!(first, second);
    }

    #[test]
    fn test_run_batch_checked_stops_at_invalid_bar() {
        let mut bars = sample_bars(&[10.0, 20.0]);
        bars.push(Bar::new(120, f64::NAN, 1.0, -1.0, 1.0, 100.0));
        bars.push(Bar::new(180, 3.0, 4.0, 2.0, 3.0, 100.0));

        let mut sma = SmaEngine::new(2);
        let result = run_batch_checked(&mut sma, &bars);
        let (partial, err) = result.unwrap_err();
        assert_eq!(partial.len(), 2);
        assert_eq!(err, BarValidationError::NonFiniteValue);
    }
}
