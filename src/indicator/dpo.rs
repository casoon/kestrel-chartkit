use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::VecDeque;

/// Detrended Price Oscillator (DPO) Engine.
/// DPO = Close[N/2 + 1] - SMA(Close, N)
#[derive(Debug, Clone)]
pub struct DpoEngine {
    period: usize,
    window: VecDeque<f64>,
}

impl DpoEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            window: VecDeque::with_capacity(period + 1),
        }
    }
}

impl Indicator for DpoEngine {
    fn name(&self) -> &str {
        "dpo"
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

        let sma = self.window.iter().sum::<f64>() / self.period as f64;
        let lookback_offset = self.period / 2 + 1;

        let past_price = if self.window.len() >= lookback_offset {
            self.window[self.window.len() - lookback_offset]
        } else {
            bar.close
        };

        let dpo_val = past_price - sma;
        Some(IndicatorOutput::new(dpo_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpo_basic() {
        let mut dpo = DpoEngine::new(20);
        let mut out = None;
        for i in 0..30 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = dpo.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
