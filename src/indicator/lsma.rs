use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::stats::linear_regression;
use std::collections::VecDeque;

/// Least Squares Moving Average (LSMA) Engine / Linear Regression Endpoint.
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
        Some(IndicatorOutput::new(lsma_val))
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
