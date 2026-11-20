use std::collections::HashMap;

use crate::indicator::kernels::efficiency_ratio;
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Kaufman Efficiency Ratio: net movement over path length.
///
/// Over the last `len + 1` closes, `ER = |close_t - close_{t-len}| / sum(|close_i - close_{i-1}|)`,
/// and `0` when nothing moved. `1` means the closes went straight from one end to the other, values
/// near `0` that they travelled far and arrived nowhere. Alerts fire at `>= 0.65` and `<= 0.20`.
///
/// First output: with the `len + 1`-th bar. [`Indicator::reset`] clears the window.
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
        self.len + 1
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

        let er = efficiency_ratio(self.closes.iter().copied()).unwrap_or(0.0);

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

pub const EFFICIENCY_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "efficiency",
        aliases: &["leg_efficiency", "er"],
        description: "Kaufman-style leg efficiency ratio",
        params: &[super::definition::ParamSpec::count("len", 14, 1, 10000)
            .meaning("bars between the two closes compared with the path travelled")],
        checks: &[],
        facts: super::definition::IndicatorFacts::ratio().bounded(0.0, 100.0),
        verification: super::definition::Verification::Verified,
        build: |p| Ok(Box::new(LegEfficiencyEngine::new(p.count("len")))),
    };
