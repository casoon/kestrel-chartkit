use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Smart Money Fair Value Gap (FVG) & Liquidity Sweep Engine.
/// Detects institutional price imbalances and liquidity pool sweeps.
pub struct LiquidityFvgEngine {
    lookback: usize,
    window: Vec<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl LiquidityFvgEngine {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            window: Vec::with_capacity(lookback + 5),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for LiquidityFvgEngine {
    fn name(&self) -> &str {
        "liquidity_fvg"
    }

    fn warmup_period(&self) -> usize {
        self.lookback.max(3)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.window.push(bar.clone());
        if self.window.len() > self.lookback + 5 {
            self.window.remove(0);
        }

        self.alerts.clear();

        if self.window.len() < 3 {
            return Some(IndicatorOutput::new(0.0));
        }

        let n = self.window.len();
        let curr = &self.window[n - 1];
        let prev2 = &self.window[n - 3];

        let mut fvg_type = 0.0f64;
        let mut gap_size = 0.0f64;

        // 1. Bullish Fair Value Gap (Low[t] > High[t-2])
        if curr.low > prev2.high {
            fvg_type = 1.0;
            gap_size = curr.low - prev2.high;
            self.alerts.push(IndicatorAlert::new(
                "bullish_fvg",
                format!(
                    "Bullish Fair Value Gap (FVG Zone ${:.2} - ${:.2})",
                    prev2.high, curr.low
                ),
                0.85,
            ));
        }
        // 2. Bearish Fair Value Gap (High[t] < Low[t-2])
        else if curr.high < prev2.low {
            fvg_type = -1.0;
            gap_size = prev2.low - curr.high;
            self.alerts.push(IndicatorAlert::new(
                "bearish_fvg",
                format!(
                    "Bearish Fair Value Gap (FVG Zone ${:.2} - ${:.2})",
                    curr.high, prev2.low
                ),
                0.85,
            ));
        }

        // 3. Liquidity Sweep Detection over lookback window
        if n > self.lookback {
            let prev_bars = &self.window[n - 1 - self.lookback..n - 1];
            let recent_highest = prev_bars
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let recent_lowest = prev_bars
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);

            // Bullish Liquidity Sweep (Low pierced recent lowest, but Close > recent lowest)
            if curr.low < recent_lowest && curr.close > recent_lowest {
                self.alerts.push(IndicatorAlert::new(
                    "bullish_liquidity_sweep",
                    format!(
                        "Bullish Liquidity Sweep (Pierced ${:.2} Support, Reclaimed Close)",
                        recent_lowest
                    ),
                    0.95,
                ));
            }
            // Bearish Liquidity Sweep (High pierced recent highest, but Close < recent highest)
            else if curr.high > recent_highest && curr.close < recent_highest {
                self.alerts.push(IndicatorAlert::new(
                    "bearish_liquidity_sweep",
                    format!(
                        "Bearish Liquidity Sweep (Pierced ${:.2} Resistance, Reclaimed Close)",
                        recent_highest
                    ),
                    0.95,
                ));
            }
        }

        let mut extra = HashMap::new();
        extra.insert("fvg_type".to_string(), fvg_type);
        extra.insert("gap_size".to_string(), gap_size);

        Some(IndicatorOutput::with_extra(fvg_type, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_liquidity_fvg(params: &HashMap<String, f64>) -> LiquidityFvgEngine {
    let lookback = params.get("lookback").copied().unwrap_or(20.0) as usize;
    LiquidityFvgEngine::new(lookback)
}
