use super::smoothing::Sma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Ease of Movement (EOM), smoothed over `period` bars.
///
/// ```text
/// distance = (high + low) / 2 - (prev_high + prev_low) / 2
/// box      = (volume / volume_divisor) / (high - low)        (range floored at 1e-8)
/// raw      = distance / box                                  (0 for box <= 1e-8)
/// EOM      = SMA(period) of raw
/// ```
///
/// `volume_divisor` (default `10000`) only rescales volume into a readable unit. The first bar has
/// no predecessor and produces no raw value.
///
/// First output: with the `period + 1`-th bar. [`Indicator::reset`] clears the midpoint and the
/// average.
#[derive(Debug, Clone)]
pub struct EomEngine {
    period: usize,
    volume_divisor: f64,
    prev_hl2: Option<f64>,
    sma: Sma,
}

impl EomEngine {
    pub fn new(period: usize, volume_divisor: f64) -> Self {
        Self {
            period: period.max(1),
            volume_divisor: volume_divisor.max(1.0),
            prev_hl2: None,
            sma: Sma::new(period),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14, 10000.0)
    }
}

impl Indicator for EomEngine {
    fn name(&self) -> &str {
        "eom"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.prev_hl2 = None;
        self.sma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let hl2 = (bar.high + bar.low) / 2.0;

        let prev = match self.prev_hl2 {
            Some(p) => p,
            None => {
                self.prev_hl2 = Some(hl2);
                return None;
            }
        };
        self.prev_hl2 = Some(hl2);

        let distance = hl2 - prev;
        let range = (bar.high - bar.low).max(1e-8);
        let box_ratio = (bar.volume / self.volume_divisor) / range;

        let raw_eom = if box_ratio > 1e-8 {
            distance / box_ratio
        } else {
            0.0
        };
        let eom_val = self.sma.update(raw_eom)?;

        Some(IndicatorOutput::new(eom_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eom_basic() {
        let mut eom = EomEngine::with_defaults();
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 10000.0);
            out = eom.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
