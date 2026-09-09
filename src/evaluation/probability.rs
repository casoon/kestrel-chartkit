//! Empirical and isotonic probability calibration, out-of-sample metrics, and validation manifests.
//!
//! Maps raw heuristic indicator/strategy scores to monotone calibrated probabilities
//! fitted exclusively on training data and frozen for evaluation on unseen test datasets.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A calibrated probability output distinguishing raw confidence from empirical likelihood.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CalibratedProbability {
    /// Raw uncalibrated score or confidence (e.g. 0.0..100.0 or 0.0..1.0).
    pub raw_score: f64,
    /// Calibrated empirical win probability in `0.0..=1.0`.
    pub probability: f64,
    /// Identifier / version of the calibration model used.
    pub model_version: String,
    /// Training sample size underlying the calibration curve.
    pub train_sample_size: usize,
}

/// A non-parametric monotonic probability calibrator using the Pool Adjacent Violators Algorithm (PAVA).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IsotonicCalibrator {
    /// Score cutoffs sorted ascending.
    pub thresholds: Vec<f64>,
    /// Corresponding calibrated probabilities in `0.0..=1.0` (guaranteed non-decreasing).
    pub probabilities: Vec<f64>,
    pub train_sample_size: usize,
}

impl IsotonicCalibrator {
    /// Fits an isotonic calibrator from a slice of `(raw_score, actual_outcome)` pairs.
    ///
    /// Out-of-sample discipline: Call this method ONLY on training data!
    pub fn fit(samples: &[(f64, bool)]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }

        // 1. Sort samples ascending by score
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));

        // 2. Initialize PAVA blocks: (score, weight, value)
        let mut scores: Vec<f64> = Vec::with_capacity(sorted.len());
        let mut weights: Vec<f64> = Vec::with_capacity(sorted.len());
        let mut values: Vec<f64> = Vec::with_capacity(sorted.len());

        for (score, outcome) in sorted {
            scores.push(score);
            weights.push(1.0);
            values.push(if outcome { 1.0 } else { 0.0 });
        }

        // 3. Pool adjacent violators
        let mut i = 0;
        while i < values.len() {
            if i > 0 && values[i] < values[i - 1] {
                // Violation: merge block i into block i-1
                let w1 = weights[i - 1];
                let w2 = weights[i];
                let v1 = values[i - 1];
                let v2 = values[i];

                let w_new = w1 + w2;
                let v_new = (w1 * v1 + w2 * v2) / w_new;

                weights[i - 1] = w_new;
                values[i - 1] = v_new;
                scores[i - 1] = scores[i]; // upper boundary of merged block

                weights.remove(i);
                values.remove(i);
                scores.remove(i);

                i -= 1; // Step back to check previous pair
            } else {
                i += 1;
            }
        }

        Some(Self {
            thresholds: scores,
            probabilities: values,
            train_sample_size: samples.len(),
        })
    }

    /// Evaluates a raw score against the frozen calibration curve.
    ///
    /// Returns a monotone calibrated probability in `0.0..=1.0`.
    pub fn predict(&self, raw_score: f64) -> f64 {
        if self.thresholds.is_empty() {
            return 0.5;
        }
        if raw_score <= self.thresholds[0] {
            return self.probabilities[0].clamp(0.0, 1.0);
        }
        let last_idx = self.thresholds.len() - 1;
        if raw_score >= self.thresholds[last_idx] {
            return self.probabilities[last_idx].clamp(0.0, 1.0);
        }

        // Binary search for bracket
        match self
            .thresholds
            .binary_search_by(|t| t.total_cmp(&raw_score))
        {
            Ok(idx) => self.probabilities[idx].clamp(0.0, 1.0),
            Err(idx) => {
                // Linear interpolation between idx - 1 and idx
                let x0 = self.thresholds[idx - 1];
                let x1 = self.thresholds[idx];
                let y0 = self.probabilities[idx - 1];
                let y1 = self.probabilities[idx];
                let frac = if (x1 - x0).abs() > 1e-12 {
                    (raw_score - x0) / (x1 - x0)
                } else {
                    0.0
                };
                (y0 + frac * (y1 - y0)).clamp(0.0, 1.0)
            }
        }
    }
}

/// Out-of-sample calibration performance metrics and baseline comparisons.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CalibrationMetrics {
    /// Mean squared error between calibrated probability and realized outcome: `sum((p - y)^2) / N`.
    pub brier_score: f64,
    /// Brier score of a trivial constant baseline prediction (e.g. historical training winrate).
    pub baseline_brier_score: f64,
    /// Brier Skill Score: `1.0 - (brier_score / baseline_brier_score)`.
    /// Positive values indicate genuine skill exceeding the trivial baseline.
    pub brier_skill_score: f64,
    /// Average logarithmic loss (binary cross entropy).
    pub log_loss: f64,
    /// Expected Calibration Error across equal-width probability bins.
    pub expected_calibration_error: f64,
    /// Number of evaluated out-of-sample observations.
    pub sample_size: usize,
}

impl fmt::Display for CalibrationMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Brier: {:.4} (BSS: {:.2}%), LogLoss: {:.4}, ECE: {:.4} (N={})",
            self.brier_score,
            self.brier_skill_score * 100.0,
            self.log_loss,
            self.expected_calibration_error,
            self.sample_size
        )
    }
}

/// Computes comprehensive calibration metrics comparing predicted probabilities against actual outcomes.
///
/// - `predicted`: slice of predicted probabilities in `0.0..=1.0`.
/// - `actual`: slice of realized binary outcomes.
/// - `baseline_rate`: base rate probability of the training population (e.g. 0.50).
/// - `num_bins`: number of bins for Expected Calibration Error (typically 10).
pub fn compute_calibration_metrics(
    predicted: &[f64],
    actual: &[bool],
    baseline_rate: f64,
    num_bins: usize,
) -> Option<CalibrationMetrics> {
    if predicted.len() != actual.len() || predicted.is_empty() {
        return None;
    }

    let n = predicted.len() as f64;
    let baseline = baseline_rate.clamp(1e-6, 1.0 - 1e-6);

    let mut brier_sum = 0.0;
    let mut baseline_brier_sum = 0.0;
    let mut log_loss_sum = 0.0;

    for (&p_raw, &y) in predicted.iter().zip(actual.iter()) {
        let p = p_raw.clamp(1e-15, 1.0 - 1e-15);
        let target = if y { 1.0 } else { 0.0 };

        brier_sum += (p - target).powi(2);
        baseline_brier_sum += (baseline - target).powi(2);

        let log_p = if y { p.ln() } else { (1.0 - p).ln() };
        log_loss_sum -= log_p;
    }

    let brier_score = brier_sum / n;
    let baseline_brier_score = baseline_brier_sum / n;
    let brier_skill_score = if baseline_brier_score > 1e-12 {
        1.0 - (brier_score / baseline_brier_score)
    } else {
        0.0
    };
    let log_loss = log_loss_sum / n;

    // Expected Calibration Error (ECE) across `num_bins`
    let bins = num_bins.max(1);
    let mut bin_counts = vec![0usize; bins];
    let mut bin_pred_sums = vec![0.0f64; bins];
    let mut bin_actual_sums = vec![0.0f64; bins];

    for (&p_raw, &y) in predicted.iter().zip(actual.iter()) {
        let p = p_raw.clamp(0.0, 1.0);
        let bin_idx = ((p * bins as f64).floor() as usize).min(bins - 1);
        bin_counts[bin_idx] += 1;
        bin_pred_sums[bin_idx] += p;
        bin_actual_sums[bin_idx] += if y { 1.0 } else { 0.0 };
    }

    let mut ece = 0.0;
    for i in 0..bins {
        if bin_counts[i] > 0 {
            let count = bin_counts[i] as f64;
            let avg_pred = bin_pred_sums[i] / count;
            let avg_actual = bin_actual_sums[i] / count;
            ece += (count / n) * (avg_pred - avg_actual).abs();
        }
    }

    Some(CalibrationMetrics {
        brier_score,
        baseline_brier_score,
        brier_skill_score,
        log_loss,
        expected_calibration_error: ece,
        sample_size: predicted.len(),
    })
}

/// Circular block bootstrap estimating confidence intervals for Brier Score under time dependence.
///
/// Returns `(mean, percentile_05, percentile_95)`.
pub fn block_bootstrap_brier(
    predicted: &[f64],
    actual: &[bool],
    block_size: usize,
    num_bootstraps: usize,
    seed: u64,
) -> Option<(f64, f64, f64)> {
    let n = predicted.len();
    if n == 0 || block_size == 0 || num_bootstraps == 0 {
        return None;
    }

    let mut lcg = seed.wrapping_add(1);
    let mut bootstrap_scores = Vec::with_capacity(num_bootstraps);

    for _ in 0..num_bootstraps {
        let mut sample_brier_sum = 0.0;
        let mut count = 0;

        while count < n {
            // LCG next pseudo-random integer
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = (lcg as usize) % n;

            let take = block_size.min(n - count);
            for k in 0..take {
                let idx = (start + k) % n;
                let p = predicted[idx].clamp(0.0, 1.0);
                let y = if actual[idx] { 1.0 } else { 0.0 };
                sample_brier_sum += (p - y).powi(2);
            }
            count += take;
        }

        bootstrap_scores.push(sample_brier_sum / count as f64);
    }

    bootstrap_scores.sort_by(|a, b| a.total_cmp(b));
    let mean = bootstrap_scores.iter().sum::<f64>() / num_bootstraps as f64;
    let p05_idx = ((0.05 * num_bootstraps as f64).floor() as usize).min(num_bootstraps - 1);
    let p95_idx = ((0.95 * num_bootstraps as f64).floor() as usize).min(num_bootstraps - 1);

    Some((mean, bootstrap_scores[p05_idx], bootstrap_scores[p95_idx]))
}

/// Verifiable experiment manifest documenting the full training and out-of-sample evaluation context.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValidationExperimentManifest {
    pub experiment_id: String,
    pub model_version: String,
    pub train_range: (usize, usize),
    pub test_range: (usize, usize),
    pub embargo_bars: usize,
    pub train_sample_size: usize,
    pub test_sample_size: usize,
    pub metrics: CalibrationMetrics,
}
