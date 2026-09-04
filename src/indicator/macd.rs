use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Ema};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

pub struct Macd {
    fast_ema: Ema,
    slow_ema: Ema,
    signal_ema: Ema,
    slow_len: usize,

    prev_macd: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,

    alerts: MacdAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MacdAlerts {
    pub bull_cross: bool,
    pub bear_cross: bool,
    pub bull_zero_cross: bool,
    pub bear_zero_cross: bool,
}

impl Macd {
    pub fn new(fast_len: usize, slow_len: usize, signal_len: usize) -> Self {
        Self {
            fast_ema: Ema::new(fast_len),
            slow_ema: Ema::new(slow_len),
            signal_ema: Ema::new(signal_len),
            slow_len,
            prev_macd: None,
            prev_signal: None,
            bars_seen: 0,
            alerts: MacdAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(12, 26, 9)
    }
}

impl Indicator for Macd {
    fn name(&self) -> &str {
        "macd"
    }

    fn warmup_period(&self) -> usize {
        self.slow_len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = MacdAlerts::default();
        self.bars_seen += 1;

        let close = bar.close;
        let fast = self.fast_ema.update(close);
        let slow = self.slow_ema.update(close);

        if self.bars_seen < self.slow_len {
            return None;
        }

        let macd_line = fast - slow;
        let signal_line = self.signal_ema.update(macd_line);
        let hist = macd_line - signal_line;

        if let (Some(prev_m), Some(prev_s)) = (self.prev_macd, self.prev_signal) {
            self.alerts.bull_cross = crossed_over(prev_m, prev_s, macd_line, signal_line);
            self.alerts.bear_cross = crossed_under(prev_m, prev_s, macd_line, signal_line);
            self.alerts.bull_zero_cross = crossed_over(prev_m, 0.0, macd_line, 0.0);
            self.alerts.bear_zero_cross = crossed_under(prev_m, 0.0, macd_line, 0.0);
        }

        self.prev_macd = Some(macd_line);
        self.prev_signal = Some(signal_line);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal_line);
        extra.insert("hist".to_string(), hist);

        Some(IndicatorOutput::with_extra(macd_line, extra))
    }

    fn reset(&mut self) {
        self.fast_ema.reset();
        self.slow_ema.reset();
        self.signal_ema.reset();
        self.prev_macd = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.alerts = MacdAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_cross {
            out.push(IndicatorAlert {
                kind: "bull_cross".to_string(),
                note: "MACD · BULL CROSS SIGNAL".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_cross {
            out.push(IndicatorAlert {
                kind: "bear_cross".to_string(),
                note: "MACD · BEAR CROSS SIGNAL".to_string(),
                strength: 1.0,
            });
        }
        if a.bull_zero_cross {
            out.push(IndicatorAlert {
                kind: "bull_zero_cross".to_string(),
                note: "MACD · CROSS ABOVE ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_zero_cross {
            out.push(IndicatorAlert {
                kind: "bear_zero_cross".to_string(),
                note: "MACD · CROSS BELOW ZERO".to_string(),
                strength: 1.0,
            });
        }
        out
    }
}
