//! Rank of an indicator's value against its own recent history, for any [`Indicator`].
//!
//! Many lines have no fixed scale (TRIX, VIDYA, PVT, ...) or no alerts of their own, so a fixed
//! threshold cannot compare them. [`RankChannel`] wraps any indicator and adds a scale-free
//! reading: `extra["rank"]` is the share (0..=100) of the previous `lookback` values that lie
//! strictly below the current `value`. Alerts fire when the rank **enters** an extreme zone:
//!
//! | alert | condition |
//! |---|---|
//! | `bull_rank_extreme` | rank ≤ `low` now, > `low` on the previous ranked bar |
//! | `bear_rank_extreme` | rank ≥ `high` now, < `high` on the previous ranked bar |
//!
//! The prefix reads the extreme as a reversion (an unusually low value → up). Whether that or the
//! opposite reading holds is an empirical question; consumers that want the continuation reading
//! swap the sides. The inner indicator's own output and alerts pass through unchanged; the rank
//! starts once `lookback` earlier values exist, and the first ranked bar cannot alert (there is no
//! previous rank).

use std::collections::VecDeque;

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Default history for the rank.
pub const DEFAULT_LOOKBACK: usize = 200;
/// Default extreme zones.
pub const DEFAULT_LOW: f64 = 5.0;
pub const DEFAULT_HIGH: f64 = 95.0;

/// Wraps `inner` and adds `extra["rank"]` plus `bull_/bear_rank_extreme` alerts.
pub struct RankChannel<I: Indicator> {
    inner: I,
    lookback: usize,
    low: f64,
    high: f64,
    history: VecDeque<f64>,
    prev_rank: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl<I: Indicator> RankChannel<I> {
    pub fn new(inner: I, lookback: usize, low: f64, high: f64) -> Self {
        Self {
            inner,
            lookback: lookback.max(1),
            low,
            high,
            history: VecDeque::with_capacity(lookback + 1),
            prev_rank: None,
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults(inner: I) -> Self {
        Self::new(inner, DEFAULT_LOOKBACK, DEFAULT_LOW, DEFAULT_HIGH)
    }
}

impl<I: Indicator> Indicator for RankChannel<I> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// The inner indicator's warmup: its value passes through unchanged from then on. `rank` is an
    /// additional channel that appears `lookback` values later; consumers check for the key.
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.history.clear();
        self.prev_rank = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let mut out = self.inner.on_bar(bar)?;
        let v = out.value;
        if !v.is_finite() {
            return Some(out);
        }
        if self.history.len() == self.lookback {
            let below = self.history.iter().filter(|h| **h < v).count();
            let rank = 100.0 * below as f64 / self.lookback as f64;
            out.extra.insert("rank".to_string(), rank);
            if let Some(prev) = self.prev_rank {
                if rank <= self.low && prev > self.low {
                    self.alerts.push(IndicatorAlert::new(
                        "bull_rank_extreme",
                        format!(
                            "{} at rank {rank:.0} of its last {}",
                            self.inner.name(),
                            self.lookback
                        ),
                        ((self.low - rank) / self.low.max(1e-9)).clamp(0.05, 1.0),
                    ));
                } else if rank >= self.high && prev < self.high {
                    self.alerts.push(IndicatorAlert::new(
                        "bear_rank_extreme",
                        format!(
                            "{} at rank {rank:.0} of its last {}",
                            self.inner.name(),
                            self.lookback
                        ),
                        ((rank - self.high) / (100.0 - self.high).max(1e-9)).clamp(0.05, 1.0),
                    ));
                }
            }
            self.prev_rank = Some(rank);
            self.history.pop_front();
        }
        self.history.push_back(v);
        Some(out)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut a = self.inner.alerts();
        a.extend(self.alerts.iter().cloned());
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emits the bar's close as its value, nothing else.
    struct Close;
    impl Indicator for Close {
        fn name(&self) -> &str {
            "close"
        }
        fn reset(&mut self) {}
        fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
            Some(IndicatorOutput::new(bar.close))
        }
    }

    fn bar(c: f64) -> Bar {
        Bar::new(0, c, c, c, c, 0.0)
    }

    /// Hand-computed, lookback 4, zones 25/75:
    ///
    /// | bar | value | previous 4 | below | rank | alert |
    /// |---|---|---|---|---|---|
    /// | 0–3 | 10 11 12 13 | – | – | – | – |
    /// | 4 | 14 | 10 11 12 13 | 4 | 100 | none (first rank) |
    /// | 5 | 12.5 | 11 12 13 14 | 2 | 50 | none |
    /// | 6 | 10.5 | 12 13 14 12.5 | 0 | 0 | bull (50 > 25) |
    /// | 7 | 10.4 | 13 14 12.5 10.5 | 0 | 0 | none (already in zone) |
    /// | 8 | 20 | 14 12.5 10.5 10.4 | 4 | 100 | bear (0 < 75) |
    #[test]
    fn rank_and_zone_entries_match_the_hand_computation() {
        let mut r = RankChannel::new(Close, 4, 25.0, 75.0);
        let values = [10.0, 11.0, 12.0, 13.0, 14.0, 12.5, 10.5, 10.4, 20.0];
        let expected_rank = [
            None,
            None,
            None,
            None,
            Some(100.0),
            Some(50.0),
            Some(0.0),
            Some(0.0),
            Some(100.0),
        ];
        let expected_alert = [
            None,
            None,
            None,
            None,
            None,
            None,
            Some("bull_rank_extreme"),
            None,
            Some("bear_rank_extreme"),
        ];
        for (i, v) in values.iter().enumerate() {
            let out = r.on_bar(&bar(*v)).unwrap();
            assert_eq!(out.extra.get("rank").copied(), expected_rank[i], "bar {i}");
            let kinds: Vec<String> = r.alerts().into_iter().map(|a| a.kind).collect();
            assert_eq!(
                kinds.first().map(String::as_str),
                expected_alert[i],
                "bar {i}"
            );
            assert_eq!(out.value, *v);
        }
    }

    #[test]
    fn warmup_is_the_inner_one() {
        assert_eq!(
            RankChannel::new(Close, 200, 5.0, 95.0).warmup_period(),
            Close.warmup_period()
        );
    }

    #[test]
    fn reset_starts_the_history_over() {
        let mut r = RankChannel::new(Close, 2, 25.0, 75.0);
        for v in [1.0, 2.0, 3.0] {
            r.on_bar(&bar(v));
        }
        r.reset();
        assert!(!r.on_bar(&bar(5.0)).unwrap().extra.contains_key("rank"));
    }
}
