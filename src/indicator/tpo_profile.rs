//! Time-at-price profile after Steidlmayer and Koy (1986): one count per bar and price bin, with the
//! point of control and the value area. It needs no volume.

use std::collections::{HashMap, VecDeque};

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Profile over the last `lookback` bars. The window's range is cut into `num_bins` equal bins; every
/// bar adds one count to each bin its closed high-low range touches (a price exactly on the edge
/// between two bins touches both). `value` is the centre of the point-of-control bin, the extras `vah`
/// and `val` the upper edge and lower edge of the value area (70 % of all counts, grown from the
/// point of control towards the larger neighbouring bin, both on a tie). Among equally filled bins the
/// point of control is the one closest to the centre of the range; of two equally close ones the one
/// closer to the close, the lower one on a further tie.
/// A window without range publishes the close for all three. First output: with the `lookback`-th bar.
/// [`Indicator::reset`] clears the window.
pub struct TpoProfileEngine {
    lookback: usize,
    num_bins: usize,
    bars: VecDeque<(f64, f64)>,
}

impl TpoProfileEngine {
    pub fn new(lookback: usize, num_bins: usize) -> Self {
        Self {
            lookback: lookback.max(1),
            num_bins: num_bins.max(1),
            bars: VecDeque::new(),
        }
    }
}

impl Indicator for TpoProfileEngine {
    fn name(&self) -> &str {
        "tpo_profile"
    }

    fn warmup_period(&self) -> usize {
        self.lookback
    }

    fn reset(&mut self) {
        self.bars.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back((bar.high, bar.low));
        if self.bars.len() > self.lookback {
            self.bars.pop_front();
        }
        if self.bars.len() < self.lookback {
            return None;
        }
        let lo = self.bars.iter().map(|b| b.1).fold(f64::INFINITY, f64::min);
        let hi = self
            .bars
            .iter()
            .map(|b| b.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let extras = |vah: f64, val: f64| {
            HashMap::from([("vah".to_string(), vah), ("val".to_string(), val)])
        };
        if hi == lo {
            return Some(IndicatorOutput::with_extra(
                bar.close,
                extras(bar.close, bar.close),
            ));
        }
        let bins = self.num_bins;
        let step = (hi - lo) / bins as f64;
        let mut count = vec![0u64; bins];
        for &(h, l) in &self.bars {
            let first = (((l - lo) / step).ceil() as i64 - 1).max(0) as usize;
            let last = (((h - lo) / step).floor() as i64).min(bins as i64 - 1);
            if last < 0 {
                continue;
            }
            for c in count.iter_mut().take(last as usize + 1).skip(first) {
                *c += 1;
            }
        }

        // Point of control: the most counts, then closest to the centre of the range, then (bins k
        // and bins - 1 - k tie there) closest to the close, then the lower bin. The close keeps the
        // choice mirror symmetric.
        let centre = |k: usize| lo + (k as f64 + 0.5) * step;
        let mut poc = 0;
        let mut poc_distance = u64::MAX;
        for (k, &c) in count.iter().enumerate() {
            let distance = (2 * k as i64 - (bins as i64 - 1)).unsigned_abs();
            let better = c > count[poc]
                || (c == count[poc]
                    && (distance < poc_distance
                        || (distance == poc_distance
                            && (centre(k) - bar.close).abs() < (centre(poc) - bar.close).abs())));
            if better {
                poc = k;
                poc_distance = distance;
            }
        }

        let total: u64 = count.iter().sum();
        let target = 0.7 * total as f64;
        let (mut bottom, mut top) = (poc, poc);
        let mut sum = count[poc];
        while (sum as f64) < target {
            let above = (top + 1 < bins).then(|| count[top + 1]);
            let below = (bottom > 0).then(|| count[bottom - 1]);
            match (above, below) {
                (Some(a), Some(b)) => {
                    if a > b {
                        top += 1;
                        sum += a;
                    } else if b > a {
                        bottom -= 1;
                        sum += b;
                    } else {
                        top += 1;
                        bottom -= 1;
                        sum += a + b;
                    }
                }
                (Some(a), None) => {
                    top += 1;
                    sum += a;
                }
                (None, Some(b)) => {
                    bottom -= 1;
                    sum += b;
                }
                (None, None) => break,
            }
        }
        Some(IndicatorOutput::with_extra(
            lo + (poc as f64 + 0.5) * step,
            extras(lo + (top as f64 + 1.0) * step, lo + bottom as f64 * step),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const TPO_PROFILE_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "tpo_profile",
        aliases: &[],
        description: "Time-at-price profile after Steidlmayer and Koy (1986) without volume: one count per bar and price bin over the window, value the point-of-control bin centre (ties toward the range centre, then toward the close), extras vah and val the 70 % value area edges",
        params: &[
            super::definition::ParamSpec::count("lookback", 48, 1, 10000)
                .meaning("bars in the profile window"),
            super::definition::ParamSpec::count("num_bins", 30, 1, 1000)
                .meaning("price bins across the window's range"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price().range_dependent(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| {
            Ok(Box::new(TpoProfileEngine::new(
                p.count("lookback"),
                p.count("num_bins"),
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lookback: usize, bins: usize, hl: &[(f64, f64)]) -> IndicatorOutput {
        let mut engine = TpoProfileEngine::new(lookback, bins);
        let mut last = None;
        for (i, (h, l)) in hl.iter().enumerate() {
            last = engine.on_bar(&Bar::new(i as i64, *l, *h, *l, *h, 0.0));
        }
        last.unwrap()
    }

    #[test]
    fn known_profile_with_edges_and_an_equal_neighbour_step() {
        // lo 0, hi 8, four bins of 2: counts [2, 3, 2, 1]. (2, 4) sits on edges and touches bins
        // 0..2; (3, 3.5) only bin 1. POC is bin 1; its neighbours tie at 2, so the value area takes
        // both at once: 7 of 8 counts, bins 0..2.
        let out = feed(3, 4, &[(8.0, 0.0), (4.0, 2.0), (3.5, 3.0)]);
        assert_eq!(out.value, 3.0);
        assert_eq!(out.extra["val"], 0.0);
        assert_eq!(out.extra["vah"], 6.0);
    }

    #[test]
    fn poc_tie_resolves_toward_the_range_centre_then_the_close() {
        // Counts [2, 2, 0, 1]: bins 0 and 1 tie, bin 1 is closer to the centre (1.5).
        let out = feed(3, 4, &[(2.0, 0.0), (2.0, 0.0), (8.0, 8.0)]);
        assert_eq!(out.value, 3.0);
        // Counts [3, 0, 0, 3]: bins 0 and 3 are equally far from the centre; the close (8) is
        // nearer bin 3.
        let out = feed(
            6,
            4,
            &[
                (1.0, 0.0),
                (1.0, 0.0),
                (1.0, 0.0),
                (8.0, 7.0),
                (8.0, 7.0),
                (8.0, 7.0),
            ],
        );
        assert_eq!(out.value, 7.0);
        // Counts [1, 3, 3, 1]: bins 1 and 2 are equally far from the centre; the close 5.5 is
        // nearer bin 2.
        let out = feed(4, 4, &[(8.0, 0.0), (5.5, 2.5), (5.5, 2.5), (5.5, 2.5)]);
        assert_eq!(out.value, 5.0);
        // Counts [1, 4, 4, 1] and the close 4 exactly between both centres: the lower bin.
        let out = feed(4, 4, &[(8.0, 0.0), (5.5, 2.5), (5.5, 2.5), (4.0, 4.0)]);
        assert_eq!(out.value, 3.0);
    }

    #[test]
    fn a_window_without_range_publishes_the_close() {
        let out = feed(3, 5, &[(7.0, 7.0), (7.0, 7.0), (7.0, 7.0)]);
        assert_eq!(
            (out.value, out.extra["vah"], out.extra["val"]),
            (7.0, 7.0, 7.0)
        );
    }

    #[test]
    fn publishes_from_the_lookback_bar_and_slides() {
        let mut engine = TpoProfileEngine::new(2, 2);
        assert!(engine
            .on_bar(&Bar::new(0, 1.0, 2.0, 1.0, 2.0, 0.0))
            .is_none());
        assert!(engine
            .on_bar(&Bar::new(1, 1.0, 2.0, 1.0, 2.0, 0.0))
            .is_some());
        // The first bar leaves the window: only (5, 4) and (6, 4) remain, lo 4, hi 6.
        engine.on_bar(&Bar::new(2, 4.0, 5.0, 4.0, 5.0, 0.0));
        let out = engine
            .on_bar(&Bar::new(3, 4.0, 6.0, 4.0, 6.0, 0.0))
            .unwrap();
        assert!(out.extra["val"] >= 4.0 && out.extra["vah"] <= 6.0);
    }
}
