use super::smoothing::{crossed_over, crossed_under, Ema, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// WaveTrend Alerts data structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct WaveTrendAlerts {
    pub bull_cross: bool,
    pub bear_cross: bool,
    pub overbought_cross: bool,
    pub oversold_cross: bool,
}

/// WaveTrend Oscillator Engine (LazyBear / Pine classic formulation).
#[derive(Debug, Clone)]
pub struct WaveTrendEngine {
    n1: usize,
    n2: usize,
    ob_level: f64,
    os_level: f64,
    ema_ap: Ema,
    ema_d: Ema,
    ema_wt1: Ema,
    sma_wt2: Sma,
    prev_wt1: Option<f64>,
    prev_wt2: Option<f64>,
    alerts: WaveTrendAlerts,
}

impl WaveTrendEngine {
    pub fn new(n1: usize, n2: usize, ob_level: f64, os_level: f64) -> Self {
        Self {
            n1,
            n2,
            ob_level,
            os_level,
            ema_ap: Ema::new(n1),
            ema_d: Ema::new(n1),
            ema_wt1: Ema::new(n2),
            sma_wt2: Sma::new(4),
            prev_wt1: None,
            prev_wt2: None,
            alerts: WaveTrendAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(10, 21, 60.0, -60.0)
    }

    pub fn alerts(&self) -> WaveTrendAlerts {
        self.alerts
    }
}

impl Indicator for WaveTrendEngine {
    fn name(&self) -> &str {
        "wavetrend"
    }

    fn warmup_period(&self) -> usize {
        self.n1 + self.n2 + 4
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let ap = bar.typical_price();
        let esa = self.ema_ap.update(ap);
        let d = self.ema_d.update((ap - esa).abs());

        let ci = if d > 1e-8 {
            (ap - esa) / (0.015 * d)
        } else {
            0.0
        };

        let wt1 = self.ema_wt1.update(ci);
        let wt2 = self.sma_wt2.update(wt1)?;

        self.alerts = WaveTrendAlerts::default();

        if let (Some(p_wt1), Some(p_wt2)) = (self.prev_wt1, self.prev_wt2) {
            self.alerts.bull_cross = crossed_over(p_wt1, p_wt2, wt1, wt2);
            self.alerts.bear_cross = crossed_under(p_wt1, p_wt2, wt1, wt2);
            self.alerts.overbought_cross = self.alerts.bear_cross && (wt1 >= self.ob_level);
            self.alerts.oversold_cross = self.alerts.bull_cross && (wt1 <= self.os_level);
        }

        self.prev_wt1 = Some(wt1);
        self.prev_wt2 = Some(wt2);

        let mut extra = HashMap::new();
        extra.insert("wt1".to_string(), wt1);
        extra.insert("wt2".to_string(), wt2);
        extra.insert("hist".to_string(), wt1 - wt2);
        extra.insert("ob_level".to_string(), self.ob_level);
        extra.insert("os_level".to_string(), self.os_level);

        Some(IndicatorOutput::with_extra(wt1, extra))
    }

    fn reset(&mut self) {
        self.ema_ap.reset();
        self.ema_d.reset();
        self.ema_wt1.reset();
        self.sma_wt2.reset();
        self.prev_wt1 = None;
        self.prev_wt2 = None;
        self.alerts = WaveTrendAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut res = Vec::new();
        if self.alerts.bull_cross {
            res.push(IndicatorAlert::new(
                "wt_bull_cross",
                "WaveTrend Bullish Cross",
                0.8,
            ));
        }
        if self.alerts.bear_cross {
            res.push(IndicatorAlert::new(
                "wt_bear_cross",
                "WaveTrend Bearish Cross",
                0.8,
            ));
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wavetrend_basic() {
        let mut wt = WaveTrendEngine::with_defaults();
        let mut outputs = Vec::new();
        for i in 0..50 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + (i as f64 * 0.5), 1000.0);
            if let Some(out) = wt.on_bar(&b) {
                outputs.push(out);
            }
        }
        assert!(!outputs.is_empty());
        let last = outputs.last().unwrap();
        assert!(last.extra.contains_key("wt1"));
        assert!(last.extra.contains_key("wt2"));
    }
}
