//! Double bottom and double top as a streaming event (Kestrel plan/58 stage 2).
//!
//! [`super::chart_patterns`] finds the same shape by scanning finished ZigZag nodes; this is the
//! causal counterpart that reports while the pattern forms, so it can be an alert.
//!
//! **Pivots** confirm `pivot_len` bars late: a pivot low is lower than every one of the
//! `pivot_len` bars before it and no higher than any of the `pivot_len` bars after it. Ties to the
//! right are allowed on purpose — two bars printing the same low are the first bottom, and a
//! strict rule on both sides would find no pivot there at all.
//!
//! **Forming** — reported on the bar that confirms the second pivot low (high), paired with the
//! most recent earlier pivot low within `max_gap` bars for which all of these hold (not just the
//! immediately preceding one — a minor low in the counter-swing must not hide the first bottom):
//! - it lies within `tolerance_atr` ATR of the new one,
//! - between them the high reached at least `min_depth_atr` ATR above the higher of the two
//!   lows (the counter-swing; its highest high is the **neckline**),
//! - they are at least `2 · pivot_len` and at most `max_gap` bars apart,
//! - no close in between fell below the lower of the two lows.
//!
//! **Confirmed** — the first close above the neckline (below it for a top) within `max_wait`
//! bars after forming. A close below the pattern's low (above its high) first **invalidates** it;
//! after `max_wait` bars it expires. Both end the pattern silently.
//!
//! Alerts: `bull_double_bottom_forming`, `bull_double_bottom`, `bear_double_top_forming`,
//! `bear_double_top`. On event bars `extra` carries `neckline`, `target` (neckline plus the
//! pattern height, the classic measured move) and `wick` — the invalidation level (the lower low
//! of a bottom, the higher high of a top). `wick` is the same key [`super::zone_rejection`] uses
//! for its natural stop, so a consumer that places stops there needs no second rule. `value` is
//! `+1`/`-1` on a forming bar, `+2`/`-2` on a confirming bar, else `0`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pivot {
    /// Bar index of the pivot itself.
    index: usize,
    price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Active {
    bottom: bool,
    neckline: f64,
    /// Invalidation level: the lower low (higher high).
    extreme: f64,
    formed_at: usize,
}

#[derive(Debug, Clone)]
pub struct DoublePattern {
    pivot_len: usize,
    atr_len: usize,
    tolerance_atr: f64,
    min_depth_atr: f64,
    max_gap: usize,
    max_wait: usize,

    index: usize,
    window: VecDeque<Bar>,
    /// Bars since the oldest pivot still in play — enough for neckline and in-between checks.
    history: VecDeque<Bar>,
    atr: Rma,
    atr_value: Option<f64>,
    prev_close: Option<f64>,
    lows: VecDeque<Pivot>,
    highs: VecDeque<Pivot>,
    active: Vec<Active>,

    event: i8,
    neckline: f64,
    target: f64,
    wick: f64,
}

impl DoublePattern {
    pub fn new(
        pivot_len: usize,
        atr_len: usize,
        tolerance_atr: f64,
        min_depth_atr: f64,
        max_gap: usize,
        max_wait: usize,
    ) -> Self {
        let pivot_len = pivot_len.max(1);
        Self {
            pivot_len,
            atr_len: atr_len.max(1),
            tolerance_atr: tolerance_atr.max(0.0),
            min_depth_atr: min_depth_atr.max(0.0),
            max_gap: max_gap.max(2 * pivot_len),
            max_wait: max_wait.max(1),
            index: 0,
            window: VecDeque::with_capacity(2 * pivot_len + 1),
            history: VecDeque::new(),
            atr: Rma::new(atr_len.max(1)),
            atr_value: None,
            prev_close: None,
            lows: VecDeque::new(),
            highs: VecDeque::new(),
            active: Vec::new(),
            event: 0,
            neckline: 0.0,
            target: 0.0,
            wick: 0.0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5, 14, 0.5, 1.5, 100, 50)
    }

    /// The bars from index `from` up to (excluding) `to`, out of `history`.
    fn bars_between(&self, from: usize, to: usize) -> impl Iterator<Item = &Bar> {
        let first = self.index + 1 - self.history.len();
        self.history
            .iter()
            .enumerate()
            .filter(move |(i, _)| {
                let at = first + i;
                at > from && at < to
            })
            .map(|(_, b)| b)
    }

    /// Checks a new pivot against the previous one of its kind; returns the pattern it completes.
    fn pair(&self, previous: Pivot, current: Pivot, bottom: bool, atr: f64) -> Option<Active> {
        let gap = current.index - previous.index;
        if gap < 2 * self.pivot_len || gap > self.max_gap {
            return None;
        }
        if (current.price - previous.price).abs() > self.tolerance_atr * atr {
            return None;
        }
        let between: Vec<&Bar> = self.bars_between(previous.index, current.index).collect();
        if between.is_empty() {
            return None;
        }
        if bottom {
            let low = previous.price.min(current.price);
            let neckline = between.iter().map(|b| b.high).fold(f64::MIN, f64::max);
            let higher_low = previous.price.max(current.price);
            if neckline - higher_low < self.min_depth_atr * atr
                || between.iter().any(|b| b.close < low)
            {
                return None;
            }
            Some(Active {
                bottom,
                neckline,
                extreme: low,
                formed_at: self.index,
            })
        } else {
            let high = previous.price.max(current.price);
            let neckline = between.iter().map(|b| b.low).fold(f64::MAX, f64::min);
            let lower_high = previous.price.min(current.price);
            if lower_high - neckline < self.min_depth_atr * atr
                || between.iter().any(|b| b.close > high)
            {
                return None;
            }
            Some(Active {
                bottom,
                neckline,
                extreme: high,
                formed_at: self.index,
            })
        }
    }

    fn set_event(&mut self, event: i8, pattern: &Active) {
        self.event = event;
        self.neckline = pattern.neckline;
        self.wick = pattern.extreme;
        let height = (pattern.neckline - pattern.extreme).abs();
        self.target = if pattern.bottom {
            pattern.neckline + height
        } else {
            pattern.neckline - height
        };
    }
}

impl Indicator for DoublePattern {
    fn name(&self) -> &str {
        "double_pattern"
    }

    fn warmup_period(&self) -> usize {
        (2 * self.pivot_len + 1).max(self.atr_len)
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.event = 0;
        self.index += 1;

        let true_range = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);
        let atr_now = self.atr.update(true_range);

        self.history.push_back(bar.clone());
        let keep = self.max_gap + 2 * self.pivot_len + 2;
        while self.history.len() > keep {
            self.history.pop_front();
        }

        // Active patterns first: they were formed on earlier bars.
        let mut confirmed: Option<Active> = None;
        let index = self.index;
        let max_wait = self.max_wait;
        self.active.retain(|p| {
            if confirmed.is_none() {
                let through = if p.bottom {
                    bar.close > p.neckline
                } else {
                    bar.close < p.neckline
                };
                if through {
                    confirmed = Some(*p);
                    return false;
                }
            }
            let broken = if p.bottom {
                bar.close < p.extreme
            } else {
                bar.close > p.extreme
            };
            !broken && index - p.formed_at <= max_wait
        });
        if let Some(p) = confirmed {
            self.set_event(if p.bottom { 2 } else { -2 }, &p);
        }

        // Pivots confirm `pivot_len` bars late.
        self.window.push_back(bar.clone());
        if self.window.len() > 2 * self.pivot_len + 1 {
            self.window.pop_front();
        }
        if let (Some(atr), true) = (self.atr_value, self.window.len() == 2 * self.pivot_len + 1) {
            let mid = self.pivot_len;
            let pivot = self.window[mid].clone();
            let pivot_index = self.index - self.pivot_len;
            let is_low = self
                .window
                .iter()
                .enumerate()
                .all(|(i, b)| i == mid || b.low > pivot.low || (i > mid && b.low == pivot.low));
            let is_high =
                self.window.iter().enumerate().all(|(i, b)| {
                    i == mid || b.high < pivot.high || (i > mid && b.high == pivot.high)
                });
            for (is_pivot, bottom, price) in
                [(is_low, true, pivot.low), (is_high, false, pivot.high)]
            {
                if !is_pivot {
                    continue;
                }
                let current = Pivot {
                    index: pivot_index,
                    price,
                };
                let earlier = if bottom { &self.lows } else { &self.highs };
                let found = earlier
                    .iter()
                    .rev()
                    .find_map(|previous| self.pair(*previous, current, bottom, atr));
                if let Some(p) = found {
                    self.active.push(p);
                    if self.event == 0 {
                        self.set_event(if bottom { 1 } else { -1 }, &p);
                    }
                }
                let max_gap = self.max_gap;
                let list = if bottom {
                    &mut self.lows
                } else {
                    &mut self.highs
                };
                list.push_back(current);
                while list
                    .front()
                    .is_some_and(|p| pivot_index - p.index > max_gap)
                {
                    list.pop_front();
                }
            }
        }
        self.atr_value = atr_now.or(self.atr_value);

        self.atr_value?;
        let mut extra = HashMap::new();
        if self.event != 0 {
            extra.insert("neckline".to_string(), self.neckline);
            extra.insert("target".to_string(), self.target);
            extra.insert("wick".to_string(), self.wick);
        }
        extra.insert("active".to_string(), self.active.len() as f64);
        Some(IndicatorOutput::with_extra(f64::from(self.event), extra))
    }

    fn reset(&mut self) {
        self.index = 0;
        self.window.clear();
        self.history.clear();
        self.atr.reset();
        self.atr_value = None;
        self.prev_close = None;
        self.lows.clear();
        self.highs.clear();
        self.active.clear();
        self.event = 0;
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let (kind, label, strength) = match self.event {
            1 => ("bull_double_bottom_forming", "DOUBLE BOTTOM · FORMING", 0.5),
            2 => ("bull_double_bottom", "DOUBLE BOTTOM · NECKLINE BROKEN", 1.0),
            -1 => ("bear_double_top_forming", "DOUBLE TOP · FORMING", 0.5),
            -2 => ("bear_double_top", "DOUBLE TOP · NECKLINE BROKEN", 1.0),
            _ => return Vec::new(),
        };
        vec![IndicatorAlert::new(kind, label, strength)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts: i64, close: f64) -> Bar {
        Bar::new(ts, close, close + 0.2, close - 0.2, close, 0.0)
    }

    /// Down to ~100, up to ~106, back down to ~100.3, then up through 106. Deliberately not
    /// symmetric: the second low is a little higher and the legs have different lengths.
    fn double_bottom() -> Vec<f64> {
        let mut c = vec![
            112.0, 111.0, 110.0, 109.2, 108.1, 107.0, 105.6, 104.4, 103.1, 101.9, 100.8, 100.0,
            100.7, 101.9, 103.2, 104.6, 105.8, 105.2, 104.1, 103.0, 102.2, 101.4, 100.9, 100.3,
            100.8, 101.6, 102.5, 103.4, 104.3, 105.1, 105.7, 106.4, 107.0,
        ];
        c.extend([107.5, 107.9]);
        c
    }

    fn run(
        ind: &mut DoublePattern,
        closes: &[f64],
    ) -> Vec<(i64, Vec<String>, Option<IndicatorOutput>)> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let out = ind.on_bar(&bar(i as i64, c));
                let kinds = ind
                    .alerts()
                    .into_iter()
                    .map(|a| a.kind.to_string())
                    .collect();
                (i as i64, kinds, out)
            })
            .collect()
    }

    #[test]
    fn double_bottom_forms_then_confirms_through_the_neckline() {
        let mut ind = DoublePattern::new(3, 5, 1.0, 1.5, 60, 40);
        let events = run(&mut ind, &double_bottom());
        let forming: Vec<_> = events
            .iter()
            .filter(|(_, k, _)| k.iter().any(|k| k == "bull_double_bottom_forming"))
            .collect();
        assert_eq!(forming.len(), 1, "{events:?}");
        let (formed_at, _, out) = forming[0];
        // The second low (index 23) confirms three bars later.
        assert_eq!(*formed_at, 26);
        let out = out.as_ref().unwrap();
        assert_eq!(out.value, 1.0);
        assert!(
            (out.extra["wick"] - 99.8).abs() < 1e-9,
            "lower low minus bar range"
        );
        assert!((out.extra["neckline"] - 106.0).abs() < 1e-9);

        let confirmed: Vec<_> = events
            .iter()
            .filter(|(_, k, _)| k.iter().any(|k| k == "bull_double_bottom"))
            .collect();
        assert_eq!(confirmed.len(), 1);
        let (at, _, out) = confirmed[0];
        // First close above 106.0 is 106.4 at index 31.
        assert_eq!(*at, 31);
        let out = out.as_ref().unwrap();
        assert_eq!(out.value, 2.0);
        assert!((out.extra["target"] - (106.0 + (106.0 - 99.8))).abs() < 1e-9);
    }

    #[test]
    fn a_close_below_the_low_invalidates_before_confirmation() {
        let mut ind = DoublePattern::new(3, 5, 1.0, 1.5, 60, 40);
        let mut closes = double_bottom();
        closes.truncate(28);
        closes.extend([102.0, 100.5, 99.0, 103.0, 106.5, 107.0]);
        let events = run(&mut ind, &closes);
        assert!(!events
            .iter()
            .any(|(_, k, _)| k.iter().any(|k| k == "bull_double_bottom")));
    }

    #[test]
    fn a_shallow_counter_swing_is_no_pattern() {
        // Same lows, but the bounce between them is barely above them.
        let closes = [
            110.0, 108.0, 106.0, 104.0, 102.0, 100.0, 100.6, 101.1, 100.9, 100.6, 100.2, 101.0,
            102.0, 103.0, 104.0, 105.0,
        ];
        let mut ind = DoublePattern::new(2, 5, 1.0, 1.5, 60, 40);
        let events = run(&mut ind, &closes);
        assert!(events.iter().all(|(_, k, _)| k.is_empty()), "{events:?}");
    }

    /// Natural Gas 2026-10-02 in Form: zwischen den beiden gleichen Tiefs liegt ein höheres
    /// Zwischentief. Es darf das erste Tief nicht verdecken.
    #[test]
    fn an_intermediate_low_does_not_hide_the_first_bottom() {
        let closes = [
            108.0, 106.5, 105.0, 103.6, 102.2, 101.0, 100.0, 100.9, 102.0, 103.4, 104.8, 106.0,
            105.1, 104.0, 103.1, 102.6, 103.3, 104.1, 103.5, 102.4, 101.3, 100.4, 100.1, 100.9,
            101.8, 102.9, 104.0, 105.2, 106.3, 107.1, 107.6,
        ];
        let mut ind = DoublePattern::new(2, 5, 1.0, 1.5, 60, 40);
        let events = run(&mut ind, &closes);
        let forming = events
            .iter()
            .find(|(_, k, _)| k.iter().any(|k| k == "bull_double_bottom_forming"))
            .expect("Doppelboden trotz Zwischentief");
        let out = forming.2.as_ref().unwrap();
        assert!((out.extra["wick"] - 99.8).abs() < 1e-9);
        assert!((out.extra["neckline"] - 106.2).abs() < 1e-9);
        assert!(events
            .iter()
            .any(|(_, k, _)| k.iter().any(|k| k == "bull_double_bottom")));
    }

    /// Zwei Kerzen mit demselben Tief bilden das erste Tief (Natural Gas 2026-10-02, 08:30 und
    /// 09:00 bei 3,0217).
    #[test]
    fn equal_lows_still_make_a_pivot() {
        // ATR über zwei Bars, damit sie bei der Pivot-Bestätigung schon steht.
        let mut ind = DoublePattern::new(2, 2, 1.0, 1.5, 60, 40);
        let lows = [104.0, 102.0, 100.0, 100.0, 101.0, 102.5, 104.0];
        let mut found = false;
        for (i, &l) in lows.iter().enumerate() {
            ind.on_bar(&Bar::new(i as i64, l + 0.5, l + 1.0, l, l + 0.5, 0.0));
            found |= ind.lows.iter().any(|p| p.index == 3 && p.price == 100.0);
        }
        assert!(found, "das erste der beiden gleichen Tiefs ist der Pivot");
    }

    #[test]
    fn double_top_mirrors() {
        let closes: Vec<f64> = double_bottom().iter().map(|c| 212.0 - c).collect();
        let mut ind = DoublePattern::new(3, 5, 1.0, 1.5, 60, 40);
        let events = run(&mut ind, &closes);
        assert!(events
            .iter()
            .any(|(_, k, _)| k.iter().any(|k| k == "bear_double_top_forming")));
        assert!(events
            .iter()
            .any(|(_, k, _)| k.iter().any(|k| k == "bear_double_top")));
    }
}
