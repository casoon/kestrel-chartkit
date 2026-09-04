use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Ema, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

#[derive(Debug, Clone)]
pub struct Adx {
    level_weak: f64,

    prev_bar: Option<(f64, f64, f64)>,
    tr_rma: Rma,
    dm_plus_rma: Rma,
    dm_minus_rma: Rma,
    dx_rma: Rma,
    adx_signal: Ema,

    prev_di_plus: Option<f64>,
    prev_di_minus: Option<f64>,
    prev_adx_raw: Option<f64>,
    bars_seen: usize,
    warmup_period: usize,

    alerts: AdxAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdxAlerts {
    pub bull_di_cross: bool,
    pub bear_di_cross: bool,
    pub adx_activated: bool,
    pub adx_deactivated: bool,
    pub regime_strength: f64,
}

impl Adx {
    pub fn new(di_len: usize, adx_smooth: usize, sig_len: usize, level_weak: f64) -> Self {
        Self {
            level_weak,
            prev_bar: None,
            tr_rma: Rma::new(di_len),
            dm_plus_rma: Rma::new(di_len),
            dm_minus_rma: Rma::new(di_len),
            dx_rma: Rma::new(adx_smooth),
            adx_signal: Ema::new(sig_len),
            prev_di_plus: None,
            prev_di_minus: None,
            prev_adx_raw: None,
            bars_seen: 0,
            warmup_period: di_len + adx_smooth,
            alerts: AdxAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        // Matches the registry's "adx" catalog default (di_len=14, adx_smooth=14, sig_len=3,
        // level_weak=20.0) -- sig_len was previously 14 here, silently diverging from the
        // registry-built default.
        Self::new(14, 14, 3, 20.0)
    }

    pub fn with_period(period: usize) -> Self {
        Self::new(period, period, 14, 20.0)
    }
}

impl Indicator for Adx {
    fn name(&self) -> &str {
        "adx"
    }

    fn warmup_period(&self) -> usize {
        self.warmup_period
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = AdxAlerts::default();
        self.bars_seen += 1;

        let (prev_high, prev_low, prev_close) = match self.prev_bar {
            None => {
                self.prev_bar = Some((bar.high, bar.low, bar.close));
                return None;
            }
            Some(p) => p,
        };
        self.prev_bar = Some((bar.high, bar.low, bar.close));

        let up_move = bar.high - prev_high;
        let down_move = prev_low - bar.low;
        let dm_plus = if up_move > down_move && up_move > 0.0 {
            up_move
        } else {
            0.0
        };
        let dm_minus = if down_move > up_move && down_move > 0.0 {
            down_move
        } else {
            0.0
        };
        let tr = (bar.high - bar.low)
            .max((bar.high - prev_close).abs())
            .max((bar.low - prev_close).abs());

        let (tr_avg, dm_plus_avg, dm_minus_avg) = match (
            self.tr_rma.update(tr),
            self.dm_plus_rma.update(dm_plus),
            self.dm_minus_rma.update(dm_minus),
        ) {
            (Some(t), Some(p), Some(m)) => (t, p, m),
            _ => return None,
        };

        let di_plus = if tr_avg != 0.0 {
            100.0 * dm_plus_avg / tr_avg
        } else {
            0.0
        };
        let di_minus = if tr_avg != 0.0 {
            100.0 * dm_minus_avg / tr_avg
        } else {
            0.0
        };
        let di_sum = di_plus + di_minus;
        let dx = if di_sum != 0.0 {
            100.0 * (di_plus - di_minus).abs() / di_sum
        } else {
            0.0
        };

        let adx_raw = self.dx_rma.update(dx)?;
        let adx_line = self.adx_signal.update(adx_raw);

        if let (Some(prev_plus), Some(prev_minus), Some(prev_adx)) =
            (self.prev_di_plus, self.prev_di_minus, self.prev_adx_raw)
        {
            self.alerts.bull_di_cross = crossed_over(prev_plus, prev_minus, di_plus, di_minus);
            self.alerts.bear_di_cross = crossed_under(prev_plus, prev_minus, di_plus, di_minus);
            self.alerts.adx_activated =
                crossed_over(prev_adx, self.level_weak, adx_raw, self.level_weak);
            self.alerts.adx_deactivated =
                crossed_under(prev_adx, self.level_weak, adx_raw, self.level_weak);
            if self.alerts.adx_activated {
                self.alerts.regime_strength =
                    ((adx_raw - self.level_weak) / self.level_weak).clamp(0.0, 1.0);
            } else if self.alerts.adx_deactivated {
                self.alerts.regime_strength =
                    ((self.level_weak - adx_raw) / self.level_weak).clamp(0.0, 1.0);
            }
        }
        self.prev_di_plus = Some(di_plus);
        self.prev_di_minus = Some(di_minus);
        self.prev_adx_raw = Some(adx_raw);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), adx_line);
        extra.insert("di_plus".to_string(), di_plus);
        extra.insert("di_minus".to_string(), di_minus);

        Some(IndicatorOutput::with_extra(adx_raw, extra))
    }

    fn reset(&mut self) {
        self.prev_bar = None;
        self.tr_rma.reset();
        self.dm_plus_rma.reset();
        self.dm_minus_rma.reset();
        self.dx_rma.reset();
        self.adx_signal.reset();
        self.prev_di_plus = None;
        self.prev_di_minus = None;
        self.prev_adx_raw = None;
        self.bars_seen = 0;
        self.alerts = AdxAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_di_cross {
            out.push(IndicatorAlert {
                kind: "bull_di_cross".to_string(),
                note: "ADX · DI BULL CROSS".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_di_cross {
            out.push(IndicatorAlert {
                kind: "bear_di_cross".to_string(),
                note: "ADX · DI BEAR CROSS".to_string(),
                strength: 1.0,
            });
        }
        if a.adx_activated {
            out.push(IndicatorAlert {
                kind: "adx_activated".to_string(),
                note: "ADX · ADX ACTIVATED".to_string(),
                strength: a.regime_strength,
            });
        }
        if a.adx_deactivated {
            out.push(IndicatorAlert {
                kind: "adx_deactivated".to_string(),
                note: "ADX · ADX DEACTIVATED".to_string(),
                strength: a.regime_strength,
            });
        }
        out
    }
}
