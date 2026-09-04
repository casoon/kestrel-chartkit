use super::smoothing::Ema;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::VecDeque;

/// Mass Index Engine.
/// Mass Index = Sum(EMA(High - Low, 9) / EMA(EMA(High - Low, 9), 9), 25)
#[derive(Debug, Clone)]
pub struct MassIndexEngine {
    period: usize,
    ema1: Ema,
    ema2: Ema,
    ratio_window: VecDeque<f64>,
}

impl MassIndexEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            ema1: Ema::new(9),
            ema2: Ema::new(9),
            ratio_window: VecDeque::with_capacity(period),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(25)
    }
}

impl Indicator for MassIndexEngine {
    fn name(&self) -> &str {
        "mass_index"
    }

    fn warmup_period(&self) -> usize {
        self.period + 18
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
        self.ratio_window.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let range = (bar.high - bar.low).max(1e-8);
        let e1 = self.ema1.update(range);
        let e2 = self.ema2.update(e1);

        let ratio = if e2 > 1e-8 { e1 / e2 } else { 1.0 };

        self.ratio_window.push_back(ratio);
        if self.ratio_window.len() > self.period {
            self.ratio_window.pop_front();
        }

        if self.ratio_window.len() < self.period {
            return None;
        }

        let mass_val: f64 = self.ratio_window.iter().sum();
        Some(IndicatorOutput::new(mass_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mass_index() {
        let mut mi = MassIndexEngine::with_defaults();
        let mut out = None;
        for i in 0..50 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = mi.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
