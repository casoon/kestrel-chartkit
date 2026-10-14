//! Contracts of the volume family, from `tests/fixtures/contract_volume.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "pvt",
    "efi",
    "rvat",
    "hires_volume_flow",
    "extended_volume_profile",
    "persistent_volume_profile",
    "volume_profile",
    "money_flow_profile",
    "volume",
    "rvol",
    "obv",
    "cmf",
    "acc_dist",
    "cvd",
    "klinger",
    "eom",
    "nvi",
    "pvi",
    "chaikin_oscillator",
];

pub const COVERAGE: &[Coverage] = &[];
