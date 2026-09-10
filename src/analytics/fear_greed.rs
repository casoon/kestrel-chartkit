//! Heuristic instrument sentiment from explicit component readings.
//! Missing readings contribute neutral 50; score is not a calibrated probability.

#[cfg(feature = "serde")]
use serde::Serialize;

use super::{
    FearGaugeReading, FearGaugeState, PriceSummary, RegimeReading, RegimeState,
    TrendPersistenceReading,
};
use crate::Bar;

const FLOW_WINDOW: usize = 34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FearGreedState {
    ExtremeFear,
    Fear,
    Neutral,
    Greed,
    ExtremeGreed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FearGreedDriver {
    FearGauge,
    Volatility,
    Flow,
    Persistence,
    Regime,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct FearGreedReading {
    /// Composite 0..100: low means stress/fear, high means greed or
    /// complacency. Components are exposed so the UI can explain the score.
    pub score: f64,
    pub state: FearGreedState,
    pub fear_gauge_score: f64,
    pub volatility_score: f64,
    pub flow_score: f64,
    pub persistence_score: f64,
    pub regime_score: f64,
    pub driver: FearGreedDriver,
    pub drag: FearGreedDriver,
}

/// Instrument sentiment `0..=100` from five component scores, each `0..=100` and 50 while its
/// reading is missing:
///
/// - fear gauge (weight 0.30): 15 in a fear spike, 85 in a complacency spike, otherwise
///   `50 + 1.25 · clamp(bwvf - wvf, -20, 20)`; an absorbed spike is pulled halfway to 50;
/// - volatility (0.20): `100 · (1 - atr_percentile)`;
/// - flow (0.20), always from `bars`: `50 · (m + 1)`, `m` the mean close location value
///   `((close - low) - (high - close)) / (high - low)`, clamped to `-1..=1`, over the last 34
///   bars, weighted by volume, or equally when no bar in that window has volume; bars without
///   range or weight are skipped, and it is 50 when none remain;
/// - trend persistence (0.20): its `score`;
/// - regime (0.10): `55 + 25 · votes / 3` when trending, `45 + 15 · votes / 3` when ranging.
///
/// `score` is the weighted sum, clamped to `0..=100`. States: extreme fear below 25, fear below
/// 45, neutral below 55, greed below 75, else extreme greed. `driver`/`drag` name the highest and
/// lowest component; on a tie the driver is the later and the drag the earlier in the order
/// above. `None` for fewer than 2 bars.
pub fn fear_greed_reading(
    bars: &[Bar],
    regime: Option<&RegimeReading>,
    price: Option<&PriceSummary>,
    fear_gauge: Option<&FearGaugeReading>,
    trend_persistence: Option<&TrendPersistenceReading>,
) -> Option<FearGreedReading> {
    if bars.len() < 2 {
        return None;
    }

    let fear_gauge_score = fear_gauge.map_or(50.0, fear_gauge_component);
    let volatility_score = price.map_or(50.0, |p| 100.0 * (1.0 - p.atr_percentile));
    let flow_score = flow_component(bars)?;
    let persistence_score = trend_persistence.map_or(50.0, |t| t.score);
    let regime_score = regime.map_or(50.0, regime_component);

    let score = (fear_gauge_score * 0.30
        + volatility_score * 0.20
        + flow_score * 0.20
        + persistence_score * 0.20
        + regime_score * 0.10)
        .clamp(0.0, 100.0);
    let state = classify(score);
    let (driver, drag) = driver_and_drag(
        fear_gauge_score,
        volatility_score,
        flow_score,
        persistence_score,
        regime_score,
    );

    Some(FearGreedReading {
        score,
        state,
        fear_gauge_score,
        volatility_score,
        flow_score,
        persistence_score,
        regime_score,
        driver,
        drag,
    })
}

fn fear_gauge_component(f: &FearGaugeReading) -> f64 {
    let base = match f.state {
        FearGaugeState::FearSpike => 15.0,
        FearGaugeState::ComplacencySpike => 85.0,
        FearGaugeState::Neutral => {
            let spread = (f.bwvf - f.wvf).clamp(-20.0, 20.0);
            50.0 + spread * 1.25
        }
    };
    if f.absorbed {
        (base + 50.0) / 2.0
    } else {
        base
    }
}

fn regime_component(r: &RegimeReading) -> f64 {
    let trendiness = r.trend_votes as f64 / 3.0;
    match r.state {
        RegimeState::Trending => 55.0 + trendiness * 25.0,
        RegimeState::Ranging => 45.0 + trendiness * 15.0,
    }
}

fn flow_component(bars: &[Bar]) -> Option<f64> {
    let start = bars.len().saturating_sub(FLOW_WINDOW);
    let window = &bars[start..];
    if window.is_empty() {
        return None;
    }

    let use_volume = window.iter().any(|b| b.volume > 0.0);
    let mut weighted = 0.0;
    let mut weight_sum = 0.0;
    for bar in window {
        let span = bar.high - bar.low;
        if span <= 0.0 {
            continue;
        }
        let clv = (((bar.close - bar.low) - (bar.high - bar.close)) / span).clamp(-1.0, 1.0);
        let weight = if use_volume { bar.volume.max(0.0) } else { 1.0 };
        if weight == 0.0 {
            continue;
        }
        weighted += clv * weight;
        weight_sum += weight;
    }
    if weight_sum == 0.0 {
        return Some(50.0);
    }
    Some(((weighted / weight_sum) + 1.0) * 50.0)
}

fn classify(score: f64) -> FearGreedState {
    if score < 25.0 {
        FearGreedState::ExtremeFear
    } else if score < 45.0 {
        FearGreedState::Fear
    } else if score < 55.0 {
        FearGreedState::Neutral
    } else if score < 75.0 {
        FearGreedState::Greed
    } else {
        FearGreedState::ExtremeGreed
    }
}

fn driver_and_drag(
    fear_gauge: f64,
    volatility: f64,
    flow: f64,
    persistence: f64,
    regime: f64,
) -> (FearGreedDriver, FearGreedDriver) {
    use FearGreedDriver::*;
    let scores = [
        (FearGauge, fear_gauge),
        (Volatility, volatility),
        (Flow, flow),
        (Persistence, persistence),
        (Regime, regime),
    ];
    let driver = scores
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("scores is non-empty")
        .0;
    let drag = scores
        .iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("scores is non-empty")
        .0;
    (driver, drag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(c: f64, h: f64, l: f64, volume: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: c,
            high: h,
            low: l,
            close: c,
            volume,
        }
    }

    #[test]
    fn close_near_high_pushes_flow_toward_greed() {
        let bars: Vec<Bar> = (0..40).map(|_| bar(109.0, 110.0, 100.0, 10.0)).collect();
        let r = fear_greed_reading(&bars, None, None, None, None).expect("reading");
        assert!(r.flow_score > 80.0);
        assert!(r.score > 50.0);
    }

    #[test]
    fn fear_spike_pulls_score_down() {
        let bars: Vec<Bar> = (0..40).map(|_| bar(101.0, 110.0, 100.0, 10.0)).collect();
        let fear = FearGaugeReading {
            state: FearGaugeState::FearSpike,
            wvf: 20.0,
            bwvf: 1.0,
            absorbed: false,
        };
        let r = fear_greed_reading(&bars, None, None, Some(&fear), None).expect("reading");
        assert!(r.fear_gauge_score < 20.0);
        assert!(r.score < 50.0);
    }
}
