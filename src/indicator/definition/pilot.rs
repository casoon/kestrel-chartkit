//! The three pilot definitions of plan 49: a plain count, a choice with a numeric code, and
//! typed-only choices with a timestamp.

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::moving_averages::SMA_DEFINITION,
    &crate::indicator::atr::ATR_DEFINITION,
    &crate::indicator::anchored_vwap::ANCHORED_VWAP_DEFINITION,
];
