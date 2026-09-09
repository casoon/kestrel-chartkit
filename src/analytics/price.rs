//! Window price summary and Wilder ATR history, in price units.

#[cfg(feature = "serde")]
use serde::Serialize;

use super::true_range;
use crate::Bar;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct PriceSummary {
    pub last: f64,
    pub prev_close: f64,
    pub change_abs: f64,
    pub change_pct: f64,
    /// Highest high / lowest low across the supplied window (the caller picks
    /// the window length — e.g. one session's worth of bars).
    pub window_high: f64,
    pub window_low: f64,
    /// Where `last` sits in `[window_low, window_high]`, 0..1 (0.5 if the
    /// window is flat).
    pub range_position: f64,
    /// Wilder ATR in price units, and as a percentage of `last`.
    pub atr: f64,
    pub atr_pct: f64,
    /// Fraction of the ATR history (0..1) at or below the current ATR — a
    /// cheap "is volatility unusually high/low right now" read.
    pub atr_percentile: f64,
    /// Suggested stop distance in price units (`stop_mult · atr`).
    pub stop_distance: f64,
}

/// Builds the summary from `bars` (oldest first). `atr_len` is the Wilder ATR
/// period, `stop_mult` the ATR multiple for the suggested stop. `None` until
/// there are enough bars to seed the ATR.
pub fn price_summary(bars: &[Bar], atr_len: usize, stop_mult: f64) -> Option<PriceSummary> {
    if atr_len < 1 || bars.len() < atr_len + 1 {
        return None;
    }
    let last = bars[bars.len() - 1].close;
    let prev_close = bars[bars.len() - 2].close;
    let change_abs = last - prev_close;
    let change_pct = if prev_close != 0.0 {
        100.0 * change_abs / prev_close
    } else {
        0.0
    };

    let window_high = bars
        .iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let window_low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
    let span = window_high - window_low;
    let range_position = if span > 0.0 {
        ((last - window_low) / span).clamp(0.0, 1.0)
    } else {
        0.5
    };

    let atr_series = wilder_atr_series(bars, atr_len);
    let atr = *atr_series.last()?;
    let atr_pct = if last != 0.0 { 100.0 * atr / last } else { 0.0 };
    let atr_percentile = percentile_of_last(&atr_series);
    let stop_distance = stop_mult * atr;

    Some(PriceSummary {
        last,
        prev_close,
        change_abs,
        change_pct,
        window_high,
        window_low,
        range_position,
        atr,
        atr_pct,
        atr_percentile,
        stop_distance,
    })
}

/// Wilder ATR (RMA of true range) as a series, one value per bar once the
/// seed window has filled — mirrors the ported `Atr` indicator's kernel but
/// returns absolute price units (not the `ATR%` display mode) and keeps the
/// whole series so `atr_percentile` has something to rank against.
fn wilder_atr_series(bars: &[Bar], len: usize) -> Vec<f64> {
    let mut smoother = crate::indicator::smoothing::Rma::new(len);
    let mut prev_close = None;
    bars.iter()
        .filter_map(|bar| {
            let tr = true_range(bar, prev_close);
            prev_close = Some(bar.close);
            smoother.update(tr)
        })
        .collect()
}

/// Fraction of `series` values ≤ its last element, 0..1.
fn percentile_of_last(series: &[f64]) -> f64 {
    let Some(&last) = series.last() else {
        return 0.0;
    };
    let below = series.iter().filter(|&&v| v <= last).count();
    below as f64 / series.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(h: f64, l: f64, c: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: c,
            high: h,
            low: l,
            close: c,
            volume: 0.0,
        }
    }

    #[test]
    fn change_and_range_position() {
        // 20 flat bars then a step up, so last sits at the top of the range.
        let mut bars: Vec<Bar> = (0..20).map(|_| bar(100.5, 99.5, 100.0)).collect();
        bars.push(bar(102.0, 101.0, 102.0));
        let s = price_summary(&bars, 14, 1.5).expect("enough bars");
        assert_eq!(s.last, 102.0);
        assert_eq!(s.prev_close, 100.0);
        assert!((s.change_abs - 2.0).abs() < 1e-9);
        assert!((s.change_pct - 2.0).abs() < 1e-9);
        assert_eq!(s.window_high, 102.0);
        assert!(s.range_position > 0.99, "last is the window high");
        assert!(s.stop_distance > 0.0);
        assert!((0.0..=1.0).contains(&s.atr_percentile));
    }

    #[test]
    fn too_few_bars_is_none() {
        let bars: Vec<Bar> = (0..5).map(|_| bar(101.0, 99.0, 100.0)).collect();
        assert!(price_summary(&bars, 14, 1.5).is_none());
    }
}
