use std::collections::VecDeque;

use crate::model::Bar;
use crate::stats::correlation;

use super::{Indicator, IndicatorOutput};

/// Rank Correlation Index: how closely the order of the last `len` prices follows the order of
/// time.
///
/// Prices in the window are ranked, time is ranked by position, and the two rank series are
/// correlated; the result is scaled by 100. A window whose prices rise monotonically gives +100,
/// one that falls monotonically -100, and a shuffled one something in between.
///
/// **Ties get the average of the ranks they share.** Two equal prices are not one before the
/// other, and pretending otherwise would invent an ordering the market did not produce. The
/// correlation is then computed over the rank series directly rather than through the
/// `1 - 6*sum(d^2)/(n^3-n)` shortcut, which is only valid when no rank is shared.
///
/// Unit: an index in `-100..=100`, not a percentage of anything.
///
/// A window without price variance — every price equal — has no ordering at all. There is no
/// correlation to compute, and the documented convention is `0`: no direction, rather than a
/// trend strength conjured from nothing.
///
/// First output: with the `len`-th bar. `len` must be at least 2, since a single price has no
/// order. A window that holds a non-finite price produces no output at all — there is no ordering
/// to compute. [`Indicator::reset`] clears the window.
#[derive(Debug, Clone)]
pub struct RciEngine {
    len: usize,
    window: VecDeque<f64>,
}

impl RciEngine {
    pub fn new(len: usize) -> Self {
        Self {
            len: len.max(2),
            window: VecDeque::with_capacity(len.max(2)),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(9)
    }
}

/// Average ranks of `values`, ascending: the smallest value gets rank 1, and values that are
/// equal share the mean of the ranks they occupy.
fn average_ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    // `total_cmp` rather than `partial_cmp`: a total order cannot fail on any input, and the
    // caller has already refused windows with non-finite prices.
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));

    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && values[order[end]] == values[order[start]] {
            end += 1;
        }
        // Ranks are one-based; the shared rank is the mean of the positions this group occupies.
        let shared = ((start + 1) + end) as f64 / 2.0;
        for slot in &order[start..end] {
            ranks[*slot] = shared;
        }
        start = end;
    }
    ranks
}

impl Indicator for RciEngine {
    fn name(&self) -> &str {
        "rci"
    }

    fn warmup_period(&self) -> usize {
        self.len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.window.push_back(bar.close);
        if self.window.len() > self.len {
            self.window.pop_front();
        }
        if self.window.len() < self.len {
            return None;
        }

        let prices: Vec<f64> = self.window.iter().copied().collect();
        if prices.iter().any(|price| !price.is_finite()) {
            // A window holding a non-finite price has no order to speak of. No output beats a
            // rank correlation over a value that is not a price.
            return None;
        }
        let price_ranks = average_ranks(&prices);
        let time_ranks: Vec<f64> = (1..=self.len).map(|i| i as f64).collect();

        // `None` means one side has no variance, which for the time ranks cannot happen — so it
        // is the price side, i.e. a window of identical prices.
        let value = correlation(&price_ranks, &time_ranks).unwrap_or(0.0) * 100.0;
        Some(IndicatorOutput::new(value))
    }

    fn reset(&mut self) {
        self.window.clear();
    }
}
