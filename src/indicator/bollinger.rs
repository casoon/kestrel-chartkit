use std::collections::{HashMap, VecDeque};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::Bar;

use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Which divisor the band standard deviation uses.
///
/// The window is the full population of the lookback in one reading and a sample drawn from an
/// unobserved wider distribution in the other; neither is a correction of the other, so the
/// choice belongs to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum VarianceConvention {
    /// Divisor `N`. The default, and the historical behaviour of this indicator.
    #[default]
    Population,
    /// Divisor `N - 1` (Bessel-corrected), which requires `N >= 2`. At `N = 20` this widens the
    /// distance between basis and band by `sqrt(20/19)`, roughly 2.6% — the band values
    /// themselves do not scale by that factor, since the basis is unaffected.
    Sample,
}

/// Bollinger Bands over the closing price.
///
/// Basis is the SMA of the last `len` closes; the bands sit `mult` standard deviations away,
/// computed from the same window around that basis (`sum((x - basis)^2) / divisor`, with the
/// divisor chosen by [`VarianceConvention`]). The centred form is kept deliberately rather than
/// `E[x^2] - E[x]^2`, which loses precision when small fluctuations ride on a large price level.
///
/// Per-bar outputs, all derived from the selected bands: `value`/`extra["basis"]`,
/// `extra["upper"]`, `extra["lower"]`, `extra["bandwidth"]` (`(upper - lower) / basis`, 0 for a
/// zero basis) and `extra["percent_b"]` (`(close - lower) / (upper - lower)`, 0.5 for a
/// degenerate band). The touch alerts compare the close against the same bands.
///
/// First output: with the `len`-th bar. [`Indicator::reset`] clears the window, so the next
/// series starts deterministically.
#[derive(Debug, Clone)]
pub struct BollingerBands {
    len: usize,
    mult: f64,
    variance: VarianceConvention,
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
            variance: VarianceConvention::Population,
            window: VecDeque::with_capacity(len),
            sum: 0.0,
            alerts: BollingerAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 2.0)
    }

    /// Selects the standard-deviation divisor; see [`VarianceConvention`].
    ///
    /// Additive to the existing constructors, which keep the population divisor. Panics on
    /// `Sample` with `len < 2`, where `N - 1` is not a divisor; the registry rejects that
    /// combination with an error instead of panicking.
    pub fn with_variance(mut self, variance: VarianceConvention) -> Self {
        assert!(
            variance != VarianceConvention::Sample || self.len >= 2,
            "sample variance requires len >= 2, got {}",
            self.len
        );
        self.variance = variance;
        self
    }

    pub fn variance(&self) -> VarianceConvention {
        self.variance
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
        let divisor = match self.variance {
            VarianceConvention::Population => self.len as f64,
            // `with_variance`/the registry rule out `len < 2` in this mode.
            VarianceConvention::Sample => (self.len - 1) as f64,
        };
        let variance = self
            .window
            .iter()
            .map(|val| {
                let diff = val - basis;
                diff * diff
            })
            .sum::<f64>()
            / divisor;

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
