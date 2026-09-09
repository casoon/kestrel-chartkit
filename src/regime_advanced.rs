//! Advanced regime-model building blocks layered on top of [`crate::regime::classify_regime`]'s
//! single-shot four-class output: empirical Markov transition probabilities, trend persistence
//! (streak length), a streaming predictability index, hysteretic (chatter-free) level transitions,
//! and adaptive swing-length tracking as a dominant-cycle-length proxy.

use std::collections::{HashMap, VecDeque};

use crate::model::MarketRegime;
use crate::stats::linear_regression;

/// Empirical Markov transition model over [`MarketRegime`] states: learns `P(to | from)` from an
/// observed regime sequence rather than assuming a fixed transition structure.
#[derive(Debug, Clone, Default)]
pub struct RegimeMarkovModel {
    counts: HashMap<(MarketRegime, MarketRegime), u32>,
    totals: HashMap<MarketRegime, u32>,
}

impl RegimeMarkovModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one observed `from -> to` transition (call with `from == to` for "stayed in the
    /// same regime this bar" too, so self-transition probability is learned as well).
    pub fn observe_transition(&mut self, from: MarketRegime, to: MarketRegime) {
        *self.counts.entry((from, to)).or_insert(0) += 1;
        *self.totals.entry(from).or_insert(0) += 1;
    }

    /// Empirical `P(to | from)`. `0.0` if `from` has never been observed.
    pub fn transition_probability(&self, from: MarketRegime, to: MarketRegime) -> f64 {
        let total = match self.totals.get(&from) {
            Some(&t) if t > 0 => t,
            _ => return 0.0,
        };
        let count = self.counts.get(&(from, to)).copied().unwrap_or(0);
        count as f64 / total as f64
    }

    /// Full distribution over next states given `from`, most probable first. Empty if `from` has
    /// never been observed.
    pub fn next_state_distribution(&self, from: MarketRegime) -> Vec<(MarketRegime, f64)> {
        let states = [
            MarketRegime::BullishExpansion,
            MarketRegime::BearishExpansion,
            MarketRegime::Consolidation,
            MarketRegime::Transition,
        ];
        let mut dist: Vec<(MarketRegime, f64)> = states
            .into_iter()
            .map(|to| (to, self.transition_probability(from, to)))
            .filter(|(_, p)| *p > 0.0)
            .collect();
        dist.sort_by(|a, b| b.1.total_cmp(&a.1));
        dist
    }
}

/// Tracks how long the current regime has persisted and, from history, its typical persistence.
#[derive(Debug, Clone)]
pub struct RegimePersistenceTracker {
    current: Option<MarketRegime>,
    bars_in_regime: u32,
    completed_streaks: VecDeque<u32>,
    max_history: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegimePersistenceOutput {
    pub bars_in_regime: u32,
    /// `true` on the bar the regime changed (from the second observed regime onward).
    pub changed: bool,
    /// Mean length (in bars) of completed regime streaks so far. `None` until at least one
    /// regime change has completed a streak.
    pub average_streak_length: Option<f64>,
}

impl RegimePersistenceTracker {
    pub fn new(max_history: usize) -> Self {
        Self {
            current: None,
            bars_in_regime: 0,
            completed_streaks: VecDeque::new(),
            max_history: max_history.max(1),
        }
    }

    pub fn reset(&mut self) {
        self.current = None;
        self.bars_in_regime = 0;
        self.completed_streaks.clear();
    }

    pub fn update(&mut self, regime: MarketRegime) -> RegimePersistenceOutput {
        let changed = match self.current {
            Some(prev) if prev != regime => {
                if self.completed_streaks.len() >= self.max_history {
                    self.completed_streaks.pop_front();
                }
                self.completed_streaks.push_back(self.bars_in_regime);
                self.bars_in_regime = 0;
                true
            }
            None => false,
            _ => false,
        };

        self.current = Some(regime);
        self.bars_in_regime += 1;

        let average_streak_length = if self.completed_streaks.is_empty() {
            None
        } else {
            Some(
                self.completed_streaks.iter().copied().sum::<u32>() as f64
                    / self.completed_streaks.len() as f64,
            )
        };

        RegimePersistenceOutput {
            bars_in_regime: self.bars_in_regime,
            changed,
            average_streak_length,
        }
    }
}

/// Streaming predictability index: the R^2 of an OLS fit over the trailing `window_len` closes —
/// close to `1.0` means a clean, linear trend; close to `0.0` means noisy/directionless movement.
#[derive(Debug, Clone)]
pub struct PredictabilityTracker {
    window_len: usize,
    buffer: VecDeque<f64>,
}

impl PredictabilityTracker {
    pub fn new(window_len: usize) -> Self {
        let window_len = window_len.max(2);
        Self {
            window_len,
            buffer: VecDeque::with_capacity(window_len),
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Returns `None` until `window_len` values have been fed.
    pub fn update(&mut self, close: f64) -> Option<f64> {
        if self.buffer.len() >= self.window_len {
            self.buffer.pop_front();
        }
        self.buffer.push_back(close);
        if self.buffer.len() < self.window_len {
            return None;
        }
        let values: Vec<f64> = self.buffer.iter().copied().collect();
        linear_regression(&values).map(|r| r.r2)
    }
}

/// Three-level hysteretic (Schmitt-trigger-style) classification, so a score oscillating near a
/// single threshold does not flap the classified level back and forth every bar.
///
/// Contract: `enter_low <= exit_low <= exit_high <= enter_high`. From [`HysteresisLevel::Neutral`]
/// a value must reach `enter_high`/`enter_low` to leave; from [`HysteresisLevel::High`]/
/// [`HysteresisLevel::Low`] a value must retreat past `exit_high`/`exit_low` to fall back to
/// `Neutral` (an extreme-to-extreme jump takes two updates: extreme -> Neutral -> the other
/// extreme, never a same-bar jump between the two extremes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HysteresisLevel {
    Low,
    Neutral,
    High,
}

#[derive(Debug, Clone, Copy)]
pub struct HysteresisBand {
    enter_low: f64,
    exit_low: f64,
    exit_high: f64,
    enter_high: f64,
    level: HysteresisLevel,
}

impl HysteresisBand {
    pub fn new(enter_low: f64, exit_low: f64, exit_high: f64, enter_high: f64) -> Self {
        Self {
            enter_low,
            exit_low,
            exit_high,
            enter_high,
            level: HysteresisLevel::Neutral,
        }
    }

    pub fn level(&self) -> HysteresisLevel {
        self.level
    }

    pub fn reset(&mut self) {
        self.level = HysteresisLevel::Neutral;
    }

    pub fn update(&mut self, value: f64) -> HysteresisLevel {
        self.level = match self.level {
            HysteresisLevel::High => {
                if value <= self.exit_high {
                    HysteresisLevel::Neutral
                } else {
                    HysteresisLevel::High
                }
            }
            HysteresisLevel::Low => {
                if value >= self.exit_low {
                    HysteresisLevel::Neutral
                } else {
                    HysteresisLevel::Low
                }
            }
            HysteresisLevel::Neutral => {
                if value >= self.enter_high {
                    HysteresisLevel::High
                } else if value <= self.enter_low {
                    HysteresisLevel::Low
                } else {
                    HysteresisLevel::Neutral
                }
            }
        };
        self.level
    }
}

/// Adaptive swing-length tracking: counts bars between sign crossings of a signed input (an
/// oscillator or signed regime score) as an empirical, adapting proxy for the dominant cycle
/// length, without claiming spectral/Hilbert-transform precision.
#[derive(Debug, Clone)]
pub struct AdaptiveCycleTracker {
    prev_sign: Option<i8>,
    bars_since_crossing: u32,
    recent_swing_lengths: VecDeque<u32>,
    max_history: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveCycleOutput {
    pub bars_since_crossing: u32,
    /// Mean bars-between-crossings over recent swings. `None` until at least one full swing
    /// (crossing to crossing) has been observed.
    pub average_swing_length: Option<f64>,
}

impl AdaptiveCycleTracker {
    pub fn new(max_history: usize) -> Self {
        Self {
            prev_sign: None,
            bars_since_crossing: 0,
            recent_swing_lengths: VecDeque::new(),
            max_history: max_history.max(1),
        }
    }

    pub fn reset(&mut self) {
        self.prev_sign = None;
        self.bars_since_crossing = 0;
        self.recent_swing_lengths.clear();
    }

    pub fn update(&mut self, value: f64) -> AdaptiveCycleOutput {
        let sign: i8 = if value > 0.0 {
            1
        } else if value < 0.0 {
            -1
        } else {
            0
        };

        // A crossing bar is the first bar of the new swing, so the completed swing's length is
        // `bars_since_crossing` *before* this bar's increment below.
        let crossed =
            matches!(self.prev_sign, Some(prev) if prev != 0 && sign != 0 && prev != sign);
        if crossed {
            if self.recent_swing_lengths.len() >= self.max_history {
                self.recent_swing_lengths.pop_front();
            }
            self.recent_swing_lengths
                .push_back(self.bars_since_crossing);
            self.bars_since_crossing = 0;
        }

        self.bars_since_crossing += 1;

        if sign != 0 {
            self.prev_sign = Some(sign);
        }

        let average_swing_length = if self.recent_swing_lengths.is_empty() {
            None
        } else {
            Some(
                self.recent_swing_lengths.iter().copied().sum::<u32>() as f64
                    / self.recent_swing_lengths.len() as f64,
            )
        };

        AdaptiveCycleOutput {
            bars_since_crossing: self.bars_since_crossing,
            average_swing_length,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markov_model_learns_empirical_transitions() {
        let mut model = RegimeMarkovModel::new();
        let sequence = [
            MarketRegime::BullishExpansion,
            MarketRegime::BullishExpansion,
            MarketRegime::Consolidation,
            MarketRegime::BullishExpansion,
            MarketRegime::BullishExpansion,
        ];
        for pair in sequence.windows(2) {
            model.observe_transition(pair[0], pair[1]);
        }

        // BullishExpansion -> BullishExpansion happened twice, -> Consolidation once (3 total).
        let p_stay = model.transition_probability(
            MarketRegime::BullishExpansion,
            MarketRegime::BullishExpansion,
        );
        assert!((p_stay - 2.0 / 3.0).abs() < 1e-9);

        let dist = model.next_state_distribution(MarketRegime::BullishExpansion);
        assert_eq!(dist[0].0, MarketRegime::BullishExpansion);

        // Never-observed source state has an empty distribution.
        assert!(model
            .next_state_distribution(MarketRegime::BearishExpansion)
            .is_empty());
    }

    #[test]
    fn test_persistence_tracker_counts_streaks_and_changes() {
        let mut tracker = RegimePersistenceTracker::new(10);
        let regimes = [
            MarketRegime::BullishExpansion,
            MarketRegime::BullishExpansion,
            MarketRegime::BullishExpansion,
            MarketRegime::Consolidation,
            MarketRegime::Consolidation,
        ];
        let mut outputs = Vec::new();
        for regime in regimes {
            outputs.push(tracker.update(regime));
        }

        assert!(!outputs[0].changed);
        assert_eq!(outputs[2].bars_in_regime, 3);
        assert!(outputs[3].changed, "regime change must be flagged");
        assert_eq!(outputs[3].bars_in_regime, 1);
        assert_eq!(outputs[3].average_streak_length, Some(3.0));
    }

    #[test]
    fn test_predictability_tracker_scores_clean_trend_high() {
        let mut tracker = PredictabilityTracker::new(10);
        let mut last = None;
        for i in 0..10 {
            last = tracker.update(100.0 + i as f64);
        }
        assert!(
            last.unwrap() > 0.99,
            "a perfectly linear trend must score near 1.0"
        );
    }

    #[test]
    fn test_predictability_tracker_scores_noise_low() {
        let mut tracker = PredictabilityTracker::new(6);
        let mut last = None;
        for v in [100.0, 105.0, 98.0, 107.0, 96.0, 109.0] {
            last = tracker.update(v);
        }
        assert!(
            last.unwrap() < 0.3,
            "zig-zagging noise must score low predictability"
        );
    }

    #[test]
    fn test_hysteresis_band_does_not_chatter_near_a_single_threshold() {
        let mut band = HysteresisBand::new(-2.0, -1.0, 1.0, 2.0);
        assert_eq!(band.update(2.5), HysteresisLevel::High);
        // Oscillates between the enter/exit thresholds without ever falling to/below exit_high:
        // must stay High the whole time (no chatter).
        for v in [1.8, 1.2, 1.9, 1.3, 1.7] {
            assert_eq!(band.update(v), HysteresisLevel::High);
        }
        // Now genuinely retreats past exit_high -> Neutral.
        assert_eq!(band.update(0.5), HysteresisLevel::Neutral);
    }

    #[test]
    fn test_hysteresis_band_extreme_to_extreme_passes_through_neutral() {
        let mut band = HysteresisBand::new(-2.0, -1.0, 1.0, 2.0);
        assert_eq!(band.update(2.5), HysteresisLevel::High);
        assert_eq!(band.update(-2.5), HysteresisLevel::Neutral);
        assert_eq!(band.update(-2.5), HysteresisLevel::Low);
    }

    #[test]
    fn test_adaptive_cycle_tracker_measures_swing_length() {
        let mut tracker = AdaptiveCycleTracker::new(10);
        // +,+,+,+ (4 bars) then -,-,-,- (4 bars): one sign crossing after 4 bars.
        let values = [1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0];
        let mut last = AdaptiveCycleOutput {
            bars_since_crossing: 0,
            average_swing_length: None,
        };
        for v in values {
            last = tracker.update(v);
        }
        assert_eq!(last.average_swing_length, Some(4.0));
        assert_eq!(last.bars_since_crossing, 4);
    }
}
