use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::Ema;
use super::{Indicator, IndicatorOutput};

/// Stochastic Momentum Index: where the close sits relative to the *midpoint* of the recent
/// high-low range, double-smoothed.
///
/// Over the last `len` bars, `HH` is the highest high and `LL` the lowest low. Two series are
/// formed and each smoothed twice with exponential averages of `smooth_1` and then `smooth_2`:
///
/// ```text
/// distance = close - (HH + LL) / 2
/// range    = HH - LL
/// SMI      = 200 * smoothed(distance) / smoothed(range)
/// ```
///
/// The factor 200 follows from the halved range in the denominator: an unsmoothed close at the
/// high gives `distance = range/2` and therefore `+100`, at the low `-100`. Double smoothing can
/// carry the published value slightly past those marks, which is left as it comes out rather than
/// clipped.
///
/// This is not the Stochastic Oscillator, which measures against the *low* of the range and lives
/// in `0..100`. It is also not Stochastic RSI, which runs the same idea over an RSI series, nor
/// the SMI Ergodic/TSI family, which double-smooths price *changes* rather than the position in a
/// range. Same three letters, different measurements.
///
/// Per-bar outputs, all index points in roughly `-100..=100`:
/// - `value`: the SMI line.
/// - `extra["signal"]`: `Ema(signal_len)` over the published SMI values, present only from the
///   `signal_len`-th of them on.
///
/// A window whose high equals its low has no range to place the close in; the line is `0` there
/// by convention rather than a division by zero.
///
/// Both smoothing stages run from the first full window on: the first is seeded with the first
/// `distance` (respectively `range`) value, the second with the first output of the first, and
/// each takes every value the stage before it produces — the second stage does not wait for the
/// first to be published. Publication waits instead: the line appears once
/// `smooth_1 + smooth_2 - 1` windows have passed through both stages, i.e. with the
/// `len + smooth_1 + smooth_2 - 2`-th bar. The signal line only ever sees published values.
/// [`Indicator::reset`] clears the window and all four averages.
#[derive(Debug, Clone)]
pub struct StochasticMomentumIndex {
    len: usize,
    smooth_1: usize,
    smooth_2: usize,
    signal_len: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    distance_1: Ema,
    distance_2: Ema,
    range_1: Ema,
    range_2: Ema,
    signal_ema: Ema,
    observations: usize,
    lines_published: usize,
}

impl StochasticMomentumIndex {
    pub fn new(len: usize, smooth_1: usize, smooth_2: usize, signal_len: usize) -> Self {
        let len = len.max(1);
        let smooth_1 = smooth_1.max(1);
        let smooth_2 = smooth_2.max(1);
        let signal_len = signal_len.max(1);
        Self {
            len,
            smooth_1,
            smooth_2,
            signal_len,
            highs: VecDeque::with_capacity(len),
            lows: VecDeque::with_capacity(len),
            distance_1: Ema::new(smooth_1),
            distance_2: Ema::new(smooth_2),
            range_1: Ema::new(smooth_1),
            range_2: Ema::new(smooth_2),
            signal_ema: Ema::new(signal_len),
            observations: 0,
            lines_published: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(10, 3, 3, 3)
    }
}

impl Indicator for StochasticMomentumIndex {
    fn name(&self) -> &str {
        "smi"
    }

    fn warmup_period(&self) -> usize {
        self.len + self.smooth_1 + self.smooth_2 - 2
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.highs.push_back(bar.high);
        self.lows.push_back(bar.low);
        if self.highs.len() > self.len {
            self.highs.pop_front();
            self.lows.pop_front();
        }
        if self.highs.len() < self.len {
            return None;
        }

        let highest = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lowest = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        let range = highest - lowest;
        let distance = bar.close - (highest + lowest) / 2.0;

        let smoothed_distance = self.distance_2.update(self.distance_1.update(distance)?)?;
        let smoothed_range = self.range_2.update(self.range_1.update(range)?)?;

        self.observations += 1;
        if self.observations < self.smooth_1 + self.smooth_2 - 1 {
            return None;
        }

        let line = if smoothed_range.abs() > 0.0 {
            200.0 * smoothed_distance / smoothed_range
        } else {
            0.0
        };
        self.lines_published += 1;

        let mut extra = HashMap::new();
        let signal = self.signal_ema.update(line)?;
        if self.lines_published >= self.signal_len {
            extra.insert("signal".to_string(), signal);
        }

        Some(IndicatorOutput::with_extra(line, extra))
    }

    fn reset(&mut self) {
        self.highs.clear();
        self.lows.clear();
        self.distance_1.reset();
        self.distance_2.reset();
        self.range_1.reset();
        self.range_2.reset();
        self.signal_ema.reset();
        self.observations = 0;
        self.lines_published = 0;
    }
}
