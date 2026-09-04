use std::collections::{HashMap, VecDeque};

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Classic Stochastic Oscillator (%K and %D).
pub struct StochasticEngine {
    k_period: usize,
    d_period: usize,
    bars: VecDeque<Bar>,
    raw_ks: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl StochasticEngine {
    pub fn new(k_period: usize, d_period: usize) -> Self {
        Self {
            k_period,
            d_period,
            bars: VecDeque::new(),
            raw_ks: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for StochasticEngine {
    fn name(&self) -> &str {
        "stochastic"
    }

    fn warmup_period(&self) -> usize {
        self.k_period + self.d_period
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.raw_ks.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.k_period {
            self.bars.pop_front();
        }

        if self.bars.len() < self.k_period {
            return None;
        }

        let highest_high = self.bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let lowest_low = self.bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);

        let k = if (highest_high - lowest_low).abs() > 1e-8 {
            ((bar.close - lowest_low) / (highest_high - lowest_low)) * 100.0
        } else {
            50.0
        }
        .clamp(0.0, 100.0);

        self.raw_ks.push_back(k);
        if self.raw_ks.len() > self.d_period {
            self.raw_ks.pop_front();
        }

        self.alerts.clear();
        if self.raw_ks.len() < self.d_period {
            return None;
        }

        let d = (self.raw_ks.iter().sum::<f64>() / self.d_period as f64).clamp(0.0, 100.0);

        let mut extra = HashMap::new();
        extra.insert("percent_k".to_string(), k);
        extra.insert("percent_d".to_string(), d);

        if k <= 20.0 && d <= 20.0 {
            self.alerts.push(IndicatorAlert::new(
                "stoch_oversold",
                format!("Stochastic Oversold (%K: {:.1}, %D: {:.1})", k, d),
                0.80,
            ));
        } else if k >= 80.0 && d >= 80.0 {
            self.alerts.push(IndicatorAlert::new(
                "stoch_overbought",
                format!("Stochastic Overbought (%K: {:.1}, %D: {:.1})", k, d),
                0.80,
            ));
        }

        Some(IndicatorOutput::with_extra(k, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Rate of Change (ROC) / Momentum.
pub struct RocEngine {
    period: usize,
    closes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl RocEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            closes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for RocEngine {
    fn name(&self) -> &str {
        "roc"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.closes.push_back(bar.close);
        if self.closes.len() > self.period + 1 {
            self.closes.pop_front();
        }

        self.alerts.clear();
        if self.closes.len() < self.period + 1 {
            return None;
        }

        let past_close = *self.closes.front().unwrap();
        let roc = if past_close > 0.0 {
            ((bar.close - past_close) / past_close) * 100.0
        } else {
            0.0
        };

        let mut extra = HashMap::new();
        extra.insert("abs_momentum".to_string(), bar.close - past_close);

        Some(IndicatorOutput::with_extra(roc, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Ultimate Oscillator (UO: multi-timeframe momentum).
pub struct UltimateOscillatorEngine {
    period1: usize,
    period2: usize,
    period3: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl UltimateOscillatorEngine {
    pub fn new(period1: usize, period2: usize, period3: usize) -> Self {
        Self {
            period1,
            period2,
            period3,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for UltimateOscillatorEngine {
    fn name(&self) -> &str {
        "ultimate_oscillator"
    }

    fn warmup_period(&self) -> usize {
        self.period3 + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period3 + 1 {
            self.bars.pop_front();
        }

        self.alerts.clear();
        if self.bars.len() < self.period3 + 1 {
            return None;
        }

        self.bars.make_contiguous();
        let calc_bp_tr_sums = |p: usize| -> (f64, f64) {
            let slice = &self.bars.as_slices().0;
            let len = slice.len();
            let window_slice = &slice[len - p - 1..];
            let mut sum_bp = 0.0f64;
            let mut sum_tr = 0.0f64;
            for pair in window_slice.windows(2) {
                let prev_c = pair[0].close;
                let b = &pair[1];
                let min_l_pc = b.low.min(prev_c);
                let max_h_pc = b.high.max(prev_c);
                let bp = b.close - min_l_pc;
                let tr = max_h_pc - min_l_pc;
                sum_bp += bp;
                sum_tr += tr;
            }
            (sum_bp, sum_tr)
        };

        let (bp1, tr1) = calc_bp_tr_sums(self.period1);
        let (bp2, tr2) = calc_bp_tr_sums(self.period2);
        let (bp3, tr3) = calc_bp_tr_sums(self.period3);

        let r1 = if tr1 > 0.0 { bp1 / tr1 } else { 0.0 };
        let r2 = if tr2 > 0.0 { bp2 / tr2 } else { 0.0 };
        let r3 = if tr3 > 0.0 { bp3 / tr3 } else { 0.0 };

        let uo = ((4.0 * r1 + 2.0 * r2 + r3) / 7.0) * 100.0;
        Some(IndicatorOutput::new(uo.clamp(0.0, 100.0)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Awesome Oscillator (AO: SMA(HL/2, 5) - SMA(HL/2, 34)).
pub struct AwesomeOscillatorEngine {
    fast_period: usize,
    slow_period: usize,
    hl2_series: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl AwesomeOscillatorEngine {
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            hl2_series: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for AwesomeOscillatorEngine {
    fn name(&self) -> &str {
        "awesome_oscillator"
    }

    fn warmup_period(&self) -> usize {
        self.slow_period
    }

    fn reset(&mut self) {
        self.hl2_series.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let hl2 = (bar.high + bar.low) / 2.0;
        self.hl2_series.push_back(hl2);
        if self.hl2_series.len() > self.slow_period {
            self.hl2_series.pop_front();
        }

        self.alerts.clear();
        if self.hl2_series.len() < self.slow_period {
            return None;
        }

        let len = self.hl2_series.len();
        let fast_sma: f64 = self
            .hl2_series
            .iter()
            .skip(len - self.fast_period)
            .sum::<f64>()
            / self.fast_period as f64;
        let slow_sma: f64 = self.hl2_series.iter().sum::<f64>() / self.slow_period as f64;

        let ao = fast_sma - slow_sma;
        Some(IndicatorOutput::new(ao))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Percentage Price Oscillator (PPO: (EMA(fast) - EMA(slow)) / EMA(slow) * 100).
pub struct PpoEngine {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    fast_ema: Option<f64>,
    slow_ema: Option<f64>,
    signal_ema: Option<f64>,
    count: usize,
    alerts: Vec<IndicatorAlert>,
}

impl PpoEngine {
    pub fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            signal_period,
            fast_ema: None,
            slow_ema: None,
            signal_ema: None,
            count: 0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for PpoEngine {
    fn name(&self) -> &str {
        "ppo"
    }

    fn warmup_period(&self) -> usize {
        self.slow_period + self.signal_period
    }

    fn reset(&mut self) {
        self.fast_ema = None;
        self.slow_ema = None;
        self.signal_ema = None;
        self.count = 0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let k_fast = 2.0 / (self.fast_period as f64 + 1.0);
        let k_slow = 2.0 / (self.slow_period as f64 + 1.0);
        let k_sig = 2.0 / (self.signal_period as f64 + 1.0);

        self.fast_ema = Some(match self.fast_ema {
            Some(prev) => bar.close * k_fast + prev * (1.0 - k_fast),
            None => bar.close,
        });

        self.slow_ema = Some(match self.slow_ema {
            Some(prev) => bar.close * k_slow + prev * (1.0 - k_slow),
            None => bar.close,
        });

        self.alerts.clear();
        if self.count < self.slow_period {
            return None;
        }

        let fast = self.fast_ema.unwrap();
        let slow = self.slow_ema.unwrap();
        let ppo_line = if slow > 0.0 {
            ((fast - slow) / slow) * 100.0
        } else {
            0.0
        };

        let sig = match self.signal_ema {
            Some(prev) => ppo_line * k_sig + prev * (1.0 - k_sig),
            None => ppo_line,
        };
        self.signal_ema = Some(sig);

        let hist = ppo_line - sig;

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), sig);
        extra.insert("hist".to_string(), hist);

        Some(IndicatorOutput::with_extra(ppo_line, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Chande Momentum Oscillator (CMO).
pub struct CmoEngine {
    period: usize,
    prev_close: Option<f64>,
    gains: VecDeque<f64>,
    losses: VecDeque<f64>,
}

impl CmoEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            prev_close: None,
            gains: VecDeque::with_capacity(period),
            losses: VecDeque::with_capacity(period),
        }
    }
}

impl Indicator for CmoEngine {
    fn name(&self) -> &str {
        "cmo"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.gains.clear();
        self.losses.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev = match self.prev_close {
            Some(p) => p,
            None => {
                self.prev_close = Some(bar.close);
                return None;
            }
        };

        let diff = bar.close - prev;
        self.prev_close = Some(bar.close);

        let gain = if diff > 0.0 { diff } else { 0.0 };
        let loss = if diff < 0.0 { diff.abs() } else { 0.0 };

        self.gains.push_back(gain);
        self.losses.push_back(loss);

        if self.gains.len() > self.period {
            self.gains.pop_front();
            self.losses.pop_front();
        }

        if self.gains.len() < self.period {
            return None;
        }

        let sum_gain: f64 = self.gains.iter().sum();
        let sum_loss: f64 = self.losses.iter().sum();
        let denom = sum_gain + sum_loss;

        let cmo_val = if denom > 1e-8 {
            (100.0 * (sum_gain - sum_loss) / denom).clamp(-100.0, 100.0)
        } else {
            0.0
        };

        Some(IndicatorOutput::new(cmo_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Elder Ray Index (Bull Power & Bear Power).
pub struct ElderRayEngine {
    ema_period: usize,
    ema: crate::indicator::smoothing::Ema,
    count: usize,
}

impl ElderRayEngine {
    pub fn new(ema_period: usize) -> Self {
        Self {
            ema_period: ema_period.max(1),
            ema: crate::indicator::smoothing::Ema::new(ema_period),
            count: 0,
        }
    }
}

impl Indicator for ElderRayEngine {
    fn name(&self) -> &str {
        "elder_ray"
    }

    fn warmup_period(&self) -> usize {
        self.ema_period
    }

    fn reset(&mut self) {
        self.ema.reset();
        self.count = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let ema_val = self.ema.update(bar.close);

        if self.count < self.ema_period {
            return None;
        }

        let bull_power = bar.high - ema_val;
        let bear_power = bar.low - ema_val;

        let mut extra = HashMap::new();
        extra.insert("bull_power".to_string(), bull_power);
        extra.insert("bear_power".to_string(), bear_power);
        extra.insert("ema".to_string(), ema_val);

        Some(IndicatorOutput::with_extra(bull_power, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}
