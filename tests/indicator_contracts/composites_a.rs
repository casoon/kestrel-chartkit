//! Contracts of the composites a family, from `tests/fixtures/contract_composites_a.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "flow_exhaustion",
    "overextension_risk",
    "smoothed_supertrend",
    "coil_release",
    "compression_zones",
    "roc_context",
    "tradability_filter",
    "reaction_speed",
    "market_state_chain",
    "rsi_cycle_stats",
    "price_average_relation",
];

pub const COVERAGE: &[Coverage] = &[];
