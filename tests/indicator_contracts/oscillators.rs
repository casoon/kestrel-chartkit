//! Contracts of the oscillators family, from `tests/fixtures/contract_oscillators.txt`.

use super::Coverage;

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[
    "macd",
    "stoch_rsi",
    "cci",
    "mfi",
    "pmo",
    "smi",
    "rci",
    "trix",
    "williams_r",
    "tsi",
    "fisher_transform",
    "stochastic",
    "roc",
    "ultimate_oscillator",
    "awesome_oscillator",
    "ppo",
    "wavetrend",
    "cmo",
    "elder_ray",
    "connors_rsi",
    "coppock",
    "dpo",
    "kst",
    "rvi",
    "bop",
    "bbtrend",
    "zscore",
];

pub const COVERAGE: &[Coverage] = &[];
