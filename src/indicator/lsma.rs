use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::stats::linear_regression;
use std::collections::HashMap;
use std::collections::VecDeque;

/// Least Squares Moving Average: the endpoint of an ordinary least-squares fit through the last
/// `period` closes.
///
/// The fit runs over the local window index `x = 0 ..= period - 1`, so `value` is the fitted
/// price at `x = period - 1` — the current bar, never beyond it.
///
/// The same fit already carries more than that endpoint, and those numbers are published rather
/// than recomputed by callers:
/// - `extra["slope"]`: price change per bar of the fitted line, in price units per bar. Not an
///   angle: a slope drawn as an angle depends on the axis scaling of whoever draws it.
/// - `extra["intercept"]`: fitted price at the local window index 0, i.e. at the oldest bar of
///   the current window — not at the start of the series.
/// - `extra["r2"]`: share of the window's price variance explained by the line, in `0..=1`. A
///   goodness of fit, not a probability that the move continues. A window without price variance
///   has nothing left to explain; the convention here is `1`.
///
/// First output: with the `period`-th bar, and only when the fit is defined (a window of
/// non-finite values yields none). [`Indicator::reset`] clears the window, so the next series
/// starts deterministically.
#[derive(Debug, Clone)]
pub struct LsmaEngine {
    period: usize,
    window: VecDeque<f64>,
}

impl LsmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            window: VecDeque::with_capacity(period),
        }
    }
}

impl Indicator for LsmaEngine {
    fn name(&self) -> &str {
        "lsma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.window.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.window.push_back(bar.close);
        if self.window.len() > self.period {
            self.window.pop_front();
        }

        if self.window.len() < self.period {
            return None;
        }

        let slice: Vec<f64> = self.window.iter().copied().collect();
        let fit = linear_regression(&slice)?;

        // Endpoint prediction at x = N - 1
        let lsma_val = fit.slope * (self.period - 1) as f64 + fit.intercept;

        let extra = HashMap::from([
            ("slope".to_string(), fit.slope),
            ("intercept".to_string(), fit.intercept),
            ("r2".to_string(), fit.r2),
        ]);
        Some(IndicatorOutput::with_extra(lsma_val, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsma_linear_trend() {
        let mut lsma = LsmaEngine::new(5);
        for i in 0..10 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + (i as f64 * 2.0), 1000.0);
            if let Some(out) = lsma.on_bar(&b) {
                if i == 9 {
                    assert!((out.value - 118.0).abs() < 1e-6);
                }
            }
        }
    }
}
