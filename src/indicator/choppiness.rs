use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::VecDeque;

/// Choppiness Index Engine (0..100).
/// Formula: 100 * log10( Sum(ATR(1), N) / (Highest(H, N) - Lowest(L, N)) ) / log10(N)
#[derive(Debug, Clone)]
pub struct ChoppinessIndexEngine {
    period: usize,
    prev_close: Option<f64>,
    tr_sum_window: VecDeque<f64>,
    bars: VecDeque<Bar>,
}

impl ChoppinessIndexEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            prev_close: None,
            tr_sum_window: VecDeque::with_capacity(period),
            bars: VecDeque::with_capacity(period),
        }
    }
}

impl Indicator for ChoppinessIndexEngine {
    fn name(&self) -> &str {
        "choppiness"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.tr_sum_window.clear();
        self.bars.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let tr = match self.prev_close {
            Some(prev_c) => (bar.high - bar.low)
                .max((bar.high - prev_c).abs())
                .max((bar.low - prev_c).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);

        self.tr_sum_window.push_back(tr);
        self.bars.push_back(bar.clone());

        if self.tr_sum_window.len() > self.period {
            self.tr_sum_window.pop_front();
            self.bars.pop_front();
        }

        if self.tr_sum_window.len() < self.period {
            return None;
        }

        let sum_tr: f64 = self.tr_sum_window.iter().sum();
        let max_h = self.bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let min_l = self.bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let range = (max_h - min_l).max(1e-8);

        let n_f64 = self.period as f64;
        let chop = 100.0 * (sum_tr / range).log10() / n_f64.log10();
        let chop_clamped = chop.clamp(0.0, 100.0);

        Some(IndicatorOutput::new(chop_clamped))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choppiness_index() {
        let mut chop = ChoppinessIndexEngine::new(14);
        let mut out = None;
        for i in 0..30 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + (i % 2) as f64, 1000.0);
            out = chop.on_bar(&b);
        }
        assert!(out.is_some());
        let val = out.unwrap().value;
        assert!((0.0..=100.0).contains(&val));
    }
}
