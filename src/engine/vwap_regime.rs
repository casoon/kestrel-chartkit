use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum SlopeState {
    StronglyFalling,
    ModeratelyFalling,
    Flat,
    ModeratelyRising,
    StronglyRising,
}

/// VWAP Regime Engine output (plan Anhang A, "Ergänzung (zehntes Video, VWAP)"): turns VWAP
/// from a simple over/under line into a regime signal via slope, normalized distance, and
/// how often/how persistently price sits on one side.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VwapRegimeOutput {
    pub slope_atr: f64,
    pub slope_state: SlopeState,
    pub distance_atr: f64,
    pub z_score: f64,
    /// Crosses per bar over the tracked window, 0.0..1.0 — high = Balance, low = Trend.
    pub cross_frequency: f64,
    /// Share of bars on the current side of VWAP, 0.0..1.0.
    pub price_persistence: f64,
}

pub fn classify_slope(slope_atr: f64, flat_threshold: f64, strong_threshold: f64) -> SlopeState {
    if slope_atr >= strong_threshold {
        SlopeState::StronglyRising
    } else if slope_atr >= flat_threshold {
        SlopeState::ModeratelyRising
    } else if slope_atr <= -strong_threshold {
        SlopeState::StronglyFalling
    } else if slope_atr <= -flat_threshold {
        SlopeState::ModeratelyFalling
    } else {
        SlopeState::Flat
    }
}

/// Tracks cross-frequency and price-persistence relative to VWAP over a rolling window — feed
/// it `price - vwap` each bar (e.g. from `indicator::vwap::Vwap`'s output).
pub struct VwapRegimeTracker {
    window: usize,
    diffs: VecDeque<f64>,
}

impl VwapRegimeTracker {
    pub fn new(window: usize) -> Self {
        Self {
            window,
            diffs: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.diffs.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        price_minus_vwap: f64,
        atr: f64,
        sigma: f64,
        slope_atr: f64,
        flat_threshold: f64,
        strong_threshold: f64,
    ) -> VwapRegimeOutput {
        self.diffs.push_back(price_minus_vwap);
        if self.diffs.len() > self.window {
            self.diffs.pop_front();
        }

        let mut crosses = 0usize;
        for pair in self.diffs.iter().collect::<Vec<_>>().windows(2) {
            if (*pair[0] >= 0.0) != (*pair[1] >= 0.0) {
                crosses += 1;
            }
        }
        let above = self.diffs.iter().filter(|d| **d >= 0.0).count();
        let n = self.diffs.len().max(1);
        let cross_frequency = crosses as f64 / n as f64;
        let side_count = above.max(n - above);
        let price_persistence = side_count as f64 / n as f64;

        VwapRegimeOutput {
            slope_atr,
            slope_state: classify_slope(slope_atr, flat_threshold, strong_threshold),
            distance_atr: if atr > 0.0 {
                price_minus_vwap / atr
            } else {
                0.0
            },
            z_score: if sigma > 0.0 {
                price_minus_vwap / sigma
            } else {
                0.0
            },
            cross_frequency,
            price_persistence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_classification_boundaries() {
        assert_eq!(classify_slope(0.5, 0.1, 0.3), SlopeState::StronglyRising);
        assert_eq!(classify_slope(0.2, 0.1, 0.3), SlopeState::ModeratelyRising);
        assert_eq!(classify_slope(0.0, 0.1, 0.3), SlopeState::Flat);
        assert_eq!(
            classify_slope(-0.2, 0.1, 0.3),
            SlopeState::ModeratelyFalling
        );
        assert_eq!(classify_slope(-0.5, 0.1, 0.3), SlopeState::StronglyFalling);
    }

    #[test]
    fn persistent_one_sided_series_has_low_cross_frequency_high_persistence() {
        let mut tracker = VwapRegimeTracker::new(20);
        let mut out = None;
        for _ in 0..20 {
            out = Some(tracker.update(1.0, 2.0, 0.5, 0.2, 0.1, 0.3));
        }
        let out = out.unwrap();
        assert!((out.cross_frequency).abs() < 1e-9);
        assert!((out.price_persistence - 1.0).abs() < 1e-9);
        assert!((out.distance_atr - 0.5).abs() < 1e-9);
        assert!((out.z_score - 2.0).abs() < 1e-9);
    }

    #[test]
    fn alternating_series_has_high_cross_frequency() {
        let mut tracker = VwapRegimeTracker::new(20);
        let mut out = None;
        for i in 0..20 {
            let diff = if i % 2 == 0 { 1.0 } else { -1.0 };
            out = Some(tracker.update(diff, 2.0, 0.5, 0.0, 0.1, 0.3));
        }
        let out = out.unwrap();
        assert!(out.cross_frequency > 0.8);
    }
}
