use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Leg Efficiency Engine (Kaufman Efficiency Ratio & Noise Filter).
/// Measures structural trend cleanliness vs. random market chop.
#[derive(Debug, Clone)]
pub struct LegEfficiencyEngine {
    len: usize,
    closes: Vec<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl LegEfficiencyEngine {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            closes: Vec::with_capacity(len + 1),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for LegEfficiencyEngine {
    fn name(&self) -> &str {
        "efficiency"
    }

    fn warmup_period(&self) -> usize {
        self.len
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.closes.push(bar.close);
        if self.closes.len() > self.len + 1 {
            self.closes.remove(0);
        }

        self.alerts.clear();

        if self.closes.len() < self.len + 1 {
            return None;
        }

        let change = (self.closes[self.closes.len() - 1] - self.closes[0]).abs();
        let mut volatility = 0.0f64;
        for i in 1..self.closes.len() {
            volatility += (self.closes[i] - self.closes[i - 1]).abs();
        }

        let er = if volatility > 0.0 {
            change / volatility
        } else {
            0.0
        };

        if er >= 0.65 {
            self.alerts.push(IndicatorAlert::new(
                "high_leg_efficiency",
                format!(
                    "High Trend Efficiency Ratio ({:.0}% Clean Move)",
                    er * 100.0
                ),
                0.85,
            ));
        } else if er <= 0.20 {
            self.alerts.push(IndicatorAlert::new(
                "low_leg_efficiency",
                format!("High Market Chop / Low Efficiency ({:.0}%)", er * 100.0),
                0.70,
            ));
        }

        Some(IndicatorOutput::new(er))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_efficiency(params: &HashMap<String, f64>) -> LegEfficiencyEngine {
    let len = params.get("len").copied().unwrap_or(14.0) as usize;
    LegEfficiencyEngine::new(len)
}
