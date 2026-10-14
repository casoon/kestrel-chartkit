//! Leg geometry: the legs price runs in and how far it has come back.
//!
//! | function | what it gives |
//! |---|---|
//! | [`window_leg`] | the last leg inside the final `len` bars: from the lowest low to the highest high, ordered by time (lowest low = earliest if tied, highest high = latest if tied) |
//! | [`retracement`] | how much of a leg `from → to` a price has given back, and in which zone that lies |
//! | [`largest_pullback`] | the largest move against a trend since the last bar that reached its extreme |
//! | [`swing_sequence`] | higher/lower highs and lows from the last two of each |
//! | [`strict_pivots`] | bars whose high (low) is strictly above (below) every other bar within `width` on both sides |
//! | [`threshold_zigzag`] | a zigzag whose reversal needs a **close** a per-bar distance beyond the running extreme |
//! | [`path_efficiency`] | net distance over summed absolute steps of a sequence of prices |
//! | [`swing_overlap`] | the share of pullbacks in a run of swings that reach back below (above) the high (low) before the last |
//!
//! Golden reference: `reference/kestrel_reference/fixtures/leg_geometry.py`.

use crate::model::Bar;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A leg from `from` (price at `from_index`) to `to` (price at `to_index`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Leg {
    pub from: f64,
    pub to: f64,
    /// Indices into the slice the leg was found in.
    pub from_index: usize,
    pub to_index: usize,
    /// `true` for a leg from a low up to a high.
    pub up: bool,
}

impl Leg {
    /// Bars from start to end.
    pub fn bars(&self) -> usize {
        self.to_index - self.from_index
    }

    /// `|to − from|`.
    pub fn size(&self) -> f64 {
        (self.to - self.from).abs()
    }
}

/// The last leg in the final `len` bars of `bars`: lowest low and highest high of that window,
/// the earlier one is the start. Lowest low = the earliest of equal lows, highest high = the
/// latest of equal highs. Indices refer to `bars`. `None` for fewer than `len` bars or `len = 0`.
pub fn window_leg(bars: &[Bar], len: usize) -> Option<Leg> {
    if len == 0 || bars.len() < len {
        return None;
    }
    let start = bars.len() - len;
    let mut lo = start;
    let mut hi = start;
    for i in start..bars.len() {
        if bars[i].low < bars[lo].low {
            lo = i;
        }
        if bars[i].high >= bars[hi].high {
            hi = i;
        }
    }
    Some(if hi > lo {
        Leg {
            from: bars[lo].low,
            to: bars[hi].high,
            from_index: lo,
            to_index: hi,
            up: true,
        }
    } else {
        Leg {
            from: bars[hi].high,
            to: bars[lo].low,
            from_index: hi,
            to_index: lo,
            up: false,
        }
    })
}

/// Where a price lies relative to a leg it retraces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RetracementZone {
    /// Beyond the leg's end: the leg extends (`share < 0`).
    Extension,
    /// `0 ≤ share < deep`.
    Correction,
    /// `deep ≤ share ≤ 1`.
    DeepCorrection,
    /// Beyond the leg's start (`share > 1`).
    Reversal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Retracement {
    /// `(to − price) / (to − from)`: 0 at the leg's end, 1 at its start.
    pub share: f64,
    pub zone: RetracementZone,
}

/// How much of the leg `from → to` the `price` has given back; `deep` is the share from which a
/// correction counts as deep (0.618 conventionally). `None` for a leg of zero size.
pub fn retracement(from: f64, to: f64, price: f64, deep: f64) -> Option<Retracement> {
    let size = to - from;
    if size == 0.0 || !size.is_finite() {
        return None;
    }
    let share = (to - price) / size;
    let zone = if share < 0.0 {
        RetracementZone::Extension
    } else if share < deep {
        RetracementZone::Correction
    } else if share <= 1.0 {
        RetracementZone::DeepCorrection
    } else {
        RetracementZone::Reversal
    };
    Some(Retracement { share, zone })
}

/// The largest pullback against a trend before bar `i`: for an up trend, `extreme` minus the
/// lowest low of `bars[k..i]`, where `k` is the last bar before `i` whose high reached `extreme`
/// (0 if none did); for a down trend mirrored with highs. An empty window gives `extreme − MAX`
/// resp. `MIN − extreme` (a large negative number) — callers pass `i > k`.
pub fn largest_pullback(bars: &[Bar], i: usize, extreme: f64, up: bool) -> f64 {
    let from = (0..i)
        .rev()
        .find(|&k| {
            if up {
                bars[k].high >= extreme
            } else {
                bars[k].low <= extreme
            }
        })
        .unwrap_or(0);
    let window = &bars[from..i];
    if up {
        extreme - window.iter().map(|b| b.low).fold(f64::MAX, f64::min)
    } else {
        window.iter().map(|b| b.high).fold(f64::MIN, f64::max) - extreme
    }
}

/// Swing sequence from the last two highs and lows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SwingSequence {
    /// Higher high and higher low.
    Up,
    /// Lower high and lower low.
    Down,
    Mixed,
    /// Fewer than two highs or two lows.
    Unknown,
}

/// From the last two entries of `highs` and `lows` (oldest first): both higher → `Up`, both
/// lower → `Down`, otherwise `Mixed`.
pub fn swing_sequence(highs: &[f64], lows: &[f64]) -> SwingSequence {
    let last_two = |v: &[f64]| (v.len() >= 2).then(|| (v[v.len() - 2], v[v.len() - 1]));
    let (Some(h), Some(l)) = (last_two(highs), last_two(lows)) else {
        return SwingSequence::Unknown;
    };
    if h.1 > h.0 && l.1 > l.0 {
        SwingSequence::Up
    } else if h.1 < h.0 && l.1 < l.0 {
        SwingSequence::Down
    } else {
        SwingSequence::Mixed
    }
}

/// Strict pivots of width `width`: `(lows, highs)`, one flag per bar. Bar `j` is a pivot low if
/// its low is strictly below every other low in `bars[j − width ..= j + width]` (highs mirrored);
/// known from bar `j + width` on. The first and last `width` bars are never pivots. Unlike
/// [`super::confirmed_pivots`] (non-strict), two equal extremes cancel each other.
pub fn strict_pivots(bars: &[Bar], width: usize) -> (Vec<bool>, Vec<bool>) {
    let n = bars.len();
    let mut lows = vec![false; n];
    let mut highs = vec![false; n];
    for j in width..n.saturating_sub(width) {
        let window = &bars[j - width..=j + width];
        lows[j] = window
            .iter()
            .enumerate()
            .all(|(k, b)| k == width || b.low > bars[j].low);
        highs[j] = window
            .iter()
            .enumerate()
            .all(|(k, b)| k == width || b.high < bars[j].high);
    }
    (lows, highs)
}

/// A confirmed pivot of [`threshold_zigzag`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ZigZagPivot {
    pub index: usize,
    pub price: f64,
    pub is_high: bool,
    /// Index of the bar whose close confirmed it.
    pub confirmed_index: usize,
}

/// Zigzag over highs and lows with a per-bar reversal distance. A high is confirmed on the first
/// bar after it whose close is at least `threshold[i]` below it (a low mirrored); bars without a
/// threshold confirm nothing but still extend the running extreme. Pivots alternate.
///
/// Before the first pivot both the running high and the running low are tracked (a new extreme
/// must be strictly beyond the old one); whichever is confirmed first starts the sequence, a high
/// being checked before a low on the same bar. After a confirmed high the running low restarts at
/// the lowest low after it up to the confirming bar (the earliest of equal lows); after a confirmed
/// low the running high restarts at the highest high after it (the latest of equal highs).
pub fn threshold_zigzag(bars: &[Bar], threshold: &[Option<f64>]) -> Vec<ZigZagPivot> {
    let mut out = Vec::new();
    let (mut hi, mut lo) = (0usize, 0usize);
    let mut up: Option<bool> = None;
    let lowest_after = |from: usize, to: usize| {
        let mut best = from;
        for k in from..=to {
            if bars[k].low < bars[best].low {
                best = k;
            }
        }
        best
    };
    let highest_after = |from: usize, to: usize| {
        let mut best = from;
        for k in from..=to {
            if bars[k].high >= bars[best].high {
                best = k;
            }
        }
        best
    };
    for (i, b) in bars.iter().enumerate() {
        match up {
            None => {
                if b.high > bars[hi].high {
                    hi = i;
                }
                if b.low < bars[lo].low {
                    lo = i;
                }
                let Some(t) = threshold.get(i).copied().flatten() else {
                    continue;
                };
                if b.close <= bars[hi].high - t && hi < i {
                    out.push(ZigZagPivot {
                        index: hi,
                        price: bars[hi].high,
                        is_high: true,
                        confirmed_index: i,
                    });
                    lo = lowest_after(hi + 1, i);
                    up = Some(false);
                } else if b.close >= bars[lo].low + t && lo < i {
                    out.push(ZigZagPivot {
                        index: lo,
                        price: bars[lo].low,
                        is_high: false,
                        confirmed_index: i,
                    });
                    hi = highest_after(lo + 1, i);
                    up = Some(true);
                }
            }
            Some(true) => {
                if b.high > bars[hi].high {
                    hi = i;
                }
                let Some(t) = threshold.get(i).copied().flatten() else {
                    continue;
                };
                if hi < i && b.close <= bars[hi].high - t {
                    out.push(ZigZagPivot {
                        index: hi,
                        price: bars[hi].high,
                        is_high: true,
                        confirmed_index: i,
                    });
                    lo = lowest_after(hi + 1, i);
                    up = Some(false);
                }
            }
            Some(false) => {
                if b.low < bars[lo].low {
                    lo = i;
                }
                let Some(t) = threshold.get(i).copied().flatten() else {
                    continue;
                };
                if lo < i && b.close >= bars[lo].low + t {
                    out.push(ZigZagPivot {
                        index: lo,
                        price: bars[lo].low,
                        is_high: false,
                        confirmed_index: i,
                    });
                    hi = highest_after(lo + 1, i);
                    up = Some(true);
                }
            }
        }
    }
    out
}

/// `|last − first| / Σ |pᵢ − pᵢ₋₁|` over `points`; `0` when nothing moved, `None` for fewer than
/// two points. 1 = a straight run, near 0 = much travel, little progress.
pub fn path_efficiency(points: &[f64]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let path: f64 = points.windows(2).map(|p| (p[1] - p[0]).abs()).sum();
    Some(if path > 0.0 {
        (points[points.len() - 1] - points[0]).abs() / path
    } else {
        0.0
    })
}

/// Share of pullbacks that overlap, for a run in direction `up` starting at `anchor` and passing
/// the swing points `swings` (`(price, is_high)`, oldest first). The anchor and every swing in the
/// run's direction (`is_high == up`) are the extremes; each swing against it with at least two
/// extremes before it is a pullback, and it overlaps if it lies beyond the extreme before the
/// last (below it for an up run, above for a down run). `None` without such a pullback.
pub fn swing_overlap(anchor: f64, swings: &[(f64, bool)], up: bool) -> Option<f64> {
    let mut extremes = vec![anchor];
    let (mut cases, mut overlapping) = (0usize, 0usize);
    for &(price, is_high) in swings {
        if is_high == up {
            extremes.push(price);
        } else if extremes.len() >= 2 {
            cases += 1;
            let before_last = extremes[extremes.len() - 2];
            if (up && price < before_last) || (!up && price > before_last) {
                overlapping += 1;
            }
        }
    }
    (cases > 0).then(|| overlapping as f64 / cases as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(i: i64, h: f64, l: f64, c: f64) -> Bar {
        Bar::new(i, c, h, l, c, 0.0)
    }

    #[test]
    fn retracement_zones_follow_the_share() {
        let z = |p| retracement(100.0, 200.0, p, 0.618).unwrap().zone;
        assert_eq!(z(210.0), RetracementZone::Extension);
        assert_eq!(z(170.0), RetracementZone::Correction);
        assert_eq!(z(130.0), RetracementZone::DeepCorrection);
        assert_eq!(z(90.0), RetracementZone::Reversal);
        // Mirrored for a down leg.
        let d = |p| retracement(200.0, 100.0, p, 0.618).unwrap().zone;
        assert_eq!(d(90.0), RetracementZone::Extension);
        assert_eq!(d(130.0), RetracementZone::Correction);
        assert!(retracement(1.0, 1.0, 1.0, 0.618).is_none());
    }

    #[test]
    fn window_leg_orders_by_time() {
        let bars = vec![
            bar(0, 11.0, 9.0, 10.0),
            bar(1, 15.0, 10.0, 14.0),
            bar(2, 14.0, 8.0, 9.0),
        ];
        let leg = window_leg(&bars, 3).unwrap();
        assert!(!leg.up);
        assert_eq!((leg.from, leg.to, leg.bars()), (15.0, 8.0, 1));
    }
}
