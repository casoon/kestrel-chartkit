//! Reversal type labels for research: how each confirmed pivot reversal actually played out.
//!
//! Port of the Pine Script "Reversal Type Classifier v1" (WavesUnchained) as a batch function.
//!
//! **Uses the bars after the pivot.** A label needs the `pivot_len` confirmation bars and the
//! `eval_bars` evaluation window after the pivot, so it is known only `pivot_len + eval_bars`
//! bars later ([`ReversalLabel::known_index`]). These are research labels — for studying which
//! context separates good from bad reversals or for training/validating a gate — never signals,
//! and deliberately not a streaming [`crate::indicator::Indicator`] with alerts.
//!
//! Per strict pivot (low → long, high → short) with risk `R = risk_atr · ATR(atr_len)` at the
//! pivot bar, over the `pivot_len + eval_bars` bars after it:
//!
//! | kind | condition (checked in this order) |
//! |---|---|
//! | [`ReversalKind::Snapback`] | `snap_r` reached before the `fail_r` stop, within `snap_bars` bars |
//! | [`ReversalKind::Grind`] | `grind_r` reached before the stop, or MFE ≥ `min_follow_r` with MAE < `fail_r` and the last close beyond the pivot |
//! | [`ReversalKind::Fake`] | the stop came first, or MFE < `min_follow_r` |
//! | [`ReversalKind::Chop`] | anything else |
//!
//! Pivots whose follow-through stays below `min_move_atr` ATR are not labelled. Each label also
//! carries the Pine context flags at the pivot bar — WaveTrend divergence against the previous
//! pivot of the same side, WaveTrend beyond ±45, trend against the reversal (close vs. EMA with
//! ADX ≥ `adx_trend`), chop (ADX ≤ `chop_adx`) — and the Pine quality score (0..100).
//!
//! Conventions where Pine leaves them open (also in the golden fixture): EMAs seeded with their
//! first value; ATR and DMI Wilder averages seeded with an SMA (DMI from the second bar); pivots
//! strict on both sides; a pivot without an ATR at the pivot bar is not labelled; a missing
//! WaveTrend/EMA/ADX value makes its flag false and a missing ATR rank scores −3. Labels are
//! ordered by [`ReversalLabel::known_index`], long before short on the same bar.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/reversal_types.py`.

use crate::indicator::adx::Adx;
use crate::indicator::smoothing::{Ema, Rma};
use crate::indicator::wavetrend::WaveTrendEngine;
use crate::indicator::Indicator;
use crate::model::Bar;

/// Parameters, defaults as in the Pine script.
#[derive(Debug, Clone)]
pub struct ReversalTypeConfig {
    pub pivot_len: usize,
    pub eval_bars: usize,
    pub min_move_atr: f64,
    pub wt_channel: usize,
    pub wt_average: usize,
    pub os_level: f64,
    pub ob_level: f64,
    pub atr_len: usize,
    pub risk_atr: f64,
    pub snap_r: f64,
    pub grind_r: f64,
    pub fail_r: f64,
    pub min_follow_r: f64,
    pub snap_bars: usize,
    pub ema_len: usize,
    pub adx_len: usize,
    pub adx_trend: f64,
    pub rank_len: usize,
    pub chop_adx: f64,
}

impl Default for ReversalTypeConfig {
    fn default() -> Self {
        Self {
            pivot_len: 5,
            eval_bars: 24,
            min_move_atr: 0.30,
            wt_channel: 10,
            wt_average: 21,
            os_level: -45.0,
            ob_level: 45.0,
            atr_len: 14,
            risk_atr: 1.5,
            snap_r: 1.2,
            grind_r: 2.0,
            fail_r: 1.0,
            min_follow_r: 0.45,
            snap_bars: 6,
            ema_len: 100,
            adx_len: 14,
            adx_trend: 22.0,
            rank_len: 100,
            chop_adx: 16.0,
        }
    }
}

/// Outcome type of one reversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReversalKind {
    Snapback,
    Grind,
    Fake,
    Chop,
}

/// One labelled pivot reversal.
#[derive(Debug, Clone, PartialEq)]
pub struct ReversalLabel {
    /// Bar of the pivot extreme.
    pub pivot_index: usize,
    /// Bar at which the label is known: pivot + `pivot_len` + `eval_bars`.
    pub known_index: usize,
    /// `+1` long (from a pivot low), `-1` short (from a pivot high).
    pub side: i8,
    pub pivot_price: f64,
    pub kind: ReversalKind,
    /// Maximum favourable excursion over the window, in R.
    pub mfe_r: f64,
    /// Maximum adverse excursion over the window, in R (≥ 0).
    pub mae_r: f64,
    pub divergence: bool,
    pub extreme: bool,
    pub trend_against: bool,
    pub chop: bool,
    pub quality: f64,
}

fn strict_pivot(values: &[f64], centre: usize, side: usize, low: bool) -> bool {
    let v = values[centre];
    (centre - side..=centre + side)
        .filter(|&k| k != centre)
        .all(|k| if low { v < values[k] } else { v > values[k] })
}

/// Labels every confirmed pivot reversal of `bars` whose evaluation window is complete.
/// **Uses future bars** — see the module documentation.
pub fn classify_reversals(bars: &[Bar], cfg: &ReversalTypeConfig) -> Vec<ReversalLabel> {
    let n = bars.len();
    let mut wt = WaveTrendEngine::new(cfg.wt_channel, cfg.wt_average, 60.0, -60.0);
    let mut adx_ind = Adx::new(cfg.adx_len, cfg.adx_len, 1, cfg.adx_trend);
    let mut atr_rma = Rma::new(cfg.atr_len);
    let mut ema_f = Ema::new(cfg.ema_len);
    let mut wt1 = Vec::with_capacity(n);
    let mut adx = Vec::with_capacity(n);
    let mut atr: Vec<Option<f64>> = Vec::with_capacity(n);
    let mut ema = Vec::with_capacity(n);
    for (i, b) in bars.iter().enumerate() {
        wt1.push(wt.on_bar(b).map(|o| o.extra["wt1"]));
        adx.push(adx_ind.on_bar(b).map(|o| o.value));
        let tr = if i == 0 {
            b.high - b.low
        } else {
            let pc = bars[i - 1].close;
            (b.high - b.low)
                .max((b.high - pc).abs())
                .max((b.low - pc).abs())
        };
        atr.push(atr_rma.update(tr));
        ema.push(ema_f.update(b.close).unwrap());
    }
    let rank: Vec<Option<f64>> = (0..n)
        .map(|t| {
            let a = atr[t]?;
            if t + 1 < cfg.rank_len {
                return None;
            }
            let window: Option<Vec<f64>> = atr[t + 1 - cfg.rank_len..=t].iter().copied().collect();
            let w = window?;
            let lo = w.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = w.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            Some(if hi != lo {
                100.0 * (a - lo) / (hi - lo)
            } else {
                50.0
            })
        })
        .collect();

    let highs: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let l = cfg.pivot_len;
    let span = cfg.eval_bars + l;
    let mut labels = Vec::new();
    // Previous pivot of each side: (price, WaveTrend at its bar).
    let mut prev: [Option<(f64, Option<f64>)>; 2] = [None, None];
    for conf in 2 * l..n {
        let p = conf - l;
        for (side_idx, low_side) in [(0usize, true), (1usize, false)] {
            let values = if low_side { &lows } else { &highs };
            if !strict_pivot(values, p, l, low_side) {
                continue;
            }
            let price = values[p];
            let w = wt1[p];
            let before = prev[side_idx];
            prev[side_idx] = Some((price, w));
            let t = conf + cfg.eval_bars;
            let Some(a) = atr[p] else { continue };
            if t >= n {
                continue;
            }
            let risk = a * cfg.risk_atr;
            let win_h = highs[p + 1..=t]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let win_l = lows[p + 1..=t]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
            let (impulse, mfe, mae) = if low_side {
                (
                    win_h - price,
                    (win_h - price) / risk,
                    (price - win_l).max(0.0) / risk,
                )
            } else {
                (
                    price - win_l,
                    (price - win_l) / risk,
                    (win_h - price).max(0.0) / risk,
                )
            };
            if impulse < cfg.min_move_atr * a || risk <= 0.0 {
                continue;
            }
            let first_hits = |target_r: f64| {
                let (mut ft, mut ff) = (None, None);
                for j in 1..=span {
                    let b = &bars[p + j];
                    let (hit_t, hit_f) = if low_side {
                        (
                            b.high >= price + target_r * risk,
                            b.low <= price - cfg.fail_r * risk,
                        )
                    } else {
                        (
                            b.low <= price - target_r * risk,
                            b.high >= price + cfg.fail_r * risk,
                        )
                    };
                    if ft.is_none() && hit_t {
                        ft = Some(j);
                    }
                    if ff.is_none() && hit_f {
                        ff = Some(j);
                    }
                }
                (ft, ff)
            };
            let (snap, fail) = first_hits(cfg.snap_r);
            let (grind, _) = first_hits(cfg.grind_r);
            let before_fail = |hit: Option<usize>| hit.is_some_and(|h| fail.is_none_or(|f| h < f));
            let beyond = if low_side {
                bars[t].close > price
            } else {
                bars[t].close < price
            };
            let kind = if before_fail(snap) && snap.unwrap() <= cfg.snap_bars {
                ReversalKind::Snapback
            } else if before_fail(grind) || (mfe >= cfg.min_follow_r && mae < cfg.fail_r && beyond)
            {
                ReversalKind::Grind
            } else if fail.is_some_and(|f| snap.is_none_or(|s| f < s)) || mfe < cfg.min_follow_r {
                ReversalKind::Fake
            } else {
                ReversalKind::Chop
            };
            let divergence = match (before, w) {
                (Some((pp, Some(pw))), Some(w)) => {
                    if low_side {
                        price < pp && w > pw
                    } else {
                        price > pp && w < pw
                    }
                }
                _ => false,
            };
            let extreme = w.is_some_and(|w| {
                if low_side {
                    w <= cfg.os_level
                } else {
                    w >= cfg.ob_level
                }
            });
            let trend_against = adx[p].is_some_and(|x| x >= cfg.adx_trend)
                && if low_side {
                    bars[p].close < ema[p]
                } else {
                    bars[p].close > ema[p]
                };
            let chop = adx[p].is_some_and(|x| x <= cfg.chop_adx);
            let mut q: f64 = 50.0;
            q += if divergence { 18.0 } else { 0.0 };
            q += if extreme { 12.0 } else { -8.0 };
            q -= if trend_against { 14.0 } else { 0.0 };
            q -= if chop { 8.0 } else { 0.0 };
            q += if mfe >= cfg.grind_r {
                12.0
            } else if mfe >= cfg.min_follow_r {
                5.0
            } else {
                -12.0
            };
            q -= if mae >= cfg.fail_r { 10.0 } else { 0.0 };
            q += if rank[p].is_some_and(|r| (20.0..=80.0).contains(&r)) {
                4.0
            } else {
                -3.0
            };
            labels.push(ReversalLabel {
                pivot_index: p,
                known_index: t,
                side: if low_side { 1 } else { -1 },
                pivot_price: price,
                kind,
                mfe_r: mfe,
                mae_r: mae,
                divergence,
                extreme,
                trend_against,
                chop,
                quality: q.clamp(0.0, 100.0),
            });
        }
    }
    labels.sort_by_key(|r| (r.known_index, -r.side));
    labels
}
