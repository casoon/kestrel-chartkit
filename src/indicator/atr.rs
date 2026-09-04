use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

#[derive(Debug, Clone)]
pub struct Atr {
    prev_close: Option<f64>,
    tr_rma: Rma,
    signal_rma: Rma,

    prev_atr_disp: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,
    warmup_period: usize,

    alerts: AtrAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AtrAlerts {
    pub expansion: bool,
    pub contraction: bool,
    pub regime_strength: f64,
}

impl Atr {
    pub fn new(atr_len: usize, sig_len: usize) -> Self {
        Self {
            prev_close: None,
            tr_rma: Rma::new(atr_len),
            signal_rma: Rma::new(sig_len),
            prev_atr_disp: None,
            prev_signal: None,
            bars_seen: 0,
            warmup_period: atr_len + sig_len - 1,
            alerts: AtrAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        // Matches the registry's "atr" catalog default (atr_len=14, sig_len=20) -- sig_len was
        // previously 14 here, silently diverging from the registry-built default.
        Self::new(14, 20)
    }

    pub fn with_period(atr_len: usize) -> Self {
        Self::new(atr_len, 14)
    }
}

impl Indicator for Atr {
    fn name(&self) -> &str {
        "atr"
    }

    fn warmup_period(&self) -> usize {
        self.warmup_period
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = AtrAlerts::default();
        self.bars_seen += 1;

        let tr = match self.prev_close {
            None => bar.high - bar.low,
            Some(prev_close) => (bar.high - bar.low)
                .max((bar.high - prev_close).abs())
                .max((bar.low - prev_close).abs()),
        };
        self.prev_close = Some(bar.close);

        let atr_raw = self.tr_rma.update(tr)?;
        let atr_disp = if bar.close > 0.0 {
            100.0 * atr_raw / bar.close
        } else {
            0.0
        };
        let atr_signal = self.signal_rma.update(atr_disp)?;

        if let (Some(prev_disp), Some(prev_sig)) = (self.prev_atr_disp, self.prev_signal) {
            self.alerts.expansion = crossed_over(prev_disp, prev_sig, atr_disp, atr_signal);
            self.alerts.contraction = crossed_under(prev_disp, prev_sig, atr_disp, atr_signal);
            self.alerts.regime_strength = if atr_signal != 0.0 {
                ((atr_disp - atr_signal) / atr_signal).abs().clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self.prev_atr_disp = Some(atr_disp);
        self.prev_signal = Some(atr_signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), atr_signal);

        Some(IndicatorOutput::with_extra(atr_disp, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.tr_rma.reset();
        self.signal_rma.reset();
        self.prev_atr_disp = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.alerts = AtrAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.expansion {
            out.push(IndicatorAlert {
                kind: "expansion".to_string(),
                note: "ATR · VOLA EXPANSION".to_string(),
                strength: a.regime_strength,
            });
        }
        if a.contraction {
            out.push(IndicatorAlert {
                kind: "contraction".to_string(),
                note: "ATR · VOLA CONTRACTION".to_string(),
                strength: a.regime_strength,
            });
        }
        out
    }
}
