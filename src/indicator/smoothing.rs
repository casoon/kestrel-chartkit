use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Pine's `ta.ema`: seeds with the input value itself on the first sample
#[derive(Debug, Clone, Copy, Default)]
pub struct Ema {
    len: usize,
    state: Option<f64>,
}

impl Ema {
    pub fn new(len: usize) -> Self {
        Self { len, state: None }
    }

    pub fn update(&mut self, src: f64) -> f64 {
        let alpha = 2.0 / (self.len as f64 + 1.0);
        let next = match self.state {
            None => src,
            Some(prev) => alpha * src + (1.0 - alpha) * prev,
        };
        self.state = Some(next);
        next
    }

    pub fn reset(&mut self) {
        self.state = None;
    }
}

/// Pine's `ta.wma` weighted moving average for scalar streams
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

/// Pine's `ta.rma` (Wilder smoothing): seeds with SMA of the first `len` samples
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

/// Pine's `ta.sma`: plain windowed average
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
pub struct ExtremeWindow {
    len: usize,
    window: VecDeque<f64>,
}

impl ExtremeWindow {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            window: VecDeque::with_capacity(len),
        }
    }

    pub fn push(&mut self, value: f64) -> Option<(f64, f64)> {
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

    pub fn reset(&mut self) {
        self.window.clear();
    }
}

pub fn crossed_over(prev_a: f64, prev_b: f64, a: f64, b: f64) -> bool {
    prev_a <= prev_b && a > b
}

pub fn crossed_under(prev_a: f64, prev_b: f64, a: f64, b: f64) -> bool {
    prev_a >= prev_b && a < b
}

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
}

impl SmootherKind {
    /// Builds a boxed [`Smoother`] of this kind with `len` bars of lookback/decay, using common
    /// Pine defaults for `Alma` (`offset = 0.85`, `sigma = 6.0`) and `Jma` (`phase = 0.0`,
    /// `power = 2.0`). Use the concrete constructors directly to override those.
    pub fn build(self, len: usize) -> Box<dyn Smoother> {
        match self {
            SmootherKind::Ema => Box::new(Ema::new(len)),
            SmootherKind::Sma => Box::new(Sma::new(len)),
            SmootherKind::Rma => Box::new(Rma::new(len)),
            SmootherKind::Alma => Box::new(Alma::new(len, 0.85, 6.0)),
            SmootherKind::Jma => Box::new(Jma::new(len, 0.0, 2.0)),
        }
    }
}

/// Common contract for streaming smoothers, letting them be chained ([`SmootherChain`]) or
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
        Some(Ema::update(self, src))
    }
    fn reset(&mut self) {
        Ema::reset(self)
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

/// A typed, ordered pipeline of [`Smoother`] stages: each stage's output feeds the next stage's
/// input. Its own warmup/reset contract composes cleanly from its stages':
/// [`SmootherChain::warmup_period`] is the sum of every stage's warmup (a downstream stage cannot
/// start accumulating until its upstream first emits), and [`SmootherChain::reset`] resets every
/// stage. Works with any mix of existing `Smoother` impls, and with future ones without changes
/// here — implement [`Smoother`] and it is chainable.
pub struct SmootherChain {
    stages: Vec<Box<dyn Smoother>>,
}

impl SmootherChain {
    pub fn new(stages: Vec<Box<dyn Smoother>>) -> Self {
        Self { stages }
    }

    pub fn warmup_period(&self) -> usize {
        self.stages.iter().map(|s| s.warmup_period()).sum()
    }

    pub fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }

    /// Feeds `src` through every stage in order. Returns `None` if any stage is still inside its
    /// own warmup this bar.
    pub fn update(&mut self, src: f64) -> Option<f64> {
        let mut value = src;
        for stage in &mut self.stages {
            value = stage.update(value)?;
        }
        Some(value)
    }
}

/// A [`SmootherChain`] is itself a [`Smoother`], so chains compose (a chain can be one stage of
/// another chain) and can be used anywhere a single smoother is expected, e.g. as one leg of
/// [`super::trend_relationship::AdaptiveTrendRelationship`].
impl Smoother for SmootherChain {
    fn update(&mut self, src: f64) -> Option<f64> {
        SmootherChain::update(self, src)
    }
    fn reset(&mut self) {
        SmootherChain::reset(self)
    }
    fn warmup_period(&self) -> usize {
        SmootherChain::warmup_period(self)
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

/// Open Jurik-style moving-average approximation used by the Pine sources.
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
    fn matches_pine_reference_formula_fixture() {
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
mod chain_tests {
    use super::*;

    #[test]
    fn test_chain_warmup_is_sum_of_stage_warmups() {
        let chain =
            SmootherChain::new(vec![SmootherKind::Sma.build(3), SmootherKind::Rma.build(4)]);
        assert_eq!(chain.warmup_period(), 3 + 4);
    }

    #[test]
    fn test_chain_none_until_every_stage_warm() {
        let mut chain =
            SmootherChain::new(vec![SmootherKind::Sma.build(2), SmootherKind::Sma.build(2)]);
        assert_eq!(chain.update(1.0), None); // stage 1 still cold
        assert_eq!(chain.update(2.0), None); // stage 1 warm (sma=1.5), stage 2 gets its 1st input
                                             // stage 1 sma(2,3)=2.5; stage 2 now has both its inputs (1.5, 2.5) -> warm.
        let value = chain.update(3.0).unwrap();
        assert!((value - 2.0).abs() < 1e-9);
        let value = chain.update(4.0).unwrap();
        assert!((value - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_chain_reset_clears_every_stage() {
        let mut chain = SmootherChain::new(vec![SmootherKind::Sma.build(2)]);
        chain.update(1.0);
        assert!(chain.update(2.0).is_some());
        chain.reset();
        assert_eq!(chain.update(5.0), None, "reset stage must re-enter warmup");
    }

    #[test]
    fn test_smoother_kind_build_matches_direct_construction() {
        let mut via_kind = SmootherKind::Ema.build(5);
        let mut direct = Ema::new(5);
        for v in [10.0, 11.0, 12.0, 9.0] {
            assert_eq!(via_kind.update(v), Some(Ema::update(&mut direct, v)));
        }
    }
}
