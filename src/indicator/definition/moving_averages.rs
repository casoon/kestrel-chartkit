//! Definitions of the moving averages family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::moving_averages::SMA_DEFINITION,
    &crate::indicator::anchored_vwap::ANCHORED_VWAP_DEFINITION,
    &crate::indicator::moving_averages::EMA_DEFINITION,
    &crate::indicator::moving_averages::WMA_DEFINITION,
    &crate::indicator::moving_averages::VWMA_DEFINITION,
    &crate::indicator::moving_averages::HMA_DEFINITION,
    &crate::indicator::moving_averages::DEMA_DEFINITION,
    &crate::indicator::tema::TEMA_DEFINITION,
    &crate::indicator::moving_averages::KAMA_DEFINITION,
    &crate::indicator::lsma::LSMA_DEFINITION,
    &crate::indicator::mcginley::MCGINLEY_DEFINITION,
    &crate::indicator::t3::T3_DEFINITION,
    &crate::indicator::vidya::VIDYA_DEFINITION,
    &crate::indicator::alligator::ALLIGATOR_DEFINITION,
    &crate::indicator::envelope::ENVELOPE_DEFINITION,
    &crate::indicator::vwap::VWAP_DEFINITION,
    &crate::indicator::trend_relationship::TREND_RELATIONSHIP_DEFINITION,
    &crate::indicator::trend_structural::ICHIMOKU_DEFINITION,
    &crate::indicator::trend_structural::SUPERTREND_DEFINITION,
    &crate::indicator::trend_structural::PARABOLIC_SAR_DEFINITION,
    &crate::indicator::volatility_indicators::DONCHIAN_DEFINITION,
    &crate::indicator::volatility_indicators::KELTNER_DEFINITION,
    &crate::indicator::bollinger::BOLLINGER_DEFINITION,
    &crate::indicator::twap::TWAP_DEFINITION,
    &crate::indicator::midas::MIDAS_DEFINITION,
];
