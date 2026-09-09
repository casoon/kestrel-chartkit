//! Deterministic clustering and robust adaptive-threshold primitives for regime/tradability
//! engines, so each one stops hand-rolling its own bucketing/outlier-sensitive threshold logic.

use std::collections::VecDeque;

use crate::stats::rolling_median;

/// Result of [`kmeans_1d`]: final centroids and, for every input value (same order/length as the
/// input slice), which centroid it was assigned to.
#[derive(Debug, Clone, PartialEq)]
pub struct KMeansResult {
    pub centroids: Vec<f64>,
    pub assignments: Vec<usize>,
    pub iterations: usize,
}

/// Deterministic 1-D k-means: `k` initial centroids are the values at evenly spaced quantiles of
/// the *sorted* input (not a random seed), so the same input always produces the same clustering
/// — no RNG dependency, fully reproducible. Lloyd's algorithm then runs until assignments stop
/// changing or `max_iterations` is reached.
///
/// Returns `None` for degenerate input: empty `values`, `k == 0`, or `k > values.len()`.
pub fn kmeans_1d(values: &[f64], k: usize, max_iterations: usize) -> Option<KMeansResult> {
    if values.is_empty() || k == 0 || k > values.len() {
        return None;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut centroids: Vec<f64> = (0..k)
        .map(|i| {
            let idx = if k == 1 {
                0
            } else {
                i * (sorted.len() - 1) / (k - 1)
            };
            sorted.get(idx).copied().unwrap_or(0.0)
        })
        .collect();

    let mut assignments = vec![0usize; values.len()];
    let mut iterations = 0;

    for _ in 0..max_iterations.max(1) {
        iterations += 1;
        let mut changed = false;

        for (i, &v) in values.iter().enumerate() {
            let mut best = 0;
            let mut best_dist = f64::INFINITY;
            for (c_idx, &c) in centroids.iter().enumerate() {
                let dist = (v - c).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best = c_idx;
                }
            }
            if let Some(assign_ref) = assignments.get_mut(i) {
                if *assign_ref != best {
                    changed = true;
                    *assign_ref = best;
                }
            }
        }

        let mut sums = vec![0.0; k];
        let mut counts = vec![0usize; k];
        for (i, &v) in values.iter().enumerate() {
            let cluster_idx = assignments.get(i).copied().unwrap_or(0);
            if let (Some(s), Some(cnt)) = (sums.get_mut(cluster_idx), counts.get_mut(cluster_idx)) {
                *s += v;
                *cnt += 1;
            }
        }
        for c in 0..k {
            let cnt = counts.get(c).copied().unwrap_or(0);
            if cnt > 0 {
                if let (Some(centroid), Some(&sum)) = (centroids.get_mut(c), sums.get(c)) {
                    *centroid = sum / cnt as f64;
                }
            }
        }

        if !changed {
            break;
        }
    }

    Some(KMeansResult {
        centroids,
        assignments,
        iterations,
    })
}

/// A robust rolling threshold band: median +/- `k` scaled median absolute deviations (MAD),
/// rather than mean +/- k*stddev, so a handful of outlier bars in the window do not blow out the
/// band the way a plain stddev-based threshold would.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RobustBand {
    pub median: f64,
    pub mad: f64,
    pub upper: f64,
    pub lower: f64,
}

impl RobustBand {
    pub fn contains(&self, value: f64) -> bool {
        value >= self.lower && value <= self.upper
    }
}

/// MAD-to-stddev consistency constant under a normal distribution (`1 / Phi^-1(3/4)`), the
/// standard scaling so a MAD-based band is comparable in width to a stddev-based one.
const MAD_CONSISTENCY_CONSTANT: f64 = 1.482_602_218_505_602;

/// Streaming robust threshold over a fixed-size trailing window.
#[derive(Debug, Clone)]
pub struct RollingRobustThreshold {
    window_len: usize,
    k: f64,
    buffer: VecDeque<f64>,
}

impl RollingRobustThreshold {
    /// `window_len` bars of history, `k` scaled-MAD multiplier for the band width.
    pub fn new(window_len: usize, k: f64) -> Self {
        let window_len = window_len.max(1);
        Self {
            window_len,
            k,
            buffer: VecDeque::with_capacity(window_len),
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Feeds one value. Returns `None` until the window has `window_len` values.
    pub fn update(&mut self, value: f64) -> Option<RobustBand> {
        if self.buffer.len() >= self.window_len {
            self.buffer.pop_front();
        }
        self.buffer.push_back(value);
        if self.buffer.len() < self.window_len {
            return None;
        }

        let values: Vec<f64> = self.buffer.iter().copied().collect();
        let median = rolling_median(&values);
        let abs_deviations: Vec<f64> = values.iter().map(|v| (v - median).abs()).collect();
        let mad = rolling_median(&abs_deviations) * MAD_CONSISTENCY_CONSTANT;

        let offset = self.k * mad;
        Some(RobustBand {
            median,
            mad,
            upper: median + offset,
            lower: median - offset,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kmeans_1d_separates_clean_clusters() {
        let values = [1.0, 1.1, 0.9, 10.0, 10.2, 9.8];
        let result = kmeans_1d(&values, 2, 20).unwrap();

        // Points 0..3 must share one cluster, 3..6 the other.
        let low_cluster = result.assignments[0];
        assert_eq!(result.assignments[1], low_cluster);
        assert_eq!(result.assignments[2], low_cluster);

        let high_cluster = result.assignments[3];
        assert_ne!(low_cluster, high_cluster);
        assert_eq!(result.assignments[4], high_cluster);
        assert_eq!(result.assignments[5], high_cluster);
    }

    #[test]
    fn test_kmeans_1d_is_deterministic_across_runs() {
        let values = [3.0, 7.0, 1.0, 9.0, 2.0, 8.0, 4.0, 6.0];
        let a = kmeans_1d(&values, 3, 50).unwrap();
        let b = kmeans_1d(&values, 3, 50).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_kmeans_1d_rejects_degenerate_input() {
        assert!(kmeans_1d(&[], 1, 10).is_none());
        assert!(kmeans_1d(&[1.0, 2.0], 0, 10).is_none());
        assert!(kmeans_1d(&[1.0, 2.0], 3, 10).is_none());
    }

    #[test]
    fn test_robust_threshold_ignores_a_single_outlier() {
        let mut robust = RollingRobustThreshold::new(9, 3.0);
        // A tight cluster around 100 plus one wild outlier.
        let values = [100.0, 101.0, 99.0, 100.5, 99.5, 100.0, 100.0, 99.0, 1000.0];
        let mut last = None;
        for v in values {
            last = robust.update(v);
        }
        let band = last.unwrap();
        // Median stays near 100 despite the outlier; a mean-based threshold would be dragged
        // toward the 1000.0 outlier instead.
        assert!((band.median - 100.0).abs() < 1.0);
        assert!(band.contains(100.5));
        assert!(
            !band.contains(1000.0),
            "outlier must fall outside the robust band"
        );
    }

    #[test]
    fn test_robust_threshold_none_until_window_full() {
        let mut robust = RollingRobustThreshold::new(5, 2.0);
        for v in [1.0, 2.0, 3.0, 4.0] {
            assert!(robust.update(v).is_none());
        }
        assert!(robust.update(5.0).is_some());
    }
}
