#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::engine::market_context::AuctionPhase;
use crate::model::Bar;

/// Balance vs Imbalance classification (plan Anhang C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum MarketBalanceState {
    Balance,
    Imbalance,
    Transition,
}

/// Balance / Imbalance output combining state classification with auction phase.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BalanceClassifierOutput {
    pub state: MarketBalanceState,
    pub auction_phase: AuctionPhase,
    pub compression_ratio: f64,
    pub balance_confidence: f64,
}

/// Classifies market state into Balance, Imbalance, or Transition based on price compression and pivot structure.
pub fn classify_balance_imbalance(
    bars: &[Bar],
    atr_raw: f64,
    pivot_score: f64,
) -> BalanceClassifierOutput {
    if bars.len() < 5 || atr_raw <= 0.0 {
        return BalanceClassifierOutput {
            state: MarketBalanceState::Transition,
            auction_phase: AuctionPhase::InsideBalance,
            compression_ratio: 1.0,
            balance_confidence: 0.5,
        };
    }

    let len = bars.len();
    let recent_slice = &bars[len - 5..];
    let range = recent_slice.iter().map(|b| b.high).fold(f64::MIN, f64::max)
        - recent_slice.iter().map(|b| b.low).fold(f64::MAX, f64::min);

    let compression_ratio = range / (atr_raw * 5.0).max(1e-8);

    let (state, phase, confidence) = if compression_ratio < 0.60 && pivot_score.abs() < 0.40 {
        (
            MarketBalanceState::Balance,
            AuctionPhase::InsideBalance,
            (1.0 - compression_ratio).clamp(0.5, 1.0),
        )
    } else if compression_ratio > 1.20 || pivot_score.abs() >= 0.70 {
        (
            MarketBalanceState::Imbalance,
            AuctionPhase::Expansion,
            (compression_ratio / 2.0).clamp(0.6, 1.0),
        )
    } else {
        (MarketBalanceState::Transition, AuctionPhase::Retest, 0.50)
    };

    BalanceClassifierOutput {
        state,
        auction_phase: phase,
        compression_ratio,
        balance_confidence: confidence,
    }
}
