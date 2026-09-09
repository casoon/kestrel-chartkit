use crate::model::Bar;

use super::smoothing::Ema;
use super::{Indicator, IndicatorOutput};

/// Tillson T3: a weighted combination of six chained exponential averages.
///
/// With `e1 .. e6` the successive `Ema(period)` stages over the close and the volume factor `v`:
///
/// ```text
/// c1 = -v^3
/// c2 =  3*v^2 + 3*v^3
/// c3 = -6*v^2 - 3*v - 3*v^3
/// c4 =  1 + 3*v + v^3 + 3*v^2
/// T3 = c1*e6 + c2*e5 + c3*e4 + c4*e3
/// ```
///
/// `v` is a shape parameter in `0..=1`, not market volume — the name is historical. At `v = 0`
/// every coefficient but `c4` vanishes and T3 is exactly the third EMA; larger `v` adds the
/// higher stages with alternating signs, which reduces lag at the price of overshoot. That
/// overshoot is left in the output: clipping it away would hide what the parameter does.
///
/// This is neither TEMA nor a plain triple EMA — both combine fewer stages with different
/// weights.
///
/// Output: `value` in the price units of the series.
///
/// First output: with the `period`-th bar, so every stage has consumed at least `period` inputs.
/// All stages use the shared [`Ema`] with its first-sample seed, so no stage is ever fed a
/// substituted value. [`Indicator::reset`] clears all six averages and the counter, so the next
/// series starts deterministically.
#[derive(Debug, Clone)]
pub struct T3 {
    period: usize,
    stages: [Ema; 6],
    coefficients: [f64; 4],
    bars_seen: usize,
}

impl T3 {
    pub fn new(period: usize, v: f64) -> Self {
        let period = period.max(1);
        let v2 = v * v;
        let v3 = v2 * v;
        Self {
            period,
            stages: [Ema::new(period); 6],
            coefficients: [
                -v3,
                3.0 * v2 + 3.0 * v3,
                -6.0 * v2 - 3.0 * v - 3.0 * v3,
                1.0 + 3.0 * v + v3 + 3.0 * v2,
            ],
            bars_seen: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5, 0.7)
    }
}

impl Indicator for T3 {
    fn name(&self) -> &str {
        "t3"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars_seen += 1;

        let mut value = bar.close;
        let mut stage_values = [0.0; 6];
        for (stage, slot) in self.stages.iter_mut().zip(stage_values.iter_mut()) {
            value = stage.update(value)?;
            *slot = value;
        }

        if self.bars_seen < self.period {
            return None;
        }

        let [c1, c2, c3, c4] = self.coefficients;
        let t3 = c1 * stage_values[5]
            + c2 * stage_values[4]
            + c3 * stage_values[3]
            + c4 * stage_values[2];

        Some(IndicatorOutput::new(t3))
    }

    fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
        self.bars_seen = 0;
    }
}
