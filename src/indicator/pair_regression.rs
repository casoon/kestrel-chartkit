use std::collections::{HashMap, VecDeque};

use super::definition::{ParamSpec, ResolvedParams};
use super::pair::{PairIndicator, PairIndicatorDefinition};
use super::registry::{OutputRange, RegistryError};
use super::unit::IndicatorUnit;
use super::IndicatorOutput;
use crate::cross_asset::{compute_pair_spread, compute_rolling_beta};
use crate::model::Bar;

/// Rolling beta of the simple returns of a close series against a reference series.
///
/// Per paired bar (see [`super::pair`] for the pairing) the two returns
/// `r = (close_t - close_{t-1}) / close_{t-1}` are taken against the previous paired bar, with the
/// return pairs collected exactly as [`super::correlation::CorrelationEngine`] does. Over the last
/// `len` pairs, with `da`, `db` the deviations of the series' and the reference's returns from
/// their window means, `beta = sum(da * db) / sum(db^2)` and `alpha = mean(a) - beta * mean(b)`.
/// The calculation is [`compute_rolling_beta`], bar by bar.
///
/// `value`: `beta`; `extra`: `alpha`, `correlation` (the Pearson correlation of the window,
/// clamped to `-1..=1`) and `r_squared` (its square).
///
/// - First output with the `len + 1`-th paired bar (`len` return pairs).
/// - A return pair is left out when a base close is `0` or a return is not finite; the window and
///   the value then stay as they were.
/// - A window in which the reference's returns do not vary (`sum(db^2) <= 1e-14`) has no beta:
///   the output is `None` for that bar.
#[derive(Debug, Clone)]
pub struct RollingBetaEngine {
    len: usize,
    previous: Option<(f64, f64)>,
    returns: VecDeque<(f64, f64)>,
}

impl RollingBetaEngine {
    /// `len` is the number of return pairs in the window, at least 3.
    pub fn new(len: usize) -> Self {
        Self {
            len: len.max(3),
            previous: None,
            returns: VecDeque::new(),
        }
    }
}

impl PairIndicator for RollingBetaEngine {
    fn name(&self) -> &str {
        "rolling_beta"
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
        let fit = compute_rolling_beta(&own, &other)?;
        let extra = HashMap::from([
            ("alpha".to_string(), fit.alpha),
            ("correlation".to_string(), fit.correlation),
            ("r_squared".to_string(), fit.r_squared),
        ]);
        Some(IndicatorOutput::with_extra(fit.beta, extra))
    }
}

pub const ROLLING_BETA_DEFINITION: PairIndicatorDefinition = PairIndicatorDefinition {
    name: "rolling_beta",
    aliases: &[],
    description: "Rolling beta of the simple returns of a close series against a reference series, with alpha, correlation and R-squared",
    params: &[ParamSpec::count("len", 60, 3, 10000).meaning("return pairs in the rolling window")],
    checks: &[],
    unit: IndicatorUnit::Ratio,
    output_range: OutputRange::Unbounded,
    build: build_rolling_beta,
};

fn build_rolling_beta(p: &ResolvedParams) -> Result<Box<dyn PairIndicator>, RegistryError> {
    Ok(Box::new(RollingBetaEngine::new(p.count("len"))))
}

/// Residual z-score of the spread between a close series and a reference series.
///
/// Over the last `len` paired closes `a_i`, `b_i` (see [`super::pair`] for the pairing) the
/// hedge ratio is the least-squares slope of the series' closes on the reference's,
/// `gamma = sum(da * db) / sum(db^2)`, the spread `s_i = a_i - gamma * b_i`, and
/// `z = (s_last - mean(s)) / sd(s)` with the population deviation of the window's spreads. The
/// calculation is [`compute_pair_spread`], bar by bar. It describes the window; it is no test of
/// cointegration and no trading rule.
///
/// `value`: `z`, `0` when the spreads do not vary (`sd <= 1e-14`); `extra`: `hedge_ratio`,
/// `spread` (the latest `s`), `mean_spread` and `std_spread`.
///
/// - First output with the `len`-th paired bar.
/// - A window in which the reference's closes do not vary (`sum(db^2) <= 1e-14`) has no hedge
///   ratio: the output is `None` for that bar.
#[derive(Debug, Clone)]
pub struct PairSpreadEngine {
    len: usize,
    closes: VecDeque<(f64, f64)>,
}

impl PairSpreadEngine {
    /// `len` is the number of paired closes in the window, at least 3.
    pub fn new(len: usize) -> Self {
        Self {
            len: len.max(3),
            closes: VecDeque::new(),
        }
    }
}

impl PairIndicator for PairSpreadEngine {
    fn name(&self) -> &str {
        "pair_spread"
    }

    fn warmup_period(&self) -> usize {
        self.len
    }

    fn reset(&mut self) {
        self.closes.clear();
    }

    fn on_bars(&mut self, bar: &Bar, reference: &Bar) -> Option<IndicatorOutput> {
        self.closes.push_back((bar.close, reference.close));
        if self.closes.len() > self.len {
            self.closes.pop_front();
        }
        if self.closes.len() < self.len {
            return None;
        }
        let (own, other): (Vec<f64>, Vec<f64>) = self.closes.iter().copied().unzip();
        let spread = compute_pair_spread(&own, &other)?;
        let extra = HashMap::from([
            ("hedge_ratio".to_string(), spread.hedge_ratio),
            ("spread".to_string(), spread.current_spread),
            ("mean_spread".to_string(), spread.mean_spread),
            ("std_spread".to_string(), spread.std_spread),
        ]);
        Some(IndicatorOutput::with_extra(spread.residual_z_score, extra))
    }
}

pub const PAIR_SPREAD_DEFINITION: PairIndicatorDefinition = PairIndicatorDefinition {
    name: "pair_spread",
    aliases: &[],
    description: "Residual z-score of the spread between a close series and a reference series, with hedge ratio, spread, mean and deviation",
    params: &[ParamSpec::count("len", 60, 3, 10000).meaning("paired closes in the rolling window")],
    checks: &[],
    unit: IndicatorUnit::Ratio,
    output_range: OutputRange::Centered { center: 0.0 },
    build: build_pair_spread,
};

fn build_pair_spread(p: &ResolvedParams) -> Result<Box<dyn PairIndicator>, RegistryError> {
    Ok(Box::new(PairSpreadEngine::new(p.count("len"))))
}
