//! Definitions of the structure family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::order_block::ORDER_BLOCK_DEFINITION,
    &crate::indicator::liquidity_fvg::LIQUIDITY_FVG_DEFINITION,
    &crate::indicator::market_structure_breaks::MARKET_STRUCTURE_BREAKS_DEFINITION,
    &crate::indicator::pivots_structure::PIVOTS_STRUCTURE_DEFINITION,
    &crate::indicator::candle_classification::CANDLE_CLASSIFICATION_DEFINITION,
    &crate::indicator::zigzag::ZIGZAG_DEFINITION,
    &crate::indicator::pivot_sets::PIVOT_SETS_DEFINITION,
    &crate::indicator::bos_choch::BOS_CHOCH_DEFINITION,
    &crate::indicator::liquidity_sweeps::LIQUIDITY_SWEEPS_DEFINITION,
    &crate::indicator::zone_rejection::ZONE_REJECTION_DEFINITION,
    &crate::indicator::double_pattern::DOUBLE_PATTERN_DEFINITION,
    &crate::indicator::bottom_formation::BOTTOM_FORMATION_DEFINITION,
    &crate::indicator::pivot_divergence::PIVOT_DIVERGENCE_DEFINITION,
    &crate::indicator::pivot_divergence::OSCILLATOR_PIVOT_DIVERGENCE_DEFINITION,
    &crate::indicator::smart_money_structure::LIQUIDITY_POOLS_DEFINITION,
    &crate::indicator::wyckoff::WYCKOFF_DEFINITION,
    &crate::indicator::zigzag_advanced::ZIGZAG_ADVANCED_DEFINITION,
];
