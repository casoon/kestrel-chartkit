use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::Ema;
use super::{Indicator, IndicatorOutput};

/// TRIX: the rate of change of a triple-smoothed price series.
///
/// Three exponential averages of `len` are chained over the close — `e1` over the price, `e2`
/// over `e1`, `e3` over `e2` — and the published line is the relative change of that third
/// average against its own previous value:
///
/// `TRIX_t = 100 * (e3_t / e3_{t-1} - 1)`
///
/// The previous value is the denominator, so the result is the percentage change *of the level
/// it came from*. Other implementations divide by the current value or publish the absolute
/// difference instead; those are different numbers under the same name, and this type does not
/// offer them.
///
/// This is not TEMA: TEMA combines the three averages linearly to reduce lag and stays in price
/// units, while TRIX differentiates the third average and is a rate.
///
/// Every stage uses the shared [`Ema`] with its first-sample seed, so each stage always receives
/// a defined input — no stage is ever fed a substituted zero.
///
/// Units: percentage points per bar for the line, the signal and the histogram alike.
///
/// Per-bar outputs:
/// - `value`: the TRIX line.
/// - `extra["signal"]`: `Ema(signal_len)` over the published TRIX values, seeded with the first
///   of them and published only from the `signal_len`-th on. Before that the key is absent — an
///   unfinished signal line is left out, not zeroed.
/// - `extra["hist"]`: `value - signal`, present exactly when the signal is.
///
/// First output: with the `len + 1`-th bar, so both `e3` values of the ratio are built from at
/// least `len` inputs each. A bar whose previous `e3` is zero or non-finite produces no output
/// rather than a division result that means nothing. [`Indicator::reset`] clears all four
/// averages and the counters, so the next series starts deterministically.
#[derive(Debug, Clone)]
pub struct Trix {
    len: usize,
    signal_len: usize,
    ema1: Ema,
    ema2: Ema,
    ema3: Ema,
    prev_ema3: Option<f64>,
    signal_ema: Ema,
    bars_seen: usize,
    lines_published: usize,
}

impl Trix {
    pub fn new(len: usize, signal_len: usize) -> Self {
        let len = len.max(1);
        Self {
            len,
            signal_len: signal_len.max(1),
            ema1: Ema::new(len),
            ema2: Ema::new(len),
            ema3: Ema::new(len),
            prev_ema3: None,
            signal_ema: Ema::new(signal_len.max(1)),
            bars_seen: 0,
            lines_published: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(15, 9)
    }
}

impl Indicator for Trix {
    fn name(&self) -> &str {
        "trix"
    }

    fn warmup_period(&self) -> usize {
        self.len + 1
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars_seen += 1;

        let e1 = self.ema1.update(bar.close)?;
        let e2 = self.ema2.update(e1)?;
        let e3 = self.ema3.update(e2)?;

        let prev_ema3 = self.prev_ema3.replace(e3);
        if self.bars_seen < self.len + 1 {
            return None;
        }
        let prev_ema3 = prev_ema3?;
        if prev_ema3 == 0.0 || !prev_ema3.is_finite() {
            return None;
        }

        let line = 100.0 * (e3 / prev_ema3 - 1.0);
        self.lines_published += 1;

        let mut extra = HashMap::new();
        let signal = self.signal_ema.update(line)?;
        if self.lines_published >= self.signal_len {
            extra.insert("signal".to_string(), signal);
            extra.insert("hist".to_string(), line - signal);
        }

        Some(IndicatorOutput::with_extra(line, extra))
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
        self.ema3.reset();
        self.signal_ema.reset();
        self.prev_ema3 = None;
        self.bars_seen = 0;
        self.lines_published = 0;
    }
}
