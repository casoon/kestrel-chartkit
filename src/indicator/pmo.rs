use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::Ema;
use super::{Indicator, IndicatorOutput};

/// An exponential average with the smoothing constant stated directly.
///
/// The crate's [`Ema`] takes a period and derives `alpha = 2/(len + 1)`. This one takes
/// `alpha = 2/len`, which is what the Price Momentum Oscillator is defined with. The two agree
/// only if the period is shifted by one, and silently doing that shift would leave a `length = 35`
/// in a caller's configuration meaning something else than it says.
///
/// Seeded with the first sample, like [`Ema`].
#[derive(Debug, Clone, Copy)]
struct DirectAlphaEma {
    alpha: f64,
    state: Option<f64>,
}

impl DirectAlphaEma {
    fn new(length: usize) -> Self {
        Self {
            alpha: 2.0 / length.max(1) as f64,
            state: None,
        }
    }

    fn update(&mut self, value: f64) -> f64 {
        let next = match self.state {
            None => value,
            Some(previous) => self.alpha * value + (1.0 - self.alpha) * previous,
        };
        self.state = Some(next);
        next
    }

    fn reset(&mut self) {
        self.state = None;
    }
}

/// Price Momentum Oscillator: the one-bar percentage return, smoothed twice and scaled.
///
/// ```text
/// roc    = 100 * (close_t / close_{t-1} - 1)
/// stage1 = ema(roc,          alpha = 2/length_1)
/// PMO    = ema(10 * stage1,  alpha = 2/length_2)
/// signal = Ema(signal_len) over the published PMO values
/// ```
///
/// Two details make this its own indicator rather than a smoothed rate of change. The smoothing
/// constant is `2/length`, not the `2/(length + 1)` of an ordinary exponential average — a
/// `length_1 = 35` here reacts like a 34-period ordinary EMA, and treating the two as the same
/// would quietly shift every period a caller configures. And the factor 10 sits *between* the two
/// stages, which is why the output is an index rather than a percentage: the value is ten times a
/// twice-smoothed percentage return.
///
/// The signal line, in contrast, is an ordinary [`Ema`] over the finished values.
///
/// Per-bar outputs, in index points:
/// - `value`: the PMO line.
/// - `extra["signal"]`: present from the `signal_len`-th published value on.
///
/// Both stages run from the first return on: the first is seeded with the first return, the
/// second with ten times the first output of the first, and each takes every value the stage
/// before it produces. Publication waits instead: the line appears once
/// `length_1 + length_2 - 1` returns have passed through both stages, i.e. with the
/// `length_1 + length_2`-th bar. There is no output on the first bar of a series, since a return
/// needs a predecessor. The signal line only ever sees published values. [`Indicator::reset`]
/// clears both stages and the signal average.
#[derive(Debug, Clone)]
pub struct PriceMomentumOscillator {
    length_1: usize,
    length_2: usize,
    signal_len: usize,
    prev_close: Option<f64>,
    stage_1: DirectAlphaEma,
    stage_2: DirectAlphaEma,
    signal_ema: Ema,
    observations: usize,
    lines_published: usize,
}

impl PriceMomentumOscillator {
    pub fn new(length_1: usize, length_2: usize, signal_len: usize) -> Self {
        Self {
            length_1: length_1.max(1),
            length_2: length_2.max(1),
            signal_len: signal_len.max(1),
            prev_close: None,
            stage_1: DirectAlphaEma::new(length_1),
            stage_2: DirectAlphaEma::new(length_2),
            signal_ema: Ema::new(signal_len.max(1)),
            observations: 0,
            lines_published: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(35, 20, 10)
    }
}

impl Indicator for PriceMomentumOscillator {
    fn name(&self) -> &str {
        "pmo"
    }

    fn warmup_period(&self) -> usize {
        self.length_1 + self.length_2
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev_close = match self.prev_close {
            None => {
                self.prev_close = Some(bar.close);
                return None;
            }
            Some(previous) => previous,
        };
        self.prev_close = Some(bar.close);

        if prev_close == 0.0 {
            return None;
        }
        let roc = 100.0 * (bar.close / prev_close - 1.0);
        let line = self.stage_2.update(10.0 * self.stage_1.update(roc));

        self.observations += 1;
        if self.observations < self.length_1 + self.length_2 - 1 {
            return None;
        }
        self.lines_published += 1;

        let mut extra = HashMap::new();
        let signal = self.signal_ema.update(line)?;
        if self.lines_published >= self.signal_len {
            extra.insert("signal".to_string(), signal);
        }

        Some(IndicatorOutput::with_extra(line, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.stage_1.reset();
        self.stage_2.reset();
        self.signal_ema.reset();
        self.observations = 0;
        self.lines_published = 0;
    }
}
