//! Definitions of the composites b family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::leg_conviction::LEG_CONVICTION_DEFINITION,
    &crate::indicator::trend_break_risk::TREND_BREAK_RISK_DEFINITION,
    &crate::indicator::sweep_reversal_score::SWEEP_REVERSAL_SCORE_DEFINITION,
    &crate::indicator::equal_level_liquidity::EQUAL_LEVEL_LIQUIDITY_DEFINITION,
    &crate::indicator::leg_character::LEG_CHARACTER_DEFINITION,
    &crate::indicator::accumulation_phase::ACCUMULATION_PHASE_DEFINITION,
    &crate::indicator::reversal_setup_score::REVERSAL_SETUP_SCORE_DEFINITION,
    &crate::indicator::structure_zones::STRUCTURE_ZONES_DEFINITION,
    &crate::indicator::structural_flow::STRUCTURAL_FLOW_DEFINITION,
    &crate::indicator::trend_quality::TREND_QUALITY_DEFINITION,
    &crate::indicator::buy_sell_pressure::BUY_SELL_PRESSURE_DEFINITION,
    &crate::indicator::multi_factor::MULTI_FACTOR_DEFINITION,
];
