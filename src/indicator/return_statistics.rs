//! Four statistics of the close as catalog entries: the Lo-MacKinlay variance ratio, the rolling
//! autocorrelation of one-bar log returns, the Hurst exponent by rescaled-range analysis and the
//! half-life of mean reversion of an AR(1) fit. Each works on a rolling window that ends at the
//! current bar; sums run from the oldest element to the newest. A window with a non-positive close
//! publishes nothing for the three that take logarithms.

use std::collections::{HashMap, VecDeque};

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// The last `len` closes, oldest first; `None` until the window is full.
fn push_close(closes: &mut VecDeque<f64>, len: usize, close: f64) -> bool {
    closes.push_back(close);
    if closes.len() > len {
        closes.pop_front();
    }
    closes.len() == len
}

/// The log returns of a window of closes, or `None` if one of them is not positive.
fn log_returns(closes: &VecDeque<f64>) -> Option<Vec<f64>> {
    if !closes.iter().all(|c| *c > 0.0) {
        return None;
    }
    Some(
        closes
            .iter()
            .zip(closes.iter().skip(1))
            .map(|(a, b)| (b / a).ln())
            .collect(),
    )
}

/// Lo-MacKinlay variance ratio of the log price over `period` one-bar returns and `lag` bars:
/// the variance of the overlapping `lag`-bar returns (about `lag` times the mean one-bar return)
/// divided by the variance of the one-bar returns, both with their small-sample corrections.
/// Above 1 the returns are positively autocorrelated, below 1 negatively; 1 is a random walk.
///
/// `value`: the ratio; `extra["z"]`: the homoskedastic test statistic `sqrt(period) * (VR - 1) /
/// sqrt(2 (2 lag - 1) (lag - 1) / (3 lag))`. Nothing is published for a window whose one-bar
/// variance is exactly zero or that holds a non-positive close. First output: with the
/// `period + 1`-th bar. [`Indicator::reset`] clears the window.
pub struct VarianceRatioEngine {
    period: usize,
    lag: usize,
    closes: VecDeque<f64>,
}

impl VarianceRatioEngine {
    pub fn new(period: usize, lag: usize) -> Self {
        Self {
            period,
            lag,
            closes: VecDeque::new(),
        }
    }
}

impl Indicator for VarianceRatioEngine {
    fn name(&self) -> &str {
        "variance_ratio"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.closes.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if !push_close(&mut self.closes, self.period + 1, bar.close) {
            return None;
        }
        if !self.closes.iter().all(|c| *c > 0.0) {
            return None;
        }
        let n = self.period;
        let q = self.lag;
        let nf = n as f64;
        let qf = q as f64;
        // Every log return is the log of a price ratio: a difference of two logarithms loses
        // digits on a high price level.
        let c = &self.closes;
        let mu = (c[n] / c[0]).ln() / nf;
        let mut sum_a = 0.0;
        for t in 1..=n {
            let r = (c[t] / c[t - 1]).ln();
            sum_a += (r - mu) * (r - mu);
        }
        let sigma_a = sum_a / (nf - 1.0);
        if sigma_a == 0.0 {
            return None;
        }

        let m = qf * (nf - qf + 1.0) * (1.0 - qf / nf);
        let mut sum_q = 0.0;
        for t in q..=n {
            let big_r = (c[t] / c[t - q]).ln();
            let d = big_r - qf * mu;
            sum_q += d * d;
        }
        let sigma_q = sum_q / m;

        let vr = sigma_q / sigma_a;
        let z = nf.sqrt() * (vr - 1.0) / (2.0 * (2.0 * qf - 1.0) * (qf - 1.0) / (3.0 * qf)).sqrt();

        let mut extra = HashMap::new();
        extra.insert("z".to_string(), z);
        Some(IndicatorOutput::with_extra(vr, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Rolling autocorrelation at `lag` of the last `period` one-bar log returns: the sum of products
/// of returns `lag` apart around their mean over the sum of squares around the mean (the
/// denominator runs over all returns, no small-sample adjustment), `-1..1`. Nothing is published
/// for a window whose returns are exactly all equal or that holds a non-positive close. First
/// output: with the `period + 1`-th bar. [`Indicator::reset`] clears the window.
pub struct AutocorrelationEngine {
    period: usize,
    lag: usize,
    closes: VecDeque<f64>,
}

impl AutocorrelationEngine {
    pub fn new(period: usize, lag: usize) -> Self {
        Self {
            period,
            lag,
            closes: VecDeque::new(),
        }
    }
}

impl Indicator for AutocorrelationEngine {
    fn name(&self) -> &str {
        "autocorrelation"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.closes.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if !push_close(&mut self.closes, self.period + 1, bar.close) {
            return None;
        }
        let x = log_returns(&self.closes)?;
        let n = self.period;
        let k = self.lag;

        let mut sum = 0.0;
        for v in &x {
            sum += v;
        }
        let mu = sum / n as f64;

        let mut num = 0.0;
        for t in k..n {
            num += (x[t] - mu) * (x[t - k] - mu);
        }
        let mut den = 0.0;
        for v in &x {
            den += (v - mu) * (v - mu);
        }
        if den == 0.0 {
            return None;
        }
        Some(IndicatorOutput::new(num / den))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Hurst exponent by rescaled-range (R/S) analysis of the last `period` one-bar log returns.
///
/// The returns are cut into chunks of `period / 2^j` returns (`j = 0, 1, ...` while the size is at
/// least `min_chunk`), counted from the newest return back; the oldest returns that do not fill a
/// chunk are left out for that size. Each chunk gives `R / S`, the range of its cumulative
/// deviations from the chunk mean over the root mean square deviation; a chunk with a zero range is
/// skipped. The exponent is the least-squares slope of `ln(mean R/S)` against `ln(size)` over the
/// sizes that have a value. It is about 0.5 for a random walk, above for persistent and below for
/// anti-persistent returns. The plain R/S estimator has no small-sample correction and is biased
/// upward on short chunks.
///
/// Nothing is published with fewer than two sizes left or a non-positive close in the window.
/// First output: with the `period + 1`-th bar. [`Indicator::reset`] clears the window.
pub struct HurstRsEngine {
    period: usize,
    min_chunk: usize,
    closes: VecDeque<f64>,
}

impl HurstRsEngine {
    pub fn new(period: usize, min_chunk: usize) -> Self {
        Self {
            period,
            min_chunk,
            closes: VecDeque::new(),
        }
    }
}

/// Mean R/S of the chunks of `size` returns taken from the newest end, or `None` if every chunk
/// has a zero range.
fn rescaled_range(x: &[f64], size: usize) -> Option<f64> {
    let count = x.len() / size;
    let mut sum = 0.0;
    let mut used = 0usize;
    // Oldest chunk first; chunk `c` counted from the newest end covers
    // `x[len - c*size .. len - (c-1)*size]`.
    for c in (1..=count).rev() {
        let chunk = &x[x.len() - c * size..x.len() - (c - 1) * size];
        let mut total = 0.0;
        for a in chunk {
            total += a;
        }
        let mean = total / size as f64;
        let mut cumulative = 0.0;
        let mut high = f64::NEG_INFINITY;
        let mut low = f64::INFINITY;
        let mut squares = 0.0;
        for a in chunk {
            let d = a - mean;
            cumulative += d;
            high = high.max(cumulative);
            low = low.min(cumulative);
            squares += d * d;
        }
        let range = high - low;
        if range == 0.0 {
            continue;
        }
        let s = (squares / size as f64).sqrt();
        sum += range / s;
        used += 1;
    }
    (used > 0).then(|| sum / used as f64)
}

impl Indicator for HurstRsEngine {
    fn name(&self) -> &str {
        "hurst_rs"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.closes.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if !push_close(&mut self.closes, self.period + 1, bar.close) {
            return None;
        }
        let x = log_returns(&self.closes)?;

        let mut points: Vec<(f64, f64)> = Vec::new();
        let mut size = self.period;
        while size >= self.min_chunk {
            if let Some(rs) = rescaled_range(&x, size) {
                points.push(((size as f64).ln(), rs.ln()));
            }
            size /= 2;
        }
        if points.len() < 2 {
            return None;
        }

        let count = points.len() as f64;
        let (mut sum_x, mut sum_y) = (0.0, 0.0);
        for (px, py) in &points {
            sum_x += px;
            sum_y += py;
        }
        let (mean_x, mean_y) = (sum_x / count, sum_y / count);
        let (mut sxy, mut sxx) = (0.0, 0.0);
        for (px, py) in &points {
            sxy += (px - mean_x) * (py - mean_y);
            sxx += (px - mean_x) * (px - mean_x);
        }
        Some(IndicatorOutput::new(sxy / sxx))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Margin below 1 within which `phi` counts as no mean reversion (largest half-life about 6.9e8 bars).
const PHI_MARGIN: f64 = 1e-9;

/// Half-life of mean reversion from a least-squares AR(1) fit with intercept to the last `period`
/// raw closes: `phi` is the slope of each close on the one before, the half-life is
/// `-ln 2 / ln(phi)` bars (the Ornstein-Uhlenbeck half-life with `theta = -ln(phi)`).
///
/// `value`: the half-life in bars; `extra["phi"]`: the coefficient. Nothing is published when the
/// closes before the last are all equal, or when `phi` is not in `(0, 1 - 1e-9)` (an oscillation has no
/// half-life, a series without mean reversion has none either). First output: with the
/// `period`-th bar. [`Indicator::reset`] clears the window.
pub struct Ar1HalfLifeEngine {
    period: usize,
    closes: VecDeque<f64>,
}

impl Ar1HalfLifeEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            closes: VecDeque::new(),
        }
    }
}

impl Indicator for Ar1HalfLifeEngine {
    fn name(&self) -> &str {
        "ar1_half_life"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.closes.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if !push_close(&mut self.closes, self.period, bar.close) {
            return None;
        }
        let pairs = (self.period - 1) as f64;
        let y: Vec<f64> = self.closes.iter().copied().collect();
        let x = &y[..self.period - 1];
        let z = &y[1..];

        let (mut sum_x, mut sum_z) = (0.0, 0.0);
        for t in 0..x.len() {
            sum_x += x[t];
            sum_z += z[t];
        }
        let (mean_x, mean_z) = (sum_x / pairs, sum_z / pairs);
        let (mut sxx, mut sxz) = (0.0, 0.0);
        for t in 0..x.len() {
            sxx += (x[t] - mean_x) * (x[t] - mean_x);
            sxz += (x[t] - mean_x) * (z[t] - mean_z);
        }
        if sxx == 0.0 {
            return None;
        }
        let phi = sxz / sxx;
        // Within PHI_MARGIN of 1 the coefficient is rounding noise of a straight line, not mean
        // reversion; publishing it would give half-lives of 10^14 bars.
        if phi <= 0.0 || phi >= 1.0 - PHI_MARGIN {
            return None;
        }
        let mut extra = HashMap::new();
        extra.insert("phi".to_string(), phi);
        Some(IndicatorOutput::with_extra(
            -std::f64::consts::LN_2 / phi.ln(),
            extra,
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const VARIANCE_RATIO_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "variance_ratio",
        aliases: &[],
        description: "Lo-MacKinlay variance ratio (1988) of the log price: the variance of overlapping lag-bar returns over the variance of one-bar returns, both bias-corrected; above 1 the returns are positively autocorrelated, below 1 negatively, 1 is a random walk; extra z is the homoskedastic test statistic and can be negative",
        params: &[
            super::definition::ParamSpec::count("period", 100, 4, 10000)
                .meaning("one-bar returns in the window"),
            super::definition::ParamSpec::count("lag", 4, 2, 5000)
                .meaning("bars in the overlapping returns"),
        ],
        checks: &[super::definition::Check::AtMost {
            value: "lag",
            times: 2,
            limit: "period",
        }],
        facts: super::definition::IndicatorFacts::unitless(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| {
            Ok(Box::new(VarianceRatioEngine::new(
                p.count("period"),
                p.count("lag"),
            )))
        },
    };

pub const AUTOCORRELATION_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "autocorrelation",
        aliases: &[],
        description: "Rolling sample autocorrelation of one-bar log returns at a lag, as in Box and Jenkins: the denominator runs over all returns of the window with no small-sample adjustment; -1..1",
        params: &[
            super::definition::ParamSpec::count("period", 50, 3, 10000)
                .meaning("one-bar returns in the window"),
            super::definition::ParamSpec::count("lag", 1, 1, 10000)
                .meaning("bars between the returns that are correlated"),
        ],
        checks: &[super::definition::Check::Less {
            lower: "lag",
            upper: "period",
        }],
        facts: super::definition::IndicatorFacts::ratio().bounded(-1.0, 1.0),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| {
            Ok(Box::new(AutocorrelationEngine::new(
                p.count("period"),
                p.count("lag"),
            )))
        },
    };

pub const HURST_RS_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "hurst_rs",
        aliases: &[],
        description: "Hurst exponent by rescaled-range analysis (Hurst 1951, Mandelbrot and Wallis 1969) of the one-bar log returns: about 0.5 for a random walk, above for persistent and below for anti-persistent returns; the plain R/S estimator has no small-sample correction and is biased upward on short chunks",
        params: &[
            super::definition::ParamSpec::count("period", 128, 8, 10000)
                .meaning("one-bar returns in the window"),
            super::definition::ParamSpec::count("min_chunk", 8, 4, 5000)
                .meaning("returns in the shortest chunk the range is rescaled over"),
        ],
        checks: &[super::definition::Check::AtMost {
            value: "min_chunk",
            times: 2,
            limit: "period",
        }],
        facts: super::definition::IndicatorFacts::unitless(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| {
            Ok(Box::new(HurstRsEngine::new(
                p.count("period"),
                p.count("min_chunk"),
            )))
        },
    };

pub const AR1_HALF_LIFE_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "ar1_half_life",
        aliases: &[],
        description: "Half-life of mean reversion in bars from a least-squares AR(1) fit with intercept to the raw closes: -ln 2 / ln(phi); extra phi is the coefficient; nothing is published when phi is at most 0 or within 1e-9 of 1",
        params: &[super::definition::ParamSpec::count("period", 100, 3, 10000)
            .meaning("closes in the window of the fit")],
        checks: &[],
        facts: super::definition::IndicatorFacts::unitless().non_negative(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| Ok(Box::new(Ar1HalfLifeEngine::new(p.count("period")))),
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, close: f64) -> Bar {
        Bar::new(t, close, close, close, close, 100.0)
    }

    fn run(engine: &mut dyn Indicator, closes: &[f64]) -> Vec<Option<IndicatorOutput>> {
        closes
            .iter()
            .enumerate()
            .map(|(i, c)| engine.on_bar(&bar(i as i64, *c)))
            .collect()
    }

    #[test]
    fn variance_ratio_by_hand() {
        // y = 0, 1, 1, 2, 2 (as log prices): returns 1, 0, 1, 0; N = 4, q = 2.
        // mu = 0.5; sigma_a = (0.25 * 4) / 3 = 1/3.
        // R_t = y_t - y_{t-2}, t = 2..4: 1, 1, 1; deviations from q*mu = 1: all 0 -> sigma_q = 0.
        let closes: Vec<f64> = [0.0f64, 1.0, 1.0, 2.0, 2.0]
            .iter()
            .map(|y| y.exp())
            .collect();
        let mut engine = VarianceRatioEngine::new(4, 2);
        let out = run(&mut engine, &closes);
        assert!(out[..4].iter().all(Option::is_none));
        let last = out[4].as_ref().unwrap();
        assert!(last.value.abs() < 1e-12);
        // z = sqrt(4) * (0 - 1) / sqrt(2 * 3 * 1 / 6) = -2.
        assert!((last.extra["z"] + 2.0).abs() < 1e-12);
    }

    #[test]
    fn variance_ratio_publishes_nothing_without_variance() {
        let mut engine = VarianceRatioEngine::new(4, 2);
        let out = run(&mut engine, &[5.0; 8]);
        assert!(out.iter().all(Option::is_none));
    }

    #[test]
    fn autocorrelation_of_alternating_returns_is_minus_one_ish() {
        // Returns +a, -a, +a, -a, +a, -a (N = 6): mean 0, num = -5 a^2, den = 6 a^2.
        let a: f64 = 0.01;
        let mut price = 100.0f64;
        let mut closes = vec![price];
        for i in 0..6 {
            price *= (if i % 2 == 0 { a } else { -a }).exp();
            closes.push(price);
        }
        let mut engine = AutocorrelationEngine::new(6, 1);
        let out = run(&mut engine, &closes);
        assert!(out[..6].iter().all(Option::is_none));
        assert!((out[6].as_ref().unwrap().value + 5.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn autocorrelation_publishes_nothing_for_a_flat_series() {
        let out = run(&mut AutocorrelationEngine::new(5, 1), &[3.0; 8]);
        assert!(out.iter().all(Option::is_none));
    }

    #[test]
    fn hurst_publishes_nothing_on_a_constant_series() {
        let mut engine = HurstRsEngine::new(16, 4);
        let out = run(&mut engine, &[7.0; 40]);
        assert!(out.iter().all(Option::is_none));
    }

    #[test]
    fn hurst_takes_chunks_from_the_newest_end() {
        // N = 8 returns, sizes 8, 4 (m = 4): two points; the oldest return of the four-chunks is
        // not used when N is not a multiple. Here N = 9: size 9 -> floor 9, 4 (9/2), chunk count 2
        // for size 4 leaves the oldest return out. Changing only that return must not change the
        // size-4 value, only the size-9 value.
        let base = [0.01, -0.02, 0.03, -0.01, 0.02, -0.03, 0.01, 0.02, -0.01];
        let mut changed = base;
        changed[0] = 0.05;
        let rs4 = |x: &[f64]| rescaled_range(x, 4).unwrap();
        assert_eq!(rs4(&base), rs4(&changed));
        let rs9 = |x: &[f64]| rescaled_range(x, 9).unwrap();
        assert_ne!(rs9(&base), rs9(&changed));
    }

    #[test]
    fn ar1_half_life_of_a_geometric_decay_is_one() {
        // y_t = 0.5 y_{t-1} + 10 has phi exactly 0.5 and half-life 1.
        let mut closes = vec![40.0f64];
        for _ in 0..30 {
            let last = *closes.last().unwrap();
            closes.push(0.5 * last + 10.0);
        }
        let mut engine = Ar1HalfLifeEngine::new(20);
        let out = run(&mut engine, &closes);
        assert!(out[..19].iter().all(Option::is_none));
        let last = out[30].as_ref().unwrap();
        assert!((last.extra["phi"] - 0.5).abs() < 1e-9);
        assert!((last.value - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ar1_half_life_publishes_nothing_for_a_trend_or_an_oscillation() {
        // A straight line has phi = 1; an alternation has phi < 0.
        let line: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
        let out = run(&mut Ar1HalfLifeEngine::new(6), &line);
        assert!(out.iter().all(Option::is_none));
        let zigzag: Vec<f64> = (0..10)
            .map(|i| if i % 2 == 0 { 99.0 } else { 101.0 })
            .collect();
        let out = run(&mut Ar1HalfLifeEngine::new(6), &zigzag);
        assert!(out.iter().all(Option::is_none));
    }

    #[test]
    fn ar1_half_life_ignores_rounding_noise_on_a_line() {
        // 100 + 0.1 i is not exactly representable: phi lands within 1e-15 of 1 and would give
        // half-lives of 10^14 bars without the margin.
        let line: Vec<f64> = (0..300).map(|i| 100.0 + 0.1 * i as f64).collect();
        for period in [20, 100] {
            let out = run(&mut Ar1HalfLifeEngine::new(period), &line);
            assert!(out.iter().all(Option::is_none), "period {period}");
        }
    }
}
