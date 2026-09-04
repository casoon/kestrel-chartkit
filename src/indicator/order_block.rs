use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

#[derive(Debug, Clone)]
pub struct OrderBlockZone {
    pub is_bullish: bool,
    pub top: f64,
    pub bottom: f64,
    pub created_bar: usize,
    pub mitigated: bool,
}

/// Institutional Order Block Engine.
/// Detects demand and supply order blocks formed by strong displacement expansions.
pub struct OrderBlockEngine {
    atr_len: usize,
    min_disp_mult: f64,
    bars: Vec<Bar>,
    atr_vals: Vec<f64>,
    active_obs: Vec<OrderBlockZone>,
    bar_count: usize,
    alerts: Vec<IndicatorAlert>,
}

impl OrderBlockEngine {
    pub fn new(atr_len: usize, min_disp_mult: f64) -> Self {
        Self {
            atr_len,
            min_disp_mult,
            bars: Vec::new(),
            atr_vals: Vec::new(),
            active_obs: Vec::new(),
            bar_count: 0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for OrderBlockEngine {
    fn name(&self) -> &str {
        "order_block"
    }

    fn warmup_period(&self) -> usize {
        self.atr_len + 5
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.atr_vals.clear();
        self.active_obs.clear();
        self.bar_count = 0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bar_count += 1;
        self.bars.push(bar.clone());
        if self.bars.len() > self.atr_len + 10 {
            self.bars.remove(0);
        }

        self.alerts.clear();

        if self.bars.len() < self.atr_len + 1 {
            return None;
        }

        // Calculate ATR over last atr_len bars
        let mut tr_sum = 0.0f64;
        let start = self.bars.len() - self.atr_len;
        for i in start..self.bars.len() {
            let tr1 = self.bars[i].high - self.bars[i].low;
            let tr2 = (self.bars[i].high - self.bars[i - 1].close).abs();
            let tr3 = (self.bars[i].low - self.bars[i - 1].close).abs();
            let tr = tr1.max(tr2).max(tr3);
            tr_sum += tr;
        }
        let current_atr = tr_sum / (self.atr_len as f64);
        self.atr_vals.push(current_atr);

        let curr_idx = self.bars.len() - 1;
        let curr_bar = &self.bars[curr_idx];
        let prev_bar = &self.bars[curr_idx - 1];

        let body_size = (curr_bar.close - curr_bar.open).abs();
        let is_displacement = body_size >= (self.min_disp_mult * current_atr);

        // Check for Bullish OB: prev bar was bearish, current bar expands violently upward
        if is_displacement && curr_bar.close > curr_bar.open && prev_bar.close < prev_bar.open {
            let ob_zone = OrderBlockZone {
                is_bullish: true,
                top: prev_bar.high,
                bottom: prev_bar.low,
                created_bar: self.bar_count,
                mitigated: false,
            };
            self.alerts.push(IndicatorAlert::new(
                "bullish_order_block",
                format!(
                    "Bullish Demand Order Block Zone (${:.2} - ${:.2})",
                    ob_zone.bottom, ob_zone.top
                ),
                0.90,
            ));
            self.active_obs.push(ob_zone);
        }

        // Check for Bearish OB: prev bar was bullish, current bar expands violently downward
        if is_displacement && curr_bar.close < curr_bar.open && prev_bar.close > prev_bar.open {
            let ob_zone = OrderBlockZone {
                is_bullish: false,
                top: prev_bar.high,
                bottom: prev_bar.low,
                created_bar: self.bar_count,
                mitigated: false,
            };
            self.alerts.push(IndicatorAlert::new(
                "bearish_order_block",
                format!(
                    "Bearish Supply Order Block Zone (${:.2} - ${:.2})",
                    ob_zone.bottom, ob_zone.top
                ),
                0.90,
            ));
            self.active_obs.push(ob_zone);
        }

        // Check mitigation of existing active OBs
        let mut active_count = 0.0f64;
        for ob in self.active_obs.iter_mut() {
            if ob.mitigated {
                continue;
            }
            if ob.is_bullish {
                active_count += 1.0;
                if curr_bar.low <= ob.top {
                    if curr_bar.close < ob.bottom {
                        ob.mitigated = true;
                    } else {
                        self.alerts.push(IndicatorAlert::new(
                            "ob_retest_bullish",
                            format!("Retest of Bullish Demand Zone (${:.2})", ob.top),
                            0.80,
                        ));
                    }
                }
            } else {
                active_count -= 1.0;
                if curr_bar.high >= ob.bottom {
                    if curr_bar.close > ob.top {
                        ob.mitigated = true;
                    } else {
                        self.alerts.push(IndicatorAlert::new(
                            "ob_retest_bearish",
                            format!("Retest of Bearish Supply Zone (${:.2})", ob.bottom),
                            0.80,
                        ));
                    }
                }
            }
        }

        let mut extra = HashMap::new();
        extra.insert("active_count".to_string(), active_count);
        if let Some(last_active) = self.active_obs.iter().rev().find(|ob| !ob.mitigated) {
            extra.insert("active_ob_top".to_string(), last_active.top);
            extra.insert("active_ob_bottom".to_string(), last_active.bottom);
            let duration = self.bar_count.saturating_sub(last_active.created_bar) as f64;
            extra.insert("active_ob_duration".to_string(), duration);
        }

        Some(IndicatorOutput::with_extra(active_count, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_order_block(params: &HashMap<String, f64>) -> OrderBlockEngine {
    let atr_len = params.get("atr_len").copied().unwrap_or(14.0) as usize;
    let min_disp_mult = params.get("min_disp").copied().unwrap_or(1.0);
    OrderBlockEngine::new(atr_len, min_disp_mult)
}
