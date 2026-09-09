use std::collections::{HashMap, VecDeque};

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// True Range Indicator (raw price units).
pub struct TrueRangeEngine {
    prev_close: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl TrueRangeEngine {
    pub fn new() -> Self {
        Self {
            prev_close: None,
            alerts: Vec::new(),
        }
    }
}

impl Default for TrueRangeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for TrueRangeEngine {
    fn name(&self) -> &str {
        "true_range"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.prev_close = None;
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
        self.prev_close = Some(bar.close);

        Some(IndicatorOutput::new(tr))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Keltner Channel Indicator (EMA Basis +/- Multiplier * ATR).
#[derive(Debug, Clone)]
pub struct KeltnerChannelEngine {
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
    closes: VecDeque<f64>,
    trs: VecDeque<f64>,
    prev_close: Option<f64>,
    current_ema: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl KeltnerChannelEngine {
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period,
            atr_period,
            multiplier,
            closes: VecDeque::new(),
            trs: VecDeque::new(),
            prev_close: None,
            current_ema: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for KeltnerChannelEngine {
    fn name(&self) -> &str {
        "keltner"
    }

    fn warmup_period(&self) -> usize {
        self.ema_period.max(self.atr_period)
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.trs.clear();
        self.prev_close = None;
        self.current_ema = None;
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
        self.prev_close = Some(bar.close);

        self.closes.push_back(bar.close);
        self.trs.push_back(tr);

        let k = 2.0 / (self.ema_period as f64 + 1.0);
        self.current_ema = match self.current_ema {
            Some(prev_ema) => Some(bar.close * k + prev_ema * (1.0 - k)),
            None => Some(bar.close),
        };

        if self.closes.len() > self.ema_period {
            self.closes.pop_front();
        }
        if self.trs.len() > self.atr_period {
            self.trs.pop_front();
        }

        self.alerts.clear();
        if self.trs.len() < self.atr_period {
            return None;
        }

        let basis = self.current_ema.unwrap_or(bar.close);
        let atr: f64 = self.trs.iter().sum::<f64>() / self.atr_period as f64;
        let upper = basis + self.multiplier * atr;
        let lower = basis - self.multiplier * atr;

        let mut extra = HashMap::new();
        extra.insert("upper".to_string(), upper);
        extra.insert("lower".to_string(), lower);
        extra.insert("atr".to_string(), atr);

        if bar.close > upper {
            self.alerts.push(IndicatorAlert::new(
                "keltner_upper_breakout",
                format!("Price Above Upper Keltner Channel (${:.2})", upper),
                0.80,
            ));
        } else if bar.close < lower {
            self.alerts.push(IndicatorAlert::new(
                "keltner_lower_breakout",
                format!("Price Below Lower Keltner Channel (${:.2})", lower),
                0.80,
            ));
        }

        Some(IndicatorOutput::with_extra(basis, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Donchian Channel Indicator (Highest High / Lowest Low over lookback).
pub struct DonchianChannelEngine {
    period: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl DonchianChannelEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for DonchianChannelEngine {
    fn name(&self) -> &str {
        "donchian"
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

        let upper = self.bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let lower = self.bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let basis = (upper + lower) / 2.0;

        let mut extra = HashMap::new();
        extra.insert("upper".to_string(), upper);
        extra.insert("lower".to_string(), lower);
        extra.insert("width".to_string(), upper - lower);

        if (bar.high - upper).abs() < 1e-8 {
            self.alerts.push(IndicatorAlert::new(
                "donchian_new_high",
                format!("{}-Period Donchian High: ${:.2}", self.period, upper),
                0.85,
            ));
        } else if (bar.low - lower).abs() < 1e-8 {
            self.alerts.push(IndicatorAlert::new(
                "donchian_new_low",
                format!("{}-Period Donchian Low: ${:.2}", self.period, lower),
                0.85,
            ));
        }

        Some(IndicatorOutput::with_extra(basis, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Historical / Realized Volatility (Annualized Standard Deviation of Log Returns).
pub struct HistoricalVolatilityEngine {
    period: usize,
    closes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl HistoricalVolatilityEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            closes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for HistoricalVolatilityEngine {
    fn name(&self) -> &str {
        "historical_volatility"
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

        let mut log_returns = Vec::with_capacity(self.period);
        for pair in self.closes.iter().collect::<Vec<_>>().windows(2) {
            let prev = *pair[0];
            let curr = *pair[1];
            if prev > 0.0 && curr > 0.0 {
                log_returns.push((curr / prev).ln());
            } else {
                log_returns.push(0.0);
            }
        }

        let mean = log_returns.iter().sum::<f64>() / log_returns.len() as f64;
        let variance = log_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
            / (log_returns.len() as f64 - 1.0).max(1.0);
        let daily_std_dev = variance.sqrt();
        let annualized_hv = daily_std_dev * (252.0f64).sqrt() * 100.0; // in %

        Some(IndicatorOutput::new(annualized_hv))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Garman-Klass Volatility Estimator (OHLC Volatility).
pub struct GarmanKlassVolatilityEngine {
    period: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl GarmanKlassVolatilityEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for GarmanKlassVolatilityEngine {
    fn name(&self) -> &str {
        "garman_klass"
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

        let mut sum_var = 0.0f64;
        for b in &self.bars {
            if b.open > 0.0 && b.close > 0.0 && b.high > 0.0 && b.low > 0.0 {
                let log_hl = (b.high / b.low).ln();
                let log_co = (b.close / b.open).ln();
                let bar_var = 0.5 * log_hl.powi(2) - (2.0 * (2.0f64).ln() - 1.0) * log_co.powi(2);
                sum_var += bar_var.max(0.0);
            }
        }

        let avg_var = sum_var / self.period as f64;
        let annualized_gk = avg_var.sqrt() * (252.0f64).sqrt() * 100.0;

        Some(IndicatorOutput::new(annualized_gk))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
