//! Definitions of the volatility family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::adx::ADX_DEFINITION,
    &crate::indicator::trend_structural::DMI_DEFINITION,
    &crate::indicator::trend_structural::AROON_DEFINITION,
    &crate::indicator::vortex::VORTEX_DEFINITION,
    &crate::indicator::choppiness::CHOPPINESS_DEFINITION,
    &crate::indicator::relative_volatility::RELATIVE_VOLATILITY_DEFINITION,
    &crate::indicator::ulcer::ULCER_INDEX_DEFINITION,
    &crate::indicator::volatility_indicators::TRUE_RANGE_DEFINITION,
    &crate::indicator::volatility_indicators::HISTORICAL_VOLATILITY_DEFINITION,
    &crate::indicator::volatility_indicators::GARMAN_KLASS_DEFINITION,
    &crate::indicator::mass_index::MASS_INDEX_DEFINITION,
    &crate::indicator::chande_kroll::CHANDE_KROLL_DEFINITION,
    &crate::indicator::chandelier_exit::CHANDELIER_EXIT_DEFINITION,
    &crate::indicator::adaptive_chandelier::ADAPTIVE_CHANDELIER_DEFINITION,
    &crate::indicator::vix_fix::VIX_FIX_DEFINITION,
    &crate::indicator::efficiency::EFFICIENCY_DEFINITION,
    &crate::indicator::volatility_regime::VOLATILITY_REGIME_DEFINITION,
];
