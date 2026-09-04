#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::MarketRegime;

/// Where the current price sits relative to the Volume Profile (plan Anhang E).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum VolumeNodeKind {
    Hvn,
    Lvn,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum AcceptanceLevel {
    High,
    Low,
    None,
}

/// Balance/Trend auction-phase lifecycle (plan Anhang F "Bestätigung/Verfeinerung, sechstes
/// Video" + Anhang G.1 "zyklisch gedacht: Impulse → Balance → Acceptance → Breakout → Impulse").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum AuctionPhase {
    InsideBalance,
    Breakout,
    Acceptance { duration_bars: u32 },
    Retest,
    Expansion,
}

/// Composite context around price relative to Volume Profile / regime (plan Anhang E,
/// `MarketContextOutput`). Deliberately kept separate from a future `AuctionOutput`
/// (Delta/Absorption) since that needs aggressor-tagged trade/tick data, not just `Bar`s —
/// see the plan's "Offene Datenmodell-Frage".
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MarketContextOutput {
    pub regime: MarketRegime,
    pub distance_to_vpoc_atr: f64,
    pub current_volume_node: VolumeNodeKind,
    pub previous_acceptance: AcceptanceLevel,
    pub auction_phase: AuctionPhase,
}

/// Classify the current volume node from a Volume-Profile-style density estimate: `density`
/// is the traded-volume share at the current price bucket (0.0..1.0).
pub fn classify_volume_node(
    density: f64,
    hvn_threshold: f64,
    lvn_threshold: f64,
) -> VolumeNodeKind {
    if density >= hvn_threshold {
        VolumeNodeKind::Hvn
    } else if density <= lvn_threshold {
        VolumeNodeKind::Lvn
    } else {
        VolumeNodeKind::Neutral
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_market_context(
    regime: MarketRegime,
    price: f64,
    vpoc: f64,
    atr: f64,
    node: VolumeNodeKind,
    previous_acceptance: AcceptanceLevel,
    auction_phase: AuctionPhase,
) -> MarketContextOutput {
    let distance_to_vpoc_atr = if atr > 0.0 {
        (price - vpoc).abs() / atr
    } else {
        0.0
    };
    MarketContextOutput {
        regime,
        distance_to_vpoc_atr,
        current_volume_node: node,
        previous_acceptance,
        auction_phase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_node_classification_thresholds() {
        assert_eq!(classify_volume_node(0.9, 0.7, 0.2), VolumeNodeKind::Hvn);
        assert_eq!(classify_volume_node(0.1, 0.7, 0.2), VolumeNodeKind::Lvn);
        assert_eq!(classify_volume_node(0.5, 0.7, 0.2), VolumeNodeKind::Neutral);
    }

    #[test]
    fn distance_to_vpoc_is_atr_normalized() {
        let ctx = build_market_context(
            MarketRegime::BullishExpansion,
            110.0,
            100.0,
            5.0,
            VolumeNodeKind::Neutral,
            AcceptanceLevel::None,
            AuctionPhase::Expansion,
        );
        assert!((ctx.distance_to_vpoc_atr - 2.0).abs() < 1e-9);
    }
}
