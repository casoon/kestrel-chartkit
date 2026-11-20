//! Fractal dimension of the price path over a trailing window: `1` for a straight line, towards
//! `2` for a path that fills the plane (noise). One catalog entry, two published estimates chosen
//! by `method`; both are computed by the one kernel `kernels::fractal_dimension`, which
//! Coil Release (`halves`) and the trend-persistence read-out (`path`) call as well.
//!
//! - `halves` (Ehlers' estimate, the one behind FRAMA): with `half = max(period / 2, 2)`, `r1` is
//!   the high-low range of the newest `half` bars, `r2` that of the `half` bars before them and
//!   `r3` that of the whole window of `period` bars, each divided by its bar count;
//!   `D = (ln(r1 + r2) - ln(r3)) / ln 2`, clamped to `1..=2`. `1.5` when `r1 + r2` or `r3` is not
//!   positive. First output with the `period`-th bar.
//! - `path`: `D = 1 + ln(path / range) / ln(period)`, with `path` the sum of the `period`
//!   absolute close-to-close moves and `range` the high-low range of the same `period + 1` bars.
//!   Not clamped (a path shorter than the range gives a value below 1); `1.5` when the range or
//!   the path is not positive. First output with the `(period + 1)`-th bar.
//!
//! [`Indicator::reset`] clears the window.

use std::collections::VecDeque;

use super::kernels::{fractal_dimension, FractalMethod};
use super::{Indicator, IndicatorOutput};
use crate::model::Bar;

/// Streaming fractal dimension; see the module documentation.
#[derive(Debug, Clone)]
pub struct FractalDimensionEngine {
    method: FractalMethod,
    period: usize,
    needed: usize,
    bars: VecDeque<(f64, f64, f64)>,
}

impl FractalDimensionEngine {
    /// `period` of at least 4, so each half holds two bars.
    pub fn new(period: usize, path: bool) -> Self {
        let method = if path {
            FractalMethod::Path
        } else {
            FractalMethod::Halves
        };
        let needed = method.bars_needed(period);
        Self {
            method,
            period,
            needed,
            bars: VecDeque::with_capacity(needed),
        }
    }
}

impl Indicator for FractalDimensionEngine {
    fn name(&self) -> &str {
        "fractal_dimension"
    }

    fn warmup_period(&self) -> usize {
        self.needed
    }

    fn reset(&mut self) {
        self.bars.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if self.bars.len() == self.needed {
            self.bars.pop_front();
        }
        self.bars.push_back((bar.high, bar.low, bar.close));
        let last = self.bars.len().checked_sub(1)?;
        fractal_dimension(self.method, self.period, |k| {
            self.bars.get(last.checked_sub(k)?).copied()
        })
        .map(IndicatorOutput::new)
    }
}

/// Registry definition of `fractal_dimension`.
pub const FRACTAL_DIMENSION_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "fractal_dimension",
        aliases: &[],
        description: "Fractal dimension of the price path over a trailing window, 1 for a straight line and towards 2 for noise: Ehlers' halving estimate (`halves`, clamped to 1..2) or path over range (`path`, unclamped)",
        params: &[
            super::definition::ParamSpec::count("period", 30, 4, 10000)
                .meaning("bars of the window (`halves`: the window; `path`: the number of close-to-close steps, one more bar)"),
            super::definition::ParamSpec::choice("method", "halves", &["halves", "path"], true)
                .meaning("`halves`: ln of the two half-window ranges against the whole one over ln 2; `path`: 1 + ln(path / range) / ln(period)"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(FractalDimensionEngine::new(
                p.count("period"),
                p.choice("method") == "path",
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(t, close, high, low, close, 100.0)
    }

    #[test]
    fn a_straight_climb_is_one_dimensional_under_halves() {
        let mut engine = FractalDimensionEngine::new(8, false);
        let mut last = None;
        for i in 0..8 {
            let c = 100.0 + i as f64;
            last = engine.on_bar(&bar(i, c + 0.5, c - 0.5, c));
        }
        let d = last.unwrap().value;
        assert!((d - 1.0).abs() < 0.1, "{d}");
    }

    #[test]
    fn flat_bars_give_the_neutral_one_and_a_half() {
        for path in [false, true] {
            let mut engine = FractalDimensionEngine::new(6, path);
            let mut last = None;
            for i in 0..8 {
                last = engine.on_bar(&bar(i, 100.0, 100.0, 100.0));
            }
            assert_eq!(last.unwrap().value, 1.5);
        }
    }

    #[test]
    fn the_window_is_period_bars_for_halves_and_one_more_for_path() {
        assert_eq!(FractalDimensionEngine::new(10, false).warmup_period(), 10);
        assert_eq!(FractalDimensionEngine::new(10, true).warmup_period(), 11);
    }
}
