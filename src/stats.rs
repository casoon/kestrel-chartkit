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
