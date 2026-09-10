use super::smoothing::Ema;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

/// Buy/Sell Pressure: where bars close within their range and which wick dominates, smoothed,
/// `-100..=100`.
///
/// Per bar, with `range = max(high - low, 1e-8)`:
///
/// - `location = 2 · (close - low) / range - 1`: `-1` at the low, `+1` at the high;
/// - `wick_balance = (lower_wick - upper_wick) / range`, where `upper_wick = high - max(open,
///   close)` and `lower_wick = min(open, close) - low`, a body outside the range clipped to it;
/// - `raw = (0.6 · location + 0.4 · wick_balance) · 100`.
///
/// `value = clamp(EMA(period)(raw), -100, 100)` with the EMA seeded by the first bar's raw value
/// (see [`Ema`]), so the first output comes with the first bar, although `warmup_period()`
/// reports `period`. `extra["location"]` and `extra["wick_balance"]` are the current bar's,
/// unsmoothed. Volume does not enter.
#[derive(Debug, Clone)]
pub struct BuySellPressureEstimator {
    period: usize,
    ema: Ema,
}

impl BuySellPressureEstimator {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            ema: Ema::new(period),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14)
    }
}

impl Indicator for BuySellPressureEstimator {
    fn name(&self) -> &str {
        "buy_sell_pressure"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.ema.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let range = (bar.high - bar.low).max(1e-8);

        // Location of close within range (-1.0 .. +1.0)
        let location = (2.0 * (bar.close - bar.low) / range) - 1.0;

        // Upper wick vs Lower wick ratio
        let upper_wick = bar.high - bar.high.min(bar.open.max(bar.close));
        let lower_wick = bar.low.max(bar.open.min(bar.close)) - bar.low;
        let wick_balance = (lower_wick - upper_wick) / range;

        // Raw pressure per bar
        let raw_pressure = (location * 0.6 + wick_balance * 0.4) * 100.0;
        let smoothed_pressure = self.ema.update(raw_pressure)?.clamp(-100.0, 100.0);

        let mut extra = HashMap::new();
        extra.insert("location".to_string(), location);
        extra.insert("wick_balance".to_string(), wick_balance);

        Some(IndicatorOutput::with_extra(smoothed_pressure, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buy_sell_pressure() {
        let mut bsp = BuySellPressureEstimator::with_defaults();
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 104.0, 1000.0);
            out = bsp.on_bar(&b);
        }
        assert!(out.is_some());
        let val = out.unwrap().value;
        assert!((-100.0..=100.0).contains(&val));
    }
}
