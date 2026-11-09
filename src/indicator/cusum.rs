use std::collections::HashMap;

use crate::changepoint::CusumState;
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Two-sided CUSUM of close-to-close returns, standardized by a fixed calibration period.
///
/// The first `len` simple returns `r = close_t / close_{t-1} - 1` form the reference period: their
/// mean `mu` and population standard deviation `sigma` stay fixed afterwards. Each later return is
/// standardized, `z = (r - mu) / sigma` (`z = 0` when `sigma` is `0`: a flat reference period gives
/// no scale), and fed through the sums of [`crate::changepoint::cusum`] with reference value `0`,
/// tolerance `k` and threshold `h`, both in units of `sigma`.
///
/// Output: the upward sum `S⁺` as the value, the downward sum `S⁻` as `extra["lower"]`. An alert
/// (`cusum_upper_alarm`, `cusum_lower_alarm`) fires on the bar where a sum rises from `<= h` to
/// `> h`; the sums are never reset, so a sum that stays above `h` raises it once.
///
/// First output: with the `len + 2`-th bar (`len + 1` closes give the calibration returns, the next
/// bar brings the first standardized return). [`Indicator::reset`] restarts the calibration.
#[derive(Debug, Clone)]
pub struct CusumEngine {
    len: usize,
    k: f64,
    h: f64,
    prev_close: Option<f64>,
    calibration: Vec<f64>,
    mu: f64,
    sigma: f64,
    state: CusumState,
    alerts: Vec<IndicatorAlert>,
}

impl CusumEngine {
    pub fn new(len: usize, k: f64, h: f64) -> Self {
        Self {
            len: len.max(2),
            k,
            h,
            prev_close: None,
            calibration: Vec::new(),
            mu: 0.0,
            sigma: 0.0,
            state: CusumState::default(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for CusumEngine {
    fn name(&self) -> &str {
        "cusum"
    }

    fn warmup_period(&self) -> usize {
        self.len + 2
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.calibration.clear();
        self.mu = 0.0;
        self.sigma = 0.0;
        self.state = CusumState::default();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let prev = self.prev_close.replace(bar.close)?;
        let r = if prev != 0.0 {
            bar.close / prev - 1.0
        } else {
            0.0
        };

        if self.calibration.len() < self.len {
            self.calibration.push(r);
            if self.calibration.len() == self.len {
                let n = self.len as f64;
                self.mu = self.calibration.iter().sum::<f64>() / n;
                let var = self
                    .calibration
                    .iter()
                    .map(|v| (v - self.mu).powi(2))
                    .sum::<f64>()
                    / n;
                self.sigma = var.sqrt();
            }
            return None;
        }

        let z = if self.sigma > 0.0 {
            (r - self.mu) / self.sigma
        } else {
            0.0
        };
        let before = self.state;
        self.state.step(z, 0.0, self.k);

        if before.upper <= self.h && self.state.upper > self.h {
            self.alerts.push(IndicatorAlert::new(
                "cusum_upper_alarm",
                format!(
                    "CUSUM: upward deviation {:.2} sigma above threshold {:.2}",
                    self.state.upper, self.h
                ),
                1.0,
            ));
        }
        if before.lower <= self.h && self.state.lower > self.h {
            self.alerts.push(IndicatorAlert::new(
                "cusum_lower_alarm",
                format!(
                    "CUSUM: downward deviation {:.2} sigma above threshold {:.2}",
                    self.state.lower, self.h
                ),
                1.0,
            ));
        }

        let mut extra = HashMap::new();
        extra.insert("lower".to_string(), self.state.lower);
        Some(IndicatorOutput::with_extra(self.state.upper, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const CUSUM_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "cusum",
        aliases: &[],
        description: "Two-sided CUSUM of standardized close-to-close returns",
        params: &[
            super::definition::ParamSpec::count("len", 20, 2, 10000)
                .meaning("returns in the fixed calibration period for mean and standard deviation"),
            super::definition::ParamSpec::number("k", 0.5, 0.0, 100.0)
                .meaning("tolerance per return, in standard deviations of the calibration period"),
            super::definition::ParamSpec::number("h", 4.0, 0.01, 1000.0)
                .meaning("alarm threshold of the accumulated sum, in standard deviations"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(CusumEngine::new(
                p.count("len"),
                p.number("k"),
                p.number("h"),
            )))
        },
    };
