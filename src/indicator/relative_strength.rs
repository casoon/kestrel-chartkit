use std::collections::HashMap;

use super::definition::{IndicatorFacts, ParamSpec, ResolvedParams, Verification};
use super::pair::{PairIndicator, PairIndicatorDefinition};
use super::registry::RegistryError;
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Relative strength of a series against a reference series (a benchmark, a sector, another
/// instrument): the price ratio and the difference of the two returns over `lookback` bars.
///
/// With the closes `a` of the series and `b` of the reference at paired bar `t` and
/// `n = lookback`:
///
/// - `own_perf_pct = (a_t - a_{t-n}) / a_{t-n} * 100`, `bench_perf_pct` likewise from `b`;
/// - `alpha_pct = own_perf_pct - bench_perf_pct`, in percentage points — the value;
/// - `ratio = a_t / b_t`.
///
/// `extra` carries `ratio`, `own_perf_pct`, `bench_perf_pct` and `alpha_pct`. The alerts
/// `outperforming_benchmark` and `underperforming_benchmark` fire while `alpha_pct` is above `5`
/// or below `-5`.
///
/// Conventions:
///
/// - First output with the `n + 1`-th paired bar.
/// - Nothing is published on a bar where one of the four closes used (both current closes and
///   both closes `n` bars back) is not finite or not positive; no substitute value is shown.
///
/// The two series come in through [`RelativeStrengthEngine::update`] or, built by name
/// (`relative_strength`, parameter `len`), through [`PairIndicator::on_bars`]; see
/// [`super::pair`] for how two series are paired. The [`Indicator`] implementation compares the
/// series with itself and is kept for compatibility only: its `alpha_pct` is always `0`.
pub struct RelativeStrengthEngine {
    lookback: usize,
    own_closes: Vec<f64>,
    bench_closes: Vec<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl RelativeStrengthEngine {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            own_closes: Vec::new(),
            bench_closes: Vec::new(),
            alerts: Vec::new(),
        }
    }

    /// Dual-bar update for multi-series Relative Strength comparison.
    pub fn update(&mut self, own_bar: &Bar, benchmark_bar: &Bar) -> Option<IndicatorOutput> {
        self.own_closes.push(own_bar.close);
        self.bench_closes.push(benchmark_bar.close);

        if self.own_closes.len() > self.lookback + 1 {
            self.own_closes.remove(0);
            self.bench_closes.remove(0);
        }

        self.alerts.clear();
        if self.own_closes.len() < self.lookback + 1 {
            return None;
        }

        let (now_own, now_bench) = (own_bar.close, benchmark_bar.close);
        let past_own = self.own_closes[0];
        let past_bench = self.bench_closes[0];
        let valid = |x: f64| x.is_finite() && x > 0.0;
        if !(valid(now_own) && valid(now_bench) && valid(past_own) && valid(past_bench)) {
            return None;
        }

        let curr_ratio = now_own / now_bench;
        let own_perf = (now_own - past_own) / past_own;
        let bench_perf = (now_bench - past_bench) / past_bench;

        let alpha = (own_perf - bench_perf) * 100.0;

        let mut extra = HashMap::new();
        extra.insert("ratio".to_string(), curr_ratio);
        extra.insert("own_perf_pct".to_string(), own_perf * 100.0);
        extra.insert("bench_perf_pct".to_string(), bench_perf * 100.0);
        extra.insert("alpha_pct".to_string(), alpha);

        if alpha > 5.0 {
            self.alerts.push(IndicatorAlert::new(
                "outperforming_benchmark",
                format!("Outperforming Benchmark by +{:.2}%", alpha),
                0.85,
            ));
        } else if alpha < -5.0 {
            self.alerts.push(IndicatorAlert::new(
                "underperforming_benchmark",
                format!("Underperforming Benchmark by {:.2}%", alpha),
                0.85,
            ));
        }

        Some(IndicatorOutput::with_extra(alpha, extra))
    }
}

impl Indicator for RelativeStrengthEngine {
    fn name(&self) -> &str {
        "relative_strength"
    }

    fn warmup_period(&self) -> usize {
        self.lookback + 1
    }

    fn reset(&mut self) {
        self.own_closes.clear();
        self.bench_closes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        // Fallback single-bar interface treating benchmark as static/1.0
        self.update(bar, bar)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

impl PairIndicator for RelativeStrengthEngine {
    fn name(&self) -> &str {
        "relative_strength"
    }

    fn warmup_period(&self) -> usize {
        self.lookback + 1
    }

    fn reset(&mut self) {
        Indicator::reset(self);
    }

    fn on_bars(&mut self, bar: &Bar, reference: &Bar) -> Option<IndicatorOutput> {
        self.update(bar, reference)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const RELATIVE_STRENGTH_DEFINITION: PairIndicatorDefinition = PairIndicatorDefinition {
    name: "relative_strength",
    aliases: &[],
    description: "Return difference and price ratio of a series against a reference series",
    params: &[
        ParamSpec::count("len", 20, 1, 10000).meaning("bars between the two closes of each return")
    ],
    checks: &[],
    facts: IndicatorFacts::ratio()
        .centered(0.0)
        .same_adjustment_and_session(),
    verification: Verification::Verified,
    build: build_relative_strength,
};

fn build_relative_strength(p: &ResolvedParams) -> Result<Box<dyn PairIndicator>, RegistryError> {
    Ok(Box::new(RelativeStrengthEngine::new(p.count("len"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, close: f64) -> Bar {
        Bar::new(t, close, close, close, close, 0.0)
    }

    #[test]
    fn publishes_nothing_where_a_close_is_not_positive_or_not_finite() {
        let mut engine = RelativeStrengthEngine::new(1);
        assert!(engine.update(&bar(0, 100.0), &bar(0, 50.0)).is_none());
        let out = engine.update(&bar(1, 110.0), &bar(1, 50.0)).unwrap();
        assert!((out.value - 10.0).abs() < 1e-12);
        // A reference close of 0 now and as the base of the next return: no output for either bar.
        assert!(engine.update(&bar(2, 120.0), &bar(2, 0.0)).is_none());
        assert!(engine.update(&bar(3, 120.0), &bar(3, 60.0)).is_none());
        assert!(engine.update(&bar(4, f64::NAN), &bar(4, 60.0)).is_none());
        assert!(engine.update(&bar(5, 120.0), &bar(5, 60.0)).is_none());
        // All four closes valid again: publishing resumes.
        let out = engine.update(&bar(6, 132.0), &bar(6, 60.0)).unwrap();
        assert!((out.value - 10.0).abs() < 1e-12);
        assert!((out.extra["ratio"] - 2.2).abs() < 1e-12);
        assert!(PairIndicator::alerts(&engine)
            .iter()
            .any(|a| a.kind == "outperforming_benchmark"));
    }
}
