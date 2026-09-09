use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::divergence::SlopeDivergence;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

pub struct Cci {
    cci_len: usize,
    avg_len: usize,
    sig_len: usize,
    lookback_extreme: usize,
    oversold: f64,
    overbought: f64,
    require_extreme_zone: bool,
    ctx_len: usize,

    source_window: VecDeque<f64>,
    cci_ema: Option<f64>,
    signal_ema: Option<f64>,
    cci_line_window: VecDeque<f64>,
    prev_cci_line: Option<f64>,
    prev_signal: Option<f64>,

    ctx_window: VecDeque<f64>,
    ctx_ema: Option<f64>,
    divergence: SlopeDivergence,

    alerts: CciAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CciAlerts {
    pub bull_extreme: bool,
    pub bear_extreme: bool,
    pub bull_zero_cross: bool,
    pub bear_zero_cross: bool,
    pub bull_divergence: bool,
    pub bear_divergence: bool,
    pub extreme_strength: f64,
    pub divergence_strength: f64,
}

impl Cci {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cci_len: usize,
        avg_len: usize,
        sig_len: usize,
        lookback_extreme: usize,
        oversold: f64,
        overbought: f64,
        require_extreme_zone: bool,
        ctx_len: usize,
        div_len: usize,
        div_min: f64,
    ) -> Self {
        Self {
            cci_len,
            avg_len,
            sig_len,
            lookback_extreme,
            oversold,
            overbought,
            require_extreme_zone,
            ctx_len,
            source_window: VecDeque::with_capacity(cci_len),
            cci_ema: None,
            signal_ema: None,
            cci_line_window: VecDeque::with_capacity(lookback_extreme),
            prev_cci_line: None,
            prev_signal: None,
            ctx_window: VecDeque::with_capacity(ctx_len),
            ctx_ema: None,
            divergence: SlopeDivergence::new(div_len, div_min),
            alerts: CciAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 3, 3, 5, -100.0, 100.0, true, 100, 4, 25.0)
    }

    fn ema_step(state: &mut Option<f64>, src: f64, len: usize) -> f64 {
        let alpha = 2.0 / (len as f64 + 1.0);
        let next = match *state {
            None => src,
            Some(prev) => alpha * src + (1.0 - alpha) * prev,
        };
        *state = Some(next);
        next
    }
}

impl Indicator for Cci {
    fn name(&self) -> &str {
        "cci"
    }

    fn warmup_period(&self) -> usize {
        self.cci_len.max(self.ctx_len)
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = CciAlerts::default();

        let source = bar.typical_price();

        if self.ctx_window.len() == self.ctx_len {
            self.ctx_window.pop_front();
        }
        self.ctx_window.push_back(source);
        let ctx_line = if self.ctx_window.len() == self.ctx_len {
            let ctx_sma: f64 = self.ctx_window.iter().sum::<f64>() / self.ctx_len as f64;
            let ctx_mean_dev: f64 = self
                .ctx_window
                .iter()
                .map(|v| (v - ctx_sma).abs())
                .sum::<f64>()
                / self.ctx_len as f64;
            let ctx_raw = if ctx_mean_dev != 0.0 {
                (source - ctx_sma) / (0.015 * ctx_mean_dev)
            } else {
                0.0
            };
            Some(Self::ema_step(&mut self.ctx_ema, ctx_raw, self.avg_len))
        } else {
            None
        };

        if self.source_window.len() == self.cci_len {
            self.source_window.pop_front();
        }
        self.source_window.push_back(source);
        if self.source_window.len() < self.cci_len {
            return None;
        }

        let sma: f64 = self.source_window.iter().sum::<f64>() / self.cci_len as f64;
        let mean_dev: f64 = self
            .source_window
            .iter()
            .map(|v| (v - sma).abs())
            .sum::<f64>()
            / self.cci_len as f64;
        let raw_cci = if mean_dev != 0.0 {
            (source - sma) / (0.015 * mean_dev)
        } else {
            0.0
        };

        let cci_line = Self::ema_step(&mut self.cci_ema, raw_cci, self.avg_len);
        let signal = Self::ema_step(&mut self.signal_ema, cci_line, self.sig_len);

        if self.cci_line_window.len() == self.lookback_extreme {
            self.cci_line_window.pop_front();
        }
        self.cci_line_window.push_back(cci_line);
        let was_oversold = self.cci_line_window.len() == self.lookback_extreme
            && self
                .cci_line_window
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min)
                <= self.oversold;
        let was_overbought = self.cci_line_window.len() == self.lookback_extreme
            && self
                .cci_line_window
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
                >= self.overbought;

        if let (Some(prev_cci), Some(prev_sig)) = (self.prev_cci_line, self.prev_signal) {
            let bull_cross = prev_cci <= prev_sig && cci_line > signal;
            let bear_cross = prev_cci >= prev_sig && cci_line < signal;
            self.alerts.bull_extreme = bull_cross && (!self.require_extreme_zone || was_oversold);
            self.alerts.bear_extreme = bear_cross && (!self.require_extreme_zone || was_overbought);
            self.alerts.bull_zero_cross = prev_cci <= 0.0 && cci_line > 0.0;
            self.alerts.bear_zero_cross = prev_cci >= 0.0 && cci_line < 0.0;

            let lowest = self
                .cci_line_window
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min);
            let highest = self
                .cci_line_window
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            self.alerts.extreme_strength = if self.alerts.bull_extreme {
                ((self.oversold - lowest) / self.oversold.abs()).clamp(0.0, 1.0)
            } else if self.alerts.bear_extreme {
                ((highest - self.overbought) / self.overbought.abs()).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self.prev_cci_line = Some(cci_line);
        self.prev_signal = Some(signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);
        if let Some(ctx_line) = ctx_line {
            let div = self.divergence.update(cci_line, ctx_line);
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

        Some(IndicatorOutput::with_extra(cci_line, extra))
    }

    fn reset(&mut self) {
        self.source_window.clear();
        self.cci_ema = None;
        self.signal_ema = None;
        self.cci_line_window.clear();
        self.prev_cci_line = None;
        self.prev_signal = None;
        self.ctx_window.clear();
        self.ctx_ema = None;
        self.divergence.reset();
        self.alerts = CciAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_extreme {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "CCI · BULL CROSS OVERSOLD".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bear_extreme {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "CCI · BEAR CROSS OVERBOUGHT".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bull_zero_cross {
            out.push(IndicatorAlert {
                kind: "bull_zero_cross".to_string(),
                note: "CCI · CROSS ABOVE ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_zero_cross {
            out.push(IndicatorAlert {
                kind: "bear_zero_cross".to_string(),
                note: "CCI · CROSS BELOW ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bull_divergence {
            out.push(IndicatorAlert {
                kind: "bull_divergence".to_string(),
                note: "CCI · BULL DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        if a.bear_divergence {
            out.push(IndicatorAlert {
                kind: "bear_divergence".to_string(),
                note: "CCI · BEAR DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        out
    }
}
