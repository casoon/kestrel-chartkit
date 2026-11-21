//! Narrowest range of the last `period` bars (NR4, NR7) and the inside bar after Crabel (1990).

use std::collections::{HashMap, VecDeque};

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Narrow-range detector. `value` is `1` when the bar's range is strictly smaller than the range of
/// each of the `period - 1` bars before it, else `0`; the extra `inside` is `1` when the bar's high
/// is at most and its low at least those of the previous bar. Alert `narrow_range` on a bar with
/// `value` 1. First output: with the `period`-th bar. [`Indicator::reset`] clears the window.
pub struct NarrowRangeEngine {
    period: usize,
    ranges: VecDeque<f64>,
    previous: Option<(f64, f64)>,
    alerts: Vec<IndicatorAlert>,
}

impl NarrowRangeEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            ranges: VecDeque::new(),
            previous: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for NarrowRangeEngine {
    fn name(&self) -> &str {
        "narrow_range"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.ranges.clear();
        self.previous = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let range = bar.high - bar.low;
        self.ranges.push_back(range);
        if self.ranges.len() > self.period {
            self.ranges.pop_front();
        }
        let inside = self
            .previous
            .map(|(h, l)| bar.high <= h && bar.low >= l)
            .unwrap_or(false);
        self.previous = Some((bar.high, bar.low));
        if self.ranges.len() < self.period {
            return None;
        }
        let n = self.ranges.len();
        let narrowest = self.ranges.iter().take(n - 1).all(|r| range < *r);
        if narrowest {
            self.alerts.push(IndicatorAlert::new(
                "narrow_range",
                format!("narrowest range of the last {} bars", self.period),
                1.0,
            ));
        }
        Some(IndicatorOutput::with_extra(
            if narrowest { 1.0 } else { 0.0 },
            HashMap::from([("inside".to_string(), if inside { 1.0 } else { 0.0 })]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const NARROW_RANGE_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "narrow_range",
        aliases: &[],
        description: "Narrowest range of the last period bars (NR4, NR7) after Crabel (1990): 1 when the bar's high-low range is strictly smaller than each of the period - 1 bars before it; extra inside marks a bar within the previous bar's range",
        params: &[
            super::definition::ParamSpec::count("period", 7, 2, 10000)
                .meaning("bars compared, the bar itself included"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::ratio()
            .bounded(0.0, 1.0)
            .range_dependent(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| Ok(Box::new(NarrowRangeEngine::new(p.count("period")))),
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, h: f64, l: f64) -> Bar {
        Bar::new(t, l, h, l, h, 100.0)
    }

    fn out(o: &Option<IndicatorOutput>) -> (f64, f64) {
        let o = o.as_ref().unwrap();
        (o.value, o.extra["inside"])
    }

    #[test]
    fn nr7_needs_a_strictly_narrower_range_than_each_of_the_six_before() {
        let mut engine = NarrowRangeEngine::new(7);
        let ranges = [5.0, 4.0, 6.0, 3.0, 5.0, 4.5, 2.0, 3.0, 2.0];
        let mut results = Vec::new();
        for (i, r) in ranges.iter().enumerate() {
            results.push(engine.on_bar(&bar(i as i64, 100.0 + r, 100.0)));
        }
        assert!(results[..6].iter().all(Option::is_none));
        // Bar 6: 2.0 is below all of 5, 4, 6, 3, 5, 4.5.
        assert_eq!(out(&results[6]).0, 1.0);
        // Bar 7: 3.0 is not below 2.0 (bar 6).
        assert_eq!(out(&results[7]).0, 0.0);
        // Bar 8: 2.0 equals the 2.0 of bar 6: not strictly narrower.
        assert_eq!(out(&results[8]).0, 0.0);
    }

    #[test]
    fn alert_fires_with_value_one_only() {
        let mut engine = NarrowRangeEngine::new(3);
        engine.on_bar(&bar(0, 105.0, 100.0));
        engine.on_bar(&bar(1, 104.0, 100.0));
        engine.on_bar(&bar(2, 102.0, 100.0));
        assert_eq!(engine.alerts()[0].kind, "narrow_range");
        engine.on_bar(&bar(3, 103.0, 100.0));
        assert!(engine.alerts().is_empty());
    }

    #[test]
    fn inside_bar_allows_equal_edges() {
        let mut engine = NarrowRangeEngine::new(2);
        engine.on_bar(&bar(0, 110.0, 100.0));
        // Same high, higher low: inside; also narrower.
        assert_eq!(out(&engine.on_bar(&bar(1, 110.0, 102.0))), (1.0, 1.0));
        // Higher high: not inside.
        assert_eq!(out(&engine.on_bar(&bar(2, 110.5, 109.0))), (1.0, 0.0));
        // Lower low: not inside.
        assert_eq!(out(&engine.on_bar(&bar(3, 109.5, 108.5))), (1.0, 0.0));
    }
}
