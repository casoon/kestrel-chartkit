//! Definitions of the composites a family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::flow_exhaustion::FLOW_EXHAUSTION_DEFINITION,
    &crate::indicator::overextension_risk::OVEREXTENSION_RISK_DEFINITION,
    &crate::indicator::smoothed_supertrend::SMOOTHED_SUPERTREND_DEFINITION,
    &crate::indicator::coil_release::COIL_RELEASE_DEFINITION,
    &crate::indicator::compression_zones::COMPRESSION_ZONES_DEFINITION,
    &crate::indicator::roc_context::ROC_CONTEXT_DEFINITION,
    &crate::indicator::tradability_filter::TRADABILITY_FILTER_DEFINITION,
    &crate::indicator::reaction_speed::REACTION_SPEED_DEFINITION,
    &crate::indicator::market_state_chain::MARKET_STATE_CHAIN_DEFINITION,
    &crate::indicator::rsi_cycle_stats::RSI_CYCLE_STATS_DEFINITION,
    &crate::indicator::price_average_relation::PRICE_AVERAGE_RELATION_DEFINITION,
];
