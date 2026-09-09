use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::divergence::SlopeDivergence;
use super::smoothing::{crossed_over, crossed_under, Ema, ExtremeWindow};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

pub struct WilliamsR {
    wpr_len: usize,
    mid_line: f64,
    oversold: f64,
    overbought: f64,
    require_extreme_zone: bool,
    ctx_len: usize,

    hl_window: VecDeque<(f64, f64)>,
    avg: Ema,
    signal_avg: Ema,
    extreme_window: ExtremeWindow,
    prev_wpr_line: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,

    ctx_hl_window: VecDeque<(f64, f64)>,
    ctx_avg: Ema,
    divergence: SlopeDivergence,

    alerts: WilliamsRAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WilliamsRAlerts {
    pub bull_extreme: bool,
    pub bear_extreme: bool,
    pub bull_mid_cross: bool,
    pub bear_mid_cross: bool,
    pub bull_divergence: bool,
    pub bear_divergence: bool,
    pub extreme_strength: f64,
    pub divergence_strength: f64,
}

impl WilliamsR {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wpr_len: usize,
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
            wpr_len,
            mid_line,
            oversold,
            overbought,
            require_extreme_zone,
            ctx_len,
            hl_window: VecDeque::with_capacity(wpr_len),
            avg: Ema::new(avg_len),
            signal_avg: Ema::new(sig_len),
            extreme_window: ExtremeWindow::new(lookback_extreme),
            prev_wpr_line: None,
            prev_signal: None,
            bars_seen: 0,
            ctx_hl_window: VecDeque::with_capacity(ctx_len),
            ctx_avg: Ema::new(avg_len),
            divergence: SlopeDivergence::new(div_len, div_min),
            alerts: WilliamsRAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14, 3, 3, 50.0, 80.0, 20.0, 5, true, 50, 4, 10.0)
    }
}

impl Indicator for WilliamsR {
    fn name(&self) -> &str {
        "williams_r"
    }

    fn warmup_period(&self) -> usize {
        self.wpr_len.max(self.ctx_len)
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = WilliamsRAlerts::default();
        self.bars_seen += 1;

        if self.ctx_hl_window.len() == self.ctx_len {
            self.ctx_hl_window.pop_front();
        }
        self.ctx_hl_window.push_back((bar.high, bar.low));
        let ctx_line = if self.ctx_hl_window.len() == self.ctx_len {
            let ctx_highest_high = self
                .ctx_hl_window
                .iter()
                .map(|(h, _)| *h)
                .fold(f64::NEG_INFINITY, f64::max);
            let ctx_lowest_low = self
                .ctx_hl_window
                .iter()
                .map(|(_, l)| *l)
                .fold(f64::INFINITY, f64::min);
            let ctx_range = ctx_highest_high - ctx_lowest_low;
            let ctx_raw = if ctx_range != 0.0 {
                100.0 * (bar.close - ctx_lowest_low) / ctx_range
            } else {
                50.0
            };
            Some(self.ctx_avg.update(ctx_raw))
        } else {
            None
        };

        if self.hl_window.len() == self.wpr_len {
            self.hl_window.pop_front();
        }
        self.hl_window.push_back((bar.high, bar.low));
        if self.hl_window.len() < self.wpr_len {
            return None;
        }

        let highest_high = self
            .hl_window
            .iter()
            .map(|(h, _)| *h)
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest_low = self
            .hl_window
            .iter()
            .map(|(_, l)| *l)
            .fold(f64::INFINITY, f64::min);
        let range = highest_high - lowest_low;
        let wpr_raw = if range != 0.0 {
            100.0 * (bar.close - lowest_low) / range
        } else {
            50.0
        };

        let wpr_line = self.avg.update(wpr_raw);
        let signal = self.signal_avg.update(wpr_line);

        let extreme = self.extreme_window.push(wpr_line);
        let was_oversold = extreme
            .map(|(low, _)| low <= self.oversold)
            .unwrap_or(false);
        let was_overbought = extreme
            .map(|(_, high)| high >= self.overbought)
            .unwrap_or(false);

        if let (Some(prev_wpr), Some(prev_sig)) = (self.prev_wpr_line, self.prev_signal) {
            let bull_cross = crossed_over(prev_wpr, prev_sig, wpr_line, signal);
            let bear_cross = crossed_under(prev_wpr, prev_sig, wpr_line, signal);
            self.alerts.bull_extreme = bull_cross && (!self.require_extreme_zone || was_oversold);
            self.alerts.bear_extreme = bear_cross && (!self.require_extreme_zone || was_overbought);
            self.alerts.bull_mid_cross =
                crossed_over(prev_wpr, self.mid_line, wpr_line, self.mid_line);
            self.alerts.bear_mid_cross =
                crossed_under(prev_wpr, self.mid_line, wpr_line, self.mid_line);

            self.alerts.extreme_strength = if let Some((low, high)) = extreme {
                if self.alerts.bull_extreme {
                    ((self.oversold - low) / self.oversold.abs()).clamp(0.0, 1.0)
                } else if self.alerts.bear_extreme {
                    ((high - self.overbought) / self.overbought.abs()).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            } else {
                0.0
            };
        }
        self.prev_wpr_line = Some(wpr_line);
        self.prev_signal = Some(signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);
        if let Some(ctx_line) = ctx_line {
            let div = self.divergence.update(wpr_line, ctx_line);
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

        Some(IndicatorOutput::with_extra(wpr_line, extra))
    }

    fn reset(&mut self) {
        self.hl_window.clear();
        self.avg.reset();
        self.signal_avg.reset();
        self.extreme_window.reset();
        self.prev_wpr_line = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.ctx_hl_window.clear();
        self.ctx_avg.reset();
        self.divergence.reset();
        self.alerts = WilliamsRAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_extreme {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "WPR · BULL CROSS OVERSOLD".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bear_extreme {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "WPR · BEAR CROSS OVERBOUGHT".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bull_mid_cross {
            out.push(IndicatorAlert {
                kind: "bull_mid_cross".to_string(),
                note: "WPR · CROSS ABOVE 50".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_mid_cross {
            out.push(IndicatorAlert {
                kind: "bear_mid_cross".to_string(),
                note: "WPR · CROSS BELOW 50".to_string(),
                strength: 1.0,
            });
        }
        if a.bull_divergence {
            out.push(IndicatorAlert {
                kind: "bull_divergence".to_string(),
                note: "WILLIAMS %R · BULL DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        if a.bear_divergence {
            out.push(IndicatorAlert {
                kind: "bear_divergence".to_string(),
                note: "WILLIAMS %R · BEAR DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        out
    }
}
