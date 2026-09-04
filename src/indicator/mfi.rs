use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Ema, ExtremeWindow};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

pub struct Mfi {
    mfi_len: usize,
    mid_line: f64,
    oversold: f64,
    overbought: f64,
    require_extreme_zone: bool,

    prev_src: Option<f64>,
    flow_window: VecDeque<(f64, f64, f64)>,
    pos_sum: f64,
    neg_sum: f64,
    vol_sum: f64,

    mfi_avg: Ema,
    signal_avg: Ema,
    extreme_window: ExtremeWindow,
    prev_mfi_line: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,

    alerts: MfiAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MfiAlerts {
    pub bull_extreme: bool,
    pub bear_extreme: bool,
    pub bull_mid_cross: bool,
    pub bear_mid_cross: bool,
    pub extreme_strength: f64,
}

impl Mfi {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mfi_len: usize,
        avg_len: usize,
        sig_len: usize,
        mid_line: f64,
        overbought: f64,
        oversold: f64,
        lookback_extreme: usize,
        require_extreme_zone: bool,
    ) -> Self {
        Self {
            mfi_len,
            mid_line,
            oversold,
            overbought,
            require_extreme_zone,
            prev_src: None,
            flow_window: VecDeque::with_capacity(mfi_len),
            pos_sum: 0.0,
            neg_sum: 0.0,
            vol_sum: 0.0,
            mfi_avg: Ema::new(avg_len),
            signal_avg: Ema::new(sig_len),
            extreme_window: ExtremeWindow::new(lookback_extreme),
            prev_mfi_line: None,
            prev_signal: None,
            bars_seen: 0,
            alerts: MfiAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14, 3, 3, 50.0, 80.0, 20.0, 5, true)
    }
}

impl Indicator for Mfi {
    fn name(&self) -> &str {
        "mfi"
    }

    fn warmup_period(&self) -> usize {
        self.mfi_len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = MfiAlerts::default();
        self.bars_seen += 1;

        let src = bar.typical_price();
        let raw_flow = src * bar.volume;
        let (pos_flow, neg_flow) = match self.prev_src {
            None => (0.0, 0.0),
            Some(prev) if src > prev => (raw_flow, 0.0),
            Some(prev) if src < prev => (0.0, raw_flow),
            Some(_) => (0.0, 0.0),
        };
        self.prev_src = Some(src);

        if self.flow_window.len() == self.mfi_len {
            let (old_pos, old_neg, old_vol) = self.flow_window.pop_front().unwrap();
            self.pos_sum -= old_pos;
            self.neg_sum -= old_neg;
            self.vol_sum -= old_vol;
        }
        self.flow_window.push_back((pos_flow, neg_flow, bar.volume));
        self.pos_sum += pos_flow;
        self.neg_sum += neg_flow;
        self.vol_sum += bar.volume;

        if self.flow_window.len() < self.mfi_len {
            return None;
        }

        let has_volume = self.vol_sum > 0.0;
        let total_flow = self.pos_sum + self.neg_sum;
        let raw_mfi = if total_flow == 0.0 {
            50.0
        } else if self.neg_sum == 0.0 {
            100.0
        } else if self.pos_sum == 0.0 {
            0.0
        } else {
            100.0 - 100.0 / (1.0 + self.pos_sum / self.neg_sum)
        };

        let mfi_line = self.mfi_avg.update(raw_mfi);
        let signal = self.signal_avg.update(mfi_line);

        let extreme = self.extreme_window.push(mfi_line);
        let was_oversold = extreme
            .map(|(low, _)| low <= self.oversold)
            .unwrap_or(false);
        let was_overbought = extreme
            .map(|(_, high)| high >= self.overbought)
            .unwrap_or(false);

        if let (Some(prev_mfi), Some(prev_sig)) = (self.prev_mfi_line, self.prev_signal) {
            let bull_cross = crossed_over(prev_mfi, prev_sig, mfi_line, signal);
            let bear_cross = crossed_under(prev_mfi, prev_sig, mfi_line, signal);
            self.alerts.bull_extreme =
                has_volume && bull_cross && (!self.require_extreme_zone || was_oversold);
            self.alerts.bear_extreme =
                has_volume && bear_cross && (!self.require_extreme_zone || was_overbought);
            self.alerts.bull_mid_cross =
                has_volume && crossed_over(prev_mfi, self.mid_line, mfi_line, self.mid_line);
            self.alerts.bear_mid_cross =
                has_volume && crossed_under(prev_mfi, self.mid_line, mfi_line, self.mid_line);

            let (lowest, highest) = extreme.unwrap_or((mfi_line, mfi_line));
            self.alerts.extreme_strength = if self.alerts.bull_extreme {
                ((self.oversold - lowest) / self.oversold.abs()).clamp(0.0, 1.0)
            } else if self.alerts.bear_extreme {
                ((highest - self.overbought) / self.overbought.abs()).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self.prev_mfi_line = Some(mfi_line);
        self.prev_signal = Some(signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);

        Some(IndicatorOutput::with_extra(mfi_line, extra))
    }

    fn reset(&mut self) {
        self.prev_src = None;
        self.flow_window.clear();
        self.pos_sum = 0.0;
        self.neg_sum = 0.0;
        self.vol_sum = 0.0;
        self.mfi_avg.reset();
        self.signal_avg.reset();
        self.extreme_window.reset();
        self.prev_mfi_line = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.alerts = MfiAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.bull_extreme {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "MFI · BULL CROSS OVERSOLD".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bear_extreme {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "MFI · BEAR CROSS OVERBOUGHT".to_string(),
                strength: a.extreme_strength,
            });
        }
        if a.bull_mid_cross {
            out.push(IndicatorAlert {
                kind: "bull_mid_cross".to_string(),
                note: "MFI · CROSS ABOVE 50".to_string(),
                strength: 1.0,
            });
        }
        if a.bear_mid_cross {
            out.push(IndicatorAlert {
                kind: "bear_mid_cross".to_string(),
                note: "MFI · CROSS BELOW 50".to_string(),
                strength: 1.0,
            });
        }
        out
    }
}
