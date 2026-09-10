//! Activity from ATR and optional volume percentiles. The caller supplies volume applicability.

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::Bar;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ActivityReading {
    pub atr_percentile: f64,
    /// `None` when the volume component is disabled or the window has no
    /// usable (non-zero) volume.
    pub volume_percentile: Option<f64>,
    /// Composite 0..1 — mean of whichever components are present.
    pub score: f64,
}

/// `atr_percentile` is the caller's already-computed
/// `PriceSummary::atr_percentile`. `use_volume` toggles the volume
/// component; `window` bounds how far back "recent history" reaches for the
/// volume percentile.
///
/// `volume_percentile` is the share of the last `window` bars (at least one) whose volume is at
/// or below the last bar's, `None` when disabled or when every volume in that window is zero;
/// `score` is the mean of `atr_percentile` and `volume_percentile`, or `atr_percentile` alone.
pub fn activity_reading(
    bars: &[Bar],
    atr_percentile: f64,
    use_volume: bool,
    window: usize,
) -> ActivityReading {
    let volume_percentile = if use_volume {
        volume_percentile_of_last(bars, window)
    } else {
        None
    };

    let score = match volume_percentile {
        Some(v) => (atr_percentile + v) / 2.0,
        None => atr_percentile,
    };

    ActivityReading {
        atr_percentile,
        volume_percentile,
        score,
    }
}

/// Fraction of the last `window` bars' volume that is ≤ the most recent
/// bar's volume, 0..1. `None` if there's no bar, or every bar in the window
/// has zero volume (no usable data, e.g. a feed that doesn't report it).
fn volume_percentile_of_last(bars: &[Bar], window: usize) -> Option<f64> {
    if bars.is_empty() {
        return None;
    }
    let start = bars.len().saturating_sub(window.max(1));
    let series = &bars[start..];
    if series.iter().all(|b| b.volume == 0.0) {
        return None;
    }
    let last = series.last()?.volume;
    let below = series.iter().filter(|b| b.volume <= last).count();
    Some(below as f64 / series.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(volume: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: 100.0,
            high: 100.5,
            low: 99.5,
            close: 100.0,
            volume,
        }
    }

    #[test]
    fn volume_disabled_uses_atr_only() {
        let bars: Vec<Bar> = (0..20).map(|i| bar(i as f64)).collect();
        let r = activity_reading(&bars, 0.7, false, 60);
        assert_eq!(r.volume_percentile, None);
        assert_eq!(r.score, 0.7);
    }

    #[test]
    fn highest_recent_volume_gives_percentile_one() {
        let bars: Vec<Bar> = (0..20).map(|i| bar(i as f64)).collect();
        let r = activity_reading(&bars, 0.5, true, 60);
        assert_eq!(r.volume_percentile, Some(1.0));
        assert!((r.score - 0.75).abs() < 1e-9);
    }

    #[test]
    fn zero_volume_series_falls_back_to_atr_only() {
        let bars: Vec<Bar> = (0..20).map(|_| bar(0.0)).collect();
        let r = activity_reading(&bars, 0.5, true, 60);
        assert_eq!(r.volume_percentile, None);
        assert_eq!(r.score, 0.5);
    }

    #[test]
    fn empty_bars_is_atr_only() {
        let r = activity_reading(&[], 0.3, true, 60);
        assert_eq!(r.volume_percentile, None);
        assert_eq!(r.score, 0.3);
    }
}
