use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct SlopeDivergence {
    div_len: usize,
    div_min: f64,
    fast_window: VecDeque<f64>,
    slow_window: VecDeque<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DivergenceAlerts {
    pub bull: bool,
    pub bear: bool,
    pub fast_dir: f64,
}

impl SlopeDivergence {
    pub fn new(div_len: usize, div_min: f64) -> Self {
        Self {
            div_len,
            div_min,
            fast_window: VecDeque::with_capacity(div_len + 1),
            slow_window: VecDeque::with_capacity(div_len + 1),
        }
    }

    pub fn update(&mut self, fast: f64, slow: f64) -> DivergenceAlerts {
        if self.fast_window.len() == self.div_len + 1 {
            self.fast_window.pop_front();
        }
        self.fast_window.push_back(fast);
        if self.slow_window.len() == self.div_len + 1 {
            self.slow_window.pop_front();
        }
        self.slow_window.push_back(slow);

        if self.fast_window.len() < self.div_len + 1 {
            return DivergenceAlerts::default();
        }
        let fast_dir = fast - self.fast_window[0];
        let slow_dir = slow - self.slow_window[0];
        DivergenceAlerts {
            bull: fast_dir < -self.div_min && slow_dir > 0.0,
            bear: fast_dir > self.div_min && slow_dir < 0.0,
            fast_dir,
        }
    }

    pub fn reset(&mut self) {
        self.fast_window.clear();
        self.slow_window.clear();
    }

    pub fn div_min(&self) -> f64 {
        self.div_min
    }
}

/// Divergence Classification Kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DivergenceKind {
    RegularBullish,
    RegularBearish,
    HiddenBullish,
    HiddenBearish,
}

/// How a pivot candidate's oscillator value is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OscillatorAnchor {
    /// The oscillator's value exactly at the candidate pivot bar.
    AtPivot,
    /// The oscillator's extreme (min for a low pivot, max for a high pivot) across the
    /// confirmation window. Matches this engine's original, sole behavior.
    #[default]
    WindowExtreme,
}

/// Enhanced Pivot-based Divergence Detection Engine across Price and Oscillator anchors.
///
/// Ranks divergence candidates: each confirmed pivot is compared against up to
/// [`PivotDivergenceEngine::with_max_prior_pivots`] prior same-type pivots (not just the
/// immediately preceding one), each candidate is scored by [`DivergenceEvent::quality_score`],
/// and only the single highest-scoring bullish and bearish candidate per confirmation is emitted
/// — the conflict resolution the plain "compare against the last pivot only" version lacked.
#[derive(Debug, Clone)]
pub struct PivotDivergenceEngine {
    left_bars: usize,
    right_bars: usize,
    min_distance: usize,
    max_distance: usize,
    max_prior_pivots: usize,
    oscillator_anchor: OscillatorAnchor,
    next_index: usize,
    window: VecDeque<PivotSample>,
    previous_lows: VecDeque<PivotAnchor>,
    previous_highs: VecDeque<PivotAnchor>,
}

#[derive(Debug, Clone)]
struct PivotSample {
    index: usize,
    timestamp: i64,
    high: f64,
    low: f64,
    oscillator: f64,
}

#[derive(Debug, Clone, Copy)]
struct PivotAnchor {
    index: usize,
    timestamp: i64,
    price: f64,
    oscillator: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DivergenceEvent {
    pub kind: DivergenceKind,
    pub previous_timestamp: i64,
    pub pivot_timestamp: i64,
    pub confirmed_timestamp: i64,
    pub previous_price: f64,
    pub pivot_price: f64,
    pub previous_oscillator: f64,
    pub pivot_oscillator: f64,
    pub bars_between: usize,
    /// Heuristic `0.0..=1.0` ranking of this candidate: 60% relative price+oscillator divergence
    /// magnitude, 40% how centrally the pivot spacing falls within
    /// `[min_distance, max_distance]`. Used to pick the winning candidate among several prior
    /// pivots and to let consumers threshold weak divergences; not a calibrated probability.
    pub quality_score: f64,
}

impl PivotDivergenceEngine {
    pub fn new(lookback: usize) -> Self {
        Self::with_confirmation(lookback.max(1), lookback.max(1), 1, usize::MAX)
    }

    pub fn with_confirmation(
        left_bars: usize,
        right_bars: usize,
        min_distance: usize,
        max_distance: usize,
    ) -> Self {
        let left_bars = left_bars.max(1);
        let right_bars = right_bars.max(1);
        Self {
            left_bars,
            right_bars,
            min_distance,
            max_distance: max_distance.max(min_distance),
            max_prior_pivots: 5,
            oscillator_anchor: OscillatorAnchor::default(),
            next_index: 0,
            window: VecDeque::with_capacity(left_bars + right_bars + 1),
            previous_lows: VecDeque::new(),
            previous_highs: VecDeque::new(),
        }
    }

    /// How many prior same-type pivots to keep and rank divergence candidates against (beyond
    /// just the immediately preceding one). Default 5.
    pub fn with_max_prior_pivots(mut self, max_prior_pivots: usize) -> Self {
        self.max_prior_pivots = max_prior_pivots.max(1);
        self
    }

    /// How a pivot candidate's oscillator value is anchored. Default
    /// [`OscillatorAnchor::WindowExtreme`].
    pub fn with_oscillator_anchor(mut self, anchor: OscillatorAnchor) -> Self {
        self.oscillator_anchor = anchor;
        self
    }

    /// Returns events only after `right_bars` have confirmed the candidate pivot.
    pub fn update(&mut self, bar: &crate::model::Bar, oscillator: f64) -> Vec<DivergenceEvent> {
        let capacity = self.left_bars + self.right_bars + 1;
        if self.window.len() == capacity {
            self.window.pop_front();
        }
        self.window.push_back(PivotSample {
            index: self.next_index,
            timestamp: bar.timestamp,
            high: bar.high,
            low: bar.low,
            oscillator,
        });
        self.next_index += 1;
        if self.window.len() < capacity {
            return Vec::new();
        }

        let candidate_index = self.left_bars;
        let candidate = &self.window[candidate_index];
        let is_low = self
            .window
            .iter()
            .enumerate()
            .all(|(index, sample)| index == candidate_index || sample.low > candidate.low);
        let is_high = self
            .window
            .iter()
            .enumerate()
            .all(|(index, sample)| index == candidate_index || sample.high < candidate.high);

        let mut events = Vec::with_capacity(2);

        if is_low {
            let oscillator = match self.oscillator_anchor {
                OscillatorAnchor::AtPivot => candidate.oscillator,
                OscillatorAnchor::WindowExtreme => self
                    .window
                    .iter()
                    .map(|sample| sample.oscillator)
                    .fold(f64::INFINITY, f64::min),
            };
            let current = PivotAnchor {
                index: candidate.index,
                timestamp: candidate.timestamp,
                price: candidate.low,
                oscillator,
            };
            if let Some(event) =
                self.best_ranked_event(&self.previous_lows, current, bar.timestamp, true)
            {
                events.push(event);
            }
            push_bounded(&mut self.previous_lows, current, self.max_prior_pivots);
        }
        if is_high {
            let oscillator = match self.oscillator_anchor {
                OscillatorAnchor::AtPivot => candidate.oscillator,
                OscillatorAnchor::WindowExtreme => self
                    .window
                    .iter()
                    .map(|sample| sample.oscillator)
                    .fold(f64::NEG_INFINITY, f64::max),
            };
            let current = PivotAnchor {
                index: candidate.index,
                timestamp: candidate.timestamp,
                price: candidate.high,
                oscillator,
            };
            if let Some(event) =
                self.best_ranked_event(&self.previous_highs, current, bar.timestamp, false)
            {
                events.push(event);
            }
            push_bounded(&mut self.previous_highs, current, self.max_prior_pivots);
        }
        events
    }

    pub fn reset(&mut self) {
        self.next_index = 0;
        self.window.clear();
        self.previous_lows.clear();
        self.previous_highs.clear();
    }

    /// Scores `current` against every stored prior pivot in `priors`, keeping only the
    /// highest-scoring valid candidate (the conflict resolution step) instead of returning one
    /// event per prior.
    fn best_ranked_event(
        &self,
        priors: &VecDeque<PivotAnchor>,
        current: PivotAnchor,
        confirmed_timestamp: i64,
        is_low: bool,
    ) -> Option<DivergenceEvent> {
        priors
            .iter()
            .filter_map(|&previous| {
                self.candidate_event(previous, current, confirmed_timestamp, is_low)
            })
            .max_by(|a, b| a.quality_score.total_cmp(&b.quality_score))
    }

    fn candidate_event(
        &self,
        previous: PivotAnchor,
        current: PivotAnchor,
        confirmed_timestamp: i64,
        is_low: bool,
    ) -> Option<DivergenceEvent> {
        let bars_between = current.index - previous.index;
        if !(self.min_distance..=self.max_distance).contains(&bars_between) {
            return None;
        }
        let kind =
            if is_low && current.price < previous.price && current.oscillator > previous.oscillator
            {
                DivergenceKind::RegularBullish
            } else if is_low
                && current.price > previous.price
                && current.oscillator < previous.oscillator
            {
                DivergenceKind::HiddenBullish
            } else if !is_low
                && current.price > previous.price
                && current.oscillator < previous.oscillator
            {
                DivergenceKind::RegularBearish
            } else if !is_low
                && current.price < previous.price
                && current.oscillator > previous.oscillator
            {
                DivergenceKind::HiddenBearish
            } else {
                return None;
            };

        let quality_score = divergence_quality_score(
            &previous,
            &current,
            bars_between,
            self.min_distance,
            self.max_distance,
        );

        Some(DivergenceEvent {
            kind,
            previous_timestamp: previous.timestamp,
            pivot_timestamp: current.timestamp,
            confirmed_timestamp,
            previous_price: previous.price,
            pivot_price: current.price,
            previous_oscillator: previous.oscillator,
            pivot_oscillator: current.oscillator,
            bars_between,
            quality_score,
        })
    }
}

fn push_bounded(deque: &mut VecDeque<PivotAnchor>, value: PivotAnchor, max_len: usize) {
    if deque.len() >= max_len {
        deque.pop_front();
    }
    deque.push_back(value);
}

/// Heuristic `0.0..=1.0` divergence quality: 60% weight on relative price+oscillator divergence
/// magnitude (bigger disagreement between the two = stronger signal), 40% weight on how close the
/// pivot spacing is to the middle of `[min_distance, max_distance]` (too close risks noise, too
/// far risks an unrelated coincidence).
fn divergence_quality_score(
    previous: &PivotAnchor,
    current: &PivotAnchor,
    bars_between: usize,
    min_distance: usize,
    max_distance: usize,
) -> f64 {
    let price_move = (current.price - previous.price).abs() / previous.price.abs().max(1e-9);
    let osc_move =
        (current.oscillator - previous.oscillator).abs() / previous.oscillator.abs().max(1e-9);
    let magnitude = ((price_move + osc_move) / 2.0).min(1.0);

    let span = max_distance.saturating_sub(min_distance).max(1) as f64;
    let mid = min_distance as f64 + span / 2.0;
    let distance_quality =
        1.0 - ((bars_between as f64 - mid).abs() / (span / 2.0).max(1.0)).min(1.0);

    (0.6 * magnitude + 0.4 * distance_quality).clamp(0.0, 1.0)
}

#[cfg(test)]
mod pivot_tests {
    use super::*;
    use crate::model::Bar;

    #[test]
    fn emits_only_after_pivot_confirmation() {
        let mut engine = PivotDivergenceEngine::with_confirmation(1, 1, 1, 10);
        let points = [
            (10.0, 50.0),
            (8.0, 20.0),
            (11.0, 40.0),
            (7.0, 30.0),
            (12.0, 45.0),
        ];
        let mut events = Vec::new();
        for (index, (low, oscillator)) in points.into_iter().enumerate() {
            let bar = Bar::new(index as i64, low + 1.0, low + 2.0, low, low + 1.0, 1.0);
            events.extend(engine.update(&bar, oscillator));
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, DivergenceKind::RegularBullish);
        assert_eq!(events[0].pivot_timestamp, 3);
        assert_eq!(events[0].confirmed_timestamp, 4);
        assert!(events[0].quality_score >= 0.0 && events[0].quality_score <= 1.0);
    }

    #[test]
    fn tracks_multiple_prior_pivots_bounded_by_max_prior_pivots() {
        let engine =
            PivotDivergenceEngine::with_confirmation(1, 1, 1, 100).with_max_prior_pivots(2);
        assert_eq!(engine.max_prior_pivots, 2);

        let mut priors = VecDeque::new();
        for i in 0..3 {
            push_bounded(
                &mut priors,
                PivotAnchor {
                    index: i,
                    timestamp: i as i64,
                    price: 10.0 - i as f64,
                    oscillator: 20.0 + i as f64,
                },
                engine.max_prior_pivots,
            );
        }
        // Bounded to 2: the oldest (index 0) must have been evicted.
        assert_eq!(priors.len(), 2);
        assert!(priors.iter().all(|p| p.index != 0));
    }

    #[test]
    fn conflict_resolution_picks_the_higher_quality_candidate() {
        let engine = PivotDivergenceEngine::with_confirmation(1, 1, 1, 100);

        // Prior A: close in time and price/oscillator barely disagree -> low quality.
        let prior_a = PivotAnchor {
            index: 5,
            timestamp: 5,
            price: 100.0,
            oscillator: 50.0,
        };
        // Prior B: farther back but with a much larger price/oscillator disagreement -> should
        // score higher on magnitude despite being farther from the ideal mid-distance.
        let prior_b = PivotAnchor {
            index: 1,
            timestamp: 1,
            price: 130.0,
            oscillator: 20.0,
        };
        let current = PivotAnchor {
            index: 6,
            timestamp: 6,
            price: 99.0,
            oscillator: 55.0,
        };

        let mut priors = VecDeque::new();
        priors.push_back(prior_a);
        priors.push_back(prior_b);

        let event = engine
            .best_ranked_event(&priors, current, 6, true)
            .expect("at least one candidate must qualify as a regular bullish divergence");

        let score_a = engine
            .candidate_event(prior_a, current, 6, true)
            .unwrap()
            .quality_score;
        let score_b = engine
            .candidate_event(prior_b, current, 6, true)
            .unwrap()
            .quality_score;
        let winner_price = if score_a >= score_b {
            prior_a.price
        } else {
            prior_b.price
        };

        assert_eq!(event.previous_price, winner_price);
        assert!((event.quality_score - score_a.max(score_b)).abs() < 1e-12);
    }

    #[test]
    fn oscillator_anchor_at_pivot_differs_from_window_extreme() {
        let mut at_pivot = PivotDivergenceEngine::with_confirmation(1, 1, 1, 10)
            .with_oscillator_anchor(OscillatorAnchor::AtPivot);
        let mut window_extreme = PivotDivergenceEngine::with_confirmation(1, 1, 1, 10)
            .with_oscillator_anchor(OscillatorAnchor::WindowExtreme);

        // Oscillator dips lower one bar after the low-price pivot bar, so the window minimum
        // (bar 2) differs from the oscillator value exactly at the pivot bar (bar 1).
        let points = [(10.0, 50.0), (8.0, 40.0), (9.0, 10.0)];
        for (index, (low, oscillator)) in points.into_iter().enumerate() {
            let bar = Bar::new(index as i64, low + 1.0, low + 2.0, low, low + 1.0, 1.0);
            at_pivot.update(&bar, oscillator);
            window_extreme.update(&bar, oscillator);
        }

        assert_eq!(at_pivot.previous_lows.back().unwrap().oscillator, 40.0);
        assert_eq!(
            window_extreme.previous_lows.back().unwrap().oscillator,
            10.0
        );
    }
}
