#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Quantitative version of "da ist nichts im Weg" (plan Anhang G.3). `lvn_width_atr` and
/// `distance_to_hvn_atr` are expected to already be ATR-normalized by the caller;
/// `volume_density` is the traded-volume share at the current price bucket (0.0..1.0, lower
/// = thinner / more "free space").
///
/// Deliberately distinct from real-time order-book liquidity — this measures **historical
/// volume void / LVN**, not **current order-book liquidity** (plan Anhang G.3: "keine
/// Liquidität im Weg" clarification). The latter needs order-book data this crate does not
/// have.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FreeSpaceScore {
    /// 0.0..100.0, higher = more free space ahead.
    pub score: f64,
    pub lvn_width_atr: f64,
    pub distance_to_hvn_atr: f64,
    pub volume_density: f64,
}

pub fn build_free_space_score(
    lvn_width_atr: f64,
    distance_to_hvn_atr: f64,
    volume_density: f64,
) -> FreeSpaceScore {
    let density_component = (1.0 - volume_density.clamp(0.0, 1.0)) * 40.0;
    let width_component = lvn_width_atr.clamp(0.0, 3.0) / 3.0 * 30.0;
    let distance_component = distance_to_hvn_atr.clamp(0.0, 3.0) / 3.0 * 30.0;
    let score = (density_component + width_component + distance_component).clamp(0.0, 100.0);
    FreeSpaceScore {
        score,
        lvn_width_atr,
        distance_to_hvn_atr,
        volume_density,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_wide_far_book_scores_high() {
        let out = build_free_space_score(3.0, 3.0, 0.0);
        assert!((out.score - 100.0).abs() < 1e-9);
    }

    #[test]
    fn dense_narrow_close_book_scores_low() {
        let out = build_free_space_score(0.0, 0.0, 1.0);
        assert!(out.score.abs() < 1e-9);
    }
}
