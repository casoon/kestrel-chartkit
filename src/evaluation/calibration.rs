//! Outcome calibration: Brier score, hit-time statistics, confidence-bucketed reliability, and
//! cohort aggregation over [`super::SignalEvaluationRecord`]s — layered on top of the existing
//! bar-wise [`super::OutcomeRecorder`]/[`super::TradeStats`], which track realized R-multiples but
//! not how well a signal's *confidence* predicted its outcome.

use std::collections::HashMap;

use crate::stats::rolling_median;

use super::{SignalEvaluationRecord, TradeOutcome, TradeStats};

/// One confidence bucket's calibration: how predicted confidence compared to the actual win rate
/// observed within that bucket (a reliability-diagram row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibrationBucket {
    pub predicted_mean: f64,
    pub observed_win_rate: f64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationReport {
    /// Mean squared error between predicted confidence (as a 0.0..=1.0 win probability) and the
    /// realized binary outcome (`1.0` for `Win`, `0.0` otherwise). Lower is better; `0.0` is
    /// perfect, `0.25` is what a constant `0.5` forecast scores against a 50/50 outcome mix.
    pub brier_score: f64,
    /// Confidence-sorted buckets (a reliability diagram): a well-calibrated signal has
    /// `observed_win_rate` tracking `predicted_mean` closely in every bucket.
    pub buckets: Vec<CalibrationBucket>,
    pub mean_hit_time_bars: f64,
    pub median_hit_time_bars: f64,
}

fn outcome_as_probability(outcome: TradeOutcome) -> f64 {
    match outcome {
        TradeOutcome::Win => 1.0,
        TradeOutcome::Loss | TradeOutcome::BreakEven | TradeOutcome::Expired => 0.0,
    }
}

/// Computes a [`CalibrationReport`] from `records`, using `confidence` (expected `0.0..=1.0`) as
/// each record's predicted win probability. `num_buckets` controls the reliability-diagram
/// resolution (records are sorted by confidence and split into roughly equal-sized buckets).
pub fn compute_calibration(
    records: &[SignalEvaluationRecord],
    num_buckets: usize,
) -> CalibrationReport {
    if records.is_empty() {
        return CalibrationReport {
            brier_score: 0.0,
            buckets: Vec::new(),
            mean_hit_time_bars: 0.0,
            median_hit_time_bars: 0.0,
        };
    }

    let brier_score = records
        .iter()
        .map(|r| {
            let p = r.confidence.clamp(0.0, 1.0);
            let o = outcome_as_probability(r.outcome);
            (p - o).powi(2)
        })
        .sum::<f64>()
        / records.len() as f64;

    let mut sorted: Vec<&SignalEvaluationRecord> = records.iter().collect();
    sorted.sort_by(|a, b| a.confidence.total_cmp(&b.confidence));

    let num_buckets = num_buckets.max(1).min(sorted.len());
    let bucket_size = sorted.len().div_ceil(num_buckets);
    let buckets = sorted
        .chunks(bucket_size.max(1))
        .map(|chunk| {
            let predicted_mean =
                chunk.iter().map(|r| r.confidence).sum::<f64>() / chunk.len() as f64;
            let wins = chunk
                .iter()
                .filter(|r| r.outcome == TradeOutcome::Win)
                .count();
            CalibrationBucket {
                predicted_mean,
                observed_win_rate: wins as f64 / chunk.len() as f64,
                count: chunk.len(),
            }
        })
        .collect();

    let durations: Vec<f64> = records.iter().map(|r| r.duration_bars as f64).collect();
    let mean_hit_time_bars = durations.iter().sum::<f64>() / durations.len() as f64;
    let median_hit_time_bars = rolling_median(&durations);

    CalibrationReport {
        brier_score,
        buckets,
        mean_hit_time_bars,
        median_hit_time_bars,
    }
}

/// One cohort's aggregated [`TradeStats`], keyed by a caller-supplied grouping label (e.g. trade
/// direction, session, or any other categorical dimension).
#[derive(Debug, Clone, PartialEq)]
pub struct Cohort {
    pub key: String,
    pub stats: TradeStats,
}

/// Groups `records` by `key_fn` and computes [`TradeStats`] per cohort, sorted by key for
/// deterministic output.
pub fn cohort_aggregate<K, F>(records: &[SignalEvaluationRecord], key_fn: F) -> Vec<Cohort>
where
    K: ToString,
    F: Fn(&SignalEvaluationRecord) -> K,
{
    let mut groups: HashMap<String, Vec<SignalEvaluationRecord>> = HashMap::new();
    for record in records {
        groups
            .entry(key_fn(record).to_string())
            .or_default()
            .push(record.clone());
    }

    let mut cohorts: Vec<Cohort> = groups
        .into_iter()
        .map(|(key, group_records)| Cohort {
            key,
            stats: TradeStats::compute(&group_records),
        })
        .collect();
    cohorts.sort_by(|a, b| a.key.cmp(&b.key));
    cohorts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::TriggerAction;

    fn record(
        confidence: f64,
        outcome: TradeOutcome,
        duration_bars: u32,
    ) -> SignalEvaluationRecord {
        SignalEvaluationRecord {
            timestamp: 0,
            trigger: TriggerAction::Buy,
            score: 0.5,
            confidence,
            entry_price: 100.0,
            exit_price: 101.0,
            realized_r_multiple: 1.0,
            duration_bars,
            outcome,
        }
    }

    #[test]
    fn test_brier_score_zero_for_perfect_forecasts() {
        let records = vec![
            record(1.0, TradeOutcome::Win, 5),
            record(0.0, TradeOutcome::Loss, 5),
        ];
        let report = compute_calibration(&records, 2);
        assert!(report.brier_score < 1e-9);
    }

    #[test]
    fn test_brier_score_positive_for_overconfident_forecasts() {
        let records = vec![
            record(0.9, TradeOutcome::Loss, 5),
            record(0.9, TradeOutcome::Loss, 5),
        ];
        let report = compute_calibration(&records, 2);
        assert!((report.brier_score - 0.81).abs() < 1e-9);
    }

    #[test]
    fn test_buckets_reveal_miscalibration() {
        // High confidence but only ever loses: the bucket's observed win rate must diverge
        // sharply from its predicted mean.
        let records: Vec<_> = (0..10)
            .map(|_| record(0.9, TradeOutcome::Loss, 3))
            .collect();
        let report = compute_calibration(&records, 1);
        assert_eq!(report.buckets.len(), 1);
        let bucket = report.buckets[0];
        assert!((bucket.predicted_mean - 0.9).abs() < 1e-9);
        assert_eq!(bucket.observed_win_rate, 0.0);
    }

    #[test]
    fn test_hit_time_statistics() {
        let records = vec![
            record(0.5, TradeOutcome::Win, 2),
            record(0.5, TradeOutcome::Win, 4),
            record(0.5, TradeOutcome::Loss, 6),
        ];
        let report = compute_calibration(&records, 1);
        assert!((report.mean_hit_time_bars - 4.0).abs() < 1e-9);
        assert!((report.median_hit_time_bars - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_cohort_aggregate_groups_and_sorts_by_key() {
        let records = vec![
            record(0.6, TradeOutcome::Win, 3),
            record(0.4, TradeOutcome::Loss, 5),
            record(0.7, TradeOutcome::Win, 2),
        ];
        let cohorts = cohort_aggregate(
            &records,
            |r| if r.confidence >= 0.5 { "high" } else { "low" },
        );

        assert_eq!(cohorts.len(), 2);
        assert_eq!(cohorts[0].key, "high");
        assert_eq!(cohorts[0].stats.total_trades, 2);
        assert_eq!(cohorts[1].key, "low");
        assert_eq!(cohorts[1].stats.total_trades, 1);
    }

    #[test]
    fn test_empty_records_produce_neutral_report() {
        let report = compute_calibration(&[], 5);
        assert_eq!(report.brier_score, 0.0);
        assert!(report.buckets.is_empty());
    }
}
