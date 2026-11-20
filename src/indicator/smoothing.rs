use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::kernels::{efficiency_ratio, rank_weight, RankTies};

/// How an [`Ema`] produces its first value.
///
/// The recurrence `alpha * src + (1 - alpha) * prev` with `alpha = 2/(len+1)` is the same in both
/// modes; only the value it starts from differs, and with it how long the series carries the
/// influence of that start. At `len == 1` (`alpha == 1`) both modes produce the same values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum EmaInit {
    /// Seeds with the input value itself and emits from the first sample on. The default, and the
    /// historical behaviour of this type.
    #[default]
    FirstSample,
    /// Seeds with the SMA of the first `len` samples and emits from the `len`-th sample on.
    /// Earlier samples produce no value — a partially accumulated average is not an EMA.
    Sma,
}

/// Exponential moving average over a scalar stream.
///
/// `EMA_t = alpha * src_t + (1 - alpha) * EMA_{t-1}` with `alpha = 2/(len + 1)`; the first value
/// comes from [`EmaInit`]. Returns `None` while the seed is not ready, which in the default
/// [`EmaInit::FirstSample`] mode never happens: there the very first sample is already the seed.
///
/// [`Ema::reset`] clears the state and any partially accumulated seed, so the next series starts
/// deterministically.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ema {
    len: usize,
    alpha: Option<f64>,
    init: EmaInit,
    state: Option<f64>,
    seed_sum: f64,
    seed_count: usize,
}

impl Ema {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            alpha: None,
            init: EmaInit::FirstSample,
            state: None,
            seed_sum: 0.0,
            seed_count: 0,
        }
    }

    /// An EMA with the smoothing constant stated directly instead of derived from a period.
    ///
    /// [`Ema::new`] derives `alpha = 2/(len + 1)`; this takes `alpha` as given (a definition such
    /// as `alpha = 2/len` is not the same average as `Ema::new(len)`). Seeded with the first sample;
    /// do not combine with [`EmaInit::Sma`], which needs a period.
    pub(crate) fn from_alpha(alpha: f64) -> Self {
        Self {
            alpha: Some(alpha),
            ..Self::new(1)
        }
    }

    /// Selects the initialisation; see [`EmaInit`].
    ///
    /// Additive to [`Ema::new`], which keeps the first-sample seed. Nested users of this type
    /// (MACD, TEMA, ...) are deliberately not switched over by this: a seed change inside a chain
    /// is a separate contract question per indicator.
    pub fn with_init(mut self, init: EmaInit) -> Self {
        self.init = init;
        self
    }

    pub fn init(&self) -> EmaInit {
        self.init
    }

    pub fn update(&mut self, src: f64) -> Option<f64> {
        let alpha = self.alpha.unwrap_or_else(|| 2.0 / (self.len as f64 + 1.0));
        if let Some(prev) = self.state {
            let next = alpha * src + (1.0 - alpha) * prev;
            self.state = Some(next);
            return Some(next);
        }

        let seed = match self.init {
            EmaInit::FirstSample => src,
            EmaInit::Sma => {
                self.seed_sum += src;
                self.seed_count += 1;
                if self.seed_count < self.len {
                    return None;
                }
                self.seed_sum / self.len as f64
            }
        };
        self.state = Some(seed);
        Some(seed)
    }

    /// Bars needed before [`Ema::update`] first returns `Some`.
    pub fn warmup_period(&self) -> usize {
        match self.init {
            EmaInit::FirstSample => 0,
            EmaInit::Sma => self.len,
        }
    }

    pub fn reset(&mut self) {
        self.state = None;
        self.seed_sum = 0.0;
        self.seed_count = 0;
    }
}

/// Mean of the last `len` values of `values`, `None` if fewer are given.
///
/// One-shot form of [`Sma`] for snapshot read-outs over a slice: the values pass through a fresh
/// kernel, so the result is the same number the streaming indicator publishes.
pub(crate) fn sma_of_last(values: impl DoubleEndedIterator<Item = f64>, len: usize) -> Option<f64> {
    let mut window: Vec<f64> = values.rev().take(len).collect();
    if window.len() < len {
        return None;
    }
    window.reverse();
    let mut sma = Sma::new(len);
    window.into_iter().filter_map(|v| sma.update(v)).last()
}

/// Weighted moving average for scalar streams: weight `len` on the most recent sample
#[derive(Debug, Clone)]
pub struct Wma {
    len: usize,
    window: VecDeque<f64>,
}

impl Wma {
    pub fn new(len: usize) -> Self {
        Self {
            len: len.max(1),
            window: VecDeque::with_capacity(len),
        }
    }

    pub fn update(&mut self, src: f64) -> Option<f64> {
        self.window.push_back(src);
        if self.window.len() > self.len {
            self.window.pop_front();
        }
        if self.window.len() < self.len {
            return None;
        }

        let denom = (self.len * (self.len + 1)) as f64 / 2.0;
        let mut sum = 0.0;
        for (i, &val) in self.window.iter().enumerate() {
            sum += val * (i + 1) as f64;
        }
        Some(sum / denom)
    }

    pub fn reset(&mut self) {
        self.window.clear();
    }
}

/// Wilder smoothing (`alpha = 1/len`): seeds with the SMA of the first `len` samples
#[derive(Debug, Clone)]
pub struct Rma {
    len: usize,
    seed: VecDeque<f64>,
    state: Option<f64>,
}

impl Rma {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            seed: VecDeque::with_capacity(len),
            state: None,
        }
    }

    pub fn update(&mut self, src: f64) -> Option<f64> {
        if let Some(prev) = self.state {
            let alpha = 1.0 / self.len as f64;
            let next = alpha * src + (1.0 - alpha) * prev;
            self.state = Some(next);
            return Some(next);
        }
        self.seed.push_back(src);
        if self.seed.len() < self.len {
            return None;
        }
        let sma = self.seed.iter().sum::<f64>() / self.len as f64;
        self.state = Some(sma);
        Some(sma)
    }

    pub fn reset(&mut self) {
        self.seed.clear();
        self.state = None;
    }
}

/// Plain windowed average
#[derive(Debug, Clone)]
pub struct Sma {
    len: usize,
    window: VecDeque<f64>,
    sum: f64,
}

impl Sma {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            window: VecDeque::with_capacity(len),
            sum: 0.0,
        }
    }

    pub fn update(&mut self, src: f64) -> Option<f64> {
        self.window.push_back(src);
        self.sum += src;
        if self.window.len() > self.len {
            self.sum -= self.window.pop_front().unwrap();
        }
        if self.window.len() < self.len {
            return None;
        }
        Some(self.sum / self.len as f64)
    }

    pub fn reset(&mut self) {
        self.window.clear();
        self.sum = 0.0;
    }
}

/// Rolling window for finding highest and lowest values
#[derive(Debug, Clone)]
pub(crate) struct ExtremeWindow {
    len: usize,
    window: VecDeque<f64>,
}

impl ExtremeWindow {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            len,
            window: VecDeque::with_capacity(len),
        }
    }

    pub(crate) fn push(&mut self, value: f64) -> Option<(f64, f64)> {
        if self.window.len() == self.len {
            self.window.pop_front();
        }
        self.window.push_back(value);
        if self.window.len() < self.len {
            return None;
        }
        let lowest = self.window.iter().cloned().fold(f64::INFINITY, f64::min);
        let highest = self
            .window
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        Some((lowest, highest))
    }

    pub(crate) fn reset(&mut self) {
        self.window.clear();
    }
}

/// Rolling percent rank: the share (in %) of the previous `n` values
/// that are `<=` the newest one.
#[derive(Debug, Clone)]
pub(crate) struct PercentRank {
    n: usize,
    window: VecDeque<Option<f64>>,
}

impl PercentRank {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            n,
            window: VecDeque::with_capacity(n + 2),
        }
    }

    /// Feeds one value (`None` = not available); `None` until the newest value and the `n`
    /// before it all exist.
    pub(crate) fn update(&mut self, v: Option<f64>) -> Option<f64> {
        self.window.push_back(v);
        if self.window.len() > self.n + 1 {
            self.window.pop_front();
        }
        if self.window.len() < self.n + 1 {
            return None;
        }
        let cur = v?;
        if self.window.iter().take(self.n).any(|p| p.is_none()) {
            return None;
        }
        let previous = self.window.iter().take(self.n).flatten().copied();
        let weight = rank_weight(previous, cur, RankTies::AtOrBelow);
        Some(100.0 * weight / self.n as f64)
    }
}

/// Relative distance below which two lines count as touching rather than crossed.
///
/// Without it a crossing fires on floating-point noise: a %K line clamped at 0 that sits at
/// `1.6e-13` in one run and `0.0` in another "crosses" its flat signal line in one and not the
/// other — an alert that depends on rounding, and on how much history preceded the bar. Scaled by
/// the magnitude of the lines (at least 1), so it means the same on a 0–100 oscillator and on a
/// price.
pub const CROSS_TOLERANCE_REL: f64 = 1e-9;

fn cross_tolerance(values: [f64; 4]) -> f64 {
    CROSS_TOLERANCE_REL * values.iter().fold(1.0_f64, |m, v| m.max(v.abs()))
}

/// `a` was at or below `b` and is now clearly above it.
pub fn crossed_over(prev_a: f64, prev_b: f64, a: f64, b: f64) -> bool {
    let tol = cross_tolerance([prev_a, prev_b, a, b]);
    prev_a <= prev_b + tol && a > b + tol
}

/// `a` was at or above `b` and is now clearly below it.
pub fn crossed_under(prev_a: f64, prev_b: f64, a: f64, b: f64) -> bool {
    let tol = cross_tolerance([prev_a, prev_b, a, b]);
    prev_a >= prev_b - tol && a < b - tol
}

/// The one list of moving-average kinds that can be chosen as a smoother parameter.
///
/// Every place that offers a choice of average reads it from here: the `fast_kind`/`slow_kind`
/// of `trend_relationship`, the `ma_kind` of `price_average_relation` and `roc_context`, the
/// smoothing of `atr` and `rsi`, and the trend read-out of `analytics`. Each variant is named
/// like its catalog entry ([`SmootherKind::name`]), which is also the name the typed parameter
/// maps accept. A parameter that deliberately offers only some of them states a subset of
/// [`SmootherKind::ALL`] (see [`TrueRangeSmoothing`](super::atr::TrueRangeSmoothing),
/// [`RsiSmoothing`](super::rsi::RsiSmoothing)) instead of keeping a list of its own.
///
/// The position in [`SmootherKind::ALL`] is the numeric code of the `f64` parameter maps
/// (`ma_kind = 3` is `alma`); new kinds are appended, so existing codes keep their meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum SmootherKind {
    #[default]
    Ema,
    Sma,
    Rma,
    Alma,
    Jma,
    SuperSmoother,
    Kama,
    Wma,
}

/// The catalog names of a subset of [`SmootherKind`], in the subset's order.
pub(crate) const fn smoother_names<const N: usize>(kinds: [SmootherKind; N]) -> [&'static str; N] {
    let mut names = [""; N];
    let mut i = 0;
    while i < N {
        names[i] = kinds[i].name();
        i += 1;
    }
    names
}

impl SmootherKind {
    /// Every kind, in code order.
    pub const ALL: [SmootherKind; 8] = [
        SmootherKind::Ema,
        SmootherKind::Sma,
        SmootherKind::Rma,
        SmootherKind::Alma,
        SmootherKind::Jma,
        SmootherKind::SuperSmoother,
        SmootherKind::Kama,
        SmootherKind::Wma,
    ];

    /// [`SmootherKind::name`] of every kind, in the order of [`SmootherKind::ALL`]: the options of
    /// a choice parameter that offers every kind.
    pub const NAMES: [&'static str; 8] = smoother_names(Self::ALL);

    /// The name of the catalog entry that computes this average.
    pub const fn name(self) -> &'static str {
        match self {
            SmootherKind::Ema => "ema",
            SmootherKind::Sma => "sma",
            SmootherKind::Rma => "rma",
            SmootherKind::Alma => "alma",
            SmootherKind::Jma => "jma",
            SmootherKind::SuperSmoother => "super_smoother",
            SmootherKind::Kama => "kama",
            SmootherKind::Wma => "wma",
        }
    }

    /// The kind with this catalog name (case-insensitive).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.name().eq_ignore_ascii_case(name))
    }

    /// The kind at this position of [`SmootherKind::ALL`], the code of the `f64` parameter maps.
    pub fn from_code(code: usize) -> Option<Self> {
        Self::ALL.get(code).copied()
    }

    /// The position of this kind in [`SmootherKind::ALL`].
    pub fn code(self) -> usize {
        Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .expect("every kind is in ALL")
    }

    /// Builds a boxed [`Smoother`] of this kind with `len` bars of lookback/decay, using common
    /// defaults for `Alma` (`offset = 0.85`, `sigma = 6.0`), `Jma` (`phase = 0.0`,
    /// `power = 2.0`) and `Kama` (Kaufman's own `fast_period = 2`, `slow_period = 30`). Use the
    /// concrete constructors directly to override those.
    pub fn build(self, len: usize) -> Box<dyn Smoother> {
        match self {
            SmootherKind::Ema => Box::new(Ema::new(len)),
            SmootherKind::Sma => Box::new(Sma::new(len)),
            SmootherKind::Rma => Box::new(Rma::new(len)),
            SmootherKind::Alma => Box::new(Alma::new(len, 0.85, 6.0)),
            SmootherKind::Jma => Box::new(Jma::new(len, 0.0, 2.0)),
            SmootherKind::SuperSmoother => Box::new(SuperSmoother::new(len)),
            SmootherKind::Kama => Box::new(Kama::with_defaults(len)),
            SmootherKind::Wma => Box::new(Wma::new(len)),
        }
    }
}

/// Common contract for streaming smoothers, letting them be
/// selected dynamically ([`SmootherKind::build`]) regardless of concrete type.
pub trait Smoother: Send + Sync {
    /// Feeds one value. Returns `None` while still inside this smoother's own warmup.
    fn update(&mut self, src: f64) -> Option<f64>;
    fn reset(&mut self);
    /// Bars needed before this smoother first returns `Some`. `0` for smoothers that emit from
    /// the first sample (`Ema`, `Jma`).
    fn warmup_period(&self) -> usize {
        0
    }
}

impl Smoother for Ema {
    fn update(&mut self, src: f64) -> Option<f64> {
        Ema::update(self, src)
    }
    fn reset(&mut self) {
        Ema::reset(self)
    }
    fn warmup_period(&self) -> usize {
        Ema::warmup_period(self)
    }
}

impl Smoother for Sma {
    fn update(&mut self, src: f64) -> Option<f64> {
        Sma::update(self, src)
    }
    fn reset(&mut self) {
        Sma::reset(self)
    }
    fn warmup_period(&self) -> usize {
        self.len
    }
}

impl Smoother for Rma {
    fn update(&mut self, src: f64) -> Option<f64> {
        Rma::update(self, src)
    }
    fn reset(&mut self) {
        Rma::reset(self)
    }
    fn warmup_period(&self) -> usize {
        self.len
    }
}

impl Smoother for Wma {
    fn update(&mut self, src: f64) -> Option<f64> {
        Wma::update(self, src)
    }
    fn reset(&mut self) {
        Wma::reset(self)
    }
    fn warmup_period(&self) -> usize {
        self.len
    }
}

impl Smoother for Alma {
    fn update(&mut self, src: f64) -> Option<f64> {
        Alma::update(self, src)
    }
    fn reset(&mut self) {
        Alma::reset(self)
    }
    fn warmup_period(&self) -> usize {
        self.len
    }
}

impl Smoother for Jma {
    fn update(&mut self, src: f64) -> Option<f64> {
        Some(Jma::update(self, src))
    }
    fn reset(&mut self) {
        Jma::reset(self)
    }
}

impl Smoother for SuperSmoother {
    fn update(&mut self, src: f64) -> Option<f64> {
        Some(SuperSmoother::update(self, src))
    }
    fn reset(&mut self) {
        SuperSmoother::reset(self)
    }
}

impl Smoother for Kama {
    fn update(&mut self, src: f64) -> Option<f64> {
        Kama::update(self, src)
    }
    fn reset(&mut self) {
        Kama::reset(self)
    }
    fn warmup_period(&self) -> usize {
        self.period + 1
    }
}

/// Arnaud Legoux Moving Average (ALMA)
#[derive(Debug, Clone)]
pub struct Alma {
    len: usize,
    offset: f64,
    sigma: f64,
    window: VecDeque<f64>,
    weights: Vec<f64>,
    sum_weights: f64,
}

impl Alma {
    pub fn new(len: usize, offset: f64, sigma: f64) -> Self {
        let len = len.max(1);
        let m = offset * (len - 1) as f64;
        let s = (len as f64 / sigma).max(1e-6);

        let mut weights = Vec::with_capacity(len);
        let mut sum_weights = 0.0;
        for i in 0..len {
            let w = (-(i as f64 - m).powi(2) / (2.0 * s * s)).exp();
            weights.push(w);
            sum_weights += w;
        }

        Self {
            len,
            offset,
            sigma,
            window: VecDeque::with_capacity(len),
            weights,
            sum_weights,
        }
    }

    pub fn offset(&self) -> f64 {
        self.offset
    }

    pub fn sigma(&self) -> f64 {
        self.sigma
    }

    pub fn update(&mut self, src: f64) -> Option<f64> {
        self.window.push_back(src);
        if self.window.len() > self.len {
            self.window.pop_front();
        }
        if self.window.len() < self.len {
            return None;
        }

        let mut weighted_sum = 0.0;
        for (i, &val) in self.window.iter().enumerate() {
            weighted_sum += val * self.weights[i];
        }
        Some(weighted_sum / self.sum_weights)
    }

    pub fn reset(&mut self) {
        self.window.clear();
    }
}

/// Open Jurik-style moving-average approximation.
#[derive(Debug, Clone)]
pub struct Jma {
    len: usize,
    phase: f64,
    power: f64,
    e0: f64,
    e1: f64,
    e2: f64,
    jma: f64,
    initialized: bool,
}

impl Jma {
    pub fn new(len: usize, phase: f64, power: f64) -> Self {
        Self {
            len: len.max(1),
            phase: phase.clamp(-100.0, 100.0),
            power: power.max(1.0),
            e0: 0.0,
            e1: 0.0,
            e2: 0.0,
            jma: 0.0,
            initialized: false,
        }
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    pub fn update(&mut self, src: f64) -> f64 {
        if !self.initialized {
            self.e0 = src;
            self.e1 = 0.0;
            self.e2 = 0.0;
            self.jma = src;
            self.initialized = true;
            return src;
        }

        let phase_ratio = self.phase / 100.0 + 1.5;
        let length_term = 0.45 * (self.len.saturating_sub(1)) as f64;
        let beta = length_term / (length_term + 2.0);
        let alpha = beta.powf(self.power);
        self.e0 = (1.0 - alpha) * src + alpha * self.e0;
        self.e1 = (src - self.e0) * (1.0 - beta) + beta * self.e1;
        self.e2 = (self.e0 + phase_ratio * self.e1 - self.jma) * (1.0 - alpha).powi(2)
            + alpha.powi(2) * self.e2;
        self.jma += self.e2;
        self.jma
    }

    pub fn reset(&mut self) {
        self.e0 = 0.0;
        self.e1 = 0.0;
        self.e2 = 0.0;
        self.jma = 0.0;
        self.initialized = false;
    }
}

/// Ehlers' SuperSmoother, a 2-pole Butterworth low-pass filter:
/// `a1 = exp(-1.414 · π / len)`, `c2 = 2 · a1 · cos(1.414 · π / len)`, `c3 = -a1²`,
/// `c1 = 1 - c2 - c3`, and `ss_t = c1 · (src_t + src_(t-1)) / 2 + c2 · ss_(t-1) + c3 · ss_(t-2)`.
/// Valid from the first sample, with one of two starts:
/// - [`SuperSmoother::new`]: the missing `src[1]`/`ss[1]`/`ss[2]` terms on the first bars
///   default to `0`, producing a transient of the size of the price level rather than a `None`
///   warmup. This is the start the smoother kinds ([`SmootherKind::build`]) use.
/// - [`SuperSmoother::price_seeded`]: the first two outputs equal the first two inputs
///   (`ss_0 = src_0`, `ss_1 = src_1`) and the recursion runs from the third sample, so the
///   remaining start-up error scales with the price change of the first bars, not the level.
#[derive(Debug, Clone)]
pub struct SuperSmoother {
    c1: f64,
    c2: f64,
    c3: f64,
    prev_src: f64,
    prev1: f64,
    prev2: f64,
    price_seed: bool,
    count: usize,
}

impl SuperSmoother {
    /// Starts from a zero state.
    pub fn new(len: usize) -> Self {
        Self::with_seed(len, false)
    }

    /// Starts from the price: the first two outputs equal the first two inputs.
    pub fn price_seeded(len: usize) -> Self {
        Self::with_seed(len, true)
    }

    fn with_seed(len: usize, price_seed: bool) -> Self {
        let len = len.max(1) as f64;
        let a1 = (-1.414 * std::f64::consts::PI / len).exp();
        let b1 = 2.0 * a1 * (1.414 * std::f64::consts::PI / len).cos();
        let c2 = b1;
        let c3 = -(a1 * a1);
        let c1 = 1.0 - c2 - c3;
        Self {
            c1,
            c2,
            c3,
            prev_src: 0.0,
            prev1: 0.0,
            prev2: 0.0,
            price_seed,
            count: 0,
        }
    }

    pub fn update(&mut self, src: f64) -> f64 {
        let ss = if self.price_seed && self.count < 2 {
            src
        } else {
            self.c1 * (src + self.prev_src) / 2.0 + self.c2 * self.prev1 + self.c3 * self.prev2
        };
        self.count += 1;
        self.prev2 = self.prev1;
        self.prev1 = ss;
        self.prev_src = src;
        ss
    }

    pub fn reset(&mut self) {
        self.prev_src = 0.0;
        self.prev1 = 0.0;
        self.prev2 = 0.0;
        self.count = 0;
    }
}

/// Kaufman's Adaptive Moving Average over a scalar stream: the one KAMA kernel. The catalog
/// indicator [`KamaEngine`](super::moving_averages::KamaEngine) wraps it, and it is a stage of a
/// [`Smoother`] stage. `fast_period`/`slow_period` are parameters (Kaufman's
/// defaults 2/30).
#[derive(Debug, Clone)]
pub struct Kama {
    period: usize,
    fast_period: usize,
    slow_period: usize,
    window: VecDeque<f64>,
    state: Option<f64>,
}

impl Kama {
    pub fn new(period: usize, fast_period: usize, slow_period: usize) -> Self {
        let period = period.max(1);
        Self {
            period,
            fast_period,
            slow_period,
            window: VecDeque::with_capacity(period + 1),
            state: None,
        }
    }

    /// Kaufman's own defaults (`fast_period = 2`, `slow_period = 30`), matching
    /// the catalog defaults of [`KamaEngine`](super::moving_averages::KamaEngine).
    pub fn with_defaults(period: usize) -> Self {
        Self::new(period, 2, 30)
    }

    pub fn update(&mut self, src: f64) -> Option<f64> {
        self.window.push_back(src);
        if self.window.len() > self.period + 1 {
            self.window.pop_front();
        }
        if self.window.len() < self.period + 1 {
            return None;
        }

        let er = efficiency_ratio(self.window.iter().copied()).unwrap_or(0.0);

        let fast_sc = 2.0 / (self.fast_period as f64 + 1.0);
        let slow_sc = 2.0 / (self.slow_period as f64 + 1.0);
        let sc = (er * (fast_sc - slow_sc) + slow_sc).powi(2);

        let next = match self.state {
            Some(prev) => prev + sc * (src - prev),
            None => src,
        };
        self.state = Some(next);
        Some(next)
    }

    pub fn reset(&mut self) {
        self.window.clear();
        self.state = None;
    }
}

#[cfg(test)]
mod kind_tests {
    use super::SmootherKind;
    use crate::indicator::atr::TrueRangeSmoothing;
    use crate::indicator::rsi::RsiSmoothing;

    #[test]
    fn names_and_codes_round_trip_and_are_unique() {
        for (code, kind) in SmootherKind::ALL.into_iter().enumerate() {
            assert_eq!(kind.code(), code);
            assert_eq!(SmootherKind::from_code(code), Some(kind));
            assert_eq!(SmootherKind::from_name(kind.name()), Some(kind));
            assert_eq!(SmootherKind::NAMES[code], kind.name());
        }
        assert_eq!(SmootherKind::from_code(SmootherKind::ALL.len()), None);
        assert_eq!(SmootherKind::from_name("nope"), None);
        let mut names = SmootherKind::NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SmootherKind::ALL.len());
    }

    /// Each kind is named like the catalog entry that computes it.
    #[test]
    fn every_kind_is_a_catalog_entry() {
        let catalog: Vec<&str> = crate::indicator::registry::catalog()
            .iter()
            .map(|entry| entry.name)
            .collect();
        for kind in SmootherKind::ALL {
            assert!(catalog.contains(&kind.name()), "{}", kind.name());
        }
    }

    /// The codes that were published before the list grew keep their meaning.
    #[test]
    fn the_first_codes_are_stable() {
        let published = [
            SmootherKind::Ema,
            SmootherKind::Sma,
            SmootherKind::Rma,
            SmootherKind::Alma,
        ];
        assert_eq!(SmootherKind::ALL[..4], published);
    }

    #[test]
    fn subsets_convert_to_and_from_their_kinds() {
        for smoothing in [
            TrueRangeSmoothing::Rma,
            TrueRangeSmoothing::Sma,
            TrueRangeSmoothing::Ema,
            TrueRangeSmoothing::Wma,
        ] {
            let kind = SmootherKind::from(smoothing);
            assert_eq!(TrueRangeSmoothing::from_kind(kind), Some(smoothing));
        }
        assert_eq!(TrueRangeSmoothing::from_kind(SmootherKind::Kama), None);
        assert_eq!(
            TrueRangeSmoothing::NAMES,
            ["rma", "sma", "ema", "wma"],
            "the f64 map codes of the atr smoothing"
        );
        for smoothing in [RsiSmoothing::Wilder, RsiSmoothing::Ema] {
            assert_eq!(
                RsiSmoothing::from_kind(SmootherKind::from(smoothing)),
                Some(smoothing)
            );
        }
        assert_eq!(RsiSmoothing::from_kind(SmootherKind::Alma), None);
    }
}

#[cfg(test)]
mod jma_tests {
    use super::Jma;

    #[test]
    fn phase_changes_the_open_jurik_approximation() {
        let mut leading = Jma::new(7, 100.0, 2.0);
        let mut lagging = Jma::new(7, -100.0, 2.0);
        let input = [10.0, 11.0, 13.0, 12.0, 15.0];
        let leading_value = input.into_iter().map(|v| leading.update(v)).last().unwrap();
        let lagging_value = input.into_iter().map(|v| lagging.update(v)).last().unwrap();
        assert!(leading_value > lagging_value);
    }

    #[test]
    fn matches_reference_formula_fixture() {
        let mut jma = Jma::new(3, 0.0, 2.0);
        let actual: Vec<_> = [1.0, 2.0, 3.0, 4.0]
            .into_iter()
            .map(|value| jma.update(value))
            .collect();
        let expected = [
            1.0,
            1.819_360_773_771_215_6,
            2.819_354_006_908_239,
            3.831_322_396_018_062,
        ];
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        }
    }
}

#[cfg(test)]
mod supersmoother_tests {
    use super::SuperSmoother;

    /// Reference values independently derived (Python, from the documented Ehlers 2-pole
    /// Butterworth formula transcribed in `SuperSmoother::new`/`update`'s doc comments — not by
    /// running this Rust code):
    /// ```python
    /// import math
    /// def super_smoother(prices, length):
    ///     a1 = math.exp(-1.414 * math.pi / length)
    ///     b1 = 2 * a1 * math.cos(1.414 * math.pi / length)
    ///     c2, c3 = b1, -(a1 * a1)
    ///     c1 = 1 - c2 - c3
    ///     prev_src = prev1 = prev2 = 0.0
    ///     out = []
    ///     for src in prices:
    ///         ss = c1 * (src + prev_src) / 2.0 + c2 * prev1 + c3 * prev2
    ///         prev2, prev1, prev_src = prev1, ss, src
    ///         out.append(ss)
    ///     return out
    /// ```
    #[test]
    fn matches_independently_derived_reference_formula_fixture() {
        let mut ss = SuperSmoother::new(3);
        let actual: Vec<_> = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
            .into_iter()
            .map(|value| ss.update(value))
            .collect();
        let expected = [
            0.505_413_249_748_865_4,
            1.536_919_267_057_587,
            2.563_799_552_420_708,
            3.563_269_185_823_832_3,
            4.561_856_630_604_048,
            5.561_826_276_936_932,
        ];
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        }
    }

    #[test]
    fn price_seed_starts_at_the_first_two_closes() {
        let mut ss = SuperSmoother::price_seeded(3);
        assert_eq!(ss.update(10_000.0), 10_000.0);
        assert_eq!(ss.update(10_010.0), 10_010.0);
        ss.reset();
        assert_eq!(ss.update(5.0), 5.0);
    }

    #[test]
    fn reset_clears_transient_state() {
        let mut ss = SuperSmoother::new(5);
        ss.update(100.0);
        ss.update(110.0);
        ss.reset();
        let mut fresh = SuperSmoother::new(5);
        assert_eq!(ss.update(50.0), fresh.update(50.0));
    }
}

#[cfg(test)]
mod kama_smoother_tests {
    use super::Kama;

    /// `KamaEngine` wraps `Kama`; the formula is golden-validated (see
    /// `tests/golden_reference_moving_averages.rs`, `kama5_last =
    /// 14.554043488814198` for `KAMA(period=5, fast_period=2, slow_period=30)` over the shared
    /// `CLOSES` series `[10, 11, 12, 11, 13, 14, 13, 15, 16, 15]`) — this is not a new formula
    /// derivation, just a check that the `Bar`-decoupled scalar-stream version computes the exact
    /// same sequence as the already-confirmed `Indicator` version, referencing that existing
    /// golden fixture rather than re-deriving it (no circularity: the fixture value predates and
    /// is independent of this struct).
    #[test]
    fn matches_already_confirmed_kama_engine_golden_value() {
        const CLOSES: [f64; 10] = [10.0, 11.0, 12.0, 11.0, 13.0, 14.0, 13.0, 15.0, 16.0, 15.0];
        let mut kama = Kama::new(5, 2, 30);
        let mut last = None;
        for &c in &CLOSES {
            if let Some(value) = kama.update(c) {
                last = Some(value);
            }
        }
        let last = last.expect("kama produced no output");
        assert!(
            (last - 14.554_043_488_814_198).abs() < 1e-9,
            "{last} != 14.554043488814198"
        );
    }

    #[test]
    fn warmup_returns_none_until_period_plus_one_samples() {
        let mut kama = Kama::new(3, 2, 30);
        assert_eq!(kama.update(1.0), None);
        assert_eq!(kama.update(2.0), None);
        assert_eq!(kama.update(3.0), None);
        assert!(kama.update(4.0).is_some());
    }
}

#[cfg(test)]
mod chain_tests {

    /// A line resting on its signal at 0 must not "cross" it by rounding noise
    /// (the stoch RSI case: 1.55e-13 against 0.0).
    #[test]
    fn rounding_noise_is_not_a_cross() {
        assert!(!crossed_over(0.0, 0.0, 1.55e-13, 0.0));
        assert!(!crossed_under(0.0, 0.0, -1.55e-13, 0.0));
        assert!(!crossed_over(100.0, 100.0, 100.0 + 1e-11, 100.0));
    }

    #[test]
    fn a_real_cross_still_counts() {
        assert!(crossed_over(19.0, 20.0, 21.0, 20.0));
        assert!(crossed_under(21.0, 20.0, 19.0, 20.0));
        // From touching to clearly above is a cross; from clearly above to further above is not.
        assert!(crossed_over(20.0, 20.0, 20.5, 20.0));
        assert!(!crossed_over(21.0, 20.0, 22.0, 20.0));
        // Noise on the previous bar does not suppress a real cross.
        assert!(crossed_over(20.0 + 1e-12, 20.0, 20.5, 20.0));
    }

    use super::*;

    #[test]
    fn test_smoother_kind_build_matches_direct_construction() {
        let mut via_kind = SmootherKind::Ema.build(5);
        let mut direct = Ema::new(5);
        for v in [10.0, 11.0, 12.0, 9.0] {
            assert_eq!(via_kind.update(v), Ema::update(&mut direct, v));
        }
    }
}
