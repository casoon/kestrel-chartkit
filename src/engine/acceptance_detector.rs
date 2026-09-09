#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::engine::market_context::AcceptanceLevel;
use crate::model::Bar;

/// Rejection kind at key structural level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum RejectionKind {
    RejectionHigh,
    RejectionLow,
    None,
}

/// Result of acceptance vs rejection evaluation over a lookback window.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AcceptanceDetectorOutput {
    pub acceptance_level: AcceptanceLevel,
    pub rejection_kind: RejectionKind,
    pub consecutive_acceptance_bars: u32,
    pub level: f64,
}

/// Evaluates whether closing price stays over N bars above/below a key level or gets rejected.
pub fn detect_acceptance_rejection(
    bars: &[Bar],
    level: f64,
    min_acceptance_bars: usize,
) -> AcceptanceDetectorOutput {
    if bars.is_empty() || level <= 0.0 {
        return AcceptanceDetectorOutput {
            acceptance_level: AcceptanceLevel::None,
            rejection_kind: RejectionKind::None,
            consecutive_acceptance_bars: 0,
            level,
        };
    }

    let mut consecutive_above = 0u32;
    let mut consecutive_below = 0u32;

    for b in bars.iter().rev() {
        if b.close > level {
            if consecutive_below > 0 {
                break;
            }
            consecutive_above += 1;
        } else if b.close < level {
            if consecutive_above > 0 {
                break;
            }
            consecutive_below += 1;
        } else {
            break;
        }
    }

    let min_b = min_acceptance_bars as u32;
    let (acc_level, rejection, count) = if consecutive_above >= min_b {
        (
            AcceptanceLevel::High,
            RejectionKind::None,
            consecutive_above,
        )
    } else if consecutive_below >= min_b {
        (AcceptanceLevel::Low, RejectionKind::None, consecutive_below)
    } else {
        let last_bar = bars.last().unwrap();
        let rejection = if last_bar.high > level && last_bar.close < level {
            RejectionKind::RejectionHigh
        } else if last_bar.low < level && last_bar.close > level {
            RejectionKind::RejectionLow
        } else {
            RejectionKind::None
        };
        (AcceptanceLevel::None, rejection, 0)
    };

    AcceptanceDetectorOutput {
        acceptance_level: acc_level,
        rejection_kind: rejection,
        consecutive_acceptance_bars: count,
        level,
    }
}
