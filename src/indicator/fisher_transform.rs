use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::divergence::SlopeDivergence;
use super::smoothing::{crossed_over, crossed_under, Ema, ExtremeWindow};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

pub struct FisherTransform {
    fish_len: usize,
    mid_line: f64,
    oversold: f64,
    overbought: f64,
    require_extreme_zone: bool,
    ctx_len: usize,

    src_window: VecDeque<f64>,
    prev_value: Option<f64>,
    prev_fish: Option<f64>,

    avg: Ema,
    signal_avg: Ema,
    extreme_window: ExtremeWindow,
    prev_fish_line: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,

    ctx_window: VecDeque<f64>,
    ctx_prev_value: Option<f64>,
    ctx_prev_fish: Option<f64>,
    ctx_avg: Ema,
    divergence: SlopeDivergence,

    alerts: FisherAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FisherAlerts {
    pub bull_extreme: bool,
    pub bear_extreme: bool,
    pub bull_mid_cross: bool,
    pub bear_mid_cross: bool,
    pub bull_divergence: bool,
    pub bear_divergence: bool,
    pub extreme_strength: f64,
    pub divergence_strength: f64,
}

impl FisherTransform {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fish_len: usize,
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
            fish_len,
            mid_line,
            oversold,
            overbought,
            require_extreme_zone,
            ctx_len,
            src_window: VecDeque::with_capacity(fish_len),
            prev_value: None,
            prev_fish: None,
            avg: Ema::new(avg_len),
            signal_avg: Ema::new(sig_len),
            extreme_window: ExtremeWindow::new(lookback_extreme),
            prev_fish_line: None,
            prev_signal: None,
            bars_seen: 0,
            ctx_window: VecDeque::with_capacity(ctx_len),
            ctx_prev_value: None,
            ctx_prev_fish: None,
            ctx_avg: Ema::new(avg_len),
            divergence: SlopeDivergence::new(div_len, div_min),
            alerts: FisherAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(10, 2, 3, 0.0, 1.5, -1.5, 5, true, 40, 4, 0.5)
    }
}

impl Indicator for FisherTransform {
    fn name(&self) -> &str {
        "fisher_transform"
    }

    fn warmup_period(&self) -> usize {
        self.fish_len.max(self.ctx_len)
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = FisherAlerts::default();
        self.bars_seen += 1;

        let src = (bar.high + bar.low) / 2.0;

        if self.ctx_window.len() == self.ctx_len {
            self.ctx_window.pop_front();
        }
        self.ctx_window.push_back(src);
        let ctx_line = if self.ctx_window.len() == self.ctx_len {
            let ctx_highest = self
                .ctx_window
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            let ctx_lowest = self
                .ctx_window
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min);
            let ctx_range = ctx_highest - ctx_lowest;
            let ctx_normalized = if ctx_range != 0.0 {
                (src - ctx_lowest) / ctx_range - 0.5
            } else {
                0.0
            };
            let ctx_prev_value = self.ctx_prev_value.unwrap_or(0.0);
            let ctx_value = (0.66 * ctx_normalized + 0.67 * ctx_prev_value).clamp(-0.999, 0.999);
            let ctx_prev_fish = self.ctx_prev_fish.unwrap_or(0.0);
            let ctx_raw = 0.5 * ((1.0 + ctx_value) / (1.0 - ctx_value)).ln() + 0.5 * ctx_prev_fish;
            self.ctx_prev_value = Some(ctx_value);
            self.ctx_prev_fish = Some(ctx_raw);
            Some(self.ctx_avg.update(ctx_raw))
        } else {
            None
        };

        if self.src_window.len() == self.fish_len {
            self.src_window.pop_front();
        }
        self.src_window.push_back(src);
        if self.src_window.len() < self.fish_len {
            return None;
        }

        let highest_src = self
            .src_window
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest_src = self
            .src_window
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let range_src = highest_src - lowest_src;
        let normalized = if range_src != 0.0 {
            (src - lowest_src) / range_src - 0.5
        } else {
            0.0
        };

        let prev_value = self.prev_value.unwrap_or(0.0);
        let value = (0.66 * normalized + 0.67 * prev_value).clamp(-0.999, 0.999);
        let prev_fish = self.prev_fish.unwrap_or(0.0);
        let fish_raw = 0.5 * ((1.0 + value) / (1.0 - value)).ln() + 0.5 * prev_fish;
        self.prev_value = Some(value);
        self.prev_fish = Some(fish_raw);

        let fish_line = self.avg.update(fish_raw);
        let signal = self.signal_avg.update(fish_line);

        let extreme = self.extreme_window.push(fish_line);
        let was_oversold = extreme
            .map(|(low, _)| low <= self.oversold)
            .unwrap_or(false);
        let was_overbought = extreme
            .map(|(_, high)| high >= self.overbought)
            .unwrap_or(false);

        if let (Some(prev_fish_line), Some(prev_sig)) = (self.prev_fish_line, self.prev_signal) {
            let bull_cross = crossed_over(prev_fish_line, prev_sig, fish_line, signal);
            let bear_cross = crossed_under(prev_fish_line, prev_sig, fish_line, signal);
            self.alerts.bull_extreme = bull_cross && (!self.require_extreme_zone || was_oversold);
            self.alerts.bear_extreme = bear_cross && (!self.require_extreme_zone || was_overbought);
            self.alerts.bull_mid_cross =
                crossed_over(prev_fish_line, self.mid_line, fish_line, self.mid_line);
            self.alerts.bear_mid_cross =
                crossed_under(prev_fish_line, self.mid_line, fish_line, self.mid_line);

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
        self.prev_fish_line = Some(fish_line);
        self.prev_signal = Some(signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);
        if let Some(ctx_line) = ctx_line {
            let div = self.divergence.update(fish_line, ctx_line);
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

        Some(IndicatorOutput::with_extra(fish_line, extra))
    }

    fn reset(&mut self) {
        self.src_window.clear();
        self.prev_value = None;
        self.prev_fish = None;
        self.avg.reset();
        self.signal_avg.reset();
        self.extreme_window.reset();
        self.prev_fish_line = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.ctx_window.clear();
        self.ctx_prev_value = None;
        self.ctx_prev_fish = None;
        self.ctx_avg.reset();
        self.divergence.reset();
        self.alerts = FisherAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_extreme {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "FISH · BULL CROSS OVERSOLD".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bear_extreme {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "FISH · BEAR CROSS OVERBOUGHT".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bull_mid_cross {
            out.push(IndicatorAlert {
                kind: "bull_mid_cross".to_string(),
                note: "FISH · CROSS ABOVE ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_mid_cross {
            out.push(IndicatorAlert {
                kind: "bear_mid_cross".to_string(),
                note: "FISH · CROSS BELOW ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bull_divergence {
            out.push(IndicatorAlert {
                kind: "bull_divergence".to_string(),
                note: "FISHER · BULL DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        if a.bear_divergence {
            out.push(IndicatorAlert {
                kind: "bear_divergence".to_string(),
                note: "FISHER · BEAR DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        out
    }
}
