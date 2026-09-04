use super::smoothing::Ema;
use super::volume_indicators::AccDistEngine;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Chaikin Oscillator Engine.
/// Chaikin Oscillator = EMA(ADL, 3) - EMA(ADL, 10)
#[derive(Debug, Clone)]
pub struct ChaikinOscillatorEngine {
    fast_len: usize,
    slow_len: usize,
    adl: AccDistEngine,
    fast_ema: Ema,
    slow_ema: Ema,
    count: usize,
}

impl ChaikinOscillatorEngine {
    pub fn new(fast_len: usize, slow_len: usize) -> Self {
        Self {
            fast_len: fast_len.max(1),
            slow_len: slow_len.max(1),
            adl: AccDistEngine::new(),
            fast_ema: Ema::new(fast_len),
            slow_ema: Ema::new(slow_len),
            count: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(3, 10)
    }

    pub fn fast_len(&self) -> usize {
        self.fast_len
    }

    pub fn slow_len(&self) -> usize {
        self.slow_len
    }
}

impl Indicator for ChaikinOscillatorEngine {
    fn name(&self) -> &str {
        "chaikin_oscillator"
    }

    fn warmup_period(&self) -> usize {
        self.slow_len
    }

    fn reset(&mut self) {
        self.adl.reset();
        self.fast_ema.reset();
        self.slow_ema.reset();
        self.count = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let adl_val = self.adl.on_bar(bar)?.value;

        let fast = self.fast_ema.update(adl_val);
        let slow = self.slow_ema.update(adl_val);

        if self.count < self.slow_len {
            return None;
        }

        let cho_val = fast - slow;
        Some(IndicatorOutput::new(cho_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaikin_oscillator() {
        let mut cho = ChaikinOscillatorEngine::with_defaults();
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = cho.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
