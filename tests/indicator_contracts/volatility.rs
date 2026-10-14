//! Contracts of the volatility family, from `tests/fixtures/contract_volatility.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "adx",
    "dmi",
    "aroon",
    "vortex",
    "choppiness",
    "relative_volatility",
    "ulcer_index",
    "true_range",
    "historical_volatility",
    "garman_klass",
    "mass_index",
    "chande_kroll",
    "chandelier_exit",
    "adaptive_chandelier",
    "vix_fix",
    "efficiency",
    "volatility_regime",
];

pub const COVERAGE: &[Coverage] = &[];
