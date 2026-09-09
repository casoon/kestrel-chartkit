use crate::model::Bar;

use super::momentum_indicators::CmoEngine;
use super::{Indicator, IndicatorOutput};

/// Variable Index Dynamic Average: an exponential average whose smoothing constant is scaled by
/// how directional the recent price movement is.
///
/// `alpha_t = 2/(ema_len + 1) * |CMO_t| / 100` and
/// `VIDYA_t = alpha_t * close_t + (1 - alpha_t) * VIDYA_{t-1}`.
///
/// The Chande Momentum Oscillator comes from the existing [`CmoEngine`] over `cmo_len` and is
/// already scaled to `-100..=100`, which is why the formula divides by 100 exactly once. In a
/// directionless market `|CMO|` approaches zero, `alpha` with it, and the line holds its level;
/// at `|CMO| = 100` the average reacts like a plain `Ema(ema_len)`.
///
/// This is the CMO-based variant. The variant driven by a ratio of standard deviations carries
/// the same name elsewhere but is a different formula; it is not offered here.
///
/// The two periods are separate on purpose: `cmo_len` sets how far back the directionality is
/// measured, `ema_len` the base smoothing that directionality scales.
///
/// Output: `value` in the price units of the series.
///
/// First output: with the first bar the CMO is defined for, i.e. after `cmo_len + 1` bars; that
/// first value is the close of that bar (the seed). A bar whose CMO is zero leaves the line
/// exactly where it was — the average then carries no new information, which is the point of the
/// construction, not a gap. [`Indicator::reset`] clears the CMO state and the line, so the next
/// series starts deterministically.
#[derive(Debug, Clone)]
pub struct Vidya {
    ema_len: usize,
    cmo: CmoEngine,
    state: Option<f64>,
}

impl Vidya {
    pub fn new(cmo_len: usize, ema_len: usize) -> Self {
        Self {
            ema_len: ema_len.max(1),
            cmo: CmoEngine::new(cmo_len.max(1)),
            state: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(9, 12)
    }
}

impl Indicator for Vidya {
    fn name(&self) -> &str {
        "vidya"
    }

    fn warmup_period(&self) -> usize {
        self.cmo.warmup_period()
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let cmo = self.cmo.on_bar(bar)?.value;

        let value = match self.state {
            None => bar.close,
            Some(prev) => {
                let alpha = 2.0 / (self.ema_len as f64 + 1.0) * (cmo.abs() / 100.0);
                alpha * bar.close + (1.0 - alpha) * prev
            }
        };
        self.state = Some(value);

        Some(IndicatorOutput::new(value))
    }

    fn reset(&mut self) {
        self.cmo.reset();
        self.state = None;
    }
}
