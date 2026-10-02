//! Rolling statistics primitives for streaming series calculations.

/// Returns the sum of all finite values in `slice`.
pub fn rolling_sum(slice: &[f64]) -> f64 {
    slice.iter().copied().filter(|v| v.is_finite()).sum()
}

/// Returns the arithmetic mean of all finite values in `slice`.
pub fn rolling_mean(slice: &[f64]) -> f64 {
    let finite: Vec<f64> = slice.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        0.0
    } else {
        finite.iter().sum::<f64>() / finite.len() as f64
    }
}

/// Returns the population variance of finite values in `slice`.
pub fn rolling_variance(slice: &[f64]) -> f64 {
    let finite: Vec<f64> = slice.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.len() < 2 {
        return 0.0;
    }
    let mean = finite.iter().sum::<f64>() / finite.len() as f64;
    let var_sum: f64 = finite.iter().map(|v| (v - mean).powi(2)).sum();
    var_sum / finite.len() as f64
}

/// Returns the standard deviation of finite values in `slice`.
pub fn rolling_stddev(slice: &[f64]) -> f64 {
    rolling_variance(slice).sqrt()
}

/// Returns the median of finite values in `slice`.
pub fn rolling_median(slice: &[f64]) -> f64 {
    rolling_quantile(slice, 0.5)
}

/// Returns the quantile (0.0..=1.0) of finite values in `slice`.
pub fn rolling_quantile(slice: &[f64], quantile: f64) -> f64 {
    let mut finite: Vec<f64> = slice.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return 0.0;
    }
    finite.sort_by(f64::total_cmp);

    let q = quantile.clamp(0.0, 1.0);
    let max_idx = finite.len().saturating_sub(1);
    let idx_f = q * max_idx as f64;
    let raw_lower = idx_f.floor();
    let raw_upper = idx_f.ceil();
    let idx_lower = if raw_lower.is_finite() && raw_lower >= 0.0 {
        (raw_lower as usize).min(max_idx)
    } else {
        0
    };
    let idx_upper = if raw_upper.is_finite() && raw_upper >= 0.0 {
        (raw_upper as usize).min(max_idx)
    } else {
        0
    };

    let v_lower = finite.get(idx_lower).copied().unwrap_or(0.0);
    let v_upper = finite.get(idx_upper).copied().unwrap_or(0.0);

    if idx_lower == idx_upper {
        v_lower
    } else {
        let weight = idx_f - idx_lower as f64;
        v_lower * (1.0 - weight) + v_upper * weight
    }
}

/// Returns the percentile rank (0.0..=100.0) of `val` within `slice`.
pub fn percent_rank(slice: &[f64], val: f64) -> f64 {
    let finite: Vec<f64> = slice.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() || !val.is_finite() {
        return 0.0;
    }
    let count_below = finite.iter().filter(|&&v| v <= val).count();
    (count_below as f64 / finite.len() as f64) * 100.0
}

/// Computes Pearson correlation from finite, positionally aligned pairs.
pub fn correlation(left: &[f64], right: &[f64]) -> Option<f64> {
    let pairs: Vec<(f64, f64)> = left
        .iter()
        .copied()
        .zip(right.iter().copied())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let count = pairs.len() as f64;
    let mean_x = pairs.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = pairs.iter().map(|(_, y)| y).sum::<f64>() / count;
    let covariance = pairs
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let variance_x = pairs.iter().map(|(x, _)| (x - mean_x).powi(2)).sum::<f64>();
    let variance_y = pairs.iter().map(|(_, y)| (y - mean_y).powi(2)).sum::<f64>();
    let denominator = (variance_x * variance_y).sqrt();
    (denominator > f64::EPSILON).then_some(covariance / denominator)
}

/// Result of a linear regression fit over a data slice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearRegressionResult {
    pub slope: f64,
    pub intercept: f64,
    pub r2: f64,
}

/// Computes ordinary least squares (OLS) linear regression over a slice of values (where X is 0..N-1).
pub fn linear_regression(slice: &[f64]) -> Option<LinearRegressionResult> {
    let n = slice.len();
    if n < 2 {
        return None;
    }

    let mut sum_x = 0.0f64;
    let mut sum_y = 0.0f64;
    let mut sum_xy = 0.0f64;
    let mut sum_xx = 0.0f64;
    let mut valid_n = 0;

    for (i, &y) in slice.iter().enumerate() {
        if y.is_finite() {
            let x = i as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
            valid_n += 1;
        }
    }

    if valid_n < 2 {
        return None;
    }

    let fn_val = valid_n as f64;
    let denom = fn_val * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-12 {
        return None;
    }

    let slope = (fn_val * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / fn_val;

    let y_mean = sum_y / fn_val;
    let ss_tot: f64 = slice
        .iter()
        .filter(|v| v.is_finite())
        .map(|&y| (y - y_mean).powi(2))
        .sum();
    let ss_res: f64 = slice
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_finite())
        .map(|(i, &y)| (y - (slope * i as f64 + intercept)).powi(2))
        .sum();

    let r2 = if ss_tot > 0.0 {
        (1.0 - (ss_res / ss_tot)).clamp(0.0, 1.0)
    } else {
        1.0
    };

    Some(LinearRegressionResult {
        slope,
        intercept,
        r2,
    })
}

/// A binomial proportion with its confidence bounds, all in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProportionInterval {
    /// `successes / trials`.
    pub estimate: f64,
    pub lower: f64,
    pub upper: f64,
}

/// Wilson score interval for `successes` out of `trials`.
///
/// With `p = successes / trials`, `n = trials` and the normal quantile `z`
/// (1.959963984540054 for 95 %):
///
/// ```text
/// centre     = (p + z²/(2n)) / (1 + z²/n)
/// half_width = z · sqrt(p(1 − p)/n + z²/(4n²)) / (1 + z²/n)
/// ```
///
/// Chosen over the Wald interval `p ± z·sqrt(p(1 − p)/n)` because Wald
/// collapses to zero width at `p = 0` or `p = 1` and leaves `0..=1` for small
/// `n` — exactly the samples where the uncertainty matters most. Bounds are
/// clamped to `0..=1` against rounding.
///
/// `None` for `trials == 0`, `successes > trials`, or a `z` that is not a
/// positive finite number.
pub fn wilson_interval(successes: usize, trials: usize, z: f64) -> Option<ProportionInterval> {
    if trials == 0 || successes > trials || !z.is_finite() || z <= 0.0 {
        return None;
    }
    let n = trials as f64;
    let p = successes as f64 / n;
    let z2 = z * z;
    let denominator = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denominator;
    let half_width = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denominator;
    Some(ProportionInterval {
        estimate: p,
        lower: (centre - half_width).clamp(0.0, 1.0),
        upper: (centre + half_width).clamp(0.0, 1.0),
    })
}

/// Length of the longest unbroken run of consecutive elements for which
/// `predicate` holds; `0` if it never does.
///
/// The longest losing streak of a trade list is
/// `longest_run(&pnl, |p| *p < 0.0)`.
pub fn longest_run<T>(values: &[T], mut predicate: impl FnMut(&T) -> bool) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for value in values {
        if predicate(value) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_stats() {
        let data = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        assert_eq!(rolling_sum(&data), 150.0);
        assert_eq!(rolling_mean(&data), 30.0);
        assert_eq!(rolling_median(&data), 30.0);
        assert_eq!(percent_rank(&data, 30.0), 60.0);
    }

    #[test]
    fn test_linear_regression() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let res = linear_regression(&data).unwrap();
        assert!((res.slope - 1.0).abs() < 1e-6);
        assert!((res.intercept - 1.0).abs() < 1e-6);
        assert!((res.r2 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_correlation() {
        assert_eq!(correlation(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]), Some(1.0));
        assert_eq!(correlation(&[1.0, 1.0], &[1.0, 2.0]), None);
    }
}

/// Quantile function of the standard normal distribution, `None` outside `(0, 1)`.
///
/// Acklam's rational approximation (relative error below 1.15e-9), refined by one Newton step
/// against [`crate::option::normal_cdf`] — which brings it to the accuracy of that function.
pub fn normal_quantile(p: f64) -> Option<f64> {
    if !(p > 0.0 && p < 1.0) {
        return None;
    }
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const P_LOW: f64 = 0.024_25;
    let x = if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    // One Newton step: x -= (Φ(x) − p) / φ(x).
    let density = (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt();
    Some(x - (crate::option::normal_cdf(x) - p) / density)
}

/// The deflated Sharpe ratio of the best of several tried strategies.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DeflatedSharpe {
    /// Per-observation Sharpe ratio of the returns (mean / sample standard deviation).
    pub sharpe: f64,
    /// The Sharpe ratio the best of `trials` strategies without skill would be expected to reach.
    pub expected_max_sharpe: f64,
    /// Probability that the true Sharpe ratio exceeds `expected_max_sharpe`, given the sample's
    /// length, skewness and kurtosis. Read as: how likely is this result *not* just the luckiest
    /// of the trials.
    pub probability: f64,
    pub observations: usize,
    pub trials: usize,
}

/// Bailey & López de Prado (2014), "The Deflated Sharpe Ratio".
///
/// With `SR` the per-observation Sharpe ratio of `returns`, `T` their number, `γ3` their skewness
/// and `γ4` their (non-excess) kurtosis, `N` the number of strategies tried and `V` the variance of
/// the Sharpe ratios across them:
///
/// ```text
/// SR0 = sqrt(V) · ((1 − γ) Φ⁻¹(1 − 1/N) + γ Φ⁻¹(1 − 1/(N e)))      γ = 0.5772… (Euler–Mascheroni)
/// DSR = Φ( (SR − SR0) · sqrt(T − 1) / sqrt(1 − γ3 SR + (γ4 − 1)/4 · SR²) )
/// ```
///
/// Skewness and kurtosis are the population moments `m3 / m2^1.5` and `m4 / m2²`. With a single
/// trial `SR0` is 0 and the result is the probabilistic Sharpe ratio against zero.
///
/// `None` for fewer than three returns, a zero standard deviation, `trials == 0`, a negative
/// variance, or a non-positive denominator under the root.
pub fn deflated_sharpe(
    returns: &[f64],
    trials: usize,
    trial_sharpe_variance: f64,
) -> Option<DeflatedSharpe> {
    let t = returns.len();
    if t < 3 || trials == 0 || trial_sharpe_variance.is_nan() || trial_sharpe_variance < 0.0 {
        return None;
    }
    let n = t as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let m2 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    if m2 <= 0.0 {
        return None;
    }
    let m3 = returns.iter().map(|r| (r - mean).powi(3)).sum::<f64>() / n;
    let m4 = returns.iter().map(|r| (r - mean).powi(4)).sum::<f64>() / n;
    let skew = m3 / m2.powf(1.5);
    let kurt = m4 / (m2 * m2);
    let sample_sd = (m2 * n / (n - 1.0)).sqrt();
    let sharpe = mean / sample_sd;

    const EULER_GAMMA: f64 = 0.577_215_664_901_532_9;
    let expected_max_sharpe = if trials <= 1 {
        0.0
    } else {
        let k = trials as f64;
        trial_sharpe_variance.sqrt()
            * ((1.0 - EULER_GAMMA) * normal_quantile(1.0 - 1.0 / k)?
                + EULER_GAMMA * normal_quantile(1.0 - 1.0 / (k * std::f64::consts::E))?)
    };
    let under_root = 1.0 - skew * sharpe + (kurt - 1.0) / 4.0 * sharpe * sharpe;
    if under_root <= 0.0 {
        return None;
    }
    let z = (sharpe - expected_max_sharpe) * (n - 1.0).sqrt() / under_root.sqrt();
    Some(DeflatedSharpe {
        sharpe,
        expected_max_sharpe,
        probability: crate::option::normal_cdf(z),
        observations: t,
        trials,
    })
}
