use std::collections::VecDeque;

use super::definition::{ParamSpec, ResolvedParams};
use super::pair::{PairIndicator, PairIndicatorDefinition};
use super::registry::{OutputRange, RegistryError};
use super::unit::IndicatorUnit;
use super::IndicatorOutput;
use crate::model::Bar;

/// Rolling Pearson correlation of the simple returns of two close series.
///
/// Per paired bar (see [`super::pair`] for the pairing) the two returns
/// `r = (close_t - close_{t-1}) / close_{t-1}` are taken against the previous paired bar. The
/// value is the Pearson correlation of the last `len` return pairs,
/// `rho = sum(da * db) / sqrt(sum(da^2) * sum(db^2))` with `da`, `db` the deviations from the
/// window means, clamped to `-1..=1` — the calculation of
/// [`correlation_matrix`](crate::cross_asset::correlation_matrix), bar by bar.
///
/// Conventions:
///
/// - First output with the `len + 1`-th paired bar (`len` return pairs); nothing before the
///   window is full.
/// - A return pair is left out when a base close is `0` or a return is not finite. The window
///   then stays as it was, and so does the value.
/// - A window in which one side does not vary (`sum(da^2) * sum(db^2) = 0`) has no correlation:
///   the output is `None` for that bar.
#[derive(Debug, Clone)]
pub struct CorrelationEngine {
    len: usize,
    previous: Option<(f64, f64)>,
    returns: VecDeque<(f64, f64)>,
}

impl CorrelationEngine {
    /// `len` is the number of return pairs in the window, at least 3.
    pub fn new(len: usize) -> Self {
        Self {
            len: len.max(3),
            previous: None,
            returns: VecDeque::new(),
        }
    }
}

impl PairIndicator for CorrelationEngine {
    fn name(&self) -> &str {
        "correlation"
    }

    fn warmup_period(&self) -> usize {
        self.len + 1
    }

    fn reset(&mut self) {
        self.previous = None;
        self.returns.clear();
    }

    fn on_bars(&mut self, bar: &Bar, reference: &Bar) -> Option<IndicatorOutput> {
        let (close, reference_close) = (bar.close, reference.close);
        let (base, reference_base) = self.previous.replace((close, reference_close))?;
        if base != 0.0 && reference_base != 0.0 {
            let pair = (
                (close - base) / base,
                (reference_close - reference_base) / reference_base,
            );
            if pair.0.is_finite() && pair.1.is_finite() {
                self.returns.push_back(pair);
                if self.returns.len() > self.len {
                    self.returns.pop_front();
                }
            }
        }
        if self.returns.len() < self.len {
            return None;
        }
        let (own, other): (Vec<f64>, Vec<f64>) = self.returns.iter().copied().unzip();
        crate::cross_asset::pearson(&own, &other).map(IndicatorOutput::new)
    }
}

pub const CORRELATION_DEFINITION: PairIndicatorDefinition = PairIndicatorDefinition {
    name: "correlation",
    aliases: &[],
    description: "Rolling Pearson correlation of the simple returns of two close series",
    params: &[ParamSpec::count("len", 20, 3, 10000).meaning("return pairs in the rolling window")],
    checks: &[],
    unit: IndicatorUnit::Ratio,
    output_range: OutputRange::Bounded {
        min: -1.0,
        max: 1.0,
    },
    build: build_correlation,
};

fn build_correlation(p: &ResolvedParams) -> Result<Box<dyn PairIndicator>, RegistryError> {
    Ok(Box::new(CorrelationEngine::new(p.count("len"))))
}
