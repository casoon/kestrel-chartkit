use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// McGinley Dynamic over the closing price.
///
/// A moving average whose effective length adapts to how fast price moves away from it:
///
/// ```text
/// MD_t = MD_{t-1} + (close_t - MD_{t-1}) / (period * (close_t / MD_{t-1})^4)
/// ```
///
/// This is the form with `period` itself in the denominator. Some descriptions scale the length
/// by a constant `0.6`; this type does not.
///
/// The first value is the first close, and output starts with the first bar — the recursion needs
/// no window. [`Indicator::warmup_period`] nonetheless reports `period`. [`Indicator::reset`]
/// clears the average.
#[derive(Debug, Clone)]
pub struct McGinleyDynamicEngine {
    period: usize,
    state: Option<f64>,
}

impl McGinleyDynamicEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            state: None,
        }
    }
}

impl Indicator for McGinleyDynamicEngine {
    fn name(&self) -> &str {
        "mcginley"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.state = None;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let price = bar.close;
        let k = self.period as f64;

        let next = match self.state {
            None => price,
            Some(prev) => {
                let ratio = (price / prev.max(1e-8)).powi(4);
                prev + (price - prev) / (k * ratio).max(1e-6)
            }
        };

        self.state = Some(next);
        Some(IndicatorOutput::new(next))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcginley_basic() {
        let mut mg = McGinleyDynamicEngine::new(14);
        let b1 = Bar::new(1, 100.0, 105.0, 95.0, 100.0, 1000.0);
        let out1 = mg.on_bar(&b1).unwrap();
        assert_eq!(out1.value, 100.0);
    }
}
