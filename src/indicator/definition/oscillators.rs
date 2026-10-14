//! Definitions of the oscillators family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::macd::MACD_DEFINITION,
    &crate::indicator::pmo::PMO_DEFINITION,
    &crate::indicator::smi::SMI_DEFINITION,
    &crate::indicator::rci::RCI_DEFINITION,
    &crate::indicator::trix::TRIX_DEFINITION,
    &crate::indicator::tsi::TSI_DEFINITION,
    &crate::indicator::momentum_indicators::STOCHASTIC_DEFINITION,
    &crate::indicator::momentum_indicators::ROC_DEFINITION,
    &crate::indicator::momentum_indicators::ULTIMATE_OSCILLATOR_DEFINITION,
    &crate::indicator::momentum_indicators::AWESOME_OSCILLATOR_DEFINITION,
    &crate::indicator::momentum_indicators::PPO_DEFINITION,
    &crate::indicator::momentum_indicators::CMO_DEFINITION,
    &crate::indicator::momentum_indicators::ELDER_RAY_DEFINITION,
    &crate::indicator::connors_rsi::CONNORS_RSI_DEFINITION,
    &crate::indicator::coppock::COPPOCK_DEFINITION,
    &crate::indicator::dpo::DPO_DEFINITION,
    &crate::indicator::kst::KST_DEFINITION,
    &crate::indicator::rvi::RVI_DEFINITION,
    &crate::indicator::bop::BOP_DEFINITION,
    &crate::indicator::bbtrend::BBTREND_DEFINITION,
    &crate::indicator::zscore::ZSCORE_DEFINITION,
    &crate::indicator::rsi::RSI_DEFINITION,
    &crate::indicator::stoch_rsi::STOCH_RSI_DEFINITION,
    &crate::indicator::cci::CCI_DEFINITION,
    &crate::indicator::mfi::MFI_DEFINITION,
    &crate::indicator::williams_r::WILLIAMS_R_DEFINITION,
    &crate::indicator::fisher_transform::FISHER_TRANSFORM_DEFINITION,
    &crate::indicator::wavetrend::WAVETREND_DEFINITION,
];
