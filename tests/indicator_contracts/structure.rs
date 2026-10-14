//! Contracts of the structure family, from `tests/fixtures/contract_structure.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "liquidity_fvg",
    "market_structure_breaks",
    "pivots_structure",
    "candle_classification",
    "zigzag",
    "zigzag_advanced",
    "pivot_sets",
    "bos_choch",
    "liquidity_sweeps",
    "zone_rejection",
    "double_pattern",
    "bottom_formation",
    "pivot_divergence",
    "oscillator_pivot_divergence",
    "liquidity_pools",
    "wyckoff",
];

pub const COVERAGE: &[Coverage] = &[];
