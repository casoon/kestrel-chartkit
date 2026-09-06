//! Batch and replay execution over a full bar history.
//!
//! [`Indicator::on_bar`](crate::indicator::Indicator::on_bar) processes one bar and returns the
//! latest value; consumers wanting a full, timestamp-aligned output series over a backfill
//! (charting, backtesting, golden-fixture generation) previously had to hand-roll the loop. These
//! helpers standardize it: deterministic (always start from [`Indicator::reset`]), timestamp-
//! aligned (one entry per input bar, in order, `None` during warmup), and reproducible (pure
//! function of the indicator + bar slice, safe to call repeatedly for replay).

use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::{Bar, BarValidationError, SeriesCapabilities};

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

/// Result of [`run_batch_with_applicability`]: the batch output series plus the applicability
/// verdict for the indicator/series-capabilities pair it was computed for.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchResult {
    pub series: Vec<TimestampedOutput>,
    pub applicability: crate::applicability::Applicability,
}

/// Like [`run_batch`], but also attaches an [`crate::applicability::Applicability`] verdict for
/// the named indicator against `capabilities`.
///
/// `run_batch`/`run_batch_checked` are generic over `I: Indicator` and never see the indicator's
/// registry name, so they cannot look up its [`crate::applicability::DataRequirements`]
/// themselves — hence this separate function that takes `name` explicitly, rather than a change
/// to either existing function's signature.
///
/// The series is always computed, even when the verdict is
/// [`crate::applicability::Applicability::Unsuitable`] — callers may legitimately want to see a
/// `Degraded` result, and a batch caller can still act on `Unsuitable` since it is always present
/// on [`BatchResult`], not silently dropped. This is what avoids the "computes and hides the
/// warning" failure mode the applicability check exists to prevent.
/// Also tags every emitted [`IndicatorOutput`] with `capabilities` (see
/// [`IndicatorOutput::series_capabilities`]) — this is the one place in the crate that already
/// receives a `SeriesCapabilities` value alongside the indicator run, so it is where the
/// origin gets attached rather than requiring every one of the ~90 `Indicator` implementations to
/// do it themselves.
pub fn run_batch_with_applicability<I: Indicator + ?Sized>(
    name: &str,
    indicator: &mut I,
    bars: &[Bar],
    capabilities: &SeriesCapabilities,
) -> BatchResult {
    let requirements = crate::applicability::data_requirements(name);
    let applicability = crate::applicability::check_applicability(&requirements, capabilities);
    let series = run_batch(indicator, bars)
        .into_iter()
        .map(|entry| TimestampedOutput {
            timestamp: entry.timestamp,
            output: entry
                .output
                .map(|output| output.with_capabilities(*capabilities)),
        })
        .collect();
    BatchResult {
        series,
        applicability,
    }
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

    fn real_volume_capabilities(volume: crate::model::VolumeKind) -> SeriesCapabilities {
        use crate::model::{
            ContinuityKind, LiquidityTier, PriceAdjustment, Provenance, SessionKind,
        };
        SeriesCapabilities {
            volume,
            trade_direction: false,
            session: SessionKind::Regular,
            continuity: ContinuityKind::SingleContract,
            price_adjustment: PriceAdjustment::Raw,
            provenance: Provenance::Exchange,
            liquidity_tier: LiquidityTier::Deep,
        }
    }

    #[test]
    fn test_run_batch_with_applicability_is_applicable_with_real_volume() {
        let bars = sample_bars(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut sma = SmaEngine::new(3);
        let capabilities = real_volume_capabilities(crate::model::VolumeKind::RealTurnover);

        let result = run_batch_with_applicability("vwap", &mut sma, &bars, &capabilities);

        assert_eq!(result.series.len(), bars.len());
        assert_eq!(
            result.applicability,
            crate::applicability::Applicability::Applicable
        );
    }

    #[test]
    fn test_run_batch_with_applicability_flags_unsuitable_but_still_computes() {
        let bars = sample_bars(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut sma = SmaEngine::new(3);
        let capabilities = real_volume_capabilities(crate::model::VolumeKind::Tick);

        let result = run_batch_with_applicability("vwap", &mut sma, &bars, &capabilities);

        // The series is still computed even though the verdict is Unsuitable.
        assert_eq!(result.series.len(), bars.len());
        assert!(matches!(
            result.applicability,
            crate::applicability::Applicability::Unsuitable { .. }
        ));
    }

    #[test]
    fn test_run_batch_with_applicability_tags_every_output_with_capabilities() {
        let bars = sample_bars(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut sma = SmaEngine::new(3);
        let capabilities = real_volume_capabilities(crate::model::VolumeKind::RealTurnover);

        let result = run_batch_with_applicability("vwap", &mut sma, &bars, &capabilities);

        // Every emitted output (i.e. every entry past warmup) carries the capabilities it was
        // computed against — this is what makes pivots_structure/zigzag/zigzag_advanced/
        // pivot_sets (and every other generic `Indicator`) traceable to their source series
        // without each of them needing its own `series_capabilities` plumbing.
        for entry in &result.series {
            if let Some(output) = &entry.output {
                assert_eq!(output.series_capabilities, Some(capabilities));
            }
        }
        assert!(result.series.iter().any(|e| e.output.is_some()));
    }

    #[test]
    fn test_run_batch_leaves_capabilities_unset() {
        let bars = sample_bars(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut sma = SmaEngine::new(3);

        let series = run_batch(&mut sma, &bars);

        // Plain `run_batch` never sees a `SeriesCapabilities` value, so it cannot attach one —
        // callers without that information get `None`, same as a direct `on_bar` call.
        for entry in &series {
            if let Some(output) = &entry.output {
                assert_eq!(output.series_capabilities, None);
            }
        }
    }
}
