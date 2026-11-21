//! Averages with a changed construction: the zero-lag EMA, the fractal adaptive average and the
//! Gaussian kernel regression endpoint.
//!
//! Every engine is written from its published formula; the conventions a formula leaves open
//! (start values, the bar of the first output, alerts) are stated on each type. Bars are counted
//! from 0 in the formulas, `warmup_period` is the 1-based number of the first bar with a value, and
//! every state runs from the first bar whether or not a value is published yet.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::definition::{IndicatorDefinition, IndicatorFacts, ParamSpec, Verification};
use super::ehlers::Crossing;
use super::momentum_oscillators::{push_capped, SeededEma};
use super::registry::RegistryError;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

const OPEN: Verification = Verification::Open("acceptance against a second implementation pending");

// ---------------------------------------------------------------------------------------------
// Zero-lag EMA
// ---------------------------------------------------------------------------------------------

/// Zero-lag exponential moving average of the close: the EMA(`period`) of `C + (C - C_{i-lag})`
/// with `lag = floor((period - 1) / 2)`, seeded with its first input on bar `lag`. With a period
/// of 1 or 2 the lag is 0 and it is the plain EMA. First output on bar `lag`; alerts `bull` and
/// `bear` when the close crosses above and below the average.
#[derive(Debug, Clone)]
pub struct ZlemaEngine {
    period: usize,
    lag: usize,
    closes: VecDeque<f64>,
    ema: SeededEma,
    cross: Crossing,
}

impl ZlemaEngine {
    pub fn new(period: usize) -> Self {
        let lag = (period - 1) / 2;
        Self {
            period,
            lag,
            closes: VecDeque::with_capacity(lag + 1),
            ema: SeededEma::new(period),
            cross: Crossing::default(),
        }
    }
}

impl Indicator for ZlemaEngine {
    fn name(&self) -> &str {
        "zlema"
    }

    fn warmup_period(&self) -> usize {
        self.lag + 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.period);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let c = bar.close;
        push_capped(&mut self.closes, c, self.lag + 1);
        if self.closes.len() <= self.lag {
            return None;
        }
        let d = c + (c - self.closes[0]);
        let z = self.ema.update(d);
        self.cross.update(c, z);
        Some(IndicatorOutput::new(z))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.cross.alerts(
            ("bull", "ZLEMA · CLOSE CROSSES ABOVE"),
            ("bear", "ZLEMA · CLOSE CROSSES BELOW"),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// FRAMA
// ---------------------------------------------------------------------------------------------

/// Fractal adaptive moving average of the median price.
///
/// The fractal dimension `D = (ln(N1 + N2) - ln N3) / ln 2` comes from the high-low ranges of the
/// newest half window (`N1`), the older half (`N2`) and the whole window (`N3`), each divided by its
/// length; while one of them is not positive the previous dimension holds (1 at the start). The
/// smoothing constant is `exp(-4.6 (D - 1))` limited to `[0.01, 1]`. The average equals the median
/// price up to bar `period - 1` and follows `F = alpha P + (1 - alpha) F_prev` from bar
/// `period`, which is the first output. Extra `dimension`. No alerts.
#[derive(Debug, Clone)]
pub struct FramaEngine {
    period: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    dimension: f64,
    frama: f64,
    count: usize,
}

impl FramaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
            dimension: 1.0,
            frama: 0.0,
            count: 0,
        }
    }
}

/// Highest high minus lowest low of the window slice `from..to`.
fn range(highs: &VecDeque<f64>, lows: &VecDeque<f64>, from: usize, to: usize) -> f64 {
    let hi = highs
        .range(from..to)
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let lo = lows.range(from..to).copied().fold(f64::INFINITY, f64::min);
    hi - lo
}

impl Indicator for FramaEngine {
    fn name(&self) -> &str {
        "frama"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.period);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let i = self.count;
        self.count += 1;
        let p = (bar.high + bar.low) / 2.0;
        push_capped(&mut self.highs, bar.high, self.period);
        push_capped(&mut self.lows, bar.low, self.period);
        if i + 1 < self.period {
            self.frama = p;
            return None;
        }
        let h = self.period / 2;
        let n1 = range(&self.highs, &self.lows, h, self.period) / h as f64;
        let n2 = range(&self.highs, &self.lows, 0, h) / h as f64;
        let n3 = range(&self.highs, &self.lows, 0, self.period) / self.period as f64;
        if n1 > 0.0 && n2 > 0.0 && n3 > 0.0 {
            self.dimension = ((n1 + n2).ln() - n3.ln()) / std::f64::consts::LN_2;
        }
        if i + 1 == self.period {
            self.frama = p;
            return None;
        }
        let alpha = (-4.6 * (self.dimension - 1.0)).exp().clamp(0.01, 1.0);
        self.frama = alpha * p + (1.0 - alpha) * self.frama;
        let mut extra = HashMap::new();
        extra.insert("dimension".to_string(), self.dimension);
        Some(IndicatorOutput::with_extra(self.frama, extra))
    }
}

// ---------------------------------------------------------------------------------------------
// Nadaraya-Watson
// ---------------------------------------------------------------------------------------------

/// Gaussian kernel regression of the close, evaluated at the newest bar only (so a value never
/// changes later), with an envelope.
///
/// `y_i = sum_j w_j C_{i-j} / sum_j w_j` with `w_j = exp(-j^2 / (2 bandwidth^2))` for
/// `j = 0..lookback - 1`, both sums in the order of `j`. The envelope is `y +- mult * mae`, `mae`
/// the mean of `|C_k - y_k|` over the last `lookback` bars. First output on bar `2 lookback - 2`;
/// alerts `above_upper` and `below_lower` when the close crosses above the upper and below the
/// lower line.
#[derive(Debug, Clone)]
pub struct NadarayaWatsonEngine {
    bandwidth: f64,
    lookback: usize,
    mult: f64,
    weights: Vec<f64>,
    weight_sum: f64,
    closes: VecDeque<f64>,
    errors: VecDeque<f64>,
    over: Crossing,
    under: Crossing,
}

impl NadarayaWatsonEngine {
    pub fn new(bandwidth: f64, lookback: usize, mult: f64) -> Self {
        let weights: Vec<f64> = (0..lookback)
            .map(|j| (-((j * j) as f64) / (2.0 * bandwidth * bandwidth)).exp())
            .collect();
        let weight_sum = weights.iter().sum();
        Self {
            bandwidth,
            lookback,
            mult,
            weights,
            weight_sum,
            closes: VecDeque::with_capacity(lookback),
            errors: VecDeque::with_capacity(lookback),
            over: Crossing::default(),
            under: Crossing::default(),
        }
    }

    /// The kernel weight of the bar `j` back.
    pub fn weight(&self, j: usize) -> f64 {
        self.weights[j]
    }
}

impl Indicator for NadarayaWatsonEngine {
    fn name(&self) -> &str {
        "nadaraya_watson"
    }

    fn warmup_period(&self) -> usize {
        2 * self.lookback - 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.bandwidth, self.lookback, self.mult);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let c = bar.close;
        push_capped(&mut self.closes, c, self.lookback);
        if self.closes.len() < self.lookback {
            return None;
        }
        let newest = self.lookback - 1;
        let weighted: f64 = self
            .weights
            .iter()
            .enumerate()
            .map(|(j, w)| w * self.closes[newest - j])
            .sum();
        let y = weighted / self.weight_sum;
        push_capped(&mut self.errors, (c - y).abs(), self.lookback);
        if self.errors.len() < self.lookback {
            return None;
        }
        let mae = self.errors.iter().sum::<f64>() / self.lookback as f64;
        let (upper, lower) = (y + self.mult * mae, y - self.mult * mae);
        self.over.update(c, upper);
        self.under.update(c, lower);
        let mut extra = HashMap::new();
        extra.insert("upper".to_string(), upper);
        extra.insert("lower".to_string(), lower);
        Some(IndicatorOutput::with_extra(y, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.over.up {
            out.push(IndicatorAlert::new(
                "above_upper",
                "NADARAYA-WATSON · CLOSE CROSSES ABOVE UPPER",
                1.0,
            ));
        }
        if self.under.down {
            out.push(IndicatorAlert::new(
                "below_lower",
                "NADARAYA-WATSON · CLOSE CROSSES BELOW LOWER",
                1.0,
            ));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

/// Registry definition of `zlema`.
pub const ZLEMA_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "zlema",
    aliases: &[],
    description: "Zero-lag EMA: the EMA of the close plus its change over the last (period - 1) / 2 bars, seeded with its first input; with a period of 1 or 2 it is the plain EMA. Published from bar lag + 1.",
    params: &[ParamSpec::count("period", 20, 1, 10000).meaning("bars of the EMA")],
    checks: &[],
    facts: IndicatorFacts::price(),
    verification: OPEN,
    build: |p| Ok(Box::new(ZlemaEngine::new(p.count("period")))),
};

/// Registry definition of `frama`.
pub const FRAMA_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "frama",
    aliases: &[],
    description: "Fractal adaptive moving average of the median price: the smoothing constant follows the fractal dimension of the last `period` bars (extra `dimension`, 1 to 2 on usual data). The average equals the median price up to bar period and a window without range keeps the previous dimension. Odd periods are rejected; published from bar period + 1.",
    params: &[ParamSpec::count("period", 16, 4, 10000).meaning("bars of the window, even")],
    checks: &[],
    facts: IndicatorFacts::price().range_dependent(),
    verification: OPEN,
    build: |p| {
        let period = p.count("period");
        if period % 2 != 0 {
            return Err(RegistryError::InvalidParameter {
                parameter: "period".to_string(),
                value: period as f64,
                reason: "must be even".to_string(),
            });
        }
        Ok(Box::new(FramaEngine::new(period)))
    },
};

/// Registry definition of `nadaraya_watson`.
pub const NADARAYA_WATSON_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "nadaraya_watson",
    aliases: &[],
    description: "Gaussian kernel regression of the close at the newest bar, with an envelope of `mult` mean absolute errors (extras `upper`, `lower`). Only past bars are used, so a value never changes later. Published from bar 2 * lookback - 1.",
    params: &[
        ParamSpec::number("bandwidth", 8.0, 1e-9, 10000.0).meaning("width of the Gaussian kernel in bars"),
        ParamSpec::count("lookback", 50, 2, 10000).meaning("bars the kernel spans"),
        ParamSpec::number("mult", 3.0, 0.0, 1000.0).meaning("envelope width in mean absolute errors"),
    ],
    checks: &[],
    facts: IndicatorFacts::price(),
    verification: OPEN,
    build: |p| {
        Ok(Box::new(NadarayaWatsonEngine::new(
            p.number("bandwidth"),
            p.count("lookback"),
            p.number("mult"),
        )))
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    fn close_bar(c: f64) -> Bar {
        Bar::new(0, c, c, c, c, 0.0)
    }

    fn bar(h: f64, l: f64) -> Bar {
        let m = (h + l) / 2.0;
        Bar::new(0, m, h, l, m, 0.0)
    }

    const CLOSES: [f64; 10] = [10.0, 11.0, 13.0, 12.0, 15.0, 14.0, 16.0, 19.0, 17.0, 18.0];

    #[test]
    fn zlema_with_period_one_is_the_close() {
        let mut z = ZlemaEngine::new(1);
        for c in CLOSES {
            assert_eq!(z.on_bar(&close_bar(c)).unwrap().value, c);
        }
    }

    #[test]
    fn zlema_of_a_line_has_no_lag_once_settled() {
        // On a line the input is C + lag * slope; the EMA of a line trails by (1 - a) / a * slope
        // = (period - 1) / 2 * slope, which the lead cancels.
        let mut z = ZlemaEngine::new(5);
        let mut last = 0.0;
        for i in 0..200 {
            let c = 100.0 + 2.0 * i as f64;
            if let Some(o) = z.on_bar(&close_bar(c)) {
                last = o.value;
            }
        }
        assert!((last - (100.0 + 2.0 * 199.0)).abs() < 1e-9, "{last}");
        assert_eq!(z.warmup_period(), 3);
    }

    #[test]
    fn frama_rejects_odd_periods_and_publishes_from_period_plus_one() {
        let mut params = HashMap::new();
        params.insert("period".to_string(), 5.0);
        assert!(crate::indicator::registry::build_checked("frama", &params).is_err());
        let mut f = FramaEngine::new(4);
        let outs: Vec<_> = (0..8)
            .map(|i| f.on_bar(&bar(11.0 + i as f64, 9.0 + i as f64)))
            .collect();
        assert!(outs[..4].iter().all(Option::is_none));
        assert!(outs[4..].iter().all(Option::is_some));
    }

    #[test]
    fn frama_dimension_of_a_straight_line_follows_the_ranges() {
        let mut f = FramaEngine::new(4);
        let mut last = None;
        for i in 0..12 {
            last = f.on_bar(&bar(11.0 + i as f64, 9.0 + i as f64));
        }
        let o = last.unwrap();
        // Both halves span 3 bars' worth of range (N1 = N2 = 3 / 2), the window 5 (N3 = 5 / 4).
        let d = ((1.5f64 + 1.5).ln() - (5.0f64 / 4.0).ln()) / std::f64::consts::LN_2;
        assert!((o.extra["dimension"] - d).abs() < 1e-12);
    }

    #[test]
    fn nadaraya_watson_weights_and_first_value() {
        let mut nw = NadarayaWatsonEngine::new(2.0, 3, 1.0);
        assert_eq!(nw.weight(0), 1.0);
        assert!((nw.weight(1) - (-1.0f64 / 8.0).exp()).abs() < 1e-15);
        assert!((nw.weight(2) - (-4.0f64 / 8.0).exp()).abs() < 1e-15);
        assert_eq!(nw.warmup_period(), 5);
        let outs: Vec<_> = CLOSES[..6]
            .iter()
            .map(|&c| nw.on_bar(&close_bar(c)))
            .collect();
        assert!(outs[..4].iter().all(Option::is_none));
        // y_2 on closes 10, 11, 13 (newest 13, weights 1, w1, w2)
        let (w1, w2) = ((-1.0f64 / 8.0).exp(), (-0.5f64).exp());
        let y = |a: f64, b: f64, c: f64| (c + w1 * b + w2 * a) / (1.0 + w1 + w2);
        let (y2, y3, y4) = (
            y(10.0, 11.0, 13.0),
            y(11.0, 13.0, 12.0),
            y(13.0, 12.0, 15.0),
        );
        let mae = ((13.0 - y2).abs() + (12.0 - y3).abs() + (15.0 - y4).abs()) / 3.0;
        let o = outs[4].as_ref().unwrap();
        assert!((o.value - y4).abs() < 1e-12);
        assert!((o.extra["upper"] - (y4 + mae)).abs() < 1e-12);
        assert!((o.extra["lower"] - (y4 - mae)).abs() < 1e-12);
    }
}
