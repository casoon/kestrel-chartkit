//! Smoothed price direction combined with an existing trend/range vote.

#[cfg(feature = "serde")]
use serde::Serialize;

use super::regime::{RegimeReading, RegimeState};
use crate::indicator::smoothing::SmootherKind;
use crate::indicator::smoothing::{Jma, Smoother};
use crate::Bar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TrendDirection {
    Up,
    Flat,
    Down,
}

/// Five-step market phase. "Strong" means the regime vote also calls it `Trending`, not
/// just that direction is non-flat — a directional move inside a `Ranging`
/// read (still choppy, just currently drifting) is the plain `Up`/`Down`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MarketPhase {
    StrongUp,
    Up,
    Range,
    Down,
    StrongDown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct TrendReading {
    pub direction: TrendDirection,
    pub phase: MarketPhase,
    /// Percent change of the smoothed price over `len` bars.
    pub slope_pct: f64,
}

/// Direction from a smoothed price's slope over `len` bars (`kind`/
/// `jma_phase`/`jma_power` select the kernel), combined with
/// `regime`'s trend/range vote into a 5-step phase. `deadband_pct` is the
/// minimum `|slope_pct|` to call it `Up`/`Down` instead of `Flat` — keeps
/// noise from flipping direction every bar. `None` until the kernel has
/// published `len + 1` values, or for a zero reference value;
/// `slope_pct = 100 · (s_last / s_(last-len) - 1)` over those published values.
#[allow(clippy::too_many_arguments)]
pub fn trend_reading(
    bars: &[Bar],
    regime: &RegimeReading,
    kind: SmootherKind,
    len: usize,
    jma_phase: f64,
    jma_power: f64,
    deadband_pct: f64,
) -> Option<TrendReading> {
    if len < 1 {
        return None;
    }
    let series = smoothed_series(bars, kind, len, jma_phase, jma_power);
    if series.len() < len + 1 {
        return None;
    }
    let last = series[series.len() - 1];
    let prior = series[series.len() - 1 - len];
    if prior == 0.0 {
        return None;
    }
    let slope_pct = 100.0 * (last / prior - 1.0);
    let direction = if slope_pct > deadband_pct {
        TrendDirection::Up
    } else if slope_pct < -deadband_pct {
        TrendDirection::Down
    } else {
        TrendDirection::Flat
    };

    let phase = match direction {
        TrendDirection::Flat => MarketPhase::Range,
        TrendDirection::Up if regime.state == RegimeState::Trending => MarketPhase::StrongUp,
        TrendDirection::Up => MarketPhase::Up,
        TrendDirection::Down if regime.state == RegimeState::Trending => MarketPhase::StrongDown,
        TrendDirection::Down => MarketPhase::Down,
    };

    Some(TrendReading {
        direction,
        phase,
        slope_pct,
    })
}

/// Maps `SmootherKind` to a `kestrel-chartkit` `Smoother` instance. Not a plain
/// `crate::indicator::smoothing::SmootherKind::build(len)` call: chartkit's generic
/// `build` bakes in the `Jma` defaults (`phase = 0.0`, `power = 2.0`) since it has no way to
/// take extra parameters — this function's callers (config-driven, `jma_phase`/`jma_power` are
/// user-configurable) construct `Jma` directly for that case instead.
fn build_smoother(
    kind: SmootherKind,
    len: usize,
    jma_phase: f64,
    jma_power: f64,
) -> Box<dyn Smoother> {
    match kind {
        SmootherKind::Jma => Box::new(Jma::new(len, jma_phase, jma_power)),
        other => other.build(len),
    }
}

/// Feeds every bar's close through the chosen kernel, keeping only the
/// values past its own warmup (matches `Smoother::update`'s `None`-while-
/// warming-up contract).
fn smoothed_series(
    bars: &[Bar],
    kind: SmootherKind,
    len: usize,
    jma_phase: f64,
    jma_power: f64,
) -> Vec<f64> {
    let mut smoother = build_smoother(kind, len, jma_phase, jma_power);
    bars.iter()
        .filter_map(|b| smoother.update(b.close))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(c: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: c,
            high: c + 0.5,
            low: c - 0.5,
            close: c,
            volume: 0.0,
        }
    }

    fn trending_regime() -> RegimeReading {
        RegimeReading {
            state: RegimeState::Trending,
            adx: 30.0,
            choppiness: 30.0,
            efficiency: 0.8,
            trend_votes: 3,
        }
    }

    fn ranging_regime() -> RegimeReading {
        RegimeReading {
            state: RegimeState::Ranging,
            adx: 15.0,
            choppiness: 70.0,
            efficiency: 0.2,
            trend_votes: 0,
        }
    }

    #[test]
    fn ramp_with_trending_regime_is_strong_up() {
        let bars: Vec<Bar> = (0..60).map(|i| bar(100.0 + i as f64)).collect();
        let r = trend_reading(
            &bars,
            &trending_regime(),
            SmootherKind::Ema,
            20,
            0.0,
            2.0,
            0.1,
        )
        .expect("enough bars");
        assert_eq!(r.direction, TrendDirection::Up);
        assert_eq!(r.phase, MarketPhase::StrongUp);
        assert!(r.slope_pct > 0.0);
    }

    #[test]
    fn ramp_with_ranging_regime_is_weak_up() {
        let bars: Vec<Bar> = (0..60).map(|i| bar(100.0 + i as f64)).collect();
        let r = trend_reading(
            &bars,
            &ranging_regime(),
            SmootherKind::Ema,
            20,
            0.0,
            2.0,
            0.1,
        )
        .expect("enough bars");
        assert_eq!(r.direction, TrendDirection::Up);
        assert_eq!(r.phase, MarketPhase::Up);
    }

    #[test]
    fn flat_price_is_range_regardless_of_regime() {
        let bars: Vec<Bar> = (0..60).map(|_| bar(100.0)).collect();
        let r = trend_reading(
            &bars,
            &trending_regime(),
            SmootherKind::Ema,
            20,
            0.0,
            2.0,
            0.1,
        )
        .expect("enough bars");
        assert_eq!(r.direction, TrendDirection::Flat);
        assert_eq!(r.phase, MarketPhase::Range);
    }

    #[test]
    fn downtrend_with_trending_regime_is_strong_down() {
        let bars: Vec<Bar> = (0..60).map(|i| bar(200.0 - i as f64)).collect();
        let r = trend_reading(
            &bars,
            &trending_regime(),
            SmootherKind::Ema,
            20,
            0.0,
            2.0,
            0.1,
        )
        .expect("enough bars");
        assert_eq!(r.direction, TrendDirection::Down);
        assert_eq!(r.phase, MarketPhase::StrongDown);
    }

    #[test]
    fn insufficient_bars_is_none() {
        let bars: Vec<Bar> = (0..5).map(|_| bar(100.0)).collect();
        assert!(trend_reading(
            &bars,
            &trending_regime(),
            SmootherKind::Ema,
            20,
            0.0,
            2.0,
            0.1
        )
        .is_none());
    }

    #[test]
    fn jma_kernel_also_produces_a_reading() {
        let bars: Vec<Bar> = (0..60).map(|i| bar(100.0 + i as f64)).collect();
        let r = trend_reading(
            &bars,
            &trending_regime(),
            SmootherKind::Jma,
            20,
            0.0,
            2.0,
            0.1,
        )
        .expect("jma is valid from the first sample");
        assert_eq!(r.direction, TrendDirection::Up);
    }
}
