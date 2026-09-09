//! Pairwise return correlation and relative-strength ranking over close samples.
//! Inputs must be finite, chronological, and use a common timestamp convention.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::Serialize;

/// Timestamp is an opaque alignment key. All series must use the same unit and bar boundary.
#[derive(Debug, Clone, Copy)]
pub struct CloseSample {
    pub timestamp: i64,
    pub close: f64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CorrelationCell {
    pub a: String,
    pub b: String,
    /// Pearson correlation of the two instruments' aligned percentage returns,
    /// in −1..1.
    pub corr: f64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct RelativeStrength {
    pub instrument: String,
    /// Percentage change of close over the lookback window.
    pub change_pct: f64,
}

/// Pairwise return correlation across `series` (each `(epic, bars)`, bars
/// oldest first). Pairs are aligned on their common bar timestamps — FX /
/// commodity instruments share the same session grid but can differ at the
/// edges (gaps, differing seed depth), so aligning on `ts` avoids correlating
/// misaligned rows. `lookback` caps how many of the most recent aligned
/// returns are used. A pair with fewer than 3 common returns is omitted.
/// For compatibility, lookbacks below 3 leave the full aligned history in use.
pub fn correlation_matrix(
    series: &[(String, Vec<CloseSample>)],
    lookback: usize,
) -> Vec<CorrelationCell> {
    // Pre-index each instrument's closes by timestamp once.
    let closes: Vec<(&str, BTreeMap<i64, f64>)> = series
        .iter()
        .map(|(epic, bars)| {
            (
                epic.as_str(),
                bars.iter()
                    .map(|b| (b.timestamp, b.close))
                    .collect::<BTreeMap<_, _>>(),
            )
        })
        .collect();

    let mut out = Vec::new();
    for i in 0..closes.len() {
        for j in (i + 1)..closes.len() {
            let (epic_a, map_a) = &closes[i];
            let (epic_b, map_b) = &closes[j];
            if let Some(corr) = pair_correlation(map_a, map_b, lookback) {
                out.push(CorrelationCell {
                    a: epic_a.to_string(),
                    b: epic_b.to_string(),
                    corr,
                });
            }
        }
    }
    out
}

/// Ranks instruments by percentage change of close over the last `lookback`
/// bars (each `(epic, bars)`, bars oldest first), strongest first. Instruments
/// without enough bars are dropped.
pub fn relative_strength(
    series: &[(String, Vec<CloseSample>)],
    lookback: usize,
) -> Vec<RelativeStrength> {
    let mut out: Vec<RelativeStrength> = series
        .iter()
        .filter_map(|(epic, bars)| {
            if bars.len() < lookback + 1 || lookback == 0 {
                return None;
            }
            let last = bars[bars.len() - 1].close;
            let base = bars[bars.len() - 1 - lookback].close;
            if base == 0.0 {
                return None;
            }
            Some(RelativeStrength {
                instrument: epic.clone(),
                change_pct: 100.0 * (last - base) / base,
            })
        })
        .collect();
    out.sort_by(|a, b| b.change_pct.total_cmp(&a.change_pct));
    out
}

/// Pearson correlation of percentage returns over the common timestamps of
/// two close series, using at most the last `lookback` returns.
fn pair_correlation(
    a: &BTreeMap<i64, f64>,
    b: &BTreeMap<i64, f64>,
    lookback: usize,
) -> Option<f64> {
    // Common timestamps, ascending, with both closes.
    let common: Vec<(f64, f64)> = a
        .iter()
        .filter_map(|(ts, ca)| b.get(ts).map(|cb| (*ca, *cb)))
        .collect();
    if common.len() < 4 {
        return None; // < 3 returns
    }
    // Percentage returns from consecutive common closes.
    let mut ra = Vec::with_capacity(common.len() - 1);
    let mut rb = Vec::with_capacity(common.len() - 1);
    for pair in common.windows(2) {
        let (a0, b0) = pair[0];
        let (a1, b1) = pair[1];
        if a0 == 0.0 || b0 == 0.0 {
            continue;
        }
        ra.push((a1 - a0) / a0);
        rb.push((b1 - b0) / b0);
    }
    if ra.len() < 3 {
        return None;
    }
    if ra.len() > lookback && lookback >= 3 {
        let start = ra.len() - lookback;
        ra.drain(0..start);
        rb.drain(0..start);
    }
    pearson(&ra, &rb)
}

fn pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (xi, yi) in x.iter().zip(y) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    let denom = (var_x * var_y).sqrt();
    if denom <= 0.0 {
        return None; // a flat series has no defined correlation
    }
    Some((cov / denom).clamp(-1.0, 1.0))
}

/// Evaluated rolling beta, alpha, and regression fit of an asset against a benchmark series.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct RollingBetaResult {
    /// Covariance(asset, benchmark) / Variance(benchmark).
    pub beta: f64,
    /// Mean(asset) - beta * Mean(benchmark).
    pub alpha: f64,
    /// Coefficient of determination (R^2), in 0.0..=1.0.
    pub r_squared: f64,
    /// Correlation between asset and benchmark returns.
    pub correlation: f64,
    /// Number of aligned return periods evaluated.
    pub periods: usize,
}

/// Computes rolling beta, alpha, and R^2 of `asset_returns` against `benchmark_returns`.
///
/// Both slices must have the same length and at least 3 observations.
pub fn compute_rolling_beta(
    asset_returns: &[f64],
    benchmark_returns: &[f64],
) -> Option<RollingBetaResult> {
    if asset_returns.len() != benchmark_returns.len() || asset_returns.len() < 3 {
        return None;
    }

    let n = asset_returns.len() as f64;
    let mean_a = asset_returns.iter().sum::<f64>() / n;
    let mean_b = benchmark_returns.iter().sum::<f64>() / n;

    let mut cov = 0.0f64;
    let mut var_b = 0.0f64;
    let mut var_a = 0.0f64;

    for (&ra, &rb) in asset_returns.iter().zip(benchmark_returns) {
        if !ra.is_finite() || !rb.is_finite() {
            return None;
        }
        let da = ra - mean_a;
        let db = rb - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }

    if var_b <= 1e-14 {
        // Benchmark is flat; beta is undefined
        return None;
    }

    let beta = cov / var_b;
    let alpha = mean_a - beta * mean_b;

    let denom = (var_a * var_b).sqrt();
    let correlation = if denom > 1e-14 {
        (cov / denom).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let r_squared = (correlation * correlation).clamp(0.0, 1.0);

    Some(RollingBetaResult {
        beta,
        alpha,
        r_squared,
        correlation,
        periods: asset_returns.len(),
    })
}

/// A snapshot observation of a constituent member within a market universe at an as-of timestamp.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct UniverseMemberObservation {
    pub symbol: String,
    /// Return over the relevant period (e.g. 1-day return).
    pub period_return: f64,
    /// Current price level.
    pub current_price: f64,
    /// Moving average reference level (e.g. SMA 20 or SMA 50), if available.
    pub ma_reference: Option<f64>,
}

/// Cross-sectional market breadth summary computed across active universe members.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct MarketBreadthSnapshot {
    /// Total number of active constituents with valid data at this as-of timestamp.
    pub total_active: usize,
    /// Number of advancing constituents (period_return > 0).
    pub advancing: usize,
    /// Number of declining constituents (period_return < 0).
    pub declining: usize,
    /// Number of unchanged constituents (period_return == 0).
    pub unchanged: usize,
    /// Ratio of advancing to declining constituents: `advancing / max(1, declining)`.
    pub advance_decline_ratio: f64,
    /// Net advance percentage: `(advancing - declining) / total_active`.
    pub net_advancing_pct: f64,
    /// Percentage of constituents trading above their moving average reference level, in 0.0..=1.0.
    pub pct_above_ma: Option<f64>,
}

/// Computes cross-sectional market breadth across active universe constituents.
///
/// Missing or inactive constituents are excluded, correctly adjusting the denominator.
pub fn compute_market_breadth(
    members: &[UniverseMemberObservation],
) -> Option<MarketBreadthSnapshot> {
    if members.is_empty() {
        return None;
    }

    let mut advancing = 0usize;
    let mut declining = 0usize;
    let mut unchanged = 0usize;
    let mut above_ma_count = 0usize;
    let mut total_with_ma = 0usize;

    for m in members {
        if !m.period_return.is_finite() || !m.current_price.is_finite() {
            continue;
        }
        if m.period_return > 1e-9 {
            advancing += 1;
        } else if m.period_return < -1e-9 {
            declining += 1;
        } else {
            unchanged += 1;
        }

        if let Some(ma) = m.ma_reference {
            if ma.is_finite() && ma > 0.0 {
                total_with_ma += 1;
                if m.current_price > ma {
                    above_ma_count += 1;
                }
            }
        }
    }

    let total_active = advancing + declining + unchanged;
    if total_active == 0 {
        return None;
    }

    let advance_decline_ratio = advancing as f64 / (declining.max(1) as f64);
    let net_advancing_pct = (advancing as f64 - declining as f64) / total_active as f64;
    let pct_above_ma = if total_with_ma > 0 {
        Some(above_ma_count as f64 / total_with_ma as f64)
    } else {
        None
    };

    Some(MarketBreadthSnapshot {
        total_active,
        advancing,
        declining,
        unchanged,
        advance_decline_ratio,
        net_advancing_pct,
        pct_above_ma,
    })
}

/// Evaluated descriptive pair spread, hedge ratio, and residual z-score.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct PairSpreadResult {
    /// OLS hedge ratio $\gamma$ (Asset A price vs. Asset B price).
    pub hedge_ratio: f64,
    /// Latest spread level: $P_A - \gamma \cdot P_B$.
    pub current_spread: f64,
    /// Historical mean of the spread over the window.
    pub mean_spread: f64,
    /// Historical standard deviation of the spread.
    pub std_spread: f64,
    /// Current normalized residual z-score: `(current_spread - mean_spread) / std_spread`.
    pub residual_z_score: f64,
}

/// Computes descriptive pair spread, OLS hedge ratio, and current residual z-score between two price series.
pub fn compute_pair_spread(prices_a: &[f64], prices_b: &[f64]) -> Option<PairSpreadResult> {
    if prices_a.len() != prices_b.len() || prices_a.len() < 3 {
        return None;
    }

    let n = prices_a.len() as f64;
    let mean_a = prices_a.iter().sum::<f64>() / n;
    let mean_b = prices_b.iter().sum::<f64>() / n;

    let mut cov = 0.0f64;
    let mut var_b = 0.0f64;

    for (&pa, &pb) in prices_a.iter().zip(prices_b) {
        if !pa.is_finite() || !pb.is_finite() {
            return None;
        }
        let da = pa - mean_a;
        let db = pb - mean_b;
        cov += da * db;
        var_b += db * db;
    }

    if var_b <= 1e-14 {
        return None;
    }

    let hedge_ratio = cov / var_b;

    // Compute spread series: S = P_a - hedge_ratio * P_b
    let mut spread_series = Vec::with_capacity(prices_a.len());
    let mut sum_spread = 0.0f64;
    for (&pa, &pb) in prices_a.iter().zip(prices_b) {
        let s = pa - hedge_ratio * pb;
        spread_series.push(s);
        sum_spread += s;
    }

    let mean_spread = sum_spread / n;
    let mut var_spread = 0.0f64;
    for &s in &spread_series {
        var_spread += (s - mean_spread).powi(2);
    }
    let std_spread = (var_spread / n).sqrt();

    let current_spread = *spread_series.last()?;
    let residual_z_score = if std_spread > 1e-14 {
        (current_spread - mean_spread) / std_spread
    } else {
        0.0
    };

    Some(PairSpreadResult {
        hedge_ratio,
        current_spread,
        mean_spread,
        std_spread,
        residual_z_score,
    })
}

/// Pairwise correlation between two indicator/signal time series to detect collinearity and double counting.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SignalCorrelationCell {
    pub signal_a: String,
    pub signal_b: String,
    /// Pearson correlation of the two signal score series, in -1.0..=1.0.
    pub correlation: f64,
    /// True if absolute correlation exceeds the redundancy threshold (e.g. >= 0.85).
    pub is_redundant: bool,
}

/// Computes pairwise signal score correlations across multiple named signal output streams.
pub fn compute_signal_correlation_matrix(
    signals: &[(String, Vec<f64>)],
    redundancy_threshold: f64,
) -> Vec<SignalCorrelationCell> {
    let mut out = Vec::new();
    let num_signals = signals.len();

    for i in 0..num_signals {
        for j in (i + 1)..num_signals {
            let (name_a, scores_a) = &signals[i];
            let (name_b, scores_b) = &signals[j];

            let min_len = scores_a.len().min(scores_b.len());
            if min_len < 3 {
                continue;
            }

            if let Some(corr) = pearson(&scores_a[..min_len], &scores_b[..min_len]) {
                out.push(SignalCorrelationCell {
                    signal_a: name_a.clone(),
                    signal_b: name_b.clone(),
                    correlation: corr,
                    is_redundant: corr.abs() >= redundancy_threshold,
                });
            }
        }
    }

    out
}
