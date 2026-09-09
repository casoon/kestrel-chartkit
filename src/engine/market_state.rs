#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::MarketRegime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum VolatilityRegime {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum OpeningType {
    InsideValue,
    OutsideValue,
    Unknown,
}

/// Permission grade for one side of the playbook (plan Anhang F/Anhang G "Permission als
/// eigene, dem Playbook vorgelagerte Ebene"). A `Preferred`/`Neutral` state never *mandates*
/// a trade — see "Bias ≠ Signal" in the plan; it only says which scenarios are in scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum PlaybookState {
    Preferred,
    Neutral,
    Suppressed,
    NotAllowed,
}

/// Leithierarchie state (plan Anhang F): `Market State → Expected Playbook → Location →
/// Trigger → Trade Geometry → Historical Edge`. `regime` reuses `model::MarketRegime`
/// (`BullishExpansion`/`BearishExpansion`/`Consolidation`/`Transition`) instead of a
/// second, competing Bull/Bear/Balance enum.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MarketStateOutput {
    pub regime: MarketRegime,
    pub volatility_regime: VolatilityRegime,
    /// 0.0..1.0, analog `trend_quality` in `indicator::swing_structure`.
    pub trend_stability: f64,
    /// 0.0..1.0. Placeholder heuristic in [`super::pipeline::build_market_state_and_playbook`]:
    /// a fixed 0.8/0.2 based only on `regime`, not a measured probability.
    pub balance_probability: f64,
    /// Opening Range size vs. the last ~20 sessions, 0.0..1.0. Placeholder heuristic in
    /// [`super::pipeline::build_market_state_and_playbook`]: always 0.5 (neutral), not derived
    /// from actual session history.
    pub opening_range_percentile: f64,
    pub opening_type: OpeningType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExpectedPlaybook {
    pub continuation_long: PlaybookState,
    pub continuation_short: PlaybookState,
    pub reversal_long: PlaybookState,
    pub reversal_short: PlaybookState,
}

/// Derive the Expected-Playbook permission grades from regime + trend stability. Follows
/// "Bias ≠ Signal" (plan Anhang F, achtes Video): a regime only limits which scenarios are
/// preferred/allowed, it never forces a trade — the lower-timeframe trigger still decides.
pub fn derive_playbook(regime: MarketRegime, trend_stability: f64) -> ExpectedPlaybook {
    let strong = trend_stability >= 0.5;
    match regime {
        MarketRegime::BullishExpansion => ExpectedPlaybook {
            continuation_long: if strong {
                PlaybookState::Preferred
            } else {
                PlaybookState::Neutral
            },
            continuation_short: PlaybookState::NotAllowed,
            reversal_long: PlaybookState::NotAllowed,
            reversal_short: PlaybookState::Suppressed,
        },
        MarketRegime::BearishExpansion => ExpectedPlaybook {
            continuation_long: PlaybookState::NotAllowed,
            continuation_short: if strong {
                PlaybookState::Preferred
            } else {
                PlaybookState::Neutral
            },
            reversal_long: PlaybookState::Suppressed,
            reversal_short: PlaybookState::NotAllowed,
        },
        MarketRegime::Consolidation => ExpectedPlaybook {
            continuation_long: PlaybookState::NotAllowed,
            continuation_short: PlaybookState::NotAllowed,
            reversal_long: PlaybookState::Preferred,
            reversal_short: PlaybookState::Preferred,
        },
        // "Strukturbruch ≠ Trendwechsel" (plan Anhang D, achtes Video): a broken trend is
        // neutral/transitional until re-acceptance confirms a new direction, so nothing gets
        // `Preferred` yet.
        MarketRegime::Transition => ExpectedPlaybook {
            continuation_long: PlaybookState::Suppressed,
            continuation_short: PlaybookState::Suppressed,
            reversal_long: PlaybookState::Neutral,
            reversal_short: PlaybookState::Neutral,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_bull_regime_prefers_continuation_long_only() {
        let pb = derive_playbook(MarketRegime::BullishExpansion, 0.8);
        assert_eq!(pb.continuation_long, PlaybookState::Preferred);
        assert_eq!(pb.continuation_short, PlaybookState::NotAllowed);
        assert_eq!(pb.reversal_long, PlaybookState::NotAllowed);
    }

    #[test]
    fn balance_regime_prefers_both_reversal_sides() {
        let pb = derive_playbook(MarketRegime::Consolidation, 0.9);
        assert_eq!(pb.reversal_long, PlaybookState::Preferred);
        assert_eq!(pb.reversal_short, PlaybookState::Preferred);
        assert_eq!(pb.continuation_long, PlaybookState::NotAllowed);
    }

    #[test]
    fn transition_regime_never_prefers_anything() {
        let pb = derive_playbook(MarketRegime::Transition, 1.0);
        assert_ne!(pb.continuation_long, PlaybookState::Preferred);
        assert_ne!(pb.continuation_short, PlaybookState::Preferred);
        assert_ne!(pb.reversal_long, PlaybookState::Preferred);
        assert_ne!(pb.reversal_short, PlaybookState::Preferred);
    }
}
