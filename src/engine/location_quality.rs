#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::engine::acceptance_detector::AcceptanceDetectorOutput;
use crate::engine::market_context::VolumeNodeKind;
use crate::model::MarketRegime;

/// Location Quality score (0..100) evaluating how attractive the current trade location is.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LocationQualityScore {
    pub score: f64, // 0.0 .. 100.0
    pub distance_score: f64,
    pub regime_alignment_score: f64,
    pub acceptance_score: f64,
}

pub fn calculate_location_quality(
    price: f64,
    key_level: f64,
    atr_raw: f64,
    volume_node: VolumeNodeKind,
    regime: MarketRegime,
    acceptance: &AcceptanceDetectorOutput,
    is_long: bool,
) -> LocationQualityScore {
    let atr = atr_raw.max(1e-8);
    let dist_atr = (price - key_level).abs() / atr;

    // 1. Distance component (ideal entry is close to structural level: < 1.0 ATR)
    let distance_score = (1.0 - (dist_atr / 2.0).min(1.0)) * 40.0;

    // 2. Regime alignment & volume node component
    let node_score = match volume_node {
        VolumeNodeKind::Lvn => 30.0, // High potential / free space
        VolumeNodeKind::Neutral => 20.0,
        VolumeNodeKind::Hvn => 10.0, // Inside high volume density balance
    };

    let regime_boost = match (regime, is_long) {
        (MarketRegime::BullishExpansion, true) => 10.0,
        (MarketRegime::BearishExpansion, false) => 10.0,
        (MarketRegime::Consolidation, _) => 5.0,
        _ => 0.0,
    };
    let regime_alignment_score = node_score + regime_boost;

    // 3. Acceptance / Rejection score
    let acceptance_score = match acceptance.rejection_kind {
        crate::engine::acceptance_detector::RejectionKind::RejectionLow if is_long => 20.0,
        crate::engine::acceptance_detector::RejectionKind::RejectionHigh if !is_long => 20.0,
        _ => match acceptance.acceptance_level {
            crate::engine::market_context::AcceptanceLevel::High if is_long => 15.0,
            crate::engine::market_context::AcceptanceLevel::Low if !is_long => 15.0,
            _ => 5.0,
        },
    };

    let total_score =
        (distance_score + regime_alignment_score + acceptance_score).clamp(0.0, 100.0);

    LocationQualityScore {
        score: total_score,
        distance_score,
        regime_alignment_score,
        acceptance_score,
    }
}
