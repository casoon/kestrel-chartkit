//! Definitions of the volume family (plan 49).

use super::IndicatorDefinition;

pub(super) const DEFINITIONS: &[&IndicatorDefinition] = &[
    &crate::indicator::pvt::PVT_DEFINITION,
    &crate::indicator::force_index::EFI_DEFINITION,
    &crate::indicator::rvat::RVAT_DEFINITION,
    &crate::indicator::volume_flow_hires::HIRES_VOLUME_FLOW_DEFINITION,
    &crate::indicator::volume_profile_extended::EXTENDED_VOLUME_PROFILE_DEFINITION,
    &crate::indicator::volume_profile_persistent::PERSISTENT_VOLUME_PROFILE_DEFINITION,
    &crate::indicator::volume_profile::VOLUME_PROFILE_DEFINITION,
    &crate::indicator::money_flow_profile::MONEY_FLOW_PROFILE_DEFINITION,
    &crate::indicator::volume_indicators::VOLUME_DEFINITION,
    &crate::indicator::volume_indicators::RVOL_DEFINITION,
    &crate::indicator::volume_indicators::OBV_DEFINITION,
    &crate::indicator::volume_indicators::CMF_DEFINITION,
    &crate::indicator::volume_indicators::ACC_DIST_DEFINITION,
    &crate::indicator::volume_flow::CVD_DEFINITION,
    &crate::indicator::volume_flow::KLINGER_DEFINITION,
    &crate::indicator::eom::EOM_DEFINITION,
    &crate::indicator::nvi_pvi::NVI_DEFINITION,
    &crate::indicator::nvi_pvi::PVI_DEFINITION,
    &crate::indicator::chaikin_osc::CHAIKIN_OSCILLATOR_DEFINITION,
];
