//! Contracts of the composites b family, from `tests/fixtures/contract_composites_b.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "leg_conviction",
    "trend_break_risk",
    "sweep_reversal_score",
    "equal_level_liquidity",
    "leg_character",
    "accumulation_phase",
    "reversal_setup_score",
    "structure_zones",
    "structural_flow",
    "trend_quality",
    "buy_sell_pressure",
    "multi_factor",
];

pub const COVERAGE: &[Coverage] = &[];
