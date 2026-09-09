//! Dual Williams VIX Fix snapshot with population-deviation bands and an absorption heuristic.
//! Uses full 22-close windows, 20-sample bands and a simple trailing ATR for absorption.
//! It is distinct from the single-sided streaming VIX Fix indicator.

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::Bar;

const WVF_LEN: usize = 22;
const BAND_LEN: usize = 20;
const BAND_MULT: f64 = 2.0;
const STALL_LEN: usize = 5;
const STALL_FLAT_ATR: f64 = 0.5;
const STALL_WVF_MIN: f64 = 2.0;
const ATR_LEN: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FearGaugeState {
    /// `wvf` (sell-off gauge) is currently at/above its StdDev spike band —
    /// historically a bottom-proximity context, not a standalone signal.
    FearSpike,
    /// `bwvf` (the inverted, rally gauge) is currently at/above its band —
    /// top-proximity context.
    ComplacencySpike,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct FearGaugeReading {
    pub state: FearGaugeState,
    pub wvf: f64,
    pub bwvf: f64,
    /// True when the spike coincides with a near-flat price move over the
    /// last `STALL_LEN` bars — likely a thin wick rather than genuine
    /// fear/complacency (the "stall/absorption" heuristic). `false` when
    /// `state` is `Neutral`.
    pub absorbed: bool,
}

/// Reads the current fear/complacency state from `bars` (oldest first,
/// current bar last). `None` until enough bars exist to seed the WVF window
/// and its spike band (`WVF_LEN + BAND_LEN - 1`).
pub fn fear_gauge_reading(bars: &[Bar]) -> Option<FearGaugeReading> {
    if bars.len() < WVF_LEN + BAND_LEN - 1 {
        return None;
    }
    let last = bars.len() - 1;

    // wvf/bwvf for the trailing `BAND_LEN` bars — all the spike band's
    // rolling mean/stdev needs; each value requires its own `WVF_LEN`-bar
    // close window.
    let mut wvf_band = Vec::with_capacity(BAND_LEN);
    let mut bwvf_band = Vec::with_capacity(BAND_LEN);
    for i in (last + 1 - BAND_LEN)..=last {
        let (wvf, bwvf) = wvf_at(bars, i);
        wvf_band.push(wvf);
        bwvf_band.push(bwvf);
    }

    let wvf = *wvf_band.last().expect("band window is non-empty");
    let bwvf = *bwvf_band.last().expect("band window is non-empty");
    let bull_active = wvf >= spike_band(&wvf_band);
    let bear_active = bwvf >= spike_band(&bwvf_band);

    // Stall/absorption: WVF surged over `STALL_LEN` bars but price barely
    // moved — likely a thin wick, not genuine fear. Needs `STALL_LEN` bars
    // of wvf/close history before the current one.
    let absorbed = if last >= STALL_LEN + WVF_LEN - 1 {
        let (wvf_then, _) = wvf_at(bars, last - STALL_LEN);
        let close_then = bars[last - STALL_LEN].close;
        let atr = simple_atr(bars, last, ATR_LEN);
        let wvf_chg = wvf - wvf_then;
        let price_chg = atr
            .filter(|a| *a != 0.0)
            .map_or(0.0, |a| (bars[last].close - close_then).abs() / a);
        wvf_chg > STALL_WVF_MIN && price_chg < STALL_FLAT_ATR
    } else {
        false
    };

    let state = if bull_active {
        FearGaugeState::FearSpike
    } else if bear_active {
        FearGaugeState::ComplacencySpike
    } else {
        FearGaugeState::Neutral
    };

    Some(FearGaugeReading {
        state,
        wvf,
        bwvf,
        absorbed: state != FearGaugeState::Neutral && absorbed,
    })
}

/// `wvfRaw`/`bwvfRaw` at bar `i` — `(ta.highest(close, WVF_LEN) - low) /
/// ta.highest(close, WVF_LEN) * 100` and its lowest/high mirror. Requires
/// `i >= WVF_LEN - 1`.
fn wvf_at(bars: &[Bar], i: usize) -> (f64, f64) {
    let window = &bars[i + 1 - WVF_LEN..=i];
    let hc = window
        .iter()
        .map(|b| b.close)
        .fold(f64::NEG_INFINITY, f64::max);
    let lc = window.iter().map(|b| b.close).fold(f64::INFINITY, f64::min);
    let wvf = if hc != 0.0 {
        (hc - bars[i].low) / hc * 100.0
    } else {
        0.0
    };
    let bwvf = if lc != 0.0 {
        (bars[i].high - lc) / lc * 100.0
    } else {
        0.0
    };
    (wvf, bwvf)
}

/// `mean(x, BAND_LEN) + BAND_MULT * stdev(x, BAND_LEN)`, with the population (biased) standard
/// deviation.
fn spike_band(values: &[f64]) -> f64 {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    mean + BAND_MULT * variance.sqrt()
}

/// A plain trailing-average true range ending at bar `end` — a simplified
/// stand-in for a Wilder-smoothed ATR(14) seeded from the start of
/// history. Acceptable here: this only feeds the soft
/// stall/absorption qualifier on an already-display-only read-out, not a
/// numerically exact port target.
fn simple_atr(bars: &[Bar], end: usize, len: usize) -> Option<f64> {
    if end < len {
        return None;
    }
    let mut sum = 0.0;
    let mut prev_close = bars[end - len].close;
    for bar in &bars[end - len + 1..=end] {
        let tr = super::true_range(bar, Some(prev_close));
        sum += tr;
        prev_close = bar.close;
    }
    Some(sum / len as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(c: f64, h: f64, l: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: c,
            high: h,
            low: l,
            close: c,
            volume: 0.0,
        }
    }

    /// A calm market still has bar-to-bar jitter — a perfectly flat/constant
    /// close series is a degenerate edge case (zero-variance spike band, so
    /// `wvf >= band` trivially holds by equality) that never occurs on real
    /// bars, so this deliberately avoids it, same deterministic-jitter
    /// approach as `atr_parity.rs`'s `synthetic_bars`.
    fn calm_bars(n: usize) -> Vec<Bar> {
        let mut state: u64 = 12345;
        let mut price = 100.0_f64;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                price += ((state % 21) as f64 - 10.0) / 200.0; // +-0.05 jitter
                bar(price, price + 0.5, price - 0.5)
            })
            .collect()
    }

    #[test]
    fn insufficient_bars_is_none() {
        let bars = calm_bars(10);
        assert!(fear_gauge_reading(&bars).is_none());
    }

    #[test]
    fn calm_flat_market_reads_neutral() {
        let bars = calm_bars(60);
        let r = fear_gauge_reading(&bars).expect("enough bars");
        assert_eq!(r.state, FearGaugeState::Neutral);
    }

    /// A calm run followed by one sharp sell-off wick (low far below the
    /// recent close range, close recovers) is exactly what `wvf` is built
    /// to flag — it should spike above its own StdDev band.
    #[test]
    fn sharp_selloff_wick_reads_as_fear_spike() {
        let mut bars = calm_bars(59);
        bars.push(bar(100.0, 100.5, 80.0));
        let r = fear_gauge_reading(&bars).expect("enough bars");
        assert_eq!(r.state, FearGaugeState::FearSpike);
        assert!(r.wvf > r.bwvf);
    }

    /// The mirrored case: a sharp rally wick (high far above the recent
    /// close range) should flag the inverted `bwvf` gauge instead.
    #[test]
    fn sharp_rally_wick_reads_as_complacency_spike() {
        let mut bars = calm_bars(59);
        bars.push(bar(100.0, 120.0, 99.5));
        let r = fear_gauge_reading(&bars).expect("enough bars");
        assert_eq!(r.state, FearGaugeState::ComplacencySpike);
        assert!(r.bwvf > r.wvf);
    }
}
