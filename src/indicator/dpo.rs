use super::smoothing::Sma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::VecDeque;

/// Detrended Price Oscillator.
///
/// `DPO_t = close_t - SMA(period)_(t - shift)`, `shift = floor(period/2) + 1`: the current close
/// against the simple average of `period` closes displaced `shift` bars back.
///
/// First output: with the `period + shift`-th bar. [`Indicator::reset`] clears the averages.
#[derive(Debug, Clone)]
pub struct DpoEngine {
    period: usize,
    sma: Sma,
    averages: VecDeque<f64>,
}

impl DpoEngine {
    pub fn new(period: usize) -> Self {
        let period = period.max(2);
        Self {
            period,
            sma: Sma::new(period),
            averages: VecDeque::with_capacity(period / 2 + 2),
        }
    }

    fn shift(&self) -> usize {
        self.period / 2 + 1
    }
}

impl Indicator for DpoEngine {
    fn name(&self) -> &str {
        "dpo"
    }

    fn warmup_period(&self) -> usize {
        self.period + self.shift()
    }

    fn reset(&mut self) {
        self.sma.reset();
        self.averages.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let average = self.sma.update(bar.close)?;
        // The average `shift` bars back is the front once `shift + 1` averages are held.
        let dpo_val = (self.averages.len() == self.shift())
            .then(|| bar.close - self.averages.pop_front().expect("held"));
        self.averages.push_back(average);
        dpo_val.map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const DPO_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "dpo",
        aliases: &[],
        description: "Detrended Price Oscillator",
        params: &[super::definition::ParamSpec::count("period", 21, 2, 10000)
            .meaning("bars in the SMA; the SMA is taken period/2 + 1 bars back")],
        checks: &[],
        build: |p| Ok(Box::new(DpoEngine::new(p.count("period")))),
    };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpo_basic() {
        let mut dpo = DpoEngine::new(20);
        let mut out = None;
        for i in 0..40 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = dpo.on_bar(&b);
        }
        assert!(out.is_some());
    }

    /// The current close against the SMA `period/2 + 1` bars back, on a doubling series.
    #[test]
    fn dpo_compares_the_close_with_the_displaced_average() {
        let closes = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0];
        let mut dpo = DpoEngine::new(4);
        assert_eq!(dpo.warmup_period(), 7);
        let outs: Vec<_> = closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                dpo.on_bar(&Bar::new(i as i64, c, c, c, c, 1.0))
                    .map(|o| o.value)
            })
            .collect();
        assert_eq!(outs.iter().position(Option::is_some), Some(6));
        // SMA(4) at bar 4: 3.75, at bar 5: 7.5.
        assert_eq!(outs[6], Some(64.0 - 3.75));
        assert_eq!(outs[7], Some(128.0 - 7.5));
    }
}
