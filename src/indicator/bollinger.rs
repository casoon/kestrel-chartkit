use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::{Indicator, IndicatorAlert, IndicatorOutput};

#[derive(Debug, Clone)]
pub struct BollingerBands {
    len: usize,
    mult: f64,
    window: VecDeque<f64>,
    sum: f64,

    alerts: BollingerAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BollingerAlerts {
    pub lower_touch: bool,
    pub upper_touch: bool,
    pub percent_b: f64,
}

impl BollingerBands {
    pub fn new(len: usize, mult: f64) -> Self {
        Self {
            len,
            mult,
            window: VecDeque::with_capacity(len),
            sum: 0.0,
            alerts: BollingerAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for BollingerBands {
    fn name(&self) -> &str {
        "bollinger"
    }

    fn warmup_period(&self) -> usize {
        self.len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = BollingerAlerts::default();
        let close = bar.close;

        self.window.push_back(close);
        self.sum += close;

        if self.window.len() > self.len {
            self.sum -= self.window.pop_front().unwrap();
        }

        if self.window.len() < self.len {
            return None;
        }

        let basis = self.sum / self.len as f64;
        let variance = self
            .window
            .iter()
            .map(|val| {
                let diff = val - basis;
                diff * diff
            })
            .sum::<f64>()
            / self.len as f64;

        let std_dev = variance.sqrt();
        let upper = basis + self.mult * std_dev;
        let lower = basis - self.mult * std_dev;

        let width = if basis != 0.0 {
            (upper - lower) / basis
        } else {
            0.0
        };

        let pct_b = if upper != lower {
            (close - lower) / (upper - lower)
        } else {
            0.5
        };

        self.alerts.lower_touch = close <= lower;
        self.alerts.upper_touch = close >= upper;
        self.alerts.percent_b = pct_b;

        let mut extra = HashMap::new();
        extra.insert("basis".to_string(), basis);
        extra.insert("upper".to_string(), upper);
        extra.insert("lower".to_string(), lower);
        extra.insert("bandwidth".to_string(), width);
        extra.insert("percent_b".to_string(), pct_b);

        Some(IndicatorOutput::with_extra(basis, extra))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum = 0.0;
        self.alerts = BollingerAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.lower_touch {
            out.push(IndicatorAlert {
                kind: "lower_touch".to_string(),
                note: "BOLLINGER · TOUCHED LOWER BAND".to_string(),
                strength: 1.0,
            });
        }
        if a.upper_touch {
            out.push(IndicatorAlert {
                kind: "upper_touch".to_string(),
                note: "BOLLINGER · TOUCHED UPPER BAND".to_string(),
                strength: 1.0,
            });
        }
        out
    }
}
