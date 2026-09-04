use super::smoothing::Ema;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

/// Cumulative Volume Delta (CVD) Engine.
/// Derives buying/selling volume delta from bar price action (or direct volume delta).
#[derive(Debug, Clone)]
pub struct CvdEngine {
    cum_cvd: f64,
}

impl CvdEngine {
    pub fn new() -> Self {
        Self { cum_cvd: 0.0 }
    }
}

impl Default for CvdEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for CvdEngine {
    fn name(&self) -> &str {
        "cvd"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.cum_cvd = 0.0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let range = (bar.high - bar.low).max(1e-8);
        // Estimate buy/sell volume fraction from bar close location within range
        let buy_pct = (bar.close - bar.low) / range;
        let buy_vol = bar.volume * buy_pct;
        let sell_vol = bar.volume * (1.0 - buy_pct);

        let delta = buy_vol - sell_vol;
        self.cum_cvd += delta;

        let mut extra = HashMap::new();
        extra.insert("delta".to_string(), delta);
        extra.insert("buy_volume".to_string(), buy_vol);
        extra.insert("sell_volume".to_string(), sell_vol);

        Some(IndicatorOutput::with_extra(self.cum_cvd, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Klinger Volume Force Engine.
#[derive(Debug, Clone)]
pub struct KlingerVolumeForceEngine {
    fast_len: usize,
    slow_len: usize,
    signal_len: usize,
    fast_ema: Ema,
    slow_ema: Ema,
    signal_ema: Ema,
    prev_hlc_sum: Option<f64>,
    prev_trend: f64,
    prev_dm: f64,
    cumulative_measurement: f64,
    count: usize,
}

impl KlingerVolumeForceEngine {
    pub fn new(fast_len: usize, slow_len: usize, signal_len: usize) -> Self {
        Self {
            fast_len,
            slow_len,
            signal_len,
            fast_ema: Ema::new(fast_len),
            slow_ema: Ema::new(slow_len),
            signal_ema: Ema::new(signal_len),
            prev_hlc_sum: None,
            prev_trend: 1.0,
            prev_dm: 0.0,
            cumulative_measurement: 0.0,
            count: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(34, 55, 13)
    }

    pub fn fast_len(&self) -> usize {
        self.fast_len
    }
}

impl Indicator for KlingerVolumeForceEngine {
    fn name(&self) -> &str {
        "klinger"
    }

    fn warmup_period(&self) -> usize {
        self.slow_len + self.signal_len
    }

    fn reset(&mut self) {
        self.fast_ema.reset();
        self.slow_ema.reset();
        self.signal_ema.reset();
        self.prev_hlc_sum = None;
        self.prev_trend = 1.0;
        self.prev_dm = 0.0;
        self.cumulative_measurement = 0.0;
        self.count = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let hlc_sum = bar.high + bar.low + bar.close;
        let trend = match self.prev_hlc_sum {
            Some(previous) => {
                if hlc_sum > previous {
                    1.0
                } else if hlc_sum < previous {
                    -1.0
                } else {
                    self.prev_trend
                }
            }
            None => 1.0,
        };
        let dm = bar.high - bar.low;
        self.cumulative_measurement = if self.prev_hlc_sum.is_none() {
            dm
        } else if trend == self.prev_trend {
            self.cumulative_measurement + dm
        } else {
            self.prev_dm + dm
        };
        let ratio = if self.cumulative_measurement.abs() > f64::EPSILON {
            dm / self.cumulative_measurement
        } else {
            0.0
        };
        let vf = bar.volume * (2.0 * ratio - 1.0).abs() * trend * 100.0;
        self.prev_hlc_sum = Some(hlc_sum);
        self.prev_trend = trend;
        self.prev_dm = dm;

        let fast_v = self.fast_ema.update(vf);
        let slow_v = self.slow_ema.update(vf);

        if self.count < self.slow_len {
            return None;
        }

        let kvo = fast_v - slow_v;
        let sig = self.signal_ema.update(kvo);

        let mut extra = HashMap::new();
        extra.insert("volume_force".to_string(), vf);
        extra.insert("kvo".to_string(), kvo);
        extra.insert("signal".to_string(), sig);
        extra.insert("hist".to_string(), kvo - sig);

        Some(IndicatorOutput::with_extra(kvo, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cvd_accumulation() {
        let mut cvd = CvdEngine::new();
        let bar1 = Bar::new(1, 100.0, 105.0, 95.0, 105.0, 1000.0); // Close at High -> 100% buy vol
        let out1 = cvd.on_bar(&bar1).unwrap();
        assert_eq!(out1.value, 1000.0);
    }

    #[test]
    fn klinger_matches_original_volume_force_formula() {
        let mut klinger = KlingerVolumeForceEngine::new(1, 2, 2);
        let first = Bar::new(1, 1.0, 3.0, 1.0, 2.0, 100.0);
        assert!(klinger.on_bar(&first).is_none());

        let second = Bar::new(2, 2.0, 3.0, 1.0, 2.0, 100.0);
        let output = klinger.on_bar(&second).unwrap();
        assert_eq!(output.extra["volume_force"], 0.0);
        assert!((output.value + 3_333.333_333_333_333).abs() < 1e-9);
    }
}
