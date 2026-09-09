//! Stateless trend persistence: R², efficiency, ADX strength/slope and fractal dimension.
//! Fixed 34-bar sensors, ADX 14, and a seeded EMA over the final five composite samples.
//! This preserves the snapshot model; it is not the streaming trend-quality indicator or a probability.
//! Classification has no state hysteresis.

use crate::Bar;
#[cfg(feature = "serde")]
use serde::Serialize;

use super::efficiency_ratio;
use crate::indicator::adx::Adx;
use crate::Indicator;

const LEN: usize = 34;
const ADX_LEN: usize = 14;
const SMOOTH_LEN: usize = 5;

const WEIGHT_R2: f64 = 40.0;
const WEIGHT_ER: f64 = 25.0;
const WEIGHT_ADX: f64 = 20.0;
const WEIGHT_FDI: f64 = 15.0;

const ADX_LO: f64 = 12.0;
const ADX_HI: f64 = 35.0;
const FDI_TREND_ANCHOR: f64 = 1.20;
const FDI_RANGE_ANCHOR: f64 = 1.65;

const DEAD_THRESH: f64 = 20.0;
const ADX_DAMP: f64 = 0.35;

const LVL_STRONG: f64 = 75.0;
const LVL_HEALTHY: f64 = 60.0;
const LVL_TRANS: f64 = 45.0;
const LVL_WEAK: f64 = 30.0;

const DIRECTION_THRESHOLD: f64 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TrendPersistenceState {
    Strong,
    Healthy,
    Transition,
    Weak,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TrendPersistenceDirection {
    Up,
    Down,
    Flat,
}

/// Which of the four sensors currently drives (`driver`) or drags down
/// (`drag`) the composite score — the original's "why did this change"
/// explanation, without the log-on-state-change machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TrendPersistenceSensor {
    Regression,
    Efficiency,
    Adx,
    Fractal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct TrendPersistenceReading {
    /// The composite score, 0..100 — higher means a cleaner, more durable
    /// trend (in either direction).
    pub score: f64,
    pub state: TrendPersistenceState,
    /// Correlation-sign direction — visual context only, not scored.
    pub direction: TrendPersistenceDirection,
    /// Derived inverse of `score` blended with the Efficiency sensor: how
    /// exposed the current trend is to breaking down, not a break detector.
    pub transition_risk: f64,
    pub r2_score: f64,
    pub er_score: f64,
    pub adx_score: f64,
    pub fdi_score: f64,
    pub driver: TrendPersistenceSensor,
    pub drag: TrendPersistenceSensor,
}

use crate::indicator::smoothing::Ema as SeededEma;

fn normalize(x: f64, lo: f64, hi: f64) -> f64 {
    if hi == lo {
        return 0.0;
    }
    ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Pearson correlation between `closes` and its own bar index (0..len).
fn correlation_with_index(closes: &[f64]) -> f64 {
    let n = closes.len() as f64;
    let mean_y = (n - 1.0) / 2.0; // mean of 0..n-1
    let mean_x = closes.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (i, &x) in closes.iter().enumerate() {
        let dx = x - mean_x;
        let dy = i as f64 - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    if var_x <= 0.0 || var_y <= 0.0 {
        return 0.0;
    }
    cov / (var_x.sqrt() * var_y.sqrt())
}

struct SubScores {
    r2_score: f64,
    er_score: f64,
    fdi_score: f64,
    corr: f64,
}

/// R², Efficiency Ratio and Fractal Dimension sensors at `bars[idx]`, each
/// over its own trailing `LEN`-bar window. Requires `idx >= LEN`.
fn sub_scores_at(bars: &[Bar], idx: usize) -> SubScores {
    let window = &bars[idx - LEN..=idx]; // LEN+1 bars
    let closes: Vec<f64> = window.iter().map(|b| b.close).collect();

    let corr = correlation_with_index(&closes[1..]); // last LEN closes
    let r2_score = corr * corr * 100.0;

    let er_score = efficiency_ratio(&bars[..=idx], LEN).unwrap_or(0.0) * 100.0;

    let highest_high = window
        .iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let lowest_low = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
    let range_hl = highest_high - lowest_low;
    let path_length: f64 = closes.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
    let fdi_raw = if range_hl > 0.0 && path_length > 0.0 {
        (path_length / range_hl).ln() / (LEN as f64).ln() + 1.0
    } else {
        1.5
    };
    let fdi_score = (1.0 - normalize(fdi_raw, FDI_TREND_ANCHOR, FDI_RANGE_ANCHOR)) * 100.0;

    SubScores {
        r2_score,
        er_score,
        fdi_score,
        corr,
    }
}

/// ADX strength+slope sub-score (undamped — the structure-dead damp depends
/// on the R²/ER sensors, computed separately at each needed index) for every
/// bar, via the already-ported `Adx` indicator fed from the start of `bars`.
/// `None` until the indicator has warmed up *and* a previous ADX value
/// exists to derive the slope from.
fn adx_score_series(bars: &[Bar]) -> Vec<Option<f64>> {
    let mut adx = Adx::new(ADX_LEN, ADX_LEN, 3, 20.0);
    let mut slope_ema = SeededEma::new(3);
    let mut prev_adx: Option<f64> = None;
    let mut out = vec![None; bars.len()];

    for (i, bar) in bars.iter().enumerate() {
        let Some(output) = adx.on_bar(bar) else {
            continue;
        };
        let raw = output.value;
        if let Some(prev) = prev_adx {
            // Default first-sample seed: this emits from the first slope on. The `else` branch
            // skips the bar rather than substituting a slope that was never observed.
            let Some(slope) = slope_ema.update(raw - prev) else {
                prev_adx = Some(raw);
                continue;
            };
            let adx_strength = normalize(raw, ADX_LO, ADX_HI);
            let adx_slope_norm = normalize(slope, -1.0, 1.5);
            out[i] = Some((adx_strength * 0.7 + adx_slope_norm * 0.3) * 100.0);
        }
        prev_adx = Some(raw);
    }
    out
}

fn classify_state(score: f64) -> TrendPersistenceState {
    if score >= LVL_STRONG {
        TrendPersistenceState::Strong
    } else if score >= LVL_HEALTHY {
        TrendPersistenceState::Healthy
    } else if score >= LVL_TRANS {
        TrendPersistenceState::Transition
    } else if score >= LVL_WEAK {
        TrendPersistenceState::Weak
    } else {
        TrendPersistenceState::Dead
    }
}

fn driver_and_drag(
    r2_score: f64,
    er_score: f64,
    adx_score: f64,
    fdi_score: f64,
) -> (TrendPersistenceSensor, TrendPersistenceSensor) {
    use TrendPersistenceSensor::*;
    let scores = [
        (Regression, r2_score),
        (Efficiency, er_score),
        (Adx, adx_score),
        (Fractal, fdi_score),
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

/// Reads the current trend-persistence state from `bars` (oldest first,
/// current bar last). `None` until enough bars exist to seed the four
/// sensors' `LEN`-bar window, the `SMOOTH_LEN`-bar composite smoothing, and
/// the `Adx` indicator's own warmup.
pub fn trend_persistence_reading(bars: &[Bar]) -> Option<TrendPersistenceReading> {
    if bars.len() < LEN + SMOOTH_LEN {
        return None;
    }
    let adx_scores = adx_score_series(bars);
    let last = bars.len() - 1;

    let mut smoother = SeededEma::new(SMOOTH_LEN);
    let mut score = 0.0;
    let mut latest: Option<SubScores> = None;
    let mut latest_adx_score = 0.0;

    #[allow(clippy::needless_range_loop)]
    for idx in (last + 1 - SMOOTH_LEN)..=last {
        if idx < LEN {
            return None;
        }
        let adx_score_undamped = adx_scores[idx]?;
        let sub = sub_scores_at(bars, idx);
        let structure_dead = sub.r2_score < DEAD_THRESH && sub.er_score < DEAD_THRESH;
        let adx_score = if structure_dead {
            adx_score_undamped * ADX_DAMP
        } else {
            adx_score_undamped
        };
        let weight_sum = WEIGHT_R2 + WEIGHT_ER + WEIGHT_ADX + WEIGHT_FDI;
        let raw = (sub.r2_score * WEIGHT_R2
            + sub.er_score * WEIGHT_ER
            + adx_score * WEIGHT_ADX
            + sub.fdi_score * WEIGHT_FDI)
            / weight_sum;
        score = smoother.update(raw)?;
        if idx == last {
            latest_adx_score = adx_score;
            latest = Some(sub);
        }
    }
    let latest = latest.expect("loop always visits idx == last");

    let transition_risk = (100.0 - score) * 0.7 + (100.0 - latest.er_score) * 0.3;
    let state = classify_state(score);
    let direction = if latest.corr > DIRECTION_THRESHOLD {
        TrendPersistenceDirection::Up
    } else if latest.corr < -DIRECTION_THRESHOLD {
        TrendPersistenceDirection::Down
    } else {
        TrendPersistenceDirection::Flat
    };
    let (driver, drag) = driver_and_drag(
        latest.r2_score,
        latest.er_score,
        latest_adx_score,
        latest.fdi_score,
    );

    Some(TrendPersistenceReading {
        score,
        state,
        direction,
        transition_risk,
        r2_score: latest.r2_score,
        er_score: latest.er_score,
        adx_score: latest_adx_score,
        fdi_score: latest.fdi_score,
        driver,
        drag,
    })
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

    /// Deterministic jitter around a level, same construction as
    /// `fear_gauge.rs`'s `calm_bars` — avoids a perfectly flat degenerate
    /// series while staying non-trending.
    fn choppy_bars(n: usize) -> Vec<Bar> {
        let mut state: u64 = 42;
        let mut price = 100.0_f64;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                price = 100.0 + ((state % 21) as f64 - 10.0) / 5.0;
                bar(price, price + 0.3, price - 0.3)
            })
            .collect()
    }

    fn ramp_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let c = 100.0 + i as f64 * 0.5;
                bar(c, c + 0.2, c - 0.2)
            })
            .collect()
    }

    #[test]
    fn insufficient_bars_is_none() {
        let bars = ramp_bars(10);
        assert!(trend_persistence_reading(&bars).is_none());
    }

    #[test]
    fn clean_ramp_reads_high_persistence() {
        let bars = ramp_bars(120);
        let r = trend_persistence_reading(&bars).expect("enough bars");
        assert!(r.score >= LVL_HEALTHY, "score should be high: {}", r.score);
        assert_eq!(r.direction, TrendPersistenceDirection::Up);
    }

    #[test]
    fn choppy_market_reads_low_persistence() {
        let bars = choppy_bars(120);
        let r = trend_persistence_reading(&bars).expect("enough bars");
        assert!(r.score < LVL_TRANS, "score should be low: {}", r.score);
    }

    #[test]
    fn state_thresholds_are_monotonic() {
        assert_eq!(classify_state(80.0), TrendPersistenceState::Strong);
        assert_eq!(classify_state(65.0), TrendPersistenceState::Healthy);
        assert_eq!(classify_state(50.0), TrendPersistenceState::Transition);
        assert_eq!(classify_state(35.0), TrendPersistenceState::Weak);
        assert_eq!(classify_state(10.0), TrendPersistenceState::Dead);
    }
}
