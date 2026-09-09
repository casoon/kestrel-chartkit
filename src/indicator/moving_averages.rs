use std::collections::VecDeque;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Simple Moving Average (SMA).
pub struct SmaEngine {
    period: usize,
    closes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl SmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            closes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for SmaEngine {
    fn name(&self) -> &str {
        "sma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.closes.push_back(bar.close);
        if self.closes.len() > self.period {
            self.closes.pop_front();
        }

        self.alerts.clear();
        if self.closes.len() < self.period {
            return None;
        }

        let sma = self.closes.iter().sum::<f64>() / self.period as f64;
        Some(IndicatorOutput::new(sma))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Exponential Moving Average (EMA).
#[derive(Debug, Clone)]
pub struct EmaEngine {
    period: usize,
    current_ema: Option<f64>,
    count: usize,
    alerts: Vec<IndicatorAlert>,
}

impl EmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            current_ema: None,
            count: 0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for EmaEngine {
    fn name(&self) -> &str {
        "ema"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.current_ema = None;
        self.count = 0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let k = 2.0 / (self.period as f64 + 1.0);
        let ema = match self.current_ema {
            Some(prev) => bar.close * k + prev * (1.0 - k),
            None => bar.close,
        };
        self.current_ema = Some(ema);

        self.alerts.clear();
        if self.count < self.period {
            return None;
        }

        Some(IndicatorOutput::new(ema))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Weighted Moving Average (WMA).
pub struct WmaEngine {
    period: usize,
    closes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl WmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            closes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for WmaEngine {
    fn name(&self) -> &str {
        "wma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.closes.push_back(bar.close);
        if self.closes.len() > self.period {
            self.closes.pop_front();
        }

        self.alerts.clear();
        if self.closes.len() < self.period {
            return None;
        }

        let mut weight_sum = 0.0f64;
        let mut weighted_val = 0.0f64;
        for (i, &val) in self.closes.iter().enumerate() {
            let w = (i + 1) as f64;
            weighted_val += val * w;
            weight_sum += w;
        }

        let wma = if weight_sum > 0.0 {
            weighted_val / weight_sum
        } else {
            bar.close
        };
        Some(IndicatorOutput::new(wma))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Volume-Weighted Moving Average (VWMA).
pub struct VwmaEngine {
    period: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl VwmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for VwmaEngine {
    fn name(&self) -> &str {
        "vwma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period {
            self.bars.pop_front();
        }

        self.alerts.clear();
        if self.bars.len() < self.period {
            return None;
        }

        let mut pv_sum = 0.0f64;
        let mut v_sum = 0.0f64;
        for b in &self.bars {
            pv_sum += b.close * b.volume;
            v_sum += b.volume;
        }

        let vwma = if v_sum > 0.0 {
            pv_sum / v_sum
        } else {
            bar.close
        };
        Some(IndicatorOutput::new(vwma))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Hull Moving Average (HMA).
pub struct HmaEngine {
    period: usize,
    wma_half: WmaEngine,
    wma_full: WmaEngine,
    wma_sqrt: WmaEngine,
    alerts: Vec<IndicatorAlert>,
}

impl HmaEngine {
    pub fn new(period: usize) -> Self {
        let half = (period / 2).max(1);
        let sqrt = ((period as f64).sqrt().round() as usize).max(1);
        Self {
            period,
            wma_half: WmaEngine::new(half),
            wma_full: WmaEngine::new(period),
            wma_sqrt: WmaEngine::new(sqrt),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for HmaEngine {
    fn name(&self) -> &str {
        "hma"
    }

    fn warmup_period(&self) -> usize {
        self.period + ((self.period as f64).sqrt().round() as usize)
    }

    fn reset(&mut self) {
        self.wma_half.reset();
        self.wma_full.reset();
        self.wma_sqrt.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let h_out = self.wma_half.on_bar(bar);
        let f_out = self.wma_full.on_bar(bar);

        self.alerts.clear();
        if let (Some(h), Some(f)) = (h_out, f_out) {
            let diff = 2.0 * h.value - f.value;
            let synthetic_bar = Bar::new(bar.timestamp, diff, diff, diff, diff, 1.0);
            return self.wma_sqrt.on_bar(&synthetic_bar);
        }

        None
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Double Exponential Moving Average (DEMA).
pub struct DemaEngine {
    period: usize,
    ema1: EmaEngine,
    ema2: EmaEngine,
    alerts: Vec<IndicatorAlert>,
}

impl DemaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            ema1: EmaEngine::new(period),
            ema2: EmaEngine::new(period),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for DemaEngine {
    fn name(&self) -> &str {
        "dema"
    }

    fn warmup_period(&self) -> usize {
        self.period * 2
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let e1_out = self.ema1.on_bar(bar);
        self.alerts.clear();

        if let Some(e1) = e1_out {
            let synth_bar = Bar::new(bar.timestamp, e1.value, e1.value, e1.value, e1.value, 1.0);
            let e2_out = self.ema2.on_bar(&synth_bar);
            if let Some(e2) = e2_out {
                let dema = 2.0 * e1.value - e2.value;
                return Some(IndicatorOutput::new(dema));
            }
        }

        None
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Kaufman's Adaptive Moving Average (KAMA).
pub struct KamaEngine {
    period: usize,
    fast_period: usize,
    slow_period: usize,
    closes: VecDeque<f64>,
    current_kama: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl KamaEngine {
    pub fn new(period: usize, fast_period: usize, slow_period: usize) -> Self {
        Self {
            period,
            fast_period,
            slow_period,
            closes: VecDeque::new(),
            current_kama: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for KamaEngine {
    fn name(&self) -> &str {
        "kama"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.current_kama = None;
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

        let change = (self.closes.back().unwrap() - self.closes.front().unwrap()).abs();
        let mut volatility = 0.0f64;
        for pair in self.closes.iter().collect::<Vec<_>>().windows(2) {
            volatility += (*pair[1] - *pair[0]).abs();
        }

        let er = if volatility > 0.0 {
            change / volatility
        } else {
            0.0
        };

        let fast_sc = 2.0 / (self.fast_period as f64 + 1.0);
        let slow_sc = 2.0 / (self.slow_period as f64 + 1.0);
        let sc = (er * (fast_sc - slow_sc) + slow_sc).powi(2);

        let kama = match self.current_kama {
            Some(prev) => prev + sc * (bar.close - prev),
            None => bar.close,
        };
        self.current_kama = Some(kama);

        Some(IndicatorOutput::new(kama))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
