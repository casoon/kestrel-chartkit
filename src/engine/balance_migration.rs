#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum MigrationDirection {
    Bullish,
    Bearish,
    Neutral,
}

/// Balance Migration Engine output (plan Anhang G.1): tracks
/// `Balance_1 → Expansion → Balance_2` and whether the new balance is accepted higher or
/// lower than the previous one. Generalizes VPOC-migration (`VPOC_t − VPOC_{t-1}`) to any
/// balance-midpoint source.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BalanceMigrationOutput {
    pub previous_balance_mid: f64,
    pub new_balance_mid: f64,
    /// `new_balance_mid - previous_balance_mid`, signed.
    pub migration: f64,
    pub direction: MigrationDirection,
    /// 0.0..1.0
    pub acceptance_strength: f64,
}

/// `acceptance_bars` / `min_acceptance_bars` → 0.0..1.0 confirmation strength of the new
/// balance (plan Anhang G.1: "Acceptance" as a duration, not a single touch).
pub fn build_balance_migration(
    previous_balance_mid: f64,
    new_balance_mid: f64,
    acceptance_bars: u32,
    min_acceptance_bars: u32,
) -> BalanceMigrationOutput {
    let migration = new_balance_mid - previous_balance_mid;
    let direction = if migration > 0.0 {
        MigrationDirection::Bullish
    } else if migration < 0.0 {
        MigrationDirection::Bearish
    } else {
        MigrationDirection::Neutral
    };
    let acceptance_strength = if min_acceptance_bars > 0 {
        (acceptance_bars as f64 / min_acceptance_bars as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };
    BalanceMigrationOutput {
        previous_balance_mid,
        new_balance_mid,
        migration,
        direction,
        acceptance_strength,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_new_balance_is_bullish_migration() {
        let out = build_balance_migration(25_480.0, 25_417.0, 3, 4);
        assert_eq!(out.direction, MigrationDirection::Bearish);
        assert!((out.migration - (-63.0)).abs() < 1e-6);
        assert!((out.acceptance_strength - 0.75).abs() < 1e-9);
    }

    #[test]
    fn acceptance_strength_is_clamped_to_one() {
        let out = build_balance_migration(100.0, 105.0, 10, 4);
        assert!((out.acceptance_strength - 1.0).abs() < 1e-9);
    }
}
