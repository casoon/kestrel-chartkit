//! Contracts of the moving averages family, from `tests/fixtures/contract_moving_averages.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "ema",
    "wma",
    "vwma",
    "hma",
    "dema",
    "tema",
    "kama",
    "lsma",
    "mcginley",
    "t3",
    "vidya",
    "alligator",
    "envelope",
    "twap",
    "vwap",
    "anchored_vwap",
    "midas",
    "trend_relationship",
    "ichimoku",
    "supertrend",
    "parabolic_sar",
    "donchian",
    "keltner",
    "bollinger",
];

pub const COVERAGE: &[Coverage] = &[];
