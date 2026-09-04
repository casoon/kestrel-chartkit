use std::collections::HashMap;

use crate::model::Bar;

use super::divergence::SlopeDivergence;
use super::smoothing::{crossed_over, crossed_under, Ema, ExtremeWindow, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

#[derive(Debug, Clone)]
pub struct Rsi {
    mid_line: f64,
    oversold: f64,
    overbought: f64,
    require_extreme_zone: bool,
    ctx_len: usize,

    prev_close: Option<f64>,
    avg_gain: Rma,
    avg_loss: Rma,
    rsi_avg: Ema,
    signal_avg: Ema,
    extreme_window: ExtremeWindow,
    prev_rsi_line: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,
    warmup_period: usize,

    ctx_avg_gain: Rma,
    ctx_avg_loss: Rma,
    ctx_avg: Ema,
    divergence: SlopeDivergence,

    alerts: RsiAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RsiAlerts {
    pub bull_extreme: bool,
    pub bear_extreme: bool,
    pub bull_mid_cross: bool,
    pub bear_mid_cross: bool,
    pub bull_divergence: bool,
    pub bear_divergence: bool,
    pub extreme_strength: f64,
    pub divergence_strength: f64,
}

impl Rsi {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rsi_len: usize,
        avg_len: usize,
        sig_len: usize,
        mid_line: f64,
        overbought: f64,
        oversold: f64,
        lookback_extreme: usize,
        require_extreme_zone: bool,
        ctx_len: usize,
        div_len: usize,
        div_min: f64,
    ) -> Self {
        Self {
            mid_line,
            oversold,
            overbought,
            require_extreme_zone,
            ctx_len,
            prev_close: None,
            avg_gain: Rma::new(rsi_len),
            avg_loss: Rma::new(rsi_len),
            rsi_avg: Ema::new(avg_len),
            signal_avg: Ema::new(sig_len),
            extreme_window: ExtremeWindow::new(lookback_extreme),
            prev_rsi_line: None,
            prev_signal: None,
            bars_seen: 0,
            warmup_period: rsi_len + 1,
            ctx_avg_gain: Rma::new(ctx_len),
            ctx_avg_loss: Rma::new(ctx_len),
            ctx_avg: Ema::new(avg_len),
            divergence: SlopeDivergence::new(div_len, div_min),
            alerts: RsiAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14, 3, 3, 50.0, 70.0, 30.0, 5, true, 100, 4, 10.0)
    }

    pub fn with_period(rsi_len: usize) -> Self {
        Self::new(rsi_len, 3, 3, 50.0, 70.0, 30.0, 5, true, 100, 4, 10.0)
    }
}

impl Indicator for Rsi {
    fn name(&self) -> &str {
        "rsi"
    }

    fn warmup_period(&self) -> usize {
        self.warmup_period.max(self.ctx_len + 1)
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = RsiAlerts::default();
        self.bars_seen += 1;

        let close = bar.close;
        let prev_close = match self.prev_close {
            None => {
                self.prev_close = Some(close);
                return None;
            }
            Some(p) => p,
        };
        self.prev_close = Some(close);

        let change = close - prev_close;
        let gain = change.max(0.0);
        let loss = (-change).max(0.0);

        let ctx_line = match (
            self.ctx_avg_gain.update(gain),
            self.ctx_avg_loss.update(loss),
        ) {
            (Some(ctx_avg_gain), Some(ctx_avg_loss)) => {
                let ctx_raw = if ctx_avg_gain == 0.0 && ctx_avg_loss == 0.0 {
                    50.0
                } else if ctx_avg_loss == 0.0 {
                    100.0
                } else if ctx_avg_gain == 0.0 {
                    0.0
                } else {
                    100.0 - 100.0 / (1.0 + ctx_avg_gain / ctx_avg_loss)
                };
                Some(self.ctx_avg.update(ctx_raw))
            }
            _ => None,
        };

        let (avg_gain, avg_loss) = match (self.avg_gain.update(gain), self.avg_loss.update(loss)) {
            (Some(g), Some(l)) => (g, l),
            _ => return None,
        };

        let raw_rsi = if avg_gain == 0.0 && avg_loss == 0.0 {
            50.0
        } else if avg_loss == 0.0 {
            100.0
        } else if avg_gain == 0.0 {
            0.0
        } else {
            (100.0 - 100.0 / (1.0 + avg_gain / avg_loss)).clamp(0.0, 100.0)
        };

        let rsi_line = self.rsi_avg.update(raw_rsi).clamp(0.0, 100.0);
        let signal = self.signal_avg.update(rsi_line).clamp(0.0, 100.0);

        let extreme = self.extreme_window.push(rsi_line);
        let was_oversold = extreme
            .map(|(low, _)| low <= self.oversold)
            .unwrap_or(false);
        let was_overbought = extreme
            .map(|(_, high)| high >= self.overbought)
            .unwrap_or(false);

        if let (Some(prev_rsi), Some(prev_sig)) = (self.prev_rsi_line, self.prev_signal) {
            let bull_cross = crossed_over(prev_rsi, prev_sig, rsi_line, signal);
            let bear_cross = crossed_under(prev_rsi, prev_sig, rsi_line, signal);
            self.alerts.bull_extreme = bull_cross && (!self.require_extreme_zone || was_oversold);
            self.alerts.bear_extreme = bear_cross && (!self.require_extreme_zone || was_overbought);
            self.alerts.bull_mid_cross =
                crossed_over(prev_rsi, self.mid_line, rsi_line, self.mid_line);
            self.alerts.bear_mid_cross =
                crossed_under(prev_rsi, self.mid_line, rsi_line, self.mid_line);

            self.alerts.extreme_strength = if self.alerts.bull_extreme {
                extreme
                    .map(|(low, _)| ((self.oversold - low) / self.oversold.abs()).clamp(0.0, 1.0))
                    .unwrap_or(0.0)
            } else if self.alerts.bear_extreme {
                extreme
                    .map(|(_, high)| {
                        ((high - self.overbought) / self.overbought.abs()).clamp(0.0, 1.0)
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };
        }
        self.prev_rsi_line = Some(rsi_line);
        self.prev_signal = Some(signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);
        if let Some(ctx_line) = ctx_line {
            let div = self.divergence.update(rsi_line, ctx_line);
            self.alerts.bull_divergence = div.bull;
            self.alerts.bear_divergence = div.bear;
            self.alerts.divergence_strength = if div.bull || div.bear {
                ((div.fast_dir.abs() - self.divergence.div_min()) / self.divergence.div_min())
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            extra.insert("ctx".to_string(), ctx_line);
        }

        Some(IndicatorOutput::with_extra(rsi_line, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.avg_gain.reset();
        self.avg_loss.reset();
        self.rsi_avg.reset();
        self.signal_avg.reset();
        self.extreme_window.reset();
        self.prev_rsi_line = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.ctx_avg_gain.reset();
        self.ctx_avg_loss.reset();
        self.ctx_avg.reset();
        self.divergence.reset();
        self.alerts = RsiAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_extreme {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "RSI · BULL CROSS OVERSOLD".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bear_extreme {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "RSI · BEAR CROSS OVERBOUGHT".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bull_mid_cross {
            out.push(IndicatorAlert {
                kind: "bull_mid_cross".to_string(),
                note: "RSI · CROSS ABOVE 50".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_mid_cross {
            out.push(IndicatorAlert {
                kind: "bear_mid_cross".to_string(),
                note: "RSI · CROSS BELOW 50".to_string(),
                strength: 1.0,
            });
        }
        if a.bull_divergence {
            out.push(IndicatorAlert {
                kind: "bull_divergence".to_string(),
                note: "RSI · BULL DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        if a.bear_divergence {
            out.push(IndicatorAlert {
                kind: "bear_divergence".to_string(),
                note: "RSI · BEAR DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        out
    }
}
