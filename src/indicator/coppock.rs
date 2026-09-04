use super::smoothing::Wma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::VecDeque;

/// Coppock Curve Engine.
/// Coppock Curve = WMA(10) of (ROC(14) + ROC(11))
#[derive(Debug, Clone)]
pub struct CoppockCurveEngine {
    wma10: Wma,
    closes: VecDeque<f64>,
}

impl CoppockCurveEngine {
    pub fn new() -> Self {
        Self {
            wma10: Wma::new(10),
            closes: VecDeque::with_capacity(15),
        }
    }
}

impl Default for CoppockCurveEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for CoppockCurveEngine {
    fn name(&self) -> &str {
        "coppock"
    }

    fn warmup_period(&self) -> usize {
        25
    }

    fn reset(&mut self) {
        self.wma10.reset();
        self.closes.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.closes.push_back(bar.close);
        if self.closes.len() > 15 {
            self.closes.pop_front();
        }

        if self.closes.len() < 15 {
            return None;
        }

        let c_curr = bar.close;
        let c_14 = self.closes[self.closes.len() - 15];
        let c_11 = self.closes[self.closes.len() - 12];

        let roc14 = if c_14 > 0.0 {
            ((c_curr - c_14) / c_14) * 100.0
        } else {
            0.0
        };
        let roc11 = if c_11 > 0.0 {
            ((c_curr - c_11) / c_11) * 100.0
        } else {
            0.0
        };

        let raw = roc14 + roc11;
        let coppock_val = self.wma10.update(raw)?;

        Some(IndicatorOutput::new(coppock_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coppock_curve() {
        let mut cc = CoppockCurveEngine::new();
        let mut out = None;
        for i in 0..30 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = cc.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
