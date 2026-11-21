//! Three range-based volatility estimators as catalog entries: Parkinson (high and low),
//! Rogers-Satchell (open, high, low and close, free of drift) and Yang-Zhang (the overnight gap, the
//! open-to-close move and Rogers-Satchell combined). Each publishes the per-bar standard deviation
//! annualised with `periods_per_year` bars, in percent: `100 * sqrt(variance) * sqrt(periods_per_year)`.

use std::collections::VecDeque;

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

fn annualised(variance: f64, periods_per_year: usize) -> f64 {
    100.0 * variance.sqrt() * (periods_per_year as f64).sqrt()
}

fn all_positive(prices: &[f64]) -> bool {
    prices.iter().all(|p| *p > 0.0)
}

/// Rogers-Satchell term of one bar.
fn rogers_satchell_term(b: &Bar) -> f64 {
    (b.high / b.open).ln() * (b.high / b.close).ln()
        + (b.low / b.open).ln() * (b.low / b.close).ln()
}

/// Parkinson volatility: the mean of `ln(high/low)^2` over the last `period` bars divided by
/// `4 ln 2`, rooted and annualised, in percent. A window with a non-positive high or low publishes
/// nothing. First output: with the `period`-th bar. [`Indicator::reset`] clears the window.
pub struct ParkinsonVolatilityEngine {
    period: usize,
    periods_per_year: usize,
    bars: VecDeque<Bar>,
}

impl ParkinsonVolatilityEngine {
    pub fn new(period: usize, periods_per_year: usize) -> Self {
        Self {
            period,
            periods_per_year,
            bars: VecDeque::new(),
        }
    }
}

impl Indicator for ParkinsonVolatilityEngine {
    fn name(&self) -> &str {
        "parkinson"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.bars.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period {
            self.bars.pop_front();
        }
        if self.bars.len() < self.period {
            return None;
        }
        if !self.bars.iter().all(|b| all_positive(&[b.high, b.low])) {
            return None;
        }
        let mut sum = 0.0;
        for b in &self.bars {
            sum += (b.high / b.low).ln().powi(2);
        }
        let variance = sum / (4.0 * std::f64::consts::LN_2 * self.period as f64);
        Some(IndicatorOutput::new(annualised(
            variance,
            self.periods_per_year,
        )))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Rogers-Satchell volatility: the mean over the last `period` bars of
/// `ln(H/O) ln(H/C) + ln(L/O) ln(L/C)`, floored at zero, rooted and annualised, in percent. A
/// window with a non-positive price publishes nothing. First output: with the `period`-th bar.
/// [`Indicator::reset`] clears the window.
pub struct RogersSatchellVolatilityEngine {
    period: usize,
    periods_per_year: usize,
    bars: VecDeque<Bar>,
}

impl RogersSatchellVolatilityEngine {
    pub fn new(period: usize, periods_per_year: usize) -> Self {
        Self {
            period,
            periods_per_year,
            bars: VecDeque::new(),
        }
    }
}

impl Indicator for RogersSatchellVolatilityEngine {
    fn name(&self) -> &str {
        "rogers_satchell"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.bars.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period {
            self.bars.pop_front();
        }
        if self.bars.len() < self.period {
            return None;
        }
        if !self
            .bars
            .iter()
            .all(|b| all_positive(&[b.open, b.high, b.low, b.close]))
        {
            return None;
        }
        let mut sum = 0.0;
        for b in &self.bars {
            sum += rogers_satchell_term(b);
        }
        let variance = (sum / self.period as f64).max(0.0);
        Some(IndicatorOutput::new(annualised(
            variance,
            self.periods_per_year,
        )))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Yang-Zhang volatility: the sample variance of the overnight returns `ln(O_j / C_{j-1})`, `k`
/// times the sample variance of the open-to-close returns `ln(C_j / O_j)` and `1 - k` times the
/// Rogers-Satchell variance, with `k = 0.34 / (1.34 + (n + 1) / (n - 1))`, over the last `period`
/// bars; rooted and annualised, in percent. Needs the close before the window, so the first
/// output comes with the `period + 1`-th bar. A window with a non-positive price publishes
/// nothing. [`Indicator::reset`] clears the window.
pub struct YangZhangVolatilityEngine {
    period: usize,
    periods_per_year: usize,
    bars: VecDeque<Bar>,
}

impl YangZhangVolatilityEngine {
    pub fn new(period: usize, periods_per_year: usize) -> Self {
        Self {
            period,
            periods_per_year,
            bars: VecDeque::new(),
        }
    }
}

impl Indicator for YangZhangVolatilityEngine {
    fn name(&self) -> &str {
        "yang_zhang"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period + 1 {
            self.bars.pop_front();
        }
        if self.bars.len() < self.period + 1 {
            return None;
        }
        let oldest = &self.bars[0];
        if oldest.close <= 0.0
            || !self
                .bars
                .iter()
                .skip(1)
                .all(|b| all_positive(&[b.open, b.high, b.low, b.close]))
        {
            return None;
        }
        let n = self.period as f64;
        let window: Vec<&Bar> = self.bars.iter().skip(1).collect();
        let previous: Vec<&Bar> = self.bars.iter().take(self.period).collect();

        let overnight: Vec<f64> = window
            .iter()
            .zip(&previous)
            .map(|(b, p)| (b.open / p.close).ln())
            .collect();
        let open_to_close: Vec<f64> = window.iter().map(|b| (b.close / b.open).ln()).collect();

        let sample_variance = |values: &[f64]| {
            let mut sum = 0.0;
            for v in values {
                sum += v;
            }
            let mean = sum / n;
            let mut squares = 0.0;
            for v in values {
                squares += (v - mean) * (v - mean);
            }
            squares / (n - 1.0)
        };
        let sigma_o = sample_variance(&overnight);
        let sigma_c = sample_variance(&open_to_close);

        let mut rs_sum = 0.0;
        for b in &window {
            rs_sum += rogers_satchell_term(b);
        }
        let sigma_rs = (rs_sum / n).max(0.0);

        let k = 0.34 / (1.34 + (n + 1.0) / (n - 1.0));
        let variance = sigma_o + k * sigma_c + (1.0 - k) * sigma_rs;
        Some(IndicatorOutput::new(annualised(
            variance,
            self.periods_per_year,
        )))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const PARKINSON_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "parkinson",
        aliases: &[],
        description: "Parkinson volatility estimator (Parkinson 1980): the mean squared high-low log range over the window, scaled by 4 ln 2, rooted and annualised with periods_per_year, in percent; a window with a non-positive high or low publishes nothing",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000)
                .meaning("bars averaged in the high-low variance estimate"),
            super::definition::ParamSpec::count("periods_per_year", 252, 1, 100000)
                .meaning("bars per year the volatility is annualised with"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::ratio()
            .non_negative()
            .range_dependent(),
        verification: super::definition::Verification::Verified,
        build: |p| {
            Ok(Box::new(ParkinsonVolatilityEngine::new(
                p.count("period"),
                p.count("periods_per_year"),
            )))
        },
    };

pub const ROGERS_SATCHELL_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "rogers_satchell",
        aliases: &[],
        description: "Rogers-Satchell volatility estimator (Rogers and Satchell 1991), independent of the drift of the price: the mean of ln(H/O) ln(H/C) + ln(L/O) ln(L/C) over the window, floored at zero, rooted and annualised with periods_per_year, in percent; a window with a non-positive price publishes nothing",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000)
                .meaning("bars averaged in the open-high-low-close variance estimate"),
            super::definition::ParamSpec::count("periods_per_year", 252, 1, 100000)
                .meaning("bars per year the volatility is annualised with"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::ratio()
            .non_negative()
            .range_dependent(),
        verification: super::definition::Verification::Verified,
        build: |p| {
            Ok(Box::new(RogersSatchellVolatilityEngine::new(
                p.count("period"),
                p.count("periods_per_year"),
            )))
        },
    };

pub const YANG_ZHANG_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "yang_zhang",
        aliases: &[],
        description: "Yang-Zhang volatility estimator (Yang and Zhang 2000): the overnight variance, the open-to-close variance and the Rogers-Satchell variance combined, rooted and annualised with periods_per_year, in percent; needs the close before the window, so the first value comes one bar after the period, and a window with a non-positive price publishes nothing",
        params: &[
            super::definition::ParamSpec::count("period", 20, 2, 10000)
                .meaning("bars in the three variance estimates"),
            super::definition::ParamSpec::count("periods_per_year", 252, 1, 100000)
                .meaning("bars per year the volatility is annualised with"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::ratio()
            .non_negative()
            .range_dependent(),
        verification: super::definition::Verification::Verified,
        build: |p| {
            Ok(Box::new(YangZhangVolatilityEngine::new(
                p.count("period"),
                p.count("periods_per_year"),
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, o: f64, h: f64, l: f64, c: f64) -> Bar {
        Bar::new(t, o, h, l, c, 100.0)
    }

    #[test]
    fn parkinson_on_a_constant_range_is_the_closed_form() {
        // ln(H/L) = ln 2 on every bar: sigma^2 = ln(2)^2 / (4 ln 2) = ln(2) / 4.
        let mut engine = ParkinsonVolatilityEngine::new(5, 252);
        let mut out = Vec::new();
        for i in 0..7 {
            out.push(engine.on_bar(&bar(i, 1.5, 2.0, 1.0, 1.5)));
        }
        assert!(out[..4].iter().all(Option::is_none));
        let expected = 100.0 * (std::f64::consts::LN_2 / 4.0).sqrt() * 252f64.sqrt();
        assert!((out[4].as_ref().unwrap().value - expected).abs() < 1e-9);
        assert!((out[6].as_ref().unwrap().value - expected).abs() < 1e-9);
    }

    #[test]
    fn parkinson_publishes_nothing_for_a_non_positive_low() {
        let mut engine = ParkinsonVolatilityEngine::new(2, 252);
        engine.on_bar(&bar(0, 1.5, 2.0, 1.0, 1.5));
        assert!(engine.on_bar(&bar(1, 1.5, 2.0, 0.0, 1.5)).is_none());
        assert!(engine.on_bar(&bar(2, 1.5, 2.0, 1.0, 1.5)).is_none());
        assert!(engine.on_bar(&bar(3, 1.5, 2.0, 1.0, 1.5)).is_some());
    }

    #[test]
    fn periods_per_year_scales_with_its_root() {
        let run = |p: usize| {
            let mut engine = ParkinsonVolatilityEngine::new(1, p);
            engine.on_bar(&bar(0, 1.5, 2.0, 1.0, 1.5)).unwrap().value
        };
        assert!((run(100) / run(1) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn rogers_satchell_of_a_bar_with_open_at_low_and_close_at_high() {
        // O = L, C = H: u = ln(H/O) * ln(1) + 0 * ln(L/C) = 0.
        let mut engine = RogersSatchellVolatilityEngine::new(1, 252);
        let out = engine.on_bar(&bar(0, 1.0, 2.0, 1.0, 2.0)).unwrap();
        assert_eq!(out.value, 0.0);
        // O = C = 1.5, H = 2, L = 1: u = ln(4/3)^2 + ln(2/3)^2.
        let u = (2.0f64 / 1.5).ln().powi(2) + (1.0f64 / 1.5).ln().powi(2);
        let out = engine.on_bar(&bar(1, 1.5, 2.0, 1.0, 1.5)).unwrap();
        assert!((out.value - 100.0 * u.sqrt() * 252f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn yang_zhang_needs_the_close_before_the_window() {
        let mut engine = YangZhangVolatilityEngine::new(3, 252);
        let mut out = Vec::new();
        for i in 0..5 {
            out.push(engine.on_bar(&bar(i, 10.0, 11.0, 9.0, 10.0)));
        }
        assert!(out[..3].iter().all(Option::is_none));
        // Identical bars: no overnight or open-to-close variance; only the Rogers-Satchell part
        // (1 - k) remains.
        let n = 3.0;
        let k = 0.34 / (1.34 + (n + 1.0) / (n - 1.0));
        let u = (11.0f64 / 10.0).ln().powi(2) + (9.0f64 / 10.0).ln().powi(2);
        let expected = 100.0 * ((1.0 - k) * u).sqrt() * 252f64.sqrt();
        assert!((out[3].as_ref().unwrap().value - expected).abs() < 1e-9);
    }
}
