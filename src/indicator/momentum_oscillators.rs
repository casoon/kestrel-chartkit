//! Momentum oscillators: Schaff Trend Cycle, QQE, DeMarker, Relative Momentum Index and the
//! squeeze momentum.
//!
//! Every engine is written from its published formula; the conventions a formula leaves open
//! (start values, the bar of the first output, alerts) are stated on each type. Bars are counted
//! from 0 in the formulas, `warmup_period` is the 1-based number of the first bar with a value, and
//! every state runs from the first bar whether or not a value is published yet. The small
//! building blocks at the top (seeded EMA, Wilder average, state changes) are shared with
//! [`super::trailing_lines`] and [`super::adaptive_averages`].

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::definition::{Check, IndicatorDefinition, IndicatorFacts, ParamSpec, Verification};
use super::ehlers::Crossing;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

// ---------------------------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------------------------

/// `EMA(n)`: `alpha = 2 / (n + 1)`, seeded with its first input, then
/// `E = alpha x + (1 - alpha) E_prev`.
#[derive(Debug, Clone)]
pub(crate) struct SeededEma {
    alpha: f64,
    value: Option<f64>,
}

impl SeededEma {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            alpha: 2.0 / (n as f64 + 1.0),
            value: None,
        }
    }

    pub(crate) fn update(&mut self, x: f64) -> f64 {
        let e = match self.value {
            None => x,
            Some(prev) => self.alpha * x + (1.0 - self.alpha) * prev,
        };
        self.value = Some(e);
        e
    }
}

/// `RMA(n)` (Wilder): no value for the first `n - 1` inputs; on the `n`-th the plain mean of the
/// first `n`; after that `R = (R_prev (n - 1) + x) / n`.
#[derive(Debug, Clone)]
pub(crate) struct WilderRma {
    n: usize,
    count: usize,
    sum: f64,
    value: Option<f64>,
}

impl WilderRma {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            n,
            count: 0,
            sum: 0.0,
            value: None,
        }
    }

    pub(crate) fn update(&mut self, x: f64) -> Option<f64> {
        self.count += 1;
        match self.value {
            None => {
                self.sum += x;
                if self.count == self.n {
                    self.value = Some(self.sum / self.n as f64);
                }
            }
            Some(prev) => {
                self.value = Some((prev * (self.n as f64 - 1.0) + x) / self.n as f64);
            }
        }
        self.value
    }
}

/// True range with `TR_0 = H_0 - L_0`.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrueRange {
    prev_close: Option<f64>,
}

impl TrueRange {
    pub(crate) fn update(&mut self, bar: &Bar) -> f64 {
        let tr = match self.prev_close {
            None => bar.high - bar.low,
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
        };
        self.prev_close = Some(bar.close);
        tr
    }
}

/// `ATR(n)`: the Wilder average of the true range from bar 0, first value on bar `n - 1`.
#[derive(Debug, Clone)]
pub(crate) struct Atr {
    tr: TrueRange,
    rma: WilderRma,
}

impl Atr {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            tr: TrueRange::default(),
            rma: WilderRma::new(n),
        }
    }

    pub(crate) fn update(&mut self, bar: &Bar) -> Option<f64> {
        let tr = self.tr.update(bar);
        self.rma.update(tr)
    }
}

/// Appends `v`, dropping the oldest value beyond `cap`.
pub(crate) fn push_capped(window: &mut VecDeque<f64>, v: f64, cap: usize) {
    if window.len() == cap {
        window.pop_front();
    }
    window.push_back(v);
}

/// Mean of a window, summed from the oldest to the newest.
pub(crate) fn window_mean(window: &VecDeque<f64>) -> f64 {
    window.iter().sum::<f64>() / window.len() as f64
}

/// The state of a direction on a bar that was published (or `None`); reports the state a bar has
/// changed to when the previous bar was published with a different one.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StateChange {
    prev: Option<i8>,
    pub(crate) to: Option<i8>,
}

impl StateChange {
    pub(crate) fn update(&mut self, state: Option<i8>) {
        self.to = match (self.prev, state) {
            (Some(p), Some(s)) if p != s => Some(s),
            _ => None,
        };
        self.prev = state;
    }

    pub(crate) fn alerts(&self, bull: &str, bear: &str) -> Vec<IndicatorAlert> {
        match self.to {
            Some(1) => vec![IndicatorAlert::new(
                "bull",
                format!("{bull} · TURNS UP"),
                1.0,
            )],
            Some(-1) => vec![IndicatorAlert::new(
                "bear",
                format!("{bear} · TURNS DOWN"),
                1.0,
            )],
            _ => Vec::new(),
        }
    }
}

/// The state label of a direction: `up`, `down`, or `undecided` before the first decision.
pub(crate) fn trend_state(trend: i8) -> &'static str {
    match trend {
        1 => "up",
        -1 => "down",
        _ => "undecided",
    }
}

fn alert(kind: &str, note: &str) -> IndicatorAlert {
    IndicatorAlert::new(kind, note, 1.0)
}

fn min_max(window: &VecDeque<f64>) -> (f64, f64) {
    window
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        })
}

// ---------------------------------------------------------------------------------------------
// Schaff Trend Cycle
// ---------------------------------------------------------------------------------------------

/// Schaff Trend Cycle over the closing price: the MACD line `EMA(fast) - EMA(slow)` run twice
/// through a stochastic of `cycle` bars, each smoothed with `PF = PF_prev + factor (K - PF_prev)`.
///
/// A stage whose window has no range (below `1e-12 max(1, |EMA(slow)|)` for the MACD, `1e-10`
/// for the first smoothed stage) repeats its previous stochastic, or when there is none 50 (first
/// stage) or the smoothed value itself (second stage, so a steadily falling series reads 0 and a
/// rising one 100); that keeps a flat series, where the MACD is rounding noise, from swinging
/// between 0 and 100. Both
/// EMAs are seeded with the first close and both smoothings with the first stochastic. First
/// output on bar `max(slow - 1, 2 (cycle - 1))`; alerts `bull` when the value crosses above
/// `lower` and `bear` when it crosses below `upper`.
#[derive(Debug, Clone)]
pub struct SchaffTrendCycleEngine {
    fast_n: usize,
    slow_n: usize,
    cycle: usize,
    factor: f64,
    lower: f64,
    upper: f64,
    fast: SeededEma,
    slow: SeededEma,
    macd: VecDeque<f64>,
    k1: Option<f64>,
    pf: Option<f64>,
    pf_window: VecDeque<f64>,
    k2: Option<f64>,
    stc: Option<f64>,
    count: usize,
    above_lower: Crossing,
    below_upper: Crossing,
}

impl SchaffTrendCycleEngine {
    pub fn new(
        fast: usize,
        slow: usize,
        cycle: usize,
        factor: f64,
        lower: f64,
        upper: f64,
    ) -> Self {
        Self {
            fast_n: fast,
            slow_n: slow,
            cycle,
            factor,
            lower,
            upper,
            fast: SeededEma::new(fast),
            slow: SeededEma::new(slow),
            macd: VecDeque::with_capacity(cycle),
            k1: None,
            pf: None,
            pf_window: VecDeque::with_capacity(cycle),
            k2: None,
            stc: None,
            count: 0,
            above_lower: Crossing::default(),
            below_upper: Crossing::default(),
        }
    }
}

impl Indicator for SchaffTrendCycleEngine {
    fn name(&self) -> &str {
        "schaff_trend_cycle"
    }

    fn warmup_period(&self) -> usize {
        self.slow_n.max(2 * self.cycle - 1)
    }

    fn reset(&mut self) {
        *self = Self::new(
            self.fast_n,
            self.slow_n,
            self.cycle,
            self.factor,
            self.lower,
            self.upper,
        );
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let i = self.count;
        self.count += 1;
        let ema_fast = self.fast.update(bar.close);
        let ema_slow = self.slow.update(bar.close);
        let macd = ema_fast - ema_slow;
        push_capped(&mut self.macd, macd, self.cycle);
        if i + 1 < self.cycle {
            return None;
        }
        let (lo, hi) = min_max(&self.macd);
        let k1 = if hi - lo > 1e-12 * ema_slow.abs().max(1.0) {
            100.0 * (macd - lo) / (hi - lo)
        } else {
            self.k1.unwrap_or(50.0)
        };
        self.k1 = Some(k1);
        let pf = match self.pf {
            None => k1,
            Some(prev) => prev + self.factor * (k1 - prev),
        };
        self.pf = Some(pf);
        push_capped(&mut self.pf_window, pf, self.cycle);
        if i < 2 * (self.cycle - 1) {
            return None;
        }
        let (lo2, hi2) = min_max(&self.pf_window);
        let k2 = if hi2 - lo2 > 1e-10 {
            100.0 * (pf - lo2) / (hi2 - lo2)
        } else {
            self.k2.unwrap_or(pf)
        };
        self.k2 = Some(k2);
        let stc = match self.stc {
            None => k2,
            Some(prev) => prev + self.factor * (k2 - prev),
        };
        self.stc = Some(stc);
        if self.count < self.warmup_period() {
            return None;
        }
        self.above_lower.update(stc, self.lower);
        self.below_upper.update(stc, self.upper);
        Some(IndicatorOutput::new(stc))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.above_lower.up {
            out.push(alert("bull", "SCHAFF · CROSS ABOVE LOWER"));
        }
        if self.below_upper.down {
            out.push(alert("bear", "SCHAFF · CROSS BELOW UPPER"));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// QQE
// ---------------------------------------------------------------------------------------------

/// The RSI of the close from the common rules: Wilder averages of the rises and falls from bar 1,
/// `100 U / (U + D)`, 50 when both are zero; first value on bar `n`.
#[derive(Debug, Clone)]
struct WilderRsi {
    prev: Option<f64>,
    up: WilderRma,
    down: WilderRma,
}

impl WilderRsi {
    fn new(n: usize) -> Self {
        Self {
            prev: None,
            up: WilderRma::new(n),
            down: WilderRma::new(n),
        }
    }

    fn update(&mut self, close: f64) -> Option<f64> {
        let prev = self.prev.replace(close)?;
        let d = close - prev;
        let u = self.up.update(d.max(0.0));
        let dn = self.down.update((-d).max(0.0));
        let (u, dn) = (u?, dn?);
        Some(if u + dn == 0.0 {
            50.0
        } else {
            100.0 * u / (u + dn)
        })
    }
}

/// Quantitative Qualitative Estimation over the closing price.
///
/// `M` is the EMA(`smoothing`) of the RSI(`rsi_len`), seeded on the RSI's first bar. The mean
/// absolute change of `M`, smoothed twice with EMA(`2 rsi_len - 1`), times `factor` is the band
/// width; the lower and upper bands ratchet toward `M` while `M` keeps to their side. The trend
/// turns up when `M` moves above the previous bar's upper band and down when it moves below the
/// previous lower band (the state form of "crosses the band"), and starts undecided (0, trail
/// `M`) instead of up. First output on bar `rsi_len + 1`; alerts `bull` and `bear` when the trend
/// turns to +1 and -1 from the other direction.
#[derive(Debug, Clone)]
pub struct QqeEngine {
    rsi_len: usize,
    smoothing: usize,
    factor: f64,
    rsi: WilderRsi,
    m_ema: SeededEma,
    s1: SeededEma,
    s2: SeededEma,
    prev_m: Option<f64>,
    bands: Option<(f64, f64)>,
    trend: i8,
    change: StateChange,
}

impl QqeEngine {
    pub fn new(rsi_len: usize, smoothing: usize, factor: f64) -> Self {
        let w = 2 * rsi_len - 1;
        Self {
            rsi_len,
            smoothing,
            factor,
            rsi: WilderRsi::new(rsi_len),
            m_ema: SeededEma::new(smoothing),
            s1: SeededEma::new(w),
            s2: SeededEma::new(w),
            prev_m: None,
            bands: None,
            trend: 0,
            change: StateChange::default(),
        }
    }
}

impl Indicator for QqeEngine {
    fn name(&self) -> &str {
        "qqe"
    }

    fn warmup_period(&self) -> usize {
        self.rsi_len + 2
    }

    fn reset(&mut self) {
        *self = Self::new(self.rsi_len, self.smoothing, self.factor);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        // No alert from an earlier bar survives into this one; the state itself is kept, so a
        // change is measured against the previous published bar.
        self.change.to = None;
        let rsi = self.rsi.update(bar.close)?;
        let m = self.m_ema.update(rsi);
        let prev_m = self.prev_m.replace(m)?;
        let a = (m - prev_m).abs();
        let dar = self.factor * self.s2.update(self.s1.update(a));
        let (nl, ns) = (m - dar, m + dar);
        match self.bands {
            None => self.bands = Some((nl, ns)),
            Some((long_prev, short_prev)) => {
                let long = if prev_m > long_prev && m > long_prev {
                    long_prev.max(nl)
                } else {
                    nl
                };
                let short = if prev_m < short_prev && m < short_prev {
                    short_prev.min(ns)
                } else {
                    ns
                };
                let tol_up = 1e-9 * m.abs().max(short_prev.abs()).max(1.0);
                let tol_down = 1e-9 * m.abs().max(long_prev.abs()).max(1.0);
                if self.trend != 1 && m > short_prev + tol_up {
                    self.trend = 1;
                } else if self.trend != -1 && m < long_prev - tol_down {
                    self.trend = -1;
                }
                self.bands = Some((long, short));
            }
        }
        let (long, short) = self.bands.unwrap_or((nl, ns));
        let trail = match self.trend {
            1 => long,
            -1 => short,
            _ => m,
        };
        self.change.update(Some(self.trend));
        let mut extra = HashMap::new();
        extra.insert("trail".to_string(), trail);
        Some(IndicatorOutput::with_extra(m, extra).with_state(trend_state(self.trend)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.change.alerts("QQE", "QQE")
    }
}

// ---------------------------------------------------------------------------------------------
// DeMarker
// ---------------------------------------------------------------------------------------------

/// DeMarker from the highs and lows: the sums over `period` bars of the rises of the high
/// (`SU`) and of the falls of the low (`SD`), `SU / (SU + SD)` (0.5 when both are zero), summed
/// from the oldest to the newest. First output on bar `period`; alerts `bull` when the value
/// crosses above `lower` and `bear` when it crosses below `upper`.
#[derive(Debug, Clone)]
pub struct DemarkerEngine {
    period: usize,
    lower: f64,
    upper: f64,
    prev: Option<(f64, f64)>,
    up: VecDeque<f64>,
    down: VecDeque<f64>,
    above_lower: Crossing,
    below_upper: Crossing,
}

impl DemarkerEngine {
    pub fn new(period: usize, lower: f64, upper: f64) -> Self {
        Self {
            period,
            lower,
            upper,
            prev: None,
            up: VecDeque::with_capacity(period),
            down: VecDeque::with_capacity(period),
            above_lower: Crossing::default(),
            below_upper: Crossing::default(),
        }
    }
}

impl Indicator for DemarkerEngine {
    fn name(&self) -> &str {
        "demarker"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.period, self.lower, self.upper);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let (ph, pl) = self.prev.replace((bar.high, bar.low))?;
        push_capped(&mut self.up, (bar.high - ph).max(0.0), self.period);
        push_capped(&mut self.down, (pl - bar.low).max(0.0), self.period);
        if self.up.len() < self.period {
            return None;
        }
        let su: f64 = self.up.iter().sum();
        let sd: f64 = self.down.iter().sum();
        let value = if su + sd == 0.0 { 0.5 } else { su / (su + sd) };
        self.above_lower.update(value, self.lower);
        self.below_upper.update(value, self.upper);
        Some(IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.above_lower.up {
            out.push(alert("bull", "DEMARKER · CROSS ABOVE LOWER"));
        }
        if self.below_upper.down {
            out.push(alert("bear", "DEMARKER · CROSS BELOW UPPER"));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Relative Momentum Index
// ---------------------------------------------------------------------------------------------

/// Relative Momentum Index of the close: the RSI form `100 U / (U + D)` over the change across
/// `momentum` bars instead of one (50 when both averages are zero); `U` and `D` are Wilder
/// averages over `length`, started with the change on bar `momentum`. With `momentum` 1 it is the
/// RSI. First output on bar `momentum + length - 1`; alerts `bull` when the value crosses above
/// `lower` and `bear` when it crosses below `upper`.
#[derive(Debug, Clone)]
pub struct RelativeMomentumIndexEngine {
    length: usize,
    momentum: usize,
    lower: f64,
    upper: f64,
    closes: VecDeque<f64>,
    up: WilderRma,
    down: WilderRma,
    above_lower: Crossing,
    below_upper: Crossing,
}

impl RelativeMomentumIndexEngine {
    pub fn new(length: usize, momentum: usize, lower: f64, upper: f64) -> Self {
        Self {
            length,
            momentum,
            lower,
            upper,
            closes: VecDeque::with_capacity(momentum + 1),
            up: WilderRma::new(length),
            down: WilderRma::new(length),
            above_lower: Crossing::default(),
            below_upper: Crossing::default(),
        }
    }
}

impl Indicator for RelativeMomentumIndexEngine {
    fn name(&self) -> &str {
        "relative_momentum_index"
    }

    fn warmup_period(&self) -> usize {
        self.momentum + self.length
    }

    fn reset(&mut self) {
        *self = Self::new(self.length, self.momentum, self.lower, self.upper);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        push_capped(&mut self.closes, bar.close, self.momentum + 1);
        if self.closes.len() <= self.momentum {
            return None;
        }
        let d = bar.close - self.closes[0];
        let u = self.up.update(d.max(0.0));
        let dn = self.down.update((-d).max(0.0));
        let (u, dn) = (u?, dn?);
        let value = if u + dn == 0.0 {
            50.0
        } else {
            100.0 * u / (u + dn)
        };
        self.above_lower.update(value, self.lower);
        self.below_upper.update(value, self.upper);
        Some(IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.above_lower.up {
            out.push(alert("bull", "RMI · CROSS ABOVE LOWER"));
        }
        if self.below_upper.down {
            out.push(alert("bear", "RMI · CROSS BELOW UPPER"));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Squeeze momentum
// ---------------------------------------------------------------------------------------------

/// Volatility squeeze with a linear-regression momentum.
///
/// The squeeze is on (+1) while the Bollinger bands (SMA(`length`) of the close plus and minus
/// `bb_mult` population deviations) lie inside the Keltner channel (SMA(`kc_length`) of the
/// close plus and minus `kc_mult` times the SMA of the true range), off (-1) while they lie
/// outside, else 0. The value is the endpoint of the least-squares line through the last
/// `kc_length` differences between the close and the mean of the window's midrange and the
/// SMA(`kc_length`) of the close; it is not a price. First output on bar
/// `max(2 kc_length - 2, length - 1)`; alerts `squeeze_fired` when the squeeze was on on the
/// previous bar and is not now, `bull` and `bear` when the value crosses above and below 0.
#[derive(Debug, Clone)]
pub struct SqueezeMomentumEngine {
    length: usize,
    bb_mult: f64,
    kc_length: usize,
    kc_mult: f64,
    closes: VecDeque<f64>,
    kc_closes: VecDeque<f64>,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    ranges: VecDeque<f64>,
    deltas: VecDeque<f64>,
    tr: TrueRange,
    prev_squeeze: Option<i8>,
    fired: bool,
    zero: Crossing,
}

impl SqueezeMomentumEngine {
    pub fn new(length: usize, bb_mult: f64, kc_length: usize, kc_mult: f64) -> Self {
        Self {
            length,
            bb_mult,
            kc_length,
            kc_mult,
            closes: VecDeque::with_capacity(length),
            kc_closes: VecDeque::with_capacity(kc_length),
            highs: VecDeque::with_capacity(kc_length),
            lows: VecDeque::with_capacity(kc_length),
            ranges: VecDeque::with_capacity(kc_length),
            deltas: VecDeque::with_capacity(kc_length),
            tr: TrueRange::default(),
            prev_squeeze: None,
            fired: false,
            zero: Crossing::default(),
        }
    }
}

/// Endpoint of the least-squares line through `y` (oldest to newest) at the newest position.
fn regression_endpoint(y: &VecDeque<f64>) -> f64 {
    let n = y.len() as f64;
    let x_mean = (n - 1.0) / 2.0;
    let y_mean = y.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (x, v) in y.iter().enumerate() {
        let dx = x as f64 - x_mean;
        sxy += dx * (v - y_mean);
        sxx += dx * dx;
    }
    y_mean + sxy / sxx * (n - 1.0 - x_mean)
}

impl Indicator for SqueezeMomentumEngine {
    fn name(&self) -> &str {
        "squeeze_momentum"
    }

    fn warmup_period(&self) -> usize {
        (2 * self.kc_length - 1).max(self.length)
    }

    fn reset(&mut self) {
        *self = Self::new(self.length, self.bb_mult, self.kc_length, self.kc_mult);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.fired = false;
        let tr = self.tr.update(bar);
        push_capped(&mut self.closes, bar.close, self.length);
        push_capped(&mut self.kc_closes, bar.close, self.kc_length);
        push_capped(&mut self.highs, bar.high, self.kc_length);
        push_capped(&mut self.lows, bar.low, self.kc_length);
        push_capped(&mut self.ranges, tr, self.kc_length);
        if self.kc_closes.len() < self.kc_length {
            self.prev_squeeze = None;
            return None;
        }
        let ma = window_mean(&self.kc_closes);
        let (_, max_h) = min_max(&self.highs);
        let (min_l, _) = min_max(&self.lows);
        let mid = ((max_h + min_l) / 2.0 + ma) / 2.0;
        push_capped(&mut self.deltas, bar.close - mid, self.kc_length);
        if self.deltas.len() < self.kc_length || self.closes.len() < self.length {
            self.prev_squeeze = None;
            return None;
        }
        let basis = window_mean(&self.closes);
        let variance = self
            .closes
            .iter()
            .map(|c| (c - basis) * (c - basis))
            .sum::<f64>()
            / self.length as f64;
        let sd = variance.sqrt();
        let (upper_bb, lower_bb) = (basis + self.bb_mult * sd, basis - self.bb_mult * sd);
        let rng = window_mean(&self.ranges);
        let (upper_kc, lower_kc) = (ma + self.kc_mult * rng, ma - self.kc_mult * rng);
        let squeeze: i8 = if lower_bb > lower_kc && upper_bb < upper_kc {
            1
        } else if lower_bb < lower_kc && upper_bb > upper_kc {
            -1
        } else {
            0
        };
        let value = regression_endpoint(&self.deltas);
        self.fired = self.prev_squeeze == Some(1) && squeeze != 1;
        self.prev_squeeze = Some(squeeze);
        self.zero.update(value, 0.0);
        let mut extra = HashMap::new();
        extra.insert("squeeze".to_string(), f64::from(squeeze));
        Some(IndicatorOutput::with_extra(value, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.fired {
            out.push(alert("squeeze_fired", "SQUEEZE · RELEASED"));
        }
        if self.zero.up {
            out.push(alert("bull", "SQUEEZE · MOMENTUM ABOVE ZERO"));
        }
        if self.zero.down {
            out.push(alert("bear", "SQUEEZE · MOMENTUM BELOW ZERO"));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

/// Registry definition of `schaff_trend_cycle`.
pub const SCHAFF_TREND_CYCLE_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "schaff_trend_cycle",
    aliases: &[],
    description: "Schaff Trend Cycle: the MACD line of the close run twice through a stochastic of `cycle` bars, each smoothed with `factor`; 0 to 100, published from bar max(slow, 2 * cycle - 1). A stage without range repeats its previous stochastic; at the start the first stage reads 50 and the second the smoothed value itself, so a flat series stays at 50, a steady fall reads 0 and a steady rise 100.",
    params: &[
        ParamSpec::count("fast", 23, 1, 10000).meaning("bars of the fast EMA in the MACD line"),
        ParamSpec::count("slow", 50, 2, 10000).meaning("bars of the slow EMA in the MACD line, above fast"),
        ParamSpec::count("cycle", 10, 2, 10000).meaning("bars of each stochastic window"),
        ParamSpec::number("factor", 0.5, 1e-9, 1.0).meaning("smoothing factor of each stage"),
        ParamSpec::number("lower", 25.0, 0.0, 100.0)
            .threshold()
            .meaning("lower zone, bull on a cross above it"),
        ParamSpec::number("upper", 75.0, 0.0, 100.0)
            .threshold()
            .meaning("upper zone, bear on a cross below it"),
    ],
    checks: &[
        Check::Less {
            lower: "fast",
            upper: "slow",
        },
        Check::Less {
            lower: "lower",
            upper: "upper",
        },
    ],
    facts: IndicatorFacts::ratio().bounded(0.0, 100.0),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(SchaffTrendCycleEngine::new(
            p.count("fast"),
            p.count("slow"),
            p.count("cycle"),
            p.number("factor"),
            p.number("lower"),
            p.number("upper"),
        )))
    },
};

/// Registry definition of `qqe`.
pub const QQE_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "qqe",
    aliases: &[],
    description: "Quantitative Qualitative Estimation: the EMA-smoothed RSI with a trailing line `factor` double-smoothed mean changes away (extra `trail`, state `up`/`down`/`undecided`); the trend starts undecided instead of up and turns when the smoothed RSI moves beyond the previous bar's opposite band. Published from bar rsi_len + 2.",
    params: &[
        ParamSpec::count("rsi_len", 14, 2, 10000).meaning("bars of the RSI"),
        ParamSpec::count("smoothing", 5, 1, 10000).meaning("bars of the EMA over the RSI"),
        ParamSpec::number("factor", 4.236, 1e-9, 1000.0).meaning("band width in smoothed mean changes of the line"),
    ],
    checks: &[],
    facts: IndicatorFacts::ratio().bounded(0.0, 100.0),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(QqeEngine::new(
            p.count("rsi_len"),
            p.count("smoothing"),
            p.number("factor"),
        )))
    },
};

/// Registry definition of `demarker`.
pub const DEMARKER_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "demarker",
    aliases: &[],
    description: "DeMarker: the sum of the rises of the high over the sum of the rises of the high and the falls of the low across `period` bars; 0 to 1, and 0.5 on a window without rises or falls. Published from bar period + 1.",
    params: &[
        ParamSpec::count("period", 14, 1, 10000).meaning("bars summed"),
        ParamSpec::number("lower", 0.3, 0.0, 1.0)
            .threshold()
            .meaning("lower zone, bull on a cross above it"),
        ParamSpec::number("upper", 0.7, 0.0, 1.0)
            .threshold()
            .meaning("upper zone, bear on a cross below it"),
    ],
    checks: &[Check::Less {
        lower: "lower",
        upper: "upper",
    }],
    facts: IndicatorFacts::ratio().bounded(0.0, 1.0).range_dependent(),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(DemarkerEngine::new(
            p.count("period"),
            p.number("lower"),
            p.number("upper"),
        )))
    },
};

/// Registry definition of `relative_momentum_index`.
pub const RELATIVE_MOMENTUM_INDEX_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "relative_momentum_index",
    aliases: &[],
    description: "Relative Momentum Index: the RSI form over the change across `momentum` bars instead of one, Wilder-averaged over `length` from the first change; 0 to 100, equal to the RSI with momentum 1. Published from bar momentum + length.",
    params: &[
        ParamSpec::count("length", 20, 1, 10000).meaning("bars of the Wilder averages"),
        ParamSpec::count("momentum", 5, 1, 10000).meaning("bars the change spans"),
        ParamSpec::number("lower", 30.0, 0.0, 100.0)
            .threshold()
            .meaning("lower zone, bull on a cross above it"),
        ParamSpec::number("upper", 70.0, 0.0, 100.0)
            .threshold()
            .meaning("upper zone, bear on a cross below it"),
    ],
    checks: &[Check::Less {
        lower: "lower",
        upper: "upper",
    }],
    facts: IndicatorFacts::ratio().bounded(0.0, 100.0),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(RelativeMomentumIndexEngine::new(
            p.count("length"),
            p.count("momentum"),
            p.number("lower"),
            p.number("upper"),
        )))
    },
};

/// Registry definition of `squeeze_momentum`.
pub const SQUEEZE_MOMENTUM_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "squeeze_momentum",
    aliases: &[],
    description: "Volatility squeeze with a linear-regression momentum: the endpoint of the least-squares line through the close's distance to the mean of the window midrange and its average (a price difference, extra `squeeze` +1 on, -1 off, 0 between). The Bollinger bands use bb_mult and the Keltner channel kc_mult. Published from bar max(2 * kc_length - 1, length).",
    params: &[
        ParamSpec::count("length", 20, 2, 10000).meaning("bars of the Bollinger bands"),
        ParamSpec::number("bb_mult", 2.0, 1e-9, 100.0).meaning("Bollinger width in population deviations"),
        ParamSpec::count("kc_length", 20, 2, 10000).meaning("bars of the Keltner channel and of the momentum"),
        ParamSpec::number("kc_mult", 1.5, 1e-9, 100.0).meaning("Keltner width in average true ranges"),
    ],
    checks: &[],
    facts: IndicatorFacts::unitless().centered(0.0).range_dependent(),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(SqueezeMomentumEngine::new(
            p.count("length"),
            p.number("bb_mult"),
            p.count("kc_length"),
            p.number("kc_mult"),
        )))
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::registry::build_checked;

    fn bar(h: f64, l: f64, c: f64) -> Bar {
        Bar::new(0, c, h, l, c, 0.0)
    }

    fn close_bar(c: f64) -> Bar {
        bar(c, c, c)
    }

    const CLOSES: [f64; 14] = [
        10.0, 11.0, 13.0, 12.0, 15.0, 14.0, 16.0, 19.0, 17.0, 18.0, 16.0, 15.0, 17.0, 20.0,
    ];

    #[test]
    fn demarker_of_a_flat_series_is_one_half() {
        let mut d = DemarkerEngine::new(3, 0.3, 0.7);
        let outs: Vec<_> = (0..6).map(|_| d.on_bar(&bar(5.0, 4.0, 4.5))).collect();
        assert!(outs[..3].iter().all(Option::is_none));
        for o in &outs[3..] {
            assert_eq!(o.as_ref().unwrap().value, 0.5);
        }
    }

    #[test]
    fn demarker_sums_rises_of_the_high_and_falls_of_the_low() {
        let mut d = DemarkerEngine::new(2, 0.3, 0.7);
        let bars = [
            bar(10.0, 9.0, 9.5),
            bar(12.0, 9.0, 11.0), // up 2, down 0
            bar(12.0, 7.0, 8.0),  // up 0, down 2
            bar(13.0, 7.0, 12.0), // up 1, down 0
        ];
        let outs: Vec<_> = bars.iter().map(|b| d.on_bar(b)).collect();
        assert!(outs[1].is_none());
        assert_eq!(outs[2].as_ref().unwrap().value, 0.5);
        assert!((outs[3].as_ref().unwrap().value - 1.0 / 3.0).abs() < 1e-15);
    }

    #[test]
    fn relative_momentum_index_with_momentum_one_is_the_rsi() {
        let mut rmi = RelativeMomentumIndexEngine::new(5, 1, 30.0, 70.0);
        let mut rsi = build_checked("rsi", &HashMap::from([("rsi_len".to_string(), 5.0)])).unwrap();
        for c in CLOSES {
            let b = close_bar(c);
            let (a, r) = (rmi.on_bar(&b), rsi.on_bar(&b));
            match (a, r) {
                (Some(a), Some(r)) => assert!((a.value - r.extra["raw"]).abs() < 1e-10),
                (None, None) => {}
                (a, r) => panic!("{a:?} / {r:?}"),
            }
        }
    }

    #[test]
    fn relative_momentum_index_starts_with_the_momentum_change() {
        let mut rmi = RelativeMomentumIndexEngine::new(2, 3, 30.0, 70.0);
        let outs: Vec<_> = CLOSES[..8]
            .iter()
            .map(|&c| rmi.on_bar(&close_bar(c)))
            .collect();
        // First change on bar 3, first value on bar 3 + 2 - 1 = 4 (warmup 5).
        assert_eq!(rmi.warmup_period(), 5);
        assert!(outs[..4].iter().all(Option::is_none));
        // changes: 2, 3, 3 (bars 3..5: 12-10, 15-11, 14-13 = 2, 4, 1) -> mean of 2 and 4 = 3 up, 0 down
        assert!((outs[4].as_ref().unwrap().value - 100.0).abs() < 1e-12);
    }

    #[test]
    fn schaff_trend_cycle_publishes_from_the_documented_bar_and_stays_in_range() {
        let mut stc = SchaffTrendCycleEngine::new(3, 6, 4, 0.5, 25.0, 75.0);
        assert_eq!(stc.warmup_period(), 7);
        let outs: Vec<_> = CLOSES.iter().map(|&c| stc.on_bar(&close_bar(c))).collect();
        assert!(outs[..6].iter().all(Option::is_none));
        for o in &outs[6..] {
            let v = o.as_ref().unwrap().value;
            assert!((0.0..=100.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn schaff_trend_cycle_of_a_flat_series_is_fifty() {
        let mut stc = SchaffTrendCycleEngine::new(3, 6, 4, 0.5, 25.0, 75.0);
        for _ in 0..30 {
            if let Some(o) = stc.on_bar(&close_bar(100.0)) {
                assert_eq!(o.value, 50.0);
            }
        }
    }

    #[test]
    fn schaff_trend_cycle_of_a_steady_line_reads_the_extreme() {
        for (step, want) in [(-1.0, 0.0), (1.0, 100.0)] {
            let mut stc = SchaffTrendCycleEngine::new(3, 6, 4, 0.5, 25.0, 75.0);
            let mut seen = 0;
            for i in 0..40 {
                if let Some(o) = stc.on_bar(&close_bar(100.0 + step * i as f64)) {
                    assert!((o.value - want).abs() < 1e-9, "{} vs {want}", o.value);
                    seen += 1;
                }
            }
            assert!(seen > 20);
        }
    }

    #[test]
    fn qqe_alerts_on_every_change_of_trend() {
        let mut q = build_checked("qqe", &HashMap::new()).unwrap();
        let (mut prev, mut changes, mut alerts) = (None, 0, 0);
        for i in 0..400 {
            let c = 100.0 + 10.0 * (f64::from(i) * 0.07).sin() + 0.5 * (f64::from(i) * 0.9).sin();
            let Some(out) = q.on_bar(&close_bar(c)) else {
                continue;
            };
            let trend = match out.state.as_deref() {
                Some("up") => 1.0,
                Some("down") => -1.0,
                _ => 0.0,
            };
            let fired = q.alerts();
            if let Some(p) = prev {
                if p != trend {
                    changes += 1;
                    let want = if trend > 0.0 { "bull" } else { "bear" };
                    assert_eq!(
                        fired.len(),
                        1,
                        "bar {i}: trend {p} -> {trend} without alert"
                    );
                    assert_eq!(fired[0].kind, want);
                }
            }
            if prev == Some(trend) {
                assert!(fired.is_empty(), "bar {i}: alert without a change of trend");
            }
            alerts += fired.len();
            prev = Some(trend);
        }
        assert!(
            changes >= 4,
            "the series has to turn the trend a few times ({changes})"
        );
        assert_eq!(alerts, changes);
    }

    #[test]
    fn qqe_value_is_the_smoothed_rsi_and_starts_undecided() {
        let mut qqe = QqeEngine::new(3, 2, 4.236);
        let outs: Vec<_> = CLOSES.iter().map(|&c| qqe.on_bar(&close_bar(c))).collect();
        assert_eq!(qqe.warmup_period(), 5);
        assert!(outs[..4].iter().all(Option::is_none));
        let first = outs[4].as_ref().unwrap();
        assert_eq!(first.state.as_deref(), Some("undecided"));
        assert_eq!(first.extra["trail"], first.value);
        for o in outs[4..].iter().flatten() {
            assert!((0.0..=100.0).contains(&o.value));
        }
    }

    #[test]
    fn squeeze_momentum_of_a_straight_line_is_zero_at_the_endpoint_offset() {
        // Closes on a rising line, high = low = close: the regression of delta is exact.
        let mut sq = SqueezeMomentumEngine::new(3, 2.0, 3, 1.5);
        assert_eq!(sq.warmup_period(), 5);
        let outs: Vec<_> = (0..8)
            .map(|i| sq.on_bar(&close_bar(10.0 + i as f64)))
            .collect();
        assert!(outs[..4].iter().all(Option::is_none));
        // delta_i = c_i - ((c_i + c_{i-2})/2 + mean)/2 = c_i - (c_i - 1 + c_i - 1)/2 = 1 on a line
        for o in outs[4..].iter().flatten() {
            assert!((o.value - 1.0).abs() < 1e-12, "{}", o.value);
            assert_eq!(o.extra["squeeze"], -1.0);
        }
    }

    #[test]
    fn state_change_needs_a_published_previous_bar() {
        let mut s = StateChange::default();
        s.update(Some(-1));
        s.update(Some(1));
        assert_eq!(s.to, Some(1));
        s.update(None);
        s.update(Some(-1));
        assert_eq!(s.to, None);
    }
}
