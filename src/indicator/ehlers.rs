//! Ehlers' filters and oscillators: UltimateSmoother, Roofing Filter, Reflex, Trendflex,
//! Correlation Trend, Laguerre RSI, Cyber Cycle and MAMA/FAMA.
//!
//! Every engine is written from the published formula; the conventions a formula leaves open
//! (start values, the bar of the first output, alerts) are stated on each type. Bars are counted
//! from 0 in the formulas, `warmup_period` is the 1-based number of the first bar with a value, and
//! every state runs from the first bar whether or not a value is published yet.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::definition::{Check, IndicatorDefinition, IndicatorFacts, ParamSpec, Verification};
use super::smoothing::SuperSmoother;
use super::smoothing::{crossed_over, crossed_under};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

use std::f64::consts::PI;

/// `(a1, c2, c3)` of the two-pole filters: `a1 = exp(-1.414 pi / period)`,
/// `c2 = 2 a1 cos(1.414 pi / period)`, `c3 = -a1^2`.
fn two_pole_coefficients(period: f64) -> (f64, f64, f64) {
    let a1 = (-1.414 * PI / period).exp();
    (a1, 2.0 * a1 * (1.414 * PI / period).cos(), -(a1 * a1))
}

/// The crossings of one line over another between two consecutive published bars, with the
/// crate's crossing tolerance ([`crossed_over`], [`crossed_under`]) so that rounding noise between
/// two nearly equal lines never fires an alert.
#[derive(Debug, Clone, Copy, Default)]
struct Crossing {
    prev: Option<(f64, f64)>,
    up: bool,
    down: bool,
}

impl Crossing {
    fn update(&mut self, a: f64, b: f64) {
        match self.prev {
            Some((pa, pb)) => {
                self.up = crossed_over(pa, pb, a, b);
                self.down = crossed_under(pa, pb, a, b);
            }
            None => {
                self.up = false;
                self.down = false;
            }
        }
        self.prev = Some((a, b));
    }

    fn alerts(&self, up: (&str, &str), down: (&str, &str)) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.up {
            out.push(IndicatorAlert {
                kind: up.0.to_string(),
                note: up.1.to_string(),
                strength: 1.0,
            });
        }
        if self.down {
            out.push(IndicatorAlert {
                kind: down.0.to_string(),
                note: down.1.to_string(),
                strength: 1.0,
            });
        }
        out
    }
}

fn zero_cross_alerts(c: &Crossing, label: &str) -> Vec<IndicatorAlert> {
    c.alerts(
        ("bull_zero_cross", &format!("{label} · CROSS ABOVE ZERO")),
        ("bear_zero_cross", &format!("{label} · CROSS BELOW ZERO")),
    )
}

// ---------------------------------------------------------------------------------------------
// UltimateSmoother
// ---------------------------------------------------------------------------------------------

/// Ehlers' UltimateSmoother over the closing price (*The Ultimate Smoother*, TASC April 2024).
///
/// With `a1 = exp(-1.414 pi / period)`, `c2 = 2 a1 cos(1.414 pi / period)`, `c3 = -a1^2` and
/// `c1 = (1 + c2 - c3) / 4`: the first three values are the closes, then
/// `US_i = (1 - c1) P_i + (2 c1 - c2) P_{i-1} - (c1 + c3) P_{i-2} + c2 US_{i-1} + c3 US_{i-2}`.
/// First output with the `period`-th bar.
#[derive(Debug, Clone)]
pub struct UltimateSmootherEngine {
    period: usize,
    c1: f64,
    c2: f64,
    c3: f64,
    p1: f64,
    p2: f64,
    u1: f64,
    u2: f64,
    count: usize,
}

impl UltimateSmootherEngine {
    pub fn new(period: usize) -> Self {
        let (_, c2, c3) = two_pole_coefficients(period as f64);
        Self {
            period,
            c1: (1.0 + c2 - c3) / 4.0,
            c2,
            c3,
            p1: 0.0,
            p2: 0.0,
            u1: 0.0,
            u2: 0.0,
            count: 0,
        }
    }
}

impl Indicator for UltimateSmootherEngine {
    fn name(&self) -> &str {
        "ultimate_smoother"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        *self = Self::new(self.period);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let p = bar.close;
        let us = if self.count < 3 {
            p
        } else {
            (1.0 - self.c1) * p + (2.0 * self.c1 - self.c2) * self.p1
                - (self.c1 + self.c3) * self.p2
                + self.c2 * self.u1
                + self.c3 * self.u2
        };
        self.count += 1;
        self.p2 = self.p1;
        self.p1 = p;
        self.u2 = self.u1;
        self.u1 = us;
        (self.count >= self.period).then(|| IndicatorOutput::new(us))
    }
}

// ---------------------------------------------------------------------------------------------
// Roofing Filter
// ---------------------------------------------------------------------------------------------

/// Ehlers' Roofing Filter over the closing price: a two-pole high-pass filter followed by a
/// SuperSmoother (*Cycle Analytics for Traders*, 2013, ch. 7).
///
/// `theta = 0.707 * 2 pi / hp_period`, `alpha = (cos theta + sin theta - 1) / cos theta`;
/// `HP_0 = HP_1 = 0`, then
/// `HP_i = (1 - alpha/2)^2 (P_i - 2 P_{i-1} + P_{i-2}) + 2 (1 - alpha) HP_{i-1} - (1 - alpha)^2 HP_{i-2}`.
/// The high-pass output runs through [`SuperSmoother`] with `lp_period` (first two values
/// passed through). First output with the `hp_period`-th bar; alerts on the crossings of zero.
#[derive(Debug, Clone)]
pub struct RoofingFilterEngine {
    hp_period: usize,
    lp_period: usize,
    alpha: f64,
    ss: SuperSmoother,
    p1: f64,
    p2: f64,
    hp1: f64,
    hp2: f64,
    count: usize,
    cross: Crossing,
}

impl RoofingFilterEngine {
    pub fn new(hp_period: usize, lp_period: usize) -> Self {
        let theta = 0.707 * 2.0 * PI / hp_period as f64;
        Self {
            hp_period,
            lp_period,
            alpha: (theta.cos() + theta.sin() - 1.0) / theta.cos(),
            ss: SuperSmoother::new(lp_period),
            p1: 0.0,
            p2: 0.0,
            hp1: 0.0,
            hp2: 0.0,
            count: 0,
            cross: Crossing::default(),
        }
    }
}

impl Indicator for RoofingFilterEngine {
    fn name(&self) -> &str {
        "roofing_filter"
    }

    fn warmup_period(&self) -> usize {
        self.hp_period
    }

    fn reset(&mut self) {
        *self = Self::new(self.hp_period, self.lp_period);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let p = bar.close;
        let a = self.alpha;
        let hp = if self.count < 2 {
            0.0
        } else {
            (1.0 - a / 2.0).powi(2) * (p - 2.0 * self.p1 + self.p2) + 2.0 * (1.0 - a) * self.hp1
                - (1.0 - a).powi(2) * self.hp2
        };
        let value = self.ss.update(hp);
        self.count += 1;
        self.p2 = self.p1;
        self.p1 = p;
        self.hp2 = self.hp1;
        self.hp1 = hp;
        if self.count < self.hp_period {
            return None;
        }
        self.cross.update(value, 0.0);
        Some(IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        zero_cross_alerts(&self.cross, "ROOFING")
    }
}

// ---------------------------------------------------------------------------------------------
// Reflex and Trendflex
// ---------------------------------------------------------------------------------------------

/// The part Reflex and Trendflex share: the SuperSmoother at half the length, the last
/// `length + 1` filter values and the normalising mean square.
#[derive(Debug, Clone)]
struct FlexCore {
    length: usize,
    reflex: bool,
    filter: SuperSmoother,
    history: VecDeque<f64>,
    ms: f64,
    count: usize,
    cross: Crossing,
}

impl FlexCore {
    fn new(length: usize, reflex: bool) -> Self {
        Self {
            length,
            reflex,
            filter: SuperSmoother::with_real_period(length as f64 / 2.0),
            history: VecDeque::with_capacity(length + 1),
            ms: 0.0,
            count: 0,
            cross: Crossing::default(),
        }
    }

    fn update(&mut self, price: f64) -> Option<f64> {
        let f = self.filter.update(price);
        if self.history.len() == self.length + 1 {
            self.history.pop_front();
        }
        self.history.push_back(f);
        self.count += 1;
        if self.count <= self.length {
            return None;
        }
        let n = self.length as f64;
        let last = self.history.len() - 1;
        let s = if self.reflex {
            let slope = (self.history[0] - f) / n;
            (1..=self.length)
                .map(|k| (f + k as f64 * slope) - self.history[last - k])
                .sum::<f64>()
                / n
        } else {
            (1..=self.length)
                .map(|k| f - self.history[last - k])
                .sum::<f64>()
                / n
        };
        let s = if s.abs() <= 1e-12 * f.abs().max(1.0) {
            0.0
        } else {
            s
        };
        self.ms = 0.04 * s * s + 0.96 * self.ms;
        let value = if self.ms > 0.0 {
            s / self.ms.sqrt()
        } else {
            0.0
        };
        self.cross.update(value, 0.0);
        Some(value)
    }
}

/// Ehlers' Reflex (*Reflex: A New Zero-Lag Indicator*, TASC February 2020) over the closing price.
///
/// A SuperSmoother `F` with the real period `length / 2` (first two values the closes); from bar
/// `length` on: `slope = (F_{i-length} - F_i) / length`,
/// `S_i = (1/length) sum_{k=1..length} ((F_i + k slope) - F_{i-k})`, `MS_i = 0.04 S_i^2 + 0.96 MS_{i-1}`
/// (`MS` starts at 0 on that bar) and `value = S_i / sqrt(MS_i)`, or 0 while `MS_i` is 0. The first
/// value is therefore ±5 or 0. First output with the `(length + 1)`-th bar; alerts on the
/// crossings of zero.
#[derive(Debug, Clone)]
pub struct ReflexEngine {
    core: FlexCore,
}

impl ReflexEngine {
    pub fn new(length: usize) -> Self {
        Self {
            core: FlexCore::new(length, true),
        }
    }
}

impl Indicator for ReflexEngine {
    fn name(&self) -> &str {
        "reflex"
    }

    fn warmup_period(&self) -> usize {
        self.core.length + 1
    }

    fn reset(&mut self) {
        self.core = FlexCore::new(self.core.length, true);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.core.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        zero_cross_alerts(&self.core.cross, "REFLEX")
    }
}

/// Ehlers' Trendflex, the companion of Reflex (TASC February 2020), over the closing price.
///
/// The same filter and normalisation as [`ReflexEngine`], with
/// `S_i = (1/length) sum_{k=1..length} (F_i - F_{i-k})` and no slope term. First output with the
/// `(length + 1)`-th bar; alerts on the crossings of zero.
#[derive(Debug, Clone)]
pub struct TrendflexEngine {
    core: FlexCore,
}

impl TrendflexEngine {
    pub fn new(length: usize) -> Self {
        Self {
            core: FlexCore::new(length, false),
        }
    }
}

impl Indicator for TrendflexEngine {
    fn name(&self) -> &str {
        "trendflex"
    }

    fn warmup_period(&self) -> usize {
        self.core.length + 1
    }

    fn reset(&mut self) {
        self.core = FlexCore::new(self.core.length, false);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.core.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        zero_cross_alerts(&self.core.cross, "TRENDFLEX")
    }
}

// ---------------------------------------------------------------------------------------------
// Correlation Trend
// ---------------------------------------------------------------------------------------------

/// Ehlers' Correlation Trend Indicator (*Correlation as a Trend Indicator*, TASC May 2020).
///
/// The Pearson correlation of the last `length` closes (`x_k = P_{i-k}`) with a straight line
/// rising with time (`y_k = -k`), from deviations about the means and clamped to -1..1; a window
/// of equal closes reads 0. First output with the `length`-th bar; alerts on the crossings of zero.
#[derive(Debug, Clone)]
pub struct CorrelationTrendEngine {
    length: usize,
    window: VecDeque<f64>,
    cross: Crossing,
}

impl CorrelationTrendEngine {
    pub fn new(length: usize) -> Self {
        Self {
            length,
            window: VecDeque::with_capacity(length),
            cross: Crossing::default(),
        }
    }
}

impl Indicator for CorrelationTrendEngine {
    fn name(&self) -> &str {
        "correlation_trend"
    }

    fn warmup_period(&self) -> usize {
        self.length
    }

    fn reset(&mut self) {
        *self = Self::new(self.length);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if self.window.len() == self.length {
            self.window.pop_front();
        }
        self.window.push_back(bar.close);
        if self.window.len() < self.length {
            return None;
        }
        let n = self.length;
        let first = self.window[0];
        let value = if self.window.iter().all(|&x| x == first) {
            0.0
        } else {
            // k = 0 is the newest close, at the back of the window.
            let x_mean = self.window.iter().sum::<f64>() / n as f64;
            let y_mean = -((n - 1) as f64) / 2.0;
            let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
            for k in 0..n {
                let dx = self.window[n - 1 - k] - x_mean;
                let dy = -(k as f64) - y_mean;
                sxy += dx * dy;
                sxx += dx * dx;
                syy += dy * dy;
            }
            (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0)
        };
        self.cross.update(value, 0.0);
        Some(IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        zero_cross_alerts(&self.cross, "CORR TREND")
    }
}

// ---------------------------------------------------------------------------------------------
// Laguerre RSI
// ---------------------------------------------------------------------------------------------

/// Ehlers' Laguerre RSI over the closing price (*Time Warp - Without Space Travel*, 2004;
/// *Cybernetic Analysis for Stocks and Futures*, 2004, ch. 14).
///
/// Four Laguerre stages seeded with the first close and updated in order from the second bar on,
/// each from the new value of the stage above and its own and that stage's previous values:
/// `L0 = (1 - g) P + g L0'`, `L1 = -g L0 + L0' + g L1'`, `L2 = -g L1 + L1' + g L2'`,
/// `L3 = -g L2 + L2' + g L3'`. `CU` is the sum of the positive and `CD` of the negative steps
/// down the stages; `value = CU / (CU + CD)` while the sum is positive, otherwise the previous
/// value is kept. There is no value until the sum has been positive once, so the first output
/// comes on the first bar that moves a stage (never before the second); the catalog declares the
/// warmup as 2. Alerts: `bull_extreme` when the value crosses above `lower`, `bear_extreme` when it
/// crosses below `upper`.
#[derive(Debug, Clone)]
pub struct LaguerreRsiEngine {
    gamma: f64,
    lower: f64,
    upper: f64,
    stages: [f64; 4],
    count: usize,
    value: Option<f64>,
    prev_value: Option<f64>,
    bull: bool,
    bear: bool,
}

impl LaguerreRsiEngine {
    pub fn new(gamma: f64, lower: f64, upper: f64) -> Self {
        Self {
            gamma,
            lower,
            upper,
            stages: [0.0; 4],
            count: 0,
            value: None,
            prev_value: None,
            bull: false,
            bear: false,
        }
    }
}

impl Indicator for LaguerreRsiEngine {
    fn name(&self) -> &str {
        "laguerre_rsi"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        *self = Self::new(self.gamma, self.lower, self.upper);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let p = bar.close;
        let g = self.gamma;
        self.bull = false;
        self.bear = false;
        if self.count == 0 {
            self.stages = [p; 4];
            self.count = 1;
            return None;
        }
        self.count += 1;
        let old = self.stages;
        let l0 = (1.0 - g) * p + g * old[0];
        let l1 = -g * l0 + old[0] + g * old[1];
        let l2 = -g * l1 + old[1] + g * old[2];
        let l3 = -g * l2 + old[2] + g * old[3];
        self.stages = [l0, l1, l2, l3];
        let (mut cu, mut cd) = (0.0, 0.0);
        for (a, b) in [(l0, l1), (l1, l2), (l2, l3)] {
            cu += (a - b).max(0.0);
            cd += (b - a).max(0.0);
        }
        if cu + cd > 0.0 {
            self.value = Some(cu / (cu + cd));
        }
        let value = self.value?;
        if let Some(prev) = self.prev_value {
            self.bull = crossed_over(prev, self.lower, value, self.lower);
            self.bear = crossed_under(prev, self.upper, value, self.upper);
        }
        self.prev_value = Some(value);
        Some(IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.bull {
            out.push(IndicatorAlert {
                kind: "bull_extreme".to_string(),
                note: "LAGUERRE RSI · CROSS ABOVE LOWER".to_string(),
                strength: 1.0,
            });
        }
        if self.bear {
            out.push(IndicatorAlert {
                kind: "bear_extreme".to_string(),
                note: "LAGUERRE RSI · CROSS BELOW UPPER".to_string(),
                strength: 1.0,
            });
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Cyber Cycle
// ---------------------------------------------------------------------------------------------

/// Ehlers' Cyber Cycle over `hl2` (*Cybernetic Analysis for Stocks and Futures*, 2004, ch. 4).
///
/// `Smooth_i = (P_i + 2 P_{i-1} + 2 P_{i-2} + P_{i-3}) / 6` from the fourth bar; `Cycle_0 = Cycle_1 = 0`,
/// `Cycle_i = (P_i - 2 P_{i-1} + P_{i-2}) / 4` for bars 2 to 5 and from bar 6
/// `(1 - a/2)^2 (Smooth_i - 2 Smooth_{i-1} + Smooth_{i-2}) + 2 (1 - a) Cycle_{i-1} - (1 - a)^2 Cycle_{i-2}`.
/// `value` is the cycle, `extra["trigger"]` the cycle of the previous bar. First output with bar
/// `max(7, ceil(2 / alpha) + 1)`; alerts `bull_cross` / `bear_cross` when the cycle crosses above /
/// below the trigger.
#[derive(Debug, Clone)]
pub struct CyberCycleEngine {
    alpha: f64,
    warmup: usize,
    price: [f64; 3],
    smooth: [f64; 2],
    cycle: [f64; 2],
    count: usize,
    cross: Crossing,
}

impl CyberCycleEngine {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            warmup: 7.max((2.0 / alpha).ceil() as usize + 1),
            price: [0.0; 3],
            smooth: [0.0; 2],
            cycle: [0.0; 2],
            count: 0,
            cross: Crossing::default(),
        }
    }
}

impl Indicator for CyberCycleEngine {
    fn name(&self) -> &str {
        "cyber_cycle"
    }

    fn warmup_period(&self) -> usize {
        self.warmup
    }

    fn reset(&mut self) {
        *self = Self::new(self.alpha);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let p = (bar.high + bar.low) / 2.0;
        let i = self.count;
        let a = self.alpha;
        let [p1, p2, p3] = self.price;
        let smooth = if i >= 3 {
            (p + 2.0 * p1 + 2.0 * p2 + p3) / 6.0
        } else {
            0.0
        };
        let cycle = if i < 2 {
            0.0
        } else if i <= 5 {
            (p - 2.0 * p1 + p2) / 4.0
        } else {
            (1.0 - a / 2.0).powi(2) * (smooth - 2.0 * self.smooth[0] + self.smooth[1])
                + 2.0 * (1.0 - a) * self.cycle[0]
                - (1.0 - a).powi(2) * self.cycle[1]
        };
        let trigger = self.cycle[0];
        self.count += 1;
        self.price = [p, p1, p2];
        self.smooth = [smooth, self.smooth[0]];
        self.cycle = [cycle, self.cycle[0]];
        if self.count < self.warmup {
            return None;
        }
        self.cross.update(cycle, trigger);
        let mut extra = HashMap::new();
        extra.insert("trigger".to_string(), trigger);
        Some(IndicatorOutput::with_extra(cycle, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.cross.alerts(
            ("bull_cross", "CYBER CYCLE · CROSS ABOVE TRIGGER"),
            ("bear_cross", "CYBER CYCLE · CROSS BELOW TRIGGER"),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// MAMA / FAMA
// ---------------------------------------------------------------------------------------------

/// Pushes `v` in front of a series history: `a[0]` is the current bar, `a[k]` the bar `k` back.
fn shift_in(a: &mut [f64; 7], v: f64) {
    a.rotate_right(1);
    a[0] = v;
}

/// The Hilbert-transform FIR of MAMA: `0.0962 s_i + 0.5769 s_{i-2} - 0.5769 s_{i-4} - 0.0962 s_{i-6}`.
fn hilbert(s: &[f64; 7]) -> f64 {
    0.0962 * s[0] + 0.5769 * s[2] - 0.5769 * s[4] - 0.0962 * s[6]
}

fn degrees(x: f64) -> f64 {
    x * 180.0 / PI
}

/// Ehlers' MESA Adaptive Moving Average with its follower FAMA over `hl2` (*MAMA - The Mother of
/// Adaptive Moving Averages*, TASC September 2001).
///
/// Follows the published code with these readings: Smooth equals the price before bar 3; every other
/// series (Detrender, Q1, I1, jI, jQ, I2, Q2, Re, Im, Period, Phase) is 0 before bar 6 and those
/// zeros are what later bars read as earlier values; the chain runs from bar 6, the first with seven
/// Smooth values (a zero price-level history would make the start depend on the price level);
/// MAMA and FAMA equal the price before bar 6; a period angle of exactly 0 keeps the previous
/// period; the angles from the one-argument `atan` are in degrees. `value` is MAMA, `extra["fama"]` FAMA,
/// `extra["period"]` the dominant-cycle period. First output with the 50th bar; alerts
/// `bull_cross` / `bear_cross` when MAMA crosses above / below FAMA.
#[derive(Debug, Clone)]
pub struct MamaEngine {
    fast_limit: f64,
    slow_limit: f64,
    price: [f64; 4],
    smooth: [f64; 7],
    detrender: [f64; 7],
    q1: [f64; 7],
    i1: [f64; 7],
    i2: f64,
    q2: f64,
    re: f64,
    im: f64,
    period: f64,
    phase: f64,
    mama: f64,
    fama: f64,
    count: usize,
    cross: Crossing,
}

impl MamaEngine {
    const WARMUP: usize = 50;

    pub fn new(fast_limit: f64, slow_limit: f64) -> Self {
        Self {
            fast_limit,
            slow_limit,
            price: [0.0; 4],
            smooth: [0.0; 7],
            detrender: [0.0; 7],
            q1: [0.0; 7],
            i1: [0.0; 7],
            i2: 0.0,
            q2: 0.0,
            re: 0.0,
            im: 0.0,
            period: 0.0,
            phase: 0.0,
            mama: 0.0,
            fama: 0.0,
            count: 0,
            cross: Crossing::default(),
        }
    }

    /// One bar of the recursion: `(mama, fama, period)`.
    fn step(&mut self, p: f64) -> (f64, f64, f64) {
        let i = self.count;
        self.count += 1;
        self.price.rotate_right(1);
        self.price[0] = p;
        let smooth = if i < 3 {
            p
        } else {
            (4.0 * p + 3.0 * self.price[1] + 2.0 * self.price[2] + self.price[3]) / 10.0
        };
        shift_in(&mut self.smooth, smooth);
        if i < 6 {
            self.mama = p;
            self.fama = p;
            return (p, p, self.period);
        }
        let prev_period = self.period;
        let k = 0.075 * prev_period + 0.54;
        shift_in(&mut self.detrender, hilbert(&self.smooth) * k);
        shift_in(&mut self.q1, hilbert(&self.detrender) * k);
        shift_in(&mut self.i1, self.detrender[3]);
        let ji = hilbert(&self.i1) * k;
        let jq = hilbert(&self.q1) * k;
        let i2 = 0.2 * (self.i1[0] - jq) + 0.8 * self.i2;
        let q2 = 0.2 * (self.q1[0] + ji) + 0.8 * self.q2;
        let re = 0.2 * (i2 * self.i2 + q2 * self.q2) + 0.8 * self.re;
        let im = 0.2 * (i2 * self.q2 - q2 * self.i2) + 0.8 * self.im;
        self.i2 = i2;
        self.q2 = q2;
        self.re = re;
        self.im = im;
        let angle = if im != 0.0 && re != 0.0 {
            degrees((im / re).atan())
        } else {
            0.0
        };
        let mut period = if angle != 0.0 {
            360.0 / angle
        } else {
            prev_period
        };
        if period > 1.5 * prev_period {
            period = 1.5 * prev_period;
        }
        if period < 0.67 * prev_period {
            period = 0.67 * prev_period;
        }
        period = period.clamp(6.0, 50.0);
        self.period = 0.2 * period + 0.8 * prev_period;
        let prev_phase = self.phase;
        if self.i1[0] != 0.0 {
            self.phase = degrees((self.q1[0] / self.i1[0]).atan());
        }
        let delta = (prev_phase - self.phase).max(1.0);
        let alpha = (self.fast_limit / delta).max(self.slow_limit);
        self.mama = alpha * p + (1.0 - alpha) * self.mama;
        self.fama = 0.5 * alpha * self.mama + (1.0 - 0.5 * alpha) * self.fama;
        (self.mama, self.fama, self.period)
    }
}

impl Indicator for MamaEngine {
    fn name(&self) -> &str {
        "mama"
    }

    fn warmup_period(&self) -> usize {
        Self::WARMUP
    }

    fn reset(&mut self) {
        *self = Self::new(self.fast_limit, self.slow_limit);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let (mama, fama, period) = self.step((bar.high + bar.low) / 2.0);
        if self.count < Self::WARMUP {
            return None;
        }
        self.cross.update(mama, fama);
        let mut extra = HashMap::new();
        extra.insert("fama".to_string(), fama);
        extra.insert("period".to_string(), period);
        Some(IndicatorOutput::with_extra(mama, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.cross.alerts(
            ("bull_cross", "MAMA · CROSS ABOVE FAMA"),
            ("bear_cross", "MAMA · CROSS BELOW FAMA"),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

/// Registry definition of `ultimate_smoother`.
pub const ULTIMATE_SMOOTHER_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "ultimate_smoother",
    aliases: &[],
    description: "Ehlers' UltimateSmoother, a two-pole smoother with unity gain at zero frequency and no lag on a straight line; the first three values are the closes, published from the `period`-th bar",
    params: &[ParamSpec::count("period", 20, 2, 10000).meaning("critical period of the filter in bars")],
    checks: &[],
    facts: IndicatorFacts::price(),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| Ok(Box::new(UltimateSmootherEngine::new(p.count("period")))),
};

/// Registry definition of `roofing_filter`.
pub const ROOFING_FILTER_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "roofing_filter",
    aliases: &[],
    description: "Ehlers' Roofing Filter: a two-pole high-pass filter (cutoff hp_period) followed by a SuperSmoother (lp_period); an oscillator around 0 on the price scale, published from the hp_period-th bar",
    params: &[
        ParamSpec::count("hp_period", 48, 3, 10000).meaning("cutoff period of the high-pass filter in bars"),
        ParamSpec::count("lp_period", 10, 2, 10000).meaning("cutoff period of the SuperSmoother in bars, shorter than hp_period"),
    ],
    checks: &[Check::Less {
        lower: "lp_period",
        upper: "hp_period",
    }],
    facts: IndicatorFacts::unitless().centered(0.0),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| {
        Ok(Box::new(RoofingFilterEngine::new(
            p.count("hp_period"),
            p.count("lp_period"),
        )))
    },
};

/// Registry definition of `reflex`.
pub const REFLEX_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "reflex",
    aliases: &[],
    description: "Ehlers' Reflex: the deviation of a SuperSmoother (period length / 2) from the line through its value `length` bars ago, normalised by its root mean square; published from bar length + 1",
    params: &[ParamSpec::count("length", 20, 2, 10000).meaning("bars the slope line spans")],
    checks: &[],
    facts: IndicatorFacts::unitless().centered(0.0),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| Ok(Box::new(ReflexEngine::new(p.count("length")))),
};

/// Registry definition of `trendflex`.
pub const TRENDFLEX_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "trendflex",
    aliases: &[],
    description: "Ehlers' Trendflex: the mean difference of a SuperSmoother (period length / 2) to its values over the last `length` bars, normalised by its root mean square; published from bar length + 1",
    params: &[ParamSpec::count("length", 20, 2, 10000).meaning("bars the difference spans")],
    checks: &[],
    facts: IndicatorFacts::unitless().centered(0.0),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| Ok(Box::new(TrendflexEngine::new(p.count("length")))),
};

/// Registry definition of `correlation_trend`.
pub const CORRELATION_TREND_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "correlation_trend",
    aliases: &[],
    description: "Ehlers' Correlation Trend Indicator: the Pearson correlation of the last `length` closes with a rising straight line; a window of equal closes reads 0",
    params: &[ParamSpec::count("length", 20, 2, 10000).meaning("closes correlated with the line")],
    checks: &[],
    facts: IndicatorFacts::unitless().bounded(-1.0, 1.0),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| Ok(Box::new(CorrelationTrendEngine::new(p.count("length")))),
};

/// Registry definition of `laguerre_rsi`.
pub const LAGUERRE_RSI_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "laguerre_rsi",
    aliases: &[],
    description: "Ehlers' Laguerre RSI: the share of upward steps among the steps between four Laguerre stages seeded with the first close; no value until a stage has moved, and the previous value is kept while all stages are level",
    params: &[
        ParamSpec::number("gamma", 0.5, 0.0, 0.99).meaning("damping of the Laguerre stages"),
        ParamSpec::number("lower", 0.2, 0.0, 1.0)
            .threshold()
            .meaning("lower zone, bull_extreme on a cross above it"),
        ParamSpec::number("upper", 0.8, 0.0, 1.0)
            .threshold()
            .meaning("upper zone, bear_extreme on a cross below it"),
    ],
    checks: &[Check::Less {
        lower: "lower",
        upper: "upper",
    }],
    facts: IndicatorFacts::ratio().bounded(0.0, 1.0),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| {
        Ok(Box::new(LaguerreRsiEngine::new(
            p.number("gamma"),
            p.number("lower"),
            p.number("upper"),
        )))
    },
};

/// Registry definition of `cyber_cycle`.
pub const CYBER_CYCLE_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "cyber_cycle",
    aliases: &[],
    description: "Ehlers' Cyber Cycle of the median price: a high-pass filtered cycle component with the previous bar's cycle as trigger (extra `trigger`)",
    params: &[ParamSpec::number("alpha", 0.07, 0.01, 1.0).meaning("smoothing constant of the cycle filter")],
    checks: &[],
    facts: IndicatorFacts::unitless().centered(0.0).native_hl2(),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| Ok(Box::new(CyberCycleEngine::new(p.number("alpha")))),
};

/// Registry definition of `mama`.
pub const MAMA_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "mama",
    aliases: &[],
    description: "Ehlers' MESA Adaptive Moving Average of the median price with its follower FAMA (extras `fama`, `period`); published from the 50th bar",
    params: &[
        ParamSpec::number("fast_limit", 0.5, 0.01, 1.0).meaning("largest smoothing constant"),
        ParamSpec::number("slow_limit", 0.05, 0.01, 1.0).meaning("smallest smoothing constant, below fast_limit"),
    ],
    checks: &[Check::Less {
        lower: "slow_limit",
        upper: "fast_limit",
    }],
    facts: IndicatorFacts::price().native_hl2(),
    verification: Verification::Open("no independent second implementation yet"),
    build: |p| {
        Ok(Box::new(MamaEngine::new(
            p.number("fast_limit"),
            p.number("slow_limit"),
        )))
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    const CLOSES: [f64; 10] = [10.0, 11.0, 13.0, 12.0, 15.0, 14.0, 16.0, 19.0, 17.0, 18.0];

    fn bar(p: f64) -> Bar {
        Bar::new(0, p, p, p, p, 0.0)
    }

    fn run(ind: &mut dyn Indicator, prices: &[f64]) -> Vec<Option<IndicatorOutput>> {
        prices.iter().map(|&p| ind.on_bar(&bar(p))).collect()
    }

    fn values(outs: &[Option<IndicatorOutput>]) -> Vec<Option<f64>> {
        outs.iter().map(|o| o.as_ref().map(|o| o.value)).collect()
    }

    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-10, "{got} != {want}");
    }

    /// Asserts the published values equal `want` from `first` on and that none comes before.
    fn assert_series(outs: &[Option<IndicatorOutput>], first: usize, want: &[f64]) {
        assert!(
            outs[..first].iter().all(Option::is_none),
            "value before bar {first}"
        );
        assert_eq!(outs.len() - first, want.len());
        for (o, w) in outs[first..].iter().zip(want) {
            assert_close(o.as_ref().expect("a value").value, *w);
        }
    }

    fn first_output(ind: &mut dyn Indicator, prices: &[f64]) -> Option<usize> {
        values(&run(ind, prices)).iter().position(Option::is_some)
    }

    fn rising(n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| 100.0 + i as f64 + ((i * 7) % 5) as f64)
            .collect()
    }

    /// Warmup is the 1-based bar of the first output, and reset reproduces the outputs.
    fn assert_warmup_and_reset(mut ind: Box<dyn Indicator>, bars: usize) {
        let prices = rising(bars);
        let first = run(ind.as_mut(), &prices);
        assert_eq!(
            first.iter().position(Option::is_some),
            Some(ind.warmup_period() - 1),
            "{}",
            ind.name()
        );
        ind.reset();
        let again = run(ind.as_mut(), &prices);
        assert_eq!(first, again, "{} after reset", ind.name());
    }

    #[test]
    fn warmup_is_the_first_output_and_reset_reproduces() {
        assert_warmup_and_reset(Box::new(UltimateSmootherEngine::new(20)), 120);
        assert_warmup_and_reset(Box::new(RoofingFilterEngine::new(48, 10)), 150);
        assert_warmup_and_reset(Box::new(ReflexEngine::new(20)), 120);
        assert_warmup_and_reset(Box::new(TrendflexEngine::new(20)), 120);
        assert_warmup_and_reset(Box::new(CorrelationTrendEngine::new(20)), 120);
        assert_warmup_and_reset(Box::new(LaguerreRsiEngine::new(0.5, 0.2, 0.8)), 120);
        assert_warmup_and_reset(Box::new(CyberCycleEngine::new(0.07)), 120);
        assert_warmup_and_reset(Box::new(MamaEngine::new(0.5, 0.05)), 150);
    }

    #[test]
    fn cyber_cycle_warmup_follows_alpha() {
        assert_eq!(CyberCycleEngine::new(0.07).warmup_period(), 30);
        assert_eq!(CyberCycleEngine::new(1.0).warmup_period(), 7);
        assert_eq!(CyberCycleEngine::new(0.5).warmup_period(), 7);
        assert_eq!(CyberCycleEngine::new(0.01).warmup_period(), 201);
    }

    #[test]
    fn constant_series_values() {
        let flat = [42.0; 120];
        let end = |ind: &mut dyn Indicator| {
            let outs = run(ind, &flat);
            outs.last().unwrap().clone().expect("a value")
        };
        // Smoothers return the level, the zero-mean filters 0.
        assert_close(end(&mut UltimateSmootherEngine::new(20)).value, 42.0);
        assert_close(end(&mut RoofingFilterEngine::new(48, 10)).value, 0.0);
        assert_close(end(&mut ReflexEngine::new(20)).value, 0.0);
        assert_close(end(&mut TrendflexEngine::new(20)).value, 0.0);
        assert_close(end(&mut CorrelationTrendEngine::new(20)).value, 0.0);
        let cyber = end(&mut CyberCycleEngine::new(0.07));
        assert_close(cyber.value, 0.0);
        assert_close(cyber.extra["trigger"], 0.0);
        let mama = end(&mut MamaEngine::new(0.5, 0.05));
        assert_close(mama.value, 42.0);
        assert_close(mama.extra["fama"], 42.0);
        // No stage ever moves, so the Laguerre RSI never has a value.
        assert_eq!(
            first_output(&mut LaguerreRsiEngine::new(0.5, 0.2, 0.8), &flat),
            None
        );
    }

    /// Period 5, evaluated from the formula of the specification (a1, c2, c3, c1 from the period,
    /// start values the first three closes) outside the crate.
    #[test]
    fn ultimate_smoother_matches_the_formula_by_hand() {
        let mut us = UltimateSmootherEngine::new(5);
        let outs = run(&mut us, &CLOSES);
        assert_series(
            &outs,
            4,
            &[
                13.968786030516096,
                14.938821842347835,
                15.395521595074573,
                18.105638090837633,
                18.748196738599873,
                17.7922296151153,
            ],
        );
        // Bar 3 with three equal closes before it: US_3 = (1 - c1) P_3.
        let (_, c2, c3) = two_pole_coefficients(2.0);
        let c1 = (1.0 + c2 - c3) / 4.0;
        let mut us = UltimateSmootherEngine::new(2);
        let outs = run(&mut us, &[0.0, 0.0, 0.0, 1.0]);
        assert_close(outs[3].as_ref().unwrap().value, 1.0 - c1);
    }

    /// `hp_period = 3, lp_period = 2`, closes 0, 0, 1: HP_2 = (1 - alpha/2)^2 with
    /// alpha = (cos t + sin t - 1) / cos t, t = 0.707 * 2 pi / 3, and the SuperSmoother
    /// passes HP_0 = HP_1 = 0 and gives c1 * HP_2 / 2 on bar 2.
    #[test]
    fn roofing_filter_matches_the_formula_by_hand() {
        let mut r = RoofingFilterEngine::new(3, 2);
        let outs = run(&mut r, &[0.0, 0.0, 1.0]);
        assert!(outs[0].is_none() && outs[1].is_none());
        assert_close(outs[2].as_ref().unwrap().value, 0.15605948171887346);

        let mut r = RoofingFilterEngine::new(6, 3);
        let outs = run(&mut r, &CLOSES);
        assert_series(
            &outs,
            5,
            &[
                0.026959332767123208,
                -0.23477817270282192,
                0.8240006535126453,
                -0.220609363846655,
                -0.9735119260283325,
            ],
        );
    }

    /// `length = 2` reduces to closed forms: the SuperSmoother period is 1, and on the first bar
    /// with a value `MS = 0.04 S^2`, so the value is `S / (0.2 |S|)`, that is ±5, and for
    /// Reflex `S = ((F_2 + F_0) / 2 - F_1) / 2`.
    #[test]
    fn first_value_of_reflex_and_trendflex_is_plus_or_minus_five() {
        // Closes 10, 11, 12: F_2 = 11.5034 (c1 = 1.00647, c2 = -0.00633, c3 = -0.000139), so
        // (F_2 + F_0) / 2 = 10.75 < F_1 = 11: S < 0.
        let mut r = ReflexEngine::new(2);
        assert_eq!(
            values(&run(&mut r, &[10.0, 11.0, 12.0])),
            vec![None, None, Some(-5.0)]
        );
        // Closes 11, 10, 11: F_2 = 10.5031, (F_2 + F_0) / 2 = 10.75 > F_1 = 10: S > 0.
        let mut r = ReflexEngine::new(2);
        assert_eq!(
            values(&run(&mut r, &[11.0, 10.0, 11.0])),
            vec![None, None, Some(5.0)]
        );
        // Trendflex: S = ((F_2 - F_1) + (F_2 - F_0)) / 2 = 1.5066 for closes 10, 11, 13.
        let mut t = TrendflexEngine::new(2);
        assert_eq!(
            values(&run(&mut t, &[10.0, 11.0, 13.0])),
            vec![None, None, Some(5.0)]
        );
    }

    /// `length = 4` (filter period 2), later bars evaluated from the formulas outside the crate.
    #[test]
    fn reflex_and_trendflex_match_the_formulas_by_hand() {
        let mut r = ReflexEngine::new(4);
        assert_series(
            &run(&mut r, &CLOSES),
            4,
            &[
                -5.0,
                1.783049240288659,
                -0.18167040294903347,
                4.967628250277325,
                -0.032057128886902285,
                -3.968623422986995,
            ],
        );
        let mut t = TrendflexEngine::new(4);
        assert_series(
            &run(&mut t, &CLOSES),
            4,
            &[
                5.0,
                3.6384690982599586,
                2.5315955246752235,
                3.7054900719394253,
                2.366982111007689,
                0.8812943310629662,
            ],
        );
    }

    #[test]
    fn correlation_trend_matches_the_formula_by_hand() {
        // Rising closes: x = 3, 2, 1 against y = 0, -1, -2: r = 1.
        let mut c = CorrelationTrendEngine::new(3);
        assert_eq!(
            values(&run(&mut c, &[1.0, 2.0, 3.0])),
            vec![None, None, Some(1.0)]
        );
        let mut c = CorrelationTrendEngine::new(3);
        assert_eq!(
            values(&run(&mut c, &[3.0, 2.0, 1.0])),
            vec![None, None, Some(-1.0)]
        );
        // Closes 1, 3, 2: x = 2, 3, 1, mean 2, deviations 0, 1, -1; y deviations 1, 0, -1;
        // sum of products 1, sums of squares 2 and 2: r = 1 / 2.
        let mut c = CorrelationTrendEngine::new(3);
        assert_close(
            run(&mut c, &[1.0, 3.0, 2.0])[2].as_ref().unwrap().value,
            0.5,
        );
    }

    #[test]
    fn laguerre_rsi_matches_the_formula_by_hand() {
        // gamma = 0 turns the stages into a delay line: L1_i = P_{i-1}, L2_i = P_{i-2}, L3_i = P_{i-3},
        // with the start values P_0. Closes 10, 12, 11: bar 1 has stages (12, 10, 10, 10), so
        // CU = 2, CD = 0 and the value is 1; bar 2 has (11, 12, 10, 10): CU = 2 (12 > 10),
        // CD = 1 (12 > 11), value 2 / 3.
        let mut l = LaguerreRsiEngine::new(0.0, 0.2, 0.8);
        let v = values(&run(&mut l, &[10.0, 12.0, 11.0]));
        assert_eq!(v[0], None);
        assert_close(v[1].unwrap(), 1.0);
        assert_close(v[2].unwrap(), 2.0 / 3.0);
        // Flat start: no value until a stage moves, then the first value is published.
        let mut l = LaguerreRsiEngine::new(0.0, 0.2, 0.8);
        let v = values(&run(&mut l, &[5.0, 5.0, 5.0, 7.0, 7.0]));
        assert_eq!(&v[..3], &[None, None, None]);
        assert_close(v[3].unwrap(), 1.0);
        // All stages level again (gamma 0, closes unchanged for four bars): the value is kept.
        let mut l = LaguerreRsiEngine::new(0.0, 0.2, 0.8);
        let v = values(&run(&mut l, &[5.0, 7.0, 7.0, 7.0, 7.0, 7.0]));
        assert_eq!(v[5], v[1]);
    }

    /// Gamma 0 and closes 10, 12, 11, 9, 8, 8, 8, 8, 12 give the values (from bar 1) 1, 2/3, 2/5,
    /// 0, 0, 0, 0 (kept while all stages are level), 1: the fall from 1 to 2/3 crosses below the
    /// upper zone 0.8, the jump from 0 to 1 crosses above the lower zone 0.2.
    #[test]
    fn laguerre_rsi_alerts_on_crossing_the_zones() {
        let mut l = LaguerreRsiEngine::new(0.0, 0.2, 0.8);
        let mut kinds = Vec::new();
        let mut vals = Vec::new();
        for p in [10.0, 12.0, 11.0, 9.0, 8.0, 8.0, 8.0, 8.0, 12.0] {
            vals.push(l.on_bar(&bar(p)).map(|o| o.value));
            kinds.push(l.alerts().into_iter().map(|a| a.kind).collect::<Vec<_>>());
        }
        let want = [1.0, 2.0 / 3.0, 0.4, 0.0, 0.0, 0.0, 0.0, 1.0];
        for (v, w) in vals[1..].iter().zip(want) {
            assert_close(v.unwrap(), w);
        }
        assert!(kinds[1].is_empty(), "the first value has no previous bar");
        assert_eq!(kinds[2], vec!["bear_extreme".to_string()]);
        for k in &kinds[3..8] {
            assert!(k.is_empty());
        }
        assert_eq!(kinds[8], vec!["bull_extreme".to_string()]);
    }

    /// With `alpha = 1` the recursion keeps only the second difference of the smoothed price:
    /// `Cycle_i = 0.25 (Smooth_i - 2 Smooth_{i-1} + Smooth_{i-2})`. For `P_i = i^2` the second
    /// difference of the price is 2, of `Smooth` too (`Smooth_i = i^2 - 3 i + 19/6`), so every
    /// cycle value from bar 2 is 0.5.
    #[test]
    fn cyber_cycle_matches_the_formula_by_hand() {
        let prices: Vec<f64> = (0..10).map(|i| (i * i) as f64).collect();
        let mut c = CyberCycleEngine::new(1.0);
        let outs = run(&mut c, &prices);
        assert_series(&outs, 6, &[0.5; 4]);
        for o in &outs[6..] {
            assert_close(o.as_ref().unwrap().extra["trigger"], 0.5);
        }
        // alpha = 0.5: (1 - a/2)^2 = 0.5625, 2 (1 - a) = 1, (1 - a)^2 = 0.25 and Cycle_4 = Cycle_5 = 0.5.
        // Bar 6: 0.5625 * 2 + 1 * 0.5 - 0.25 * 0.5 = 1.5; bar 7: 1.125 + 1.5 - 0.25 * 0.5 = 2.5.
        let mut c = CyberCycleEngine::new(0.5);
        let outs = run(&mut c, &prices);
        assert_close(outs[6].as_ref().unwrap().value, 1.5);
        assert_close(outs[6].as_ref().unwrap().extra["trigger"], 0.5);
        assert_close(outs[7].as_ref().unwrap().value, 2.5);
        assert_close(outs[7].as_ref().unwrap().extra["trigger"], 1.5);
    }

    /// High and low around the same median give the same cycle: the input is `(high + low) / 2`.
    #[test]
    fn cyber_cycle_reads_the_median_price() {
        let mut c = CyberCycleEngine::new(1.0);
        let mut last = None;
        for i in 0..7 {
            let m = (i * i) as f64;
            last = c.on_bar(&Bar::new(0, m, m + 1.0, m - 1.0, m + 0.5, 0.0));
        }
        assert_close(last.unwrap().value, 0.5);
    }

    /// Closes 0 (six times) then 1: Smooth_i = 0 before bar 6 and the chain starts on bar 6, where
    /// Smooth_6 = 0.4 (4 * 1 / 10), Detrender_6 = 0.0962 * 0.4 * 0.54, I1_6 = Detrender_3 = 0, so
    /// Re and Im stay 0 and the period falls back to 0, then to the floor 6: Period_6 = 0.2 * 6 = 1.2.
    /// Phase keeps 0, delta is raised to 1 and alpha = 0.5 / 1: MAMA_6 = 0.5, FAMA_6 = 0.25 * 0.5.
    #[test]
    fn mama_first_recursion_bar_by_hand() {
        let mut m = MamaEngine::new(0.5, 0.05);
        for p in [0.0; 6] {
            assert_eq!(m.step(p), (p, p, 0.0));
        }
        let (mama, fama, period) = m.step(1.0);
        assert_close(mama, 0.5);
        assert_close(fama, 0.125);
        assert_close(period, 1.2);
    }

    /// Closes 10, 11, 13, 12, 15, 14, 16, 19, 17: MAMA_5 = FAMA_5 = 14 (the chain starts on bar 6).
    /// Alpha stays 0.5 here (the phase moves by less than 1 degree): MAMA_6 = 0.5 * 16 + 7 = 15,
    /// MAMA_7 = 0.5 * 19 + 7.5 = 17, MAMA_8 = 0.5 * 17 + 8.5 = 17; FAMA_6 = 0.25 * 15 + 0.75 * 14 = 14.25,
    /// FAMA_7 = 0.25 * 17 + 0.75 * 14.25 = 14.9375, FAMA_8 = 0.25 * 17 + 0.75 * 14.9375 = 15.453125;
    /// the period steps 1.2, 2.16, 2.928 (0.2 * 6 + 0.8 * previous).
    #[test]
    fn mama_following_bars_by_hand() {
        let prices = [10.0, 11.0, 13.0, 12.0, 15.0, 14.0, 16.0, 19.0, 17.0];
        let mut m = MamaEngine::new(0.5, 0.05);
        let steps: Vec<_> = prices.iter().map(|&p| m.step(p)).collect();
        assert_eq!(steps[5], (14.0, 14.0, 0.0));
        let want = [
            (15.0, 14.25, 1.2),
            (17.0, 14.9375, 2.16),
            (17.0, 15.453125, 2.928),
        ];
        for (got, want) in steps[6..].iter().zip(want) {
            assert_close(got.0, want.0);
            assert_close(got.1, want.1);
            assert_close(got.2, want.2);
        }
    }

    #[test]
    fn flat_series_at_a_high_level_gives_zero_for_reflex_and_trendflex() {
        let flat = [1.0e6; 60];
        let mut r = ReflexEngine::new(20);
        let mut t = TrendflexEngine::new(20);
        for (r, t) in run(&mut r, &flat).into_iter().zip(run(&mut t, &flat)) {
            if let Some(r) = r {
                assert_eq!(r.value, 0.0);
            }
            if let Some(t) = t {
                assert_eq!(t.value, 0.0);
            }
        }
    }

    #[test]
    fn zero_cross_alerts_fire_on_the_bar_of_the_cross_only() {
        let mut c = CorrelationTrendEngine::new(3);
        let mut kinds = Vec::new();
        for p in [3.0, 2.0, 1.0, 2.0, 4.0, 5.0] {
            c.on_bar(&bar(p));
            kinds.push(c.alerts().into_iter().map(|a| a.kind).collect::<Vec<_>>());
        }
        // Windows: (3,2,1) -> -1, (2,1,2) -> 0 [x = 2,1,2; y = 0,-1,-2], (1,2,4) -> +0.9..
        assert!(kinds[2].is_empty(), "the first value has no previous bar");
        assert!(kinds[3].is_empty());
        assert_eq!(kinds[4], vec!["bull_zero_cross".to_string()]);
        assert!(kinds[5].is_empty());
    }
}
