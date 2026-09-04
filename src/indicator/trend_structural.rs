use std::collections::{HashMap, VecDeque};

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Directional Movement Index (DMI: +DI and -DI).
pub struct DmiEngine {
    period: usize,
    prev_bar: Option<Bar>,
    plus_dms: VecDeque<f64>,
    minus_dms: VecDeque<f64>,
    trs: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl DmiEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            prev_bar: None,
            plus_dms: VecDeque::new(),
            minus_dms: VecDeque::new(),
            trs: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for DmiEngine {
    fn name(&self) -> &str {
        "dmi"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.prev_bar = None;
        self.plus_dms.clear();
        self.minus_dms.clear();
        self.trs.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if let Some(ref prev) = self.prev_bar {
            let tr = (bar.high - bar.low)
                .max((bar.high - prev.close).abs())
                .max((bar.low - prev.close).abs());
            let up_move = bar.high - prev.high;
            let down_move = prev.low - bar.low;

            let plus_dm = if up_move > down_move && up_move > 0.0 {
                up_move
            } else {
                0.0
            };
            let minus_dm = if down_move > up_move && down_move > 0.0 {
                down_move
            } else {
                0.0
            };

            self.trs.push_back(tr);
            self.plus_dms.push_back(plus_dm);
            self.minus_dms.push_back(minus_dm);

            if self.trs.len() > self.period {
                self.trs.pop_front();
                self.plus_dms.pop_front();
                self.minus_dms.pop_front();
            }
        }
        self.prev_bar = Some(bar.clone());

        self.alerts.clear();
        if self.trs.len() < self.period {
            return None;
        }

        let sum_tr: f64 = self.trs.iter().sum();
        let sum_plus_dm: f64 = self.plus_dms.iter().sum();
        let sum_minus_dm: f64 = self.minus_dms.iter().sum();

        let plus_di = if sum_tr > 0.0 {
            (sum_plus_dm / sum_tr) * 100.0
        } else {
            0.0
        };
        let minus_di = if sum_tr > 0.0 {
            (sum_minus_dm / sum_tr) * 100.0
        } else {
            0.0
        };

        let mut extra = HashMap::new();
        extra.insert("plus_di".to_string(), plus_di.clamp(0.0, 100.0));
        extra.insert("minus_di".to_string(), minus_di.clamp(0.0, 100.0));
        extra.insert("di_diff".to_string(), plus_di - minus_di);

        if plus_di > minus_di + 10.0 {
            self.alerts.push(IndicatorAlert::new(
                "dmi_bullish_dominance",
                format!("+DI dominates -DI ({:.1} vs {:.1})", plus_di, minus_di),
                0.80,
            ));
        } else if minus_di > plus_di + 10.0 {
            self.alerts.push(IndicatorAlert::new(
                "dmi_bearish_dominance",
                format!("-DI dominates +DI ({:.1} vs {:.1})", minus_di, plus_di),
                0.80,
            ));
        }

        Some(IndicatorOutput::with_extra(plus_di - minus_di, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Aroon Indicator (Aroon Up, Aroon Down, Oscillator).
pub struct AroonEngine {
    period: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl AroonEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for AroonEngine {
    fn name(&self) -> &str {
        "aroon"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period + 1 {
            self.bars.pop_front();
        }

        self.alerts.clear();
        if self.bars.len() < self.period + 1 {
            return None;
        }

        let mut high_idx = 0usize;
        let mut max_high = f64::MIN;
        let mut low_idx = 0usize;
        let mut min_low = f64::MAX;

        for (i, b) in self.bars.iter().enumerate() {
            if b.high >= max_high {
                max_high = b.high;
                high_idx = i;
            }
            if b.low <= min_low {
                min_low = b.low;
                low_idx = i;
            }
        }

        let bars_since_high = self.period - high_idx;
        let bars_since_low = self.period - low_idx;

        let aroon_up = ((self.period - bars_since_high) as f64 / self.period as f64) * 100.0;
        let aroon_down = ((self.period - bars_since_low) as f64 / self.period as f64) * 100.0;
        let oscillator = aroon_up - aroon_down;

        let mut extra = HashMap::new();
        extra.insert("aroon_up".to_string(), aroon_up.clamp(0.0, 100.0));
        extra.insert("aroon_down".to_string(), aroon_down.clamp(0.0, 100.0));
        extra.insert("oscillator".to_string(), oscillator.clamp(-100.0, 100.0));

        Some(IndicatorOutput::with_extra(oscillator, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Parabolic SAR Indicator.
pub struct ParabolicSarEngine {
    step: f64,
    max_step: f64,
    is_long: bool,
    sar: f64,
    ep: f64,
    af: f64,
    prev_bar: Option<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl ParabolicSarEngine {
    pub fn new(step: f64, max_step: f64) -> Self {
        Self {
            step,
            max_step,
            is_long: true,
            sar: 0.0,
            ep: 0.0,
            af: step,
            prev_bar: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for ParabolicSarEngine {
    fn name(&self) -> &str {
        "parabolic_sar"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.is_long = true;
        self.sar = 0.0;
        self.ep = 0.0;
        self.af = self.step;
        self.prev_bar = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if self.prev_bar.is_none() {
            self.prev_bar = Some(bar.clone());
            self.sar = bar.low;
            self.ep = bar.high;
            return Some(IndicatorOutput::new(self.sar));
        }

        let prev = self.prev_bar.as_ref().unwrap();
        let mut next_sar = self.sar + self.af * (self.ep - self.sar);

        self.alerts.clear();
        if self.is_long {
            if bar.low < next_sar {
                self.is_long = false;
                next_sar = self.ep;
                self.ep = bar.low;
                self.af = self.step;
                self.alerts.push(IndicatorAlert::new(
                    "psar_reversal_bearish",
                    format!("PSAR Bearish Reversal (${:.2})", next_sar),
                    0.85,
                ));
            } else {
                if bar.high > self.ep {
                    self.ep = bar.high;
                    self.af = (self.af + self.step).min(self.max_step);
                }
                next_sar = next_sar.min(prev.low).min(bar.low);
            }
        } else {
            if bar.high > next_sar {
                self.is_long = true;
                next_sar = self.ep;
                self.ep = bar.high;
                self.af = self.step;
                self.alerts.push(IndicatorAlert::new(
                    "psar_reversal_bullish",
                    format!("PSAR Bullish Reversal (${:.2})", next_sar),
                    0.85,
                ));
            } else {
                if bar.low < self.ep {
                    self.ep = bar.low;
                    self.af = (self.af + self.step).min(self.max_step);
                }
                next_sar = next_sar.max(prev.high).max(bar.high);
            }
        }

        self.sar = next_sar;
        self.prev_bar = Some(bar.clone());

        let mut extra = HashMap::new();
        extra.insert("is_long".to_string(), if self.is_long { 1.0 } else { 0.0 });
        extra.insert("af".to_string(), self.af);

        Some(IndicatorOutput::with_extra(self.sar, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Supertrend Indicator (ATR-based trailing stop).
pub struct SupertrendEngine {
    period: usize,
    multiplier: f64,
    trs: VecDeque<f64>,
    prev_close: Option<f64>,
    trend: i32, // 1 = Bullish, -1 = Bearish
    upper_band: f64,
    lower_band: f64,
    supertrend: f64,
    alerts: Vec<IndicatorAlert>,
}

impl SupertrendEngine {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period,
            multiplier,
            trs: VecDeque::new(),
            prev_close: None,
            trend: 1,
            upper_band: 0.0,
            lower_band: 0.0,
            supertrend: 0.0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for SupertrendEngine {
    fn name(&self) -> &str {
        "supertrend"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.trs.clear();
        self.prev_close = None;
        self.trend = 1;
        self.upper_band = 0.0;
        self.lower_band = 0.0;
        self.supertrend = 0.0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let tr = if let Some(prev) = self.prev_close {
            (bar.high - bar.low)
                .max((bar.high - prev).abs())
                .max((bar.low - prev).abs())
        } else {
            bar.high - bar.low
        };

        self.trs.push_back(tr);
        if self.trs.len() > self.period {
            self.trs.pop_front();
        }

        self.alerts.clear();
        if self.trs.len() < self.period {
            self.prev_close = Some(bar.close);
            return None;
        }

        let atr = self.trs.iter().sum::<f64>() / self.period as f64;
        let hl2 = (bar.high + bar.low) / 2.0;

        let basic_upper = hl2 + self.multiplier * atr;
        let basic_lower = hl2 - self.multiplier * atr;

        let prev_close = self.prev_close.unwrap_or(bar.close);

        let final_upper = if basic_upper < self.upper_band || prev_close > self.upper_band {
            basic_upper
        } else {
            self.upper_band
        };

        let final_lower = if basic_lower > self.lower_band || prev_close < self.lower_band {
            basic_lower
        } else {
            self.lower_band
        };

        let prev_trend = self.trend;
        if self.trend == 1 && bar.close < final_lower {
            self.trend = -1;
        } else if self.trend == -1 && bar.close > final_upper {
            self.trend = 1;
        }

        self.upper_band = final_upper;
        self.lower_band = final_lower;
        self.supertrend = if self.trend == 1 {
            final_lower
        } else {
            final_upper
        };
        self.prev_close = Some(bar.close);

        if self.trend != prev_trend {
            if self.trend == 1 {
                self.alerts.push(IndicatorAlert::new(
                    "supertrend_bullish",
                    format!("Supertrend Bullish Flip (${:.2})", self.supertrend),
                    0.90,
                ));
            } else {
                self.alerts.push(IndicatorAlert::new(
                    "supertrend_bearish",
                    format!("Supertrend Bearish Flip (${:.2})", self.supertrend),
                    0.90,
                ));
            }
        }

        let mut extra = HashMap::new();
        extra.insert("trend".to_string(), self.trend as f64);
        extra.insert("upper".to_string(), final_upper);
        extra.insert("lower".to_string(), final_lower);

        Some(IndicatorOutput::with_extra(self.supertrend, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Ichimoku Kinko Hyo Cloud Indicator (Tenkan-sen, Kijun-sen, Senkou A, Senkou B, Chikou).
pub struct IchimokuEngine {
    tenkan_p: usize,
    kijun_p: usize,
    senkou_b_p: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl IchimokuEngine {
    pub fn new(tenkan_p: usize, kijun_p: usize, senkou_b_p: usize) -> Self {
        Self {
            tenkan_p,
            kijun_p,
            senkou_b_p,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for IchimokuEngine {
    fn name(&self) -> &str {
        "ichimoku"
    }

    fn warmup_period(&self) -> usize {
        self.senkou_b_p
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.senkou_b_p {
            self.bars.pop_front();
        }

        self.alerts.clear();
        if self.bars.len() < self.senkou_b_p {
            return None;
        }

        let calc_midpoint = |slice: &[Bar]| -> f64 {
            let h = slice.iter().map(|b| b.high).fold(f64::MIN, f64::max);
            let l = slice.iter().map(|b| b.low).fold(f64::MAX, f64::min);
            (h + l) / 2.0
        };

        self.bars.make_contiguous();
        let slice = self.bars.as_slices().0;
        let len = slice.len();
        let tenkan = calc_midpoint(&slice[len - self.tenkan_p..]);
        let kijun = calc_midpoint(&slice[len - self.kijun_p..]);
        let senkou_a = (tenkan + kijun) / 2.0;
        let senkou_b = calc_midpoint(slice);

        let mut extra = HashMap::new();
        extra.insert("tenkan".to_string(), tenkan);
        extra.insert("kijun".to_string(), kijun);
        extra.insert("senkou_a".to_string(), senkou_a);
        extra.insert("senkou_b".to_string(), senkou_b);

        if bar.close > senkou_a && bar.close > senkou_b {
            self.alerts.push(IndicatorAlert::new(
                "ichimoku_above_cloud",
                format!(
                    "Price Above Ichimoku Cloud (SpanA ${:.2}, SpanB ${:.2})",
                    senkou_a, senkou_b
                ),
                0.85,
            ));
        } else if bar.close < senkou_a && bar.close < senkou_b {
            self.alerts.push(IndicatorAlert::new(
                "ichimoku_below_cloud",
                format!(
                    "Price Below Ichimoku Cloud (SpanA ${:.2}, SpanB ${:.2})",
                    senkou_a, senkou_b
                ),
                0.85,
            ));
        }

        Some(IndicatorOutput::with_extra(tenkan - kijun, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
