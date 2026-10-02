//! Exhaustion: an extreme that is fading while price still pushes.
//!
//! An extreme reading on its own usually marks a strong, healthy move, not a turn. Exhaustion
//! additionally requires the oscillator to be fading already while price is still making a new
//! high (low) — the definition of the Elder Ray Pressure Engine (WavesUnchained), applied to any
//! oscillator line. Kestrel plan/55 point 2.
//!
//! - **Bearish exhaustion** (an up-move tiring): within the last `recency` bars, including the
//!   current one, the line ranked at or above `percentile` of its previous `lookback` values;
//!   the line is lower than on the previous bar; and the bar's high reaches at least the highest
//!   high of the previous `push` bars.
//! - **Bullish exhaustion**: the mirror image, with a rank at or below `100 - percentile`, a
//!   rising line and a low at or below the lowest low of the previous `push` bars.
//!
//! An outside bar that satisfies both reports neither — there is no basis for a direction.
//! The rank is a percentile against the line's own recent history, not a fixed OB/OS level, so
//! the same parameters fit RSI, CCI or any other line without rescaling.

use std::collections::VecDeque;

/// Default history for the percentile rank.
pub const DEFAULT_LOOKBACK: usize = 200;
/// Default percentile that counts as extreme.
pub const DEFAULT_PERCENTILE: f64 = 90.0;
/// Default number of bars, including the current one, for which an extreme stays relevant.
pub const DEFAULT_RECENCY: usize = 3;
/// Default number of previous bars price must still exceed.
pub const DEFAULT_PUSH: usize = 3;

/// What one bar of [`Exhaustion::update`] found.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExhaustionSignal {
    /// A down-move is exhausting — bullish.
    pub bull: bool,
    /// An up-move is exhausting — bearish.
    pub bear: bool,
    /// `0..=1`: how far beyond the percentile the recent extreme reached.
    pub strength: f64,
}

/// Streaming exhaustion detector over one oscillator line.
#[derive(Debug, Clone)]
pub struct Exhaustion {
    lookback: usize,
    percentile: f64,
    recency: usize,
    push: usize,
    values: VecDeque<f64>,
    ranks: VecDeque<f64>,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    prev_value: Option<f64>,
}

impl Exhaustion {
    pub fn new(lookback: usize, percentile: f64, recency: usize, push: usize) -> Self {
        Self {
            lookback: lookback.max(1),
            percentile: percentile.clamp(50.0, 100.0),
            recency: recency.max(1),
            push: push.max(1),
            values: VecDeque::new(),
            ranks: VecDeque::new(),
            highs: VecDeque::new(),
            lows: VecDeque::new(),
            prev_value: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            DEFAULT_LOOKBACK,
            DEFAULT_PERCENTILE,
            DEFAULT_RECENCY,
            DEFAULT_PUSH,
        )
    }

    /// Bars until the first signal can appear: the rank needs `lookback` previous values.
    pub fn warmup_period(&self) -> usize {
        self.lookback + 1
    }

    /// Feeds one bar: the oscillator's line value and the bar's high and low.
    pub fn update(&mut self, value: f64, high: f64, low: f64) -> ExhaustionSignal {
        // Percentile rank of the current value among the previous `lookback` values, as Pine's
        // `ta.percentrank`: the share of previous values less than or equal to it.
        let rank = (self.values.len() == self.lookback).then(|| {
            let below = self.values.iter().filter(|v| **v <= value).count();
            100.0 * below as f64 / self.lookback as f64
        });
        let previous_high = self.highs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let previous_low = self.lows.iter().cloned().fold(f64::INFINITY, f64::min);
        let price_history_full = self.highs.len() == self.push;

        push_capped(&mut self.values, value, self.lookback);
        push_capped(&mut self.highs, high, self.push);
        push_capped(&mut self.lows, low, self.push);
        push_capped(&mut self.ranks, rank.unwrap_or(f64::NAN), self.recency);

        let prev_value = self.prev_value.replace(value);
        let (Some(prev_value), true) = (prev_value, price_history_full) else {
            return ExhaustionSignal::default();
        };

        let upper = self.percentile;
        let lower = 100.0 - self.percentile;
        let recent_max = self
            .ranks
            .iter()
            .filter(|r| r.is_finite())
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let recent_min = self
            .ranks
            .iter()
            .filter(|r| r.is_finite())
            .cloned()
            .fold(f64::INFINITY, f64::min);

        let bear = recent_max >= upper && value < prev_value && high >= previous_high;
        let bull = recent_min <= lower && value > prev_value && low <= previous_low;
        let span = (100.0 - upper).max(f64::EPSILON);
        match (bull, bear) {
            (true, false) => ExhaustionSignal {
                bull: true,
                bear: false,
                strength: ((lower - recent_min) / span).clamp(0.0, 1.0),
            },
            (false, true) => ExhaustionSignal {
                bull: false,
                bear: true,
                strength: ((recent_max - upper) / span).clamp(0.0, 1.0),
            },
            // Both at once (outside bar) or neither: no direction.
            _ => ExhaustionSignal::default(),
        }
    }

    pub fn reset(&mut self) {
        self.values.clear();
        self.ranks.clear();
        self.highs.clear();
        self.lows.clear();
        self.prev_value = None;
    }
}

fn push_capped(window: &mut VecDeque<f64>, value: f64, cap: usize) {
    if window.len() == cap {
        window.pop_front();
    }
    window.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `(value, high, low)` triples and returns the last signal.
    fn feed(detector: &mut Exhaustion, bars: &[(f64, f64, f64)]) -> ExhaustionSignal {
        let mut last = ExhaustionSignal::default();
        for &(v, h, l) in bars {
            last = detector.update(v, h, l);
        }
        last
    }

    /// History for the rank: values spread over 0..50 with a price drifting gently, so neither
    /// the rank nor the push conditions sit exactly in the middle of anything.
    fn history(n: usize) -> Vec<(f64, f64, f64)> {
        (0..n)
            .map(|i| {
                let v = (i * 7 % 50) as f64 + 0.3;
                let p = 100.0 + i as f64 * 0.01;
                (v, p + 0.4, p - 0.6)
            })
            .collect()
    }

    #[test]
    fn fading_extreme_with_new_high_is_bearish_exhaustion() {
        let mut d = Exhaustion::new(20, 90.0, 3, 3);
        let mut bars = history(20);
        // An extreme reading on a new high: strong move, not yet exhaustion.
        bars.push((80.0, 102.0, 101.0));
        assert_eq!(feed(&mut d, &bars), ExhaustionSignal::default());
        // The line fades while price still makes a new high: exhaustion.
        let s = d.update(70.0, 102.5, 101.5);
        assert!(s.bear && !s.bull, "{s:?}");
        assert!(s.strength > 0.0 && s.strength <= 1.0, "{s:?}");
    }

    #[test]
    fn fading_line_without_new_high_is_not_exhaustion() {
        let mut d = Exhaustion::new(20, 90.0, 3, 3);
        let mut bars = history(20);
        bars.push((80.0, 102.0, 101.0));
        feed(&mut d, &bars);
        // The line fades but price does not push any more: an ordinary pullback.
        let s = d.update(70.0, 101.2, 100.4);
        assert_eq!(s, ExhaustionSignal::default());
    }

    #[test]
    fn rising_extreme_low_with_new_low_is_bullish_exhaustion() {
        let mut d = Exhaustion::new(20, 90.0, 3, 3);
        let mut bars = history(20);
        bars.push((-30.0, 99.0, 98.0));
        feed(&mut d, &bars);
        let s = d.update(-20.0, 98.9, 97.4);
        assert!(s.bull && !s.bear, "{s:?}");
    }

    #[test]
    fn an_old_extreme_no_longer_counts() {
        let mut d = Exhaustion::new(20, 90.0, 3, 3);
        let mut bars = history(20);
        bars.push((80.0, 102.0, 101.0));
        // Three ordinary bars push the extreme out of the recency window …
        bars.push((30.0, 101.0, 100.5));
        bars.push((28.0, 101.1, 100.6));
        bars.push((26.0, 101.2, 100.7));
        feed(&mut d, &bars);
        // … so a fading line on a new high is no longer exhaustion of that extreme.
        let s = d.update(20.0, 103.0, 102.0);
        assert_eq!(s, ExhaustionSignal::default());
    }

    #[test]
    fn nothing_before_the_rank_history_is_full() {
        let mut d = Exhaustion::new(20, 90.0, 3, 3);
        let s = feed(&mut d, &history(10));
        assert_eq!(s, ExhaustionSignal::default());
        assert_eq!(d.warmup_period(), 21);
    }

    #[test]
    fn reset_forgets_everything() {
        let mut d = Exhaustion::new(20, 90.0, 3, 3);
        let mut bars = history(20);
        bars.push((80.0, 102.0, 101.0));
        feed(&mut d, &bars);
        d.reset();
        assert_eq!(d.update(70.0, 102.5, 101.5), ExhaustionSignal::default());
    }
}
