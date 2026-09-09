use std::collections::VecDeque;

use crate::model::Bar;

use super::{Indicator, IndicatorOutput};

/// Streaming Ulcer Index over any series of positive values.
///
/// Two questions, kept apart:
///
/// 1. **How far below its own high was the series at each point?** For every value the running
///    maximum of the last `len` values *up to and including that point* is taken, and the
///    percentage below it recorded: `100 * (value - running_max) / running_max`, at most zero.
///    A past drawdown is never recomputed against a later high — what it felt like then is what
///    it was, and rewriting it against today's peak would flatter or worsen history depending on
///    what happened afterwards.
/// 2. **How deep and how persistent were those drawdowns?** The squared percentages over the last
///    `len` points are averaged and the root taken.
///
/// Squaring is what separates this from an average drawdown: it weighs one deep, long decline
/// more heavily than a series of shallow dips, which is the property the measure exists for.
///
/// Unit: percent. Zero means the series never traded below its running high in the window; there
/// is no upper bound.
///
/// First value: after `2 * len - 1` observations — `len` to fill the running-maximum window, then
/// another `len - 1` so that every squared drawdown being averaged is itself fully formed.
///
/// The series must be positive: a percentage below a non-positive high has no meaning. Such a
/// value is refused rather than folded in, and the state is left untouched.
///
/// Works on prices, on an equity curve, or on any other positive series — nothing here is
/// specific to bars.
#[derive(Debug, Clone)]
pub struct UlcerIndexCore {
    len: usize,
    window: VecDeque<f64>,
    squared_drawdowns: VecDeque<f64>,
}

impl UlcerIndexCore {
    pub fn new(len: usize) -> Self {
        let len = len.max(1);
        Self {
            len,
            window: VecDeque::with_capacity(len),
            squared_drawdowns: VecDeque::with_capacity(len),
        }
    }

    /// Observations needed before [`UlcerIndexCore::update`] first returns `Some`.
    pub fn warmup_period(&self) -> usize {
        2 * self.len - 1
    }

    pub fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }

        self.window.push_back(value);
        if self.window.len() > self.len {
            self.window.pop_front();
        }
        if self.window.len() < self.len {
            return None;
        }

        let running_max = self
            .window
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let drawdown_pct = 100.0 * (value - running_max) / running_max;

        self.squared_drawdowns
            .push_back(drawdown_pct * drawdown_pct);
        if self.squared_drawdowns.len() > self.len {
            self.squared_drawdowns.pop_front();
        }
        if self.squared_drawdowns.len() < self.len {
            return None;
        }

        let mean = self.squared_drawdowns.iter().sum::<f64>() / self.len as f64;
        Some(mean.sqrt())
    }

    pub fn reset(&mut self) {
        self.window.clear();
        self.squared_drawdowns.clear();
    }
}

/// The Ulcer Index of a complete series, or `None` if it is shorter than the warmup.
///
/// The batch counterpart to [`UlcerIndexCore`], for an equity curve or a return series that is
/// already in hand rather than arriving bar by bar. Returns the value at the end of the series.
pub fn ulcer_index(values: &[f64], len: usize) -> Option<f64> {
    let mut core = UlcerIndexCore::new(len);
    values.iter().filter_map(|value| core.update(*value)).last()
}

/// Ulcer Index over the closing price; see [`UlcerIndexCore`] for the definition.
///
/// This measures the same thing as [`crate::portfolio::compute_drawdown`] does not: that one
/// reports the single worst peak-to-trough decline of a whole series, this one how much time the
/// series spent below its high and how far. Both stay.
#[derive(Debug, Clone)]
pub struct UlcerIndexEngine {
    core: UlcerIndexCore,
}

impl UlcerIndexEngine {
    pub fn new(len: usize) -> Self {
        Self {
            core: UlcerIndexCore::new(len),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14)
    }
}

impl Indicator for UlcerIndexEngine {
    fn name(&self) -> &str {
        "ulcer_index"
    }

    fn warmup_period(&self) -> usize {
        self.core.warmup_period()
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.core.update(bar.close).map(IndicatorOutput::new)
    }

    fn reset(&mut self) {
        self.core.reset();
    }
}
