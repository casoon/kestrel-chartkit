//! Out-of-sample data splits with purging and embargo to eliminate lookahead bias and label overlap.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Representation of a trade's temporal lifespan for purging calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TradeSpan {
    pub id: usize,
    /// Bar index when the trade setup was entered / signal evaluated.
    pub entry_bar: usize,
    /// Bar index when the trade reached its target, stop, or horizon expiry.
    pub exit_bar: usize,
}

/// Configuration for a train/test chronological split with embargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PurgedSplitConfig {
    /// Beginning of training window (inclusive).
    pub train_start: usize,
    /// End of training window (exclusive).
    pub train_end: usize,
    /// Beginning of testing window (inclusive). Must be >= `train_end + embargo_bars`.
    pub test_start: usize,
    /// End of testing window (exclusive).
    pub test_end: usize,
    /// Minimum separation buffer between training end and test start.
    pub embargo_bars: usize,
}

/// Error during split configuration or purging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    InvalidRange(&'static str),
    EmbargoViolation {
        actual_gap: usize,
        required_embargo: usize,
    },
}

impl fmt::Display for SplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange(msg) => write!(f, "invalid range: {msg}"),
            Self::EmbargoViolation {
                actual_gap,
                required_embargo,
            } => write!(
                f,
                "embargo violation: actual gap {actual_gap} bars < required {required_embargo} bars"
            ),
        }
    }
}

impl std::error::Error for SplitError {}

/// The result of a purged train/test split.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PurgedTrainTestSplit {
    /// Trade IDs retained in the training set.
    pub train_trade_ids: Vec<usize>,
    /// Trade IDs retained in the testing set.
    pub test_trade_ids: Vec<usize>,
    /// Trade IDs purged from training because their evaluation window overlaps the test period.
    pub purged_trade_ids: Vec<usize>,
}

/// Partitions trades into train and test sets, purging any training trade whose exit bar
/// reaches into or beyond the test set start.
pub fn split_trades_purged(
    trades: &[TradeSpan],
    config: &PurgedSplitConfig,
) -> Result<PurgedTrainTestSplit, SplitError> {
    if config.train_end <= config.train_start {
        return Err(SplitError::InvalidRange(
            "train_end must be strictly greater than train_start",
        ));
    }
    if config.test_end <= config.test_start {
        return Err(SplitError::InvalidRange(
            "test_end must be strictly greater than test_start",
        ));
    }
    if config.test_start < config.train_end {
        return Err(SplitError::InvalidRange(
            "test_start must be >= train_end chronologically",
        ));
    }

    let actual_gap = config.test_start - config.train_end;
    if actual_gap < config.embargo_bars {
        return Err(SplitError::EmbargoViolation {
            actual_gap,
            required_embargo: config.embargo_bars,
        });
    }

    let mut train_trade_ids = Vec::new();
    let mut test_trade_ids = Vec::new();
    let mut purged_trade_ids = Vec::new();

    for trade in trades {
        if trade.exit_bar < trade.entry_bar {
            return Err(SplitError::InvalidRange(
                "trade exit_bar must be >= entry_bar",
            ));
        }

        // Training candidate: entered during the training interval
        if trade.entry_bar >= config.train_start && trade.entry_bar < config.train_end {
            // Purge condition: if trade's outcome is not fully realized before test starts,
            // it carries lookahead information across the split boundary.
            if trade.exit_bar >= config.test_start {
                purged_trade_ids.push(trade.id);
            } else {
                train_trade_ids.push(trade.id);
            }
        } else if trade.entry_bar >= config.test_start && trade.entry_bar < config.test_end {
            // Testing candidate
            test_trade_ids.push(trade.id);
        }
    }

    Ok(PurgedTrainTestSplit {
        train_trade_ids,
        test_trade_ids,
        purged_trade_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_purging_trade_overlapping_test_start() {
        let config = PurgedSplitConfig {
            train_start: 0,
            train_end: 100,
            test_start: 110,
            test_end: 200,
            embargo_bars: 10,
        };

        let trades = vec![
            // Trade 1: entry 20, exit 35 -> safe train
            TradeSpan {
                id: 1,
                entry_bar: 20,
                exit_bar: 35,
            },
            // Trade 2: entry 95, exit 115 -> overlaps test_start 110! MUST be purged!
            TradeSpan {
                id: 2,
                entry_bar: 95,
                exit_bar: 115,
            },
            // Trade 3: entry 115, exit 130 -> test set
            TradeSpan {
                id: 3,
                entry_bar: 115,
                exit_bar: 130,
            },
        ];

        let split = split_trades_purged(&trades, &config).unwrap();
        assert_eq!(split.train_trade_ids, vec![1]);
        assert_eq!(split.purged_trade_ids, vec![2]);
        assert_eq!(split.test_trade_ids, vec![3]);
    }

    #[test]
    fn test_embargo_violation_detection() {
        let config = PurgedSplitConfig {
            train_start: 0,
            train_end: 100,
            test_start: 105, // gap = 5
            test_end: 200,
            embargo_bars: 10, // required = 10
        };
        let res = split_trades_purged(&[], &config);
        assert!(matches!(res, Err(SplitError::EmbargoViolation { .. })));
    }
}
