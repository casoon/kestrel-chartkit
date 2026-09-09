//! Three-measure trend/range vote (ADX, unbounded Choppiness, Kaufman efficiency).
//! Distinct from the four-state market regime classifier: a flat efficiency window is unavailable.

#[cfg(feature = "serde")]
use serde::Serialize;

use super::{efficiency_ratio, true_range};
use crate::indicator::adx::Adx;
use crate::Bar;
use crate::Indicator;

/// Standard trending/ranging thresholds for each component (widely used
/// defaults; see the module doc). A component votes "trending" when it clears
/// its threshold.
const ADX_TREND: f64 = 25.0;
const CHOP_TREND: f64 = 38.2; // below → trending, above ~61.8 → ranging
const EFFICIENCY_TREND: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RegimeState {
    Trending,
    Ranging,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct RegimeReading {
    pub state: RegimeState,
    pub adx: f64,
    pub choppiness: f64,
    pub efficiency: f64,
    /// How many of the three components voted "trending" (0..=3).
    pub trend_votes: u8,
}

/// Classifies the regime from `bars` (oldest first). `None` until enough bars
/// exist to seed all three measures. `adx_len` also drives the ADX smoothing
/// window (Wilder default: both 14); `window` is the lookback for Choppiness
/// and the Efficiency Ratio.
pub fn classify_regime(bars: &[Bar], adx_len: usize, window: usize) -> Option<RegimeReading> {
    if window < 2 || adx_len < 1 {
        return None;
    }
    // ADX needs the most warmup (di_len + adx_smooth); the two window measures
    // need `window`+1 bars for their true-range / return terms.
    let adx = latest_adx(bars, adx_len)?;
    let choppiness = choppiness(bars, window)?;
    let efficiency = efficiency_ratio(bars, window)?;

    let trend_votes = (adx >= ADX_TREND) as u8
        + (choppiness <= CHOP_TREND) as u8
        + (efficiency >= EFFICIENCY_TREND) as u8;
    let state = if trend_votes >= 2 {
        RegimeState::Trending
    } else {
        RegimeState::Ranging
    };

    Some(RegimeReading {
        state,
        adx,
        choppiness,
        efficiency,
        trend_votes,
    })
}

/// Feeds every bar through the ported `Adx` and returns its last value —
/// reuses the exact Wilder DMI/ADX already parity-checked rather than
/// re-deriving it here.
fn latest_adx(bars: &[Bar], adx_len: usize) -> Option<f64> {
    let mut adx = Adx::new(adx_len, adx_len, 3, 20.0);
    let mut last = None;
    for bar in bars {
        if let Some(out) = adx.on_bar(bar) {
            last = Some(out.value);
        }
    }
    last
}

/// Choppiness Index over the last `n` bars. True range of the first bar in
/// the window references the close just before it, so `n`+1 bars are needed.
fn choppiness(bars: &[Bar], n: usize) -> Option<f64> {
    if bars.len() < n + 1 {
        return None;
    }
    let window = &bars[bars.len() - n..];
    let prev_close = bars[bars.len() - n - 1].close;

    let mut tr_sum = 0.0;
    let mut prev = Some(prev_close);
    let mut highest = f64::NEG_INFINITY;
    let mut lowest = f64::INFINITY;
    for bar in window {
        tr_sum += true_range(bar, prev);
        highest = highest.max(bar.high);
        lowest = lowest.min(bar.low);
        prev = Some(bar.close);
    }
    let range = highest - lowest;
    if range <= 0.0 || tr_sum <= 0.0 {
        return None;
    }
    Some(100.0 * (tr_sum / range).log10() / (n as f64).log10())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(o: f64, h: f64, l: f64, c: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: o,
            high: h,
            low: l,
            close: c,
            volume: 0.0,
        }
    }

    /// A clean monotonic ramp is maximally efficient (ER = 1) and minimally
    /// choppy.
    #[test]
    fn straight_trend_reads_as_trending() {
        let bars: Vec<Bar> = (0..60)
            .map(|i| {
                let c = 100.0 + i as f64;
                bar(c - 0.5, c + 0.2, c - 0.7, c)
            })
            .collect();
        let r = classify_regime(&bars, 14, 20).expect("enough bars");
        assert!((r.efficiency - 1.0).abs() < 1e-9, "ramp ER should be 1");
        assert_eq!(r.state, RegimeState::Trending);
        assert!(r.trend_votes >= 2);
    }

    /// A flat zig-zag oscillating around a level nets ~zero travel → low
    /// efficiency, high choppiness → ranging.
    #[test]
    fn oscillation_reads_as_ranging() {
        let bars: Vec<Bar> = (0..60)
            .map(|i| {
                let c = if i % 2 == 0 { 100.0 } else { 101.0 };
                bar(c, c + 0.5, c - 0.5, c)
            })
            .collect();
        let r = classify_regime(&bars, 14, 20).expect("enough bars");
        assert!(
            r.efficiency < 0.3,
            "zig-zag ER should be low: {}",
            r.efficiency
        );
        assert_eq!(r.state, RegimeState::Ranging);
    }

    #[test]
    fn insufficient_bars_is_none() {
        let bars: Vec<Bar> = (0..5).map(|_| bar(100.0, 101.0, 99.0, 100.0)).collect();
        assert!(classify_regime(&bars, 14, 20).is_none());
    }

    #[test]
    fn efficiency_ratio_half_on_one_retrace() {
        // up 10 then down 5: net 5, path 15 → ER = 1/3.
        let bars = vec![
            bar(100.0, 100.0, 100.0, 100.0),
            bar(110.0, 110.0, 110.0, 110.0),
            bar(105.0, 105.0, 105.0, 105.0),
        ];
        let er = efficiency_ratio(&bars, 2).unwrap();
        assert!((er - (5.0 / 15.0)).abs() < 1e-9);
    }
}
