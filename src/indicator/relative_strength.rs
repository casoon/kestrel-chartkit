use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Relative Strength Ratio & Momentum vs. Benchmark (e.g., Asset / BTC or Stock / SPY).
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

        let curr_ratio = if benchmark_bar.close > 0.0 {
            own_bar.close / benchmark_bar.close
        } else {
            1.0
        };

        let past_own = self.own_closes[0];
        let past_bench = self.bench_closes[0];

        let own_perf = if past_own > 0.0 {
            (own_bar.close - past_own) / past_own
        } else {
            0.0
        };
        let bench_perf = if past_bench > 0.0 {
            (benchmark_bar.close - past_bench) / past_bench
        } else {
            0.0
        };

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
