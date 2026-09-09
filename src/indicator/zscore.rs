use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::stats::{rolling_mean, rolling_stddev};
use std::collections::VecDeque;

/// Rolling Z-Score Engine.
/// Z = (Price - Mean) / StdDev
#[derive(Debug, Clone)]
pub struct ZScoreEngine {
    period: usize,
    window: VecDeque<f64>,
}

impl ZScoreEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            window: VecDeque::with_capacity(period),
        }
    }
}

impl Indicator for ZScoreEngine {
    fn name(&self) -> &str {
        "zscore"
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
        let mean = rolling_mean(&slice);
        let stddev = rolling_stddev(&slice);

        let z = if stddev > 1e-8 {
            (bar.close - mean) / stddev
        } else {
            0.0
        };

        Some(IndicatorOutput::new(z))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zscore_calculation() {
        let mut zsec = ZScoreEngine::new(5);
        for i in 0..5 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64 * 10.0, 1000.0);
            zsec.on_bar(&b);
        }
        let out = zsec
            .on_bar(&Bar::new(5, 100.0, 105.0, 95.0, 140.0, 1000.0))
            .unwrap();
        assert!(out.value > 0.0);
    }
}
