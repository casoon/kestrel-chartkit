use std::collections::HashMap;

use crate::model::Bar;

use super::divergence::SlopeDivergence;
use super::smoothing::{crossed_over, crossed_under, Ema, ExtremeWindow};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

pub struct Tsi {
    mid_line: f64,
    oversold: f64,
    overbought: f64,
    require_extreme_zone: bool,
    #[allow(dead_code)]
    ctx_long_len: usize,
    #[allow(dead_code)]
    ctx_short_len: usize,
    div_len: usize,

    prev_close: Option<f64>,
    mom_long: Ema,
    mom_short: Ema,
    abs_long: Ema,
    abs_short: Ema,
    signal_avg: Ema,
    extreme_window: ExtremeWindow,
    prev_tsi_line: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,

    ctx_mom_long: Ema,
    ctx_mom_short: Ema,
    ctx_abs_long: Ema,
    ctx_abs_short: Ema,
    divergence: SlopeDivergence,

    alerts: TsiAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TsiAlerts {
    pub bull_extreme: bool,
    pub bear_extreme: bool,
    pub bull_mid_cross: bool,
    pub bear_mid_cross: bool,
    pub bull_divergence: bool,
    pub bear_divergence: bool,
    pub extreme_strength: f64,
    pub divergence_strength: f64,
}

impl Tsi {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        long_len: usize,
        short_len: usize,
        sig_len: usize,
        mid_line: f64,
        overbought: f64,
        oversold: f64,
        lookback_extreme: usize,
        require_extreme_zone: bool,
        ctx_long_len: usize,
        ctx_short_len: usize,
        div_len: usize,
        div_min: f64,
    ) -> Self {
        Self {
            mid_line,
            oversold,
            overbought,
            require_extreme_zone,
            ctx_long_len,
            ctx_short_len,
            div_len,
            prev_close: None,
            mom_long: Ema::new(long_len),
            mom_short: Ema::new(short_len),
            abs_long: Ema::new(long_len),
            abs_short: Ema::new(short_len),
            signal_avg: Ema::new(sig_len),
            extreme_window: ExtremeWindow::new(lookback_extreme),
            prev_tsi_line: None,
            prev_signal: None,
            bars_seen: 0,
            ctx_mom_long: Ema::new(ctx_long_len),
            ctx_mom_short: Ema::new(ctx_short_len),
            ctx_abs_long: Ema::new(ctx_long_len),
            ctx_abs_short: Ema::new(ctx_short_len),
            divergence: SlopeDivergence::new(div_len, div_min),
            alerts: TsiAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(25, 13, 7, 0.0, 25.0, -25.0, 5, true, 50, 25, 4, 5.0)
    }
}

impl Indicator for Tsi {
    fn name(&self) -> &str {
        "tsi"
    }

    fn warmup_period(&self) -> usize {
        1 + self.div_len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = TsiAlerts::default();
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

        let mom = close - prev_close;
        let double_mom = self.mom_short.update(self.mom_long.update(mom));
        let double_abs = self.abs_short.update(self.abs_long.update(mom.abs()));
        let tsi_line = if double_abs != 0.0 {
            100.0 * double_mom / double_abs
        } else {
            0.0
        };
        let signal = self.signal_avg.update(tsi_line);

        let ctx_double_mom = self.ctx_mom_short.update(self.ctx_mom_long.update(mom));
        let ctx_double_abs = self
            .ctx_abs_short
            .update(self.ctx_abs_long.update(mom.abs()));
        let ctx_line = if ctx_double_abs != 0.0 {
            100.0 * ctx_double_mom / ctx_double_abs
        } else {
            0.0
        };

        let extreme = self.extreme_window.push(tsi_line);
        let was_oversold = extreme
            .map(|(low, _)| low <= self.oversold)
            .unwrap_or(false);
        let was_overbought = extreme
            .map(|(_, high)| high >= self.overbought)
            .unwrap_or(false);

        if let (Some(prev_tsi), Some(prev_sig)) = (self.prev_tsi_line, self.prev_signal) {
            let bull_cross = crossed_over(prev_tsi, prev_sig, tsi_line, signal);
            let bear_cross = crossed_under(prev_tsi, prev_sig, tsi_line, signal);
            self.alerts.bull_extreme = bull_cross && (!self.require_extreme_zone || was_oversold);
            self.alerts.bear_extreme = bear_cross && (!self.require_extreme_zone || was_overbought);
            self.alerts.bull_mid_cross =
                crossed_over(prev_tsi, self.mid_line, tsi_line, self.mid_line);
            self.alerts.bear_mid_cross =
                crossed_under(prev_tsi, self.mid_line, tsi_line, self.mid_line);

            self.alerts.extreme_strength = if self.alerts.bull_extreme {
                let (lowest, _) = extreme.unwrap_or((self.oversold, self.overbought));
                ((self.oversold - lowest) / self.oversold.abs()).clamp(0.0, 1.0)
            } else if self.alerts.bear_extreme {
                let (_, highest) = extreme.unwrap_or((self.oversold, self.overbought));
                ((highest - self.overbought) / self.overbought.abs()).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self.prev_tsi_line = Some(tsi_line);
        self.prev_signal = Some(signal);

        let div = self.divergence.update(tsi_line, ctx_line);
        self.alerts.bull_divergence = div.bull;
        self.alerts.bear_divergence = div.bear;
        self.alerts.divergence_strength = if div.bull || div.bear {
            ((div.fast_dir.abs() - self.divergence.div_min()) / self.divergence.div_min())
                .clamp(0.0, 1.0)
        } else {
            0.0
        };

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);
        extra.insert("ctx".to_string(), ctx_line);

        Some(IndicatorOutput::with_extra(tsi_line, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.mom_long.reset();
        self.mom_short.reset();
        self.abs_long.reset();
        self.abs_short.reset();
        self.signal_avg.reset();
        self.extreme_window.reset();
        self.prev_tsi_line = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.ctx_mom_long.reset();
        self.ctx_mom_short.reset();
        self.ctx_abs_long.reset();
        self.ctx_abs_short.reset();
        self.divergence.reset();
        self.alerts = TsiAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_extreme {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "TSI · BULL CROSS OVERSOLD".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bear_extreme {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "TSI · BEAR CROSS OVERBOUGHT".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bull_mid_cross {
            out.push(IndicatorAlert {
                kind: "bull_mid_cross".to_string(),
                note: "TSI · CROSS ABOVE ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_mid_cross {
            out.push(IndicatorAlert {
                kind: "bear_mid_cross".to_string(),
                note: "TSI · CROSS BELOW ZERO".to_string(),
                strength: 1.0,
            });
        }
        if a.bull_divergence {
            out.push(IndicatorAlert {
                kind: "bull_divergence".to_string(),
                note: "TSI · BULL DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        if a.bear_divergence {
            out.push(IndicatorAlert {
                kind: "bear_divergence".to_string(),
                note: "TSI · BEAR DIVERGENCE".to_string(),
                strength: a.divergence_strength,
            });
        }
        out
    }
}
