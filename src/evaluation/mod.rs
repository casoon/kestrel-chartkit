#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::signal::TriggerAction;

pub mod calibration;
pub mod exporter;
pub mod recorder;

pub use calibration::{
    cohort_aggregate, compute_calibration, CalibrationBucket, CalibrationReport, Cohort,
};
pub use exporter::{FeatureExporter, FeatureRecord};
pub use recorder::{
    ActiveSetup, IntrabarFillPolicy, OutcomeExcursion, OutcomeRecorder, RecordSetupError,
};

/// Historic execution result of a triggered setup.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum TradeOutcome {
    Win,
    Loss,
    BreakEven,
    Expired,
}

/// Recorded evaluation entry of a signal execution.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SignalEvaluationRecord {
    pub timestamp: i64,
    pub trigger: TriggerAction,
    pub score: f64,
    pub confidence: f64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub realized_r_multiple: f64,
    pub duration_bars: u32,
    pub outcome: TradeOutcome,
}

/// Aggregated statistical metrics over a series of evaluations.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TradeStats {
    pub total_trades: usize,
    pub winrate: f64, // 0.0 .. 1.0
    pub profit_factor: f64,
    pub average_r_multiple: f64,
    pub expectancy_r: f64, // EV in R
    pub max_drawdown_r: f64,
}

impl TradeStats {
    /// Computes realized statistics after normalizing outcome/R inconsistencies:
    /// wins are positive, losses are negative, break-even records are zero, and expired records
    /// retain their finite realized R value. Non-finite R values are treated as zero.
    pub fn compute(records: &[SignalEvaluationRecord]) -> Self {
        if records.is_empty() {
            return Self {
                total_trades: 0,
                winrate: 0.0,
                profit_factor: 0.0,
                average_r_multiple: 0.0,
                expectancy_r: 0.0,
                max_drawdown_r: 0.0,
            };
        }

        let total = records.len();
        let wins = records
            .iter()
            .filter(|r| r.outcome == TradeOutcome::Win)
            .count();
        // Winrate: Ratio of winning trades to total trades
        let winrate = wins as f64 / total as f64;

        let normalized_r = |record: &SignalEvaluationRecord| {
            let realized = if record.realized_r_multiple.is_finite() {
                record.realized_r_multiple
            } else {
                0.0
            };
            match record.outcome {
                TradeOutcome::Win => realized.abs(),
                TradeOutcome::Loss => -realized.abs(),
                TradeOutcome::BreakEven => 0.0,
                TradeOutcome::Expired => realized,
            }
        };

        let mut total_gain = 0.0f64;
        let mut total_loss = 0.0f64;
        let mut sum_r = 0.0f64;

        for r in records {
            let r_val = normalized_r(r);
            sum_r += r_val;
            if r_val > 0.0 {
                total_gain += r_val;
            } else if r_val < 0.0 {
                total_loss += r_val.abs();
            }
        }

        let profit_factor = if total_loss > 0.0 {
            total_gain / total_loss
        } else if total_gain > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        let average_r_multiple = sum_r / total as f64;
        let expectancy_r = average_r_multiple;

        let mut equity = 0.0f64;
        let mut peak = 0.0f64;
        let mut max_dd = 0.0f64;

        for r in records {
            let r_val = normalized_r(r);
            equity += r_val;
            if equity > peak {
                peak = equity;
            }
            let dd = peak - equity;
            if dd > max_dd {
                max_dd = dd;
            }
        }

        Self {
            total_trades: total,
            winrate,
            profit_factor,
            average_r_multiple,
            expectancy_r,
            max_drawdown_r: max_dd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_stats_edge_cases() {
        // 1. All wins
        let wins_only = vec![SignalEvaluationRecord {
            timestamp: 1000,
            trigger: TriggerAction::Buy,
            score: 0.8,
            confidence: 0.9,
            entry_price: 100.0,
            exit_price: 105.0,
            realized_r_multiple: 2.0,
            duration_bars: 5,
            outcome: TradeOutcome::Win,
        }];
        let stats_wins = TradeStats::compute(&wins_only);
        assert_eq!(stats_wins.winrate, 1.0);
        assert_eq!(stats_wins.profit_factor, f64::INFINITY);
        assert_eq!(stats_wins.average_r_multiple, 2.0);

        // 2. All losses
        let losses_only = vec![SignalEvaluationRecord {
            timestamp: 1000,
            trigger: TriggerAction::Sell,
            score: 0.8,
            confidence: 0.9,
            entry_price: 100.0,
            exit_price: 105.0,
            realized_r_multiple: -1.0,
            duration_bars: 5,
            outcome: TradeOutcome::Loss,
        }];
        let stats_losses = TradeStats::compute(&losses_only);
        assert_eq!(stats_losses.winrate, 0.0);
        assert_eq!(stats_losses.profit_factor, 0.0);
        assert_eq!(stats_losses.average_r_multiple, -1.0);

        // 3. BreakEven & Expired
        let breakeven_and_expired = vec![
            SignalEvaluationRecord {
                timestamp: 1000,
                trigger: TriggerAction::Buy,
                score: 0.8,
                confidence: 0.9,
                entry_price: 100.0,
                exit_price: 100.0,
                realized_r_multiple: 0.5, // Should be sanitized to 0.0
                duration_bars: 5,
                outcome: TradeOutcome::BreakEven,
            },
            SignalEvaluationRecord {
                timestamp: 2000,
                trigger: TriggerAction::Buy,
                score: 0.8,
                confidence: 0.9,
                entry_price: 100.0,
                exit_price: 100.2,
                realized_r_multiple: 0.1,
                duration_bars: 20,
                outcome: TradeOutcome::Expired,
            },
        ];
        let stats_be = TradeStats::compute(&breakeven_and_expired);
        assert_eq!(stats_be.winrate, 0.0);
        assert_eq!(stats_be.average_r_multiple, 0.05);

        // 4. Outcome is authoritative when the realized R sign is inconsistent
        let inconsistent = vec![
            SignalEvaluationRecord {
                timestamp: 3000,
                trigger: TriggerAction::Buy,
                score: 0.8,
                confidence: 0.9,
                entry_price: 100.0,
                exit_price: 90.0,
                realized_r_multiple: -2.0,
                duration_bars: 5,
                outcome: TradeOutcome::Win,
            },
            SignalEvaluationRecord {
                timestamp: 4000,
                trigger: TriggerAction::Sell,
                score: -0.8,
                confidence: 0.9,
                entry_price: 100.0,
                exit_price: 90.0,
                realized_r_multiple: 1.0,
                duration_bars: 5,
                outcome: TradeOutcome::Loss,
            },
        ];
        let stats_inconsistent = TradeStats::compute(&inconsistent);
        assert_eq!(stats_inconsistent.average_r_multiple, 0.5);
        assert_eq!(stats_inconsistent.expectancy_r, 0.5);
        assert_eq!(stats_inconsistent.profit_factor, 2.0);
    }
}

/// Parameter optimization feedback hook for adjusting strategy parameters based on performance.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ParameterOptimizationHook {
    pub indicator_weights: std::collections::HashMap<String, f64>,
    pub min_confidence_threshold: f64,
    pub min_rr_threshold: f64,
}

impl ParameterOptimizationHook {
    pub fn default_preset() -> Self {
        Self {
            indicator_weights: std::collections::HashMap::new(),
            min_confidence_threshold: 0.50,
            min_rr_threshold: 1.5,
        }
    }

    /// Recommends weight adjustments based on trade statistics.
    pub fn optimize_from_stats(&mut self, stats: &TradeStats) {
        if stats.winrate < 0.40 {
            self.min_confidence_threshold = (self.min_confidence_threshold + 0.05).min(0.80);
        } else if stats.winrate > 0.65 {
            self.min_confidence_threshold = (self.min_confidence_threshold - 0.05).max(0.40);
        }

        if stats.average_r_multiple < 1.0 {
            self.min_rr_threshold = (self.min_rr_threshold + 0.2).min(3.0);
        }
    }
}
