use super::smoothing::Sma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

/// Relative Vigor Index (RVI) Engine.
/// RVI = SMA((Close - Open) / (High - Low), period)
#[derive(Debug, Clone)]
pub struct RviEngine {
    period: usize,
    val_sma: Sma,
    sig_sma: Sma,
}

impl RviEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            val_sma: Sma::new(period),
            sig_sma: Sma::new(4),
        }
    }
}

impl Indicator for RviEngine {
    fn name(&self) -> &str {
        "rvi"
    }

    fn warmup_period(&self) -> usize {
        self.period + 4
    }

    fn reset(&mut self) {
        self.val_sma.reset();
        self.sig_sma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let range = (bar.high - bar.low).max(1e-8);
        let raw_val = (bar.close - bar.open) / range;

        let rvi_val = self.val_sma.update(raw_val)?;
        let sig_val = self.sig_sma.update(rvi_val)?;

        let mut extra = HashMap::new();
        extra.insert("rvi".to_string(), rvi_val);
        extra.insert("signal".to_string(), sig_val);

        Some(IndicatorOutput::with_extra(rvi_val, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rvi_basic() {
        let mut rvi = RviEngine::new(10);
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 104.0, 1000.0);
            out = rvi.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
