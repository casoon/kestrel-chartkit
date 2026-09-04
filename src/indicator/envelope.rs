use super::smoothing::Sma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

/// Moving Average Envelopes Engine (% upper and lower bands around SMA).
#[derive(Debug, Clone)]
pub struct EnvelopeEngine {
    period: usize,
    percent: f64,
    sma: Sma,
}

impl EnvelopeEngine {
    pub fn new(period: usize, percent: f64) -> Self {
        Self {
            period: period.max(1),
            percent: percent.max(0.001),
            sma: Sma::new(period),
        }
    }
}

impl Indicator for EnvelopeEngine {
    fn name(&self) -> &str {
        "envelope"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.sma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let basis = self.sma.update(bar.close)?;
        let band_margin = basis * (self.percent / 100.0);

        let upper = basis + band_margin;
        let lower = basis - band_margin;

        let mut extra = HashMap::new();
        extra.insert("basis".to_string(), basis);
        extra.insert("upper".to_string(), upper);
        extra.insert("lower".to_string(), lower);

        Some(IndicatorOutput::with_extra(basis, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_bands() {
        let mut env = EnvelopeEngine::new(5, 5.0);
        let mut out = None;
        for i in 0..10 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0, 1000.0);
            out = env.on_bar(&b);
        }
        let o = out.unwrap();
        assert_eq!(o.extra["basis"], 100.0);
        assert_eq!(o.extra["upper"], 105.0);
        assert_eq!(o.extra["lower"], 95.0);
    }
}
