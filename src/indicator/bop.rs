use super::smoothing::Sma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Balance of Power (BOP) Engine.
/// BOP = SMA((Close - Open) / (High - Low), period)
#[derive(Debug, Clone)]
pub struct BalanceOfPowerEngine {
    period: usize,
    sma: Sma,
}

impl BalanceOfPowerEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            sma: Sma::new(period),
        }
    }
}

impl Indicator for BalanceOfPowerEngine {
    fn name(&self) -> &str {
        "bop"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.sma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let range = (bar.high - bar.low).max(1e-8);
        let raw_bop = (bar.close - bar.open) / range;
        let bop_val = self.sma.update(raw_bop)?;
        Some(IndicatorOutput::new(bop_val.clamp(-1.0, 1.0)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bop_bounds() {
        let mut bop = BalanceOfPowerEngine::new(14);
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 104.0, 1000.0);
            out = bop.on_bar(&b);
        }
        assert!(out.is_some());
        let val = out.unwrap().value;
        assert!((-1.0..=1.0).contains(&val));
    }
}
