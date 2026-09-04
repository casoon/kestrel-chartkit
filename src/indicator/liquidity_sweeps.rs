use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

/// Liquidity Sweep and Equal High/Low (EQH/EQL) Detector Engine.
#[derive(Debug, Clone)]
pub struct LiquiditySweepEngine {
    pivot_len: usize,
    tolerance_pct: f64,
    bars: VecDeque<Bar>,
    pivot_highs: Vec<f64>,
    pivot_lows: Vec<f64>,
    sweep_detected: i8, // 1 = Bullish sweep (sweep low & reclaim), -1 = Bearish sweep (sweep high & reclaim)
}

impl LiquiditySweepEngine {
    pub fn new(pivot_len: usize, tolerance_pct: f64) -> Self {
        Self {
            pivot_len: pivot_len.max(2),
            tolerance_pct: tolerance_pct.max(0.01),
            bars: VecDeque::with_capacity(pivot_len * 2 + 1),
            pivot_highs: Vec::new(),
            pivot_lows: Vec::new(),
            sweep_detected: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5, 0.2) // 5 pivot len, 0.2% tolerance for Equal Highs/Lows
    }
}

impl Indicator for LiquiditySweepEngine {
    fn name(&self) -> &str {
        "liquidity_sweeps"
    }

    fn warmup_period(&self) -> usize {
        self.pivot_len * 2 + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.pivot_highs.clear();
        self.pivot_lows.clear();
        self.sweep_detected = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.pivot_len * 2 + 1 {
            self.bars.pop_front();
        }

        if self.bars.len() < self.pivot_len * 2 + 1 {
            return None;
        }

        let mid_idx = self.pivot_len;
        let mid_bar = &self.bars[mid_idx];

        let is_pivot_high = self
            .bars
            .iter()
            .enumerate()
            .all(|(i, b)| i == mid_idx || b.high <= mid_bar.high);
        let is_pivot_low = self
            .bars
            .iter()
            .enumerate()
            .all(|(i, b)| i == mid_idx || b.low >= mid_bar.low);

        if is_pivot_high {
            self.pivot_highs.push(mid_bar.high);
            if self.pivot_highs.len() > 20 {
                self.pivot_highs.remove(0);
            }
        }
        if is_pivot_low {
            self.pivot_lows.push(mid_bar.low);
            if self.pivot_lows.len() > 20 {
                self.pivot_lows.remove(0);
            }
        }

        self.sweep_detected = 0;

        // Check for Bearish Liquidity Sweep (High pierced but Close reclaimed below high)
        for &ph in &self.pivot_highs {
            if bar.high > ph && bar.close < ph {
                self.sweep_detected = -1;
                break;
            }
        }

        // Check for Bullish Liquidity Sweep (Low pierced but Close reclaimed above low)
        if self.sweep_detected == 0 {
            for &pl in &self.pivot_lows {
                if bar.low < pl && bar.close > pl {
                    self.sweep_detected = 1;
                    break;
                }
            }
        }

        // Check for Equal Highs / Equal Lows cluster count
        let eqh_count = self
            .pivot_highs
            .windows(2)
            .filter(|w| (w[0] - w[1]).abs() / w[0] * 100.0 <= self.tolerance_pct)
            .count();
        let eql_count = self
            .pivot_lows
            .windows(2)
            .filter(|w| (w[0] - w[1]).abs() / w[0] * 100.0 <= self.tolerance_pct)
            .count();

        let mut extra = HashMap::new();
        extra.insert("sweep".to_string(), self.sweep_detected as f64);
        extra.insert("eqh_count".to_string(), eqh_count as f64);
        extra.insert("eql_count".to_string(), eql_count as f64);

        Some(IndicatorOutput::with_extra(
            self.sweep_detected as f64,
            extra,
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut alerts = Vec::new();
        if self.sweep_detected == 1 {
            alerts.push(IndicatorAlert::new(
                "sweep",
                "Bullish Liquidity Sweep & Reclaim",
                0.85,
            ));
        } else if self.sweep_detected == -1 {
            alerts.push(IndicatorAlert::new(
                "sweep",
                "Bearish Liquidity Sweep & Reclaim",
                0.85,
            ));
        }
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liquidity_sweep_detection() {
        let mut sweep = LiquiditySweepEngine::with_defaults();
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0, 1000.0);
            sweep.on_bar(&b);
        }
        // Sweep bar: low dips to 90.0 (piercing 95.0), close reclaims at 98.0
        let sweep_bar = Bar::new(20, 99.0, 101.0, 90.0, 98.0, 1000.0);
        let out = sweep.on_bar(&sweep_bar);
        assert!(out.is_some());
    }
}
