//! Market Motion DNA: the movement character of the live pivot-to-price leg — impulsive,
//! corrective or neutral — from ten measurable price-action features.
//!
//! Port of the live-leg engine of the Pine Script "Market Motion DNA" (WavesUnchained, v1.5.2).
//! Impulsive is not bullish and corrective is not bearish: an impulsive down-leg is impulsive.
//!
//! **Pivot engine.** Pivots (`pivot_left`/`pivot_right`, strict) alternate: a pivot of the same
//! type as the last one replaces it when more extreme and is ignored otherwise; an opposite pivot
//! is added when it lies after the last one and the leg is at least `min_leg_atr` times the ATR
//! at the pivot bar. A bar confirming both a high and a low takes the type opposite to the last
//! pivot (the first pivot: the extreme farther from the pivot bar's close).
//!
//! **Live leg.** From the last pivot to the current bar's high (after a low) or low (after a
//! high), over at most `max_leg_bars` bars. Per bar (close-to-close against the bar before):
//!
//! | feature | per bar, averaged (or as a whole) |
//! |---|---|
//! | efficiency | net distance / summed true range |
//! | overlap | overlap with the previous bar's range / the smaller range |
//! | retracement | largest adverse excursion from the running favourable extreme / net distance / `retracement_ceiling` |
//! | commitment | close location in the bar's range, measured in leg direction |
//! | wick cleanliness | body / range |
//! | momentum expansion | directional body, second half against first half (first half floored at ATR / 4), mapped from `momentum_low..momentum_high` |
//! | true-range expansion | true range, second half against first half, mapped from `tr_low..tr_high` |
//! | MA support | 0.6 for the close on the leg side of the EMA (`ma_len`), 0.4 for its slope |
//! | cleanliness | 1 - sign changes of the close-to-close move / (bars - 1) |
//! | acceptance | 1 for a close in leg direction, 0 against, 0.5 unchanged |
//!
//! Impulse = weighted mean of the features (overlap and retracement inverted) · 100, correction =
//! 100 - impulse; both are pulled to 50 by maturity `min(bars / mature_bars, 1)`. Confidence =
//! `|impulse - correction| · maturity`. The raw class is impulse/correction only at maturity 1
//! with a separation of at least `min_separation` and the score at its threshold; the live class
//! keeps impulse (or correction) while the score stays within `change_buffer` of its threshold
//! and above the other score.
//!
//! Internal sub-swings (diagnostic, not in the score): strict pivots of width `sub_pivot_width`
//! inside the leg that are already confirmed, alternating (a more extreme same-type pivot moves
//! the last one), give `internal_swings` (pivots counted), `avg_internal_swing_atr`,
//! `progression_ratio` (net distance / summed swing sizes, at most 1) and `sub_leg_growth`
//! (average of the later half of the swings over the earlier half).
//!
//! `value`: the live impulse score. `extra`: `correction`, `confidence`, `maturity`, `bars`,
//! `direction` (+1 up-leg, -1 down-leg), `class` (+1 impulse, 0 neutral, -1 correction),
//! `leg_start`, the ten features (`efficiency`, `overlap`, `retracement`, `commitment`,
//! `wick_clean`, `momentum_expansion`, `tr_expansion`, `ma_support`, `cleanliness`,
//! `acceptance`; each `0..=1`) and the sub-swing diagnostics. `state`: `impulse`, `correction`
//! or `neutral`. Output starts with the first pivot.
//!
//! Alerts when the live class changes: `bull_impulse` / `bear_impulse` (the live up-/down-leg
//! became impulsive — prefix = the leg's direction, the expected move of an impulse),
//! `correction_started` and `character_lost` (corrective or back to neutral; no direction).
//!
//! Not ported: the completed-leg envelopes, segment colouring and merged waves — drawings over
//! past spans, redrawn when a later pivot replaces an earlier one.
//!
//! Conventions where Pine leaves them open (also in the golden fixture): ATR = Wilder average of
//! the true range (first bar high - low), SMA-seeded, 0 where an ATR is read before it exists
//! (Pine's `nz`); a missing ATR at the pivot bar blocks a new pivot; EMA seeded with the first
//! close; `syminfo.mintick` floors replaced by `1e-12`; pivots strict on both sides.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/market_motion_dna.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults as in the Pine script (EMA support MA).
#[derive(Debug, Clone)]
pub struct MotionDnaConfig {
    pub pivot_left: usize,
    pub pivot_right: usize,
    pub max_leg_bars: usize,
    pub min_leg_atr: f64,
    pub atr_len: usize,
    pub mature_bars: usize,
    pub retracement_ceiling: f64,
    pub momentum_low: f64,
    pub momentum_high: f64,
    pub tr_low: f64,
    pub tr_high: f64,
    pub ma_len: usize,
    pub sub_pivot_width: usize,
    /// Efficiency, overlap, retracement, commitment, wicks, momentum, true range, MA support,
    /// cleanliness, acceptance.
    pub weights: [f64; 10],
    pub impulse_threshold: f64,
    pub correction_threshold: f64,
    pub min_separation: f64,
    pub change_buffer: f64,
}

impl Default for MotionDnaConfig {
    fn default() -> Self {
        Self {
            pivot_left: 5,
            pivot_right: 5,
            max_leg_bars: 350,
            min_leg_atr: 0.50,
            atr_len: 14,
            mature_bars: 8,
            retracement_ceiling: 0.80,
            momentum_low: 0.60,
            momentum_high: 1.40,
            tr_low: 0.70,
            tr_high: 1.30,
            ma_len: 34,
            sub_pivot_width: 2,
            weights: [20.0, 15.0, 12.5, 10.0, 7.5, 10.0, 5.0, 7.5, 7.5, 5.0],
            impulse_threshold: 62.0,
            correction_threshold: 62.0,
            min_separation: 18.0,
            change_buffer: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    ma: f64,
    atr: Option<f64>,
    sub_high: Option<f64>,
    sub_low: Option<f64>,
}

/// The live leg's scores and features.
#[derive(Debug, Clone, Copy)]
struct Leg {
    impulse: f64,
    correction: f64,
    confidence: f64,
    maturity: f64,
    bars: usize,
    features: [f64; 10],
    internal_swings: usize,
    avg_swing_atr: f64,
    progression: f64,
    growth: f64,
}

const FEATURES: [&str; 10] = [
    "efficiency",
    "overlap",
    "retracement",
    "commitment",
    "wick_clean",
    "momentum_expansion",
    "tr_expansion",
    "ma_support",
    "cleanliness",
    "acceptance",
];

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn map01(v: f64, lo: f64, hi: f64) -> f64 {
    if hi > lo {
        clamp01((v - lo) / (hi - lo))
    } else {
        0.5
    }
}

fn safe_div(n: f64, d: f64, fallback: f64) -> f64 {
    if d != 0.0 {
        n / d
    } else {
        fallback
    }
}

/// Streaming Market Motion DNA; see the module documentation.
pub struct MarketMotionDna {
    cfg: MotionDnaConfig,
    atr: Rma,
    ma: Ema,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    /// Last pivot: (bar, price, type +1 high / -1 low).
    last_pivot: Option<(usize, f64, i8)>,
    class: i8,
    alerts: Vec<IndicatorAlert>,
}

impl MarketMotionDna {
    pub fn new(cfg: MotionDnaConfig) -> Self {
        let cap = (cfg.max_leg_bars + 2)
            .max(cfg.pivot_left + cfg.pivot_right + 2)
            .max(2 * cfg.sub_pivot_width + 2);
        Self {
            atr: Rma::new(cfg.atr_len),
            ma: Ema::new(cfg.ma_len),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            cfg,
            bars: 0,
            last_pivot: None,
            class: 0,
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &MotionDnaConfig {
        &self.cfg
    }

    /// The row of absolute bar `b`.
    fn row(&self, b: usize) -> &Row {
        let first = self.bars - self.rows.len();
        &self.rows[b - first]
    }

    /// Strict pivot centred `right` bars back, `left` bars before it.
    fn pivot(&self, left: usize, right: usize, low_side: bool) -> Option<f64> {
        let n = self.rows.len();
        if n < left + right + 1 {
            return None;
        }
        let at = |k: usize| &self.rows[n - 1 - k];
        let v = if low_side {
            at(right).low
        } else {
            at(right).high
        };
        for k in (0..right).chain(right + 1..=right + left) {
            let r = at(k);
            let ok = if low_side { v < r.low } else { v > r.high };
            if !ok {
                return None;
            }
        }
        Some(v)
    }

    fn leg(
        &self,
        start_bar: usize,
        start_price: f64,
        end_bar: usize,
        end_price: f64,
        dir: f64,
    ) -> Leg {
        let c = &self.cfg;
        let raw = (end_bar - start_bar).max(1);
        let n = raw.min(c.max_leg_bars);
        let eff_start = end_bar - n;
        let eff_start_price = if eff_start == start_bar {
            start_price
        } else {
            self.row(eff_start).close
        };
        let net = (end_price - eff_start_price).abs().max(1e-12);
        let mut path = 0.0;
        let (mut overlap, mut commit, mut wick, mut accept, mut ma_sup) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut changes = 0usize;
        let mut prev_sign = 0i8;
        let mut favourable = eff_start_price;
        let mut adverse: f64 = 0.0;
        let (mut body1, mut body2, mut tr1, mut tr2, mut n1, mut n2) = (0.0, 0.0, 0.0, 0.0, 0, 0);
        let half = n.div_ceil(2);
        let mut last_sub_type = 0i8;
        let mut last_sub_price = 0.0;
        let mut swing_count = 0usize;
        let mut swing_sum = 0.0;
        let mut swings: Vec<f64> = Vec::new();
        for step in 1..=n {
            let b = eff_start + step;
            let cur = self.row(b);
            let prev = self.row(b - 1);
            let range = (cur.high - cur.low).max(1e-12);
            let prange = (prev.high - prev.low).max(1e-12);
            let delta = cur.close - prev.close;
            let sign = if delta > 0.0 {
                1
            } else if delta < 0.0 {
                -1
            } else {
                0
            };
            let dd = dir * delta;
            let dbody = (dir * (cur.close - cur.open)).max(0.0);
            let tr = (cur.high - cur.low)
                .max((cur.high - prev.close).abs())
                .max((cur.low - prev.close).abs());
            path += tr;
            let ov = (cur.high.min(prev.high) - cur.low.max(prev.low)).max(0.0);
            overlap += safe_div(ov, range.min(prange), 0.0);
            let loc = if dir > 0.0 {
                (cur.close - cur.low) / range
            } else {
                (cur.high - cur.close) / range
            };
            commit += clamp01(loc);
            let total_wick = range - (cur.close - cur.open).abs();
            wick += 1.0 - clamp01(total_wick / range);
            accept += if dd > 0.0 {
                1.0
            } else if dd < 0.0 {
                0.0
            } else {
                0.5
            };
            let (price_ok, slope_ok) = if dir > 0.0 {
                (cur.close >= cur.ma, cur.ma >= prev.ma)
            } else {
                (cur.close <= cur.ma, cur.ma <= prev.ma)
            };
            ma_sup += if price_ok { 0.6 } else { 0.0 } + if slope_ok { 0.4 } else { 0.0 };
            if sign != 0 {
                if prev_sign != 0 && sign != prev_sign {
                    changes += 1;
                }
                prev_sign = sign;
            }
            if dir > 0.0 {
                favourable = favourable.max(cur.high);
                adverse = adverse.max(favourable - cur.low);
            } else {
                favourable = favourable.min(cur.low);
                adverse = adverse.max(cur.high - favourable);
            }
            if step <= half {
                body1 += dbody;
                tr1 += tr;
                n1 += 1;
            } else {
                body2 += dbody;
                tr2 += tr;
                n2 += 1;
            }
            // Sub-pivots centred on this bar, if already confirmed.
            let confirmed = b + c.sub_pivot_width <= end_bar;
            let (sh, sl) = if confirmed {
                (cur.sub_high, cur.sub_low)
            } else {
                (None, None)
            };
            let chosen = match (sh, sl) {
                (Some(h), Some(l)) => {
                    if last_sub_type <= 0 {
                        Some((1i8, h))
                    } else {
                        Some((-1, l))
                    }
                }
                (Some(h), None) => Some((1, h)),
                (None, Some(l)) => Some((-1, l)),
                (None, None) => None,
            };
            if let Some((ty, price)) = chosen {
                if last_sub_type == 0 {
                    last_sub_type = ty;
                    last_sub_price = price;
                    swing_count += 1;
                } else if ty == last_sub_type {
                    let more = if ty == 1 {
                        price > last_sub_price
                    } else {
                        price < last_sub_price
                    };
                    if more {
                        last_sub_price = price;
                    }
                } else {
                    let size = (price - last_sub_price).abs();
                    swing_sum += size;
                    swings.push(size);
                    swing_count += 1;
                    last_sub_type = ty;
                    last_sub_price = price;
                }
            }
        }
        let nf = n as f64;
        let efficiency = clamp01(safe_div(net, path.max(net), 0.0));
        let overlap = clamp01(overlap / nf);
        let retracement = clamp01(safe_div(adverse, net, 0.0) / c.retracement_ceiling);
        let commitment = clamp01(commit / nf);
        let wick_clean = clamp01(wick / nf);
        let acceptance = clamp01(accept / nf);
        let ma_support = clamp01(ma_sup / nf);
        let cleanliness = if n > 1 {
            1.0 - clamp01(changes as f64 / (nf - 1.0).max(1.0))
        } else {
            0.5
        };
        let atr_end = self.row(end_bar).atr.unwrap_or(0.0);
        let avg_b1 = if n1 > 0 { body1 / n1 as f64 } else { 0.0 };
        let avg_b2 = if n2 > 0 { body2 / n2 as f64 } else { avg_b1 };
        let base = avg_b1.max(atr_end * 0.25);
        let mom_ratio = if base > 1e-12 { avg_b2 / base } else { 1.0 };
        let momentum = map01(mom_ratio, c.momentum_low, c.momentum_high);
        let avg_t1 = if n1 > 0 { tr1 / n1 as f64 } else { 0.0 };
        let avg_t2 = if n2 > 0 { tr2 / n2 as f64 } else { avg_t1 };
        let tr_ratio = if avg_t1 > 1e-12 { avg_t2 / avg_t1 } else { 1.0 };
        let tr_exp = map01(tr_ratio, c.tr_low, c.tr_high);
        let features = [
            efficiency,
            overlap,
            retracement,
            commitment,
            wick_clean,
            momentum,
            tr_exp,
            ma_support,
            cleanliness,
            acceptance,
        ];
        let w = &c.weights;
        let wsum = w.iter().sum::<f64>().max(0.0001);
        let mut imp = 0.0;
        let mut cor = 0.0;
        for (i, f) in features.iter().enumerate() {
            // Overlap and retracement count against an impulse.
            let g = if i == 1 || i == 2 { 1.0 - f } else { *f };
            imp += g * w[i];
            cor += (1.0 - g) * w[i];
        }
        let raw_imp = imp / wsum * 100.0;
        let raw_cor = cor / wsum * 100.0;
        let maturity = clamp01(nf / c.mature_bars as f64);
        let impulse = 50.0 + (raw_imp - 50.0) * maturity;
        let correction = 50.0 + (raw_cor - 50.0) * maturity;
        let k = swings.len();
        let avg_swing_atr = if k > 0 {
            safe_div(swing_sum / k as f64, atr_end, 0.0)
        } else {
            0.0
        };
        let progression = clamp01(safe_div(net, swing_sum, 1.0));
        let bound = k.div_ceil(2);
        let (mut s1, mut s2, mut k1, mut k2) = (0.0, 0.0, 0, 0);
        for (i, s) in swings.iter().enumerate() {
            if i < bound {
                s1 += s;
                k1 += 1;
            } else {
                s2 += s;
                k2 += 1;
            }
        }
        let a1 = if k1 > 0 { s1 / k1 as f64 } else { 0.0 };
        let a2 = if k2 > 0 { s2 / k2 as f64 } else { a1 };
        let growth = if a1 > 1e-12 { a2 / a1 } else { 1.0 };
        Leg {
            impulse,
            correction,
            confidence: (impulse - correction).abs() * maturity,
            maturity,
            bars: n,
            features,
            internal_swings: swing_count,
            avg_swing_atr,
            progression,
            growth,
        }
    }

    fn classify(&self, leg: &Leg) -> i8 {
        let c = &self.cfg;
        let sep = (leg.impulse - leg.correction).abs();
        if leg.maturity >= 0.999 && sep >= c.min_separation {
            if leg.impulse >= c.impulse_threshold && leg.impulse > leg.correction {
                return 1;
            }
            if leg.correction >= c.correction_threshold && leg.correction > leg.impulse {
                return -1;
            }
        }
        0
    }
}

impl Indicator for MarketMotionDna {
    fn name(&self) -> &str {
        "market_motion_dna"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.pivot_left + self.cfg.pivot_right + 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let t = self.bars;
        self.bars += 1;
        let tr = match self.rows.back() {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let ma = self.ma.update(bar.close).expect("first-sample seed");
        self.rows.push_back(Row {
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            ma,
            atr,
            sub_high: None,
            sub_low: None,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        let w = c.sub_pivot_width;
        let (sh, sl) = (self.pivot(w, w, false), self.pivot(w, w, true));
        if sh.is_some() || sl.is_some() {
            let n = self.rows.len();
            let r = &mut self.rows[n - 1 - w];
            r.sub_high = sh;
            r.sub_low = sl;
        }

        // Pivot engine.
        let right = c.pivot_right;
        let cand_high = self.pivot(c.pivot_left, right, false);
        let cand_low = self.pivot(c.pivot_left, right, true);
        let chosen = match (cand_high, cand_low) {
            (Some(h), Some(l)) => {
                let ty = match self.last_pivot {
                    None => {
                        let pc = self.row(t - right).close;
                        if (h - pc).abs() >= (pc - l).abs() {
                            1
                        } else {
                            -1
                        }
                    }
                    Some((_, _, last)) => -last,
                };
                Some((ty, if ty == 1 { h } else { l }))
            }
            (Some(h), None) => Some((1, h)),
            (None, Some(l)) => Some((-1, l)),
            (None, None) => None,
        };
        if let Some((ty, price)) = chosen {
            let cand_bar = t - right;
            match self.last_pivot {
                None => self.last_pivot = Some((cand_bar, price, ty)),
                Some((last_bar, last_price, last_ty)) => {
                    if ty == last_ty {
                        let more = if ty == 1 {
                            price > last_price
                        } else {
                            price < last_price
                        };
                        if more {
                            self.last_pivot = Some((cand_bar, price, ty));
                        }
                    } else {
                        let large = c.min_leg_atr <= 0.0
                            || matches!(self.row(cand_bar).atr, Some(a) if (price - last_price).abs() >= a * c.min_leg_atr);
                        if large && cand_bar > last_bar {
                            self.last_pivot = Some((cand_bar, price, ty));
                        }
                    }
                }
            }
        }

        // Live leg.
        let (start_bar, start_price, last_ty) = self.last_pivot?;
        let dir = if last_ty == -1 { 1.0 } else { -1.0 };
        let end_price = if dir > 0.0 { bar.high } else { bar.low };
        let leg = self.leg(start_bar, start_price, t, end_price, dir);
        let raw_class = self.classify(&leg);
        let prev_class = self.class;
        self.class = match prev_class {
            1 if leg.impulse >= c.impulse_threshold - c.change_buffer
                && leg.impulse >= leg.correction =>
            {
                1
            }
            -1 if leg.correction >= c.correction_threshold - c.change_buffer
                && leg.correction >= leg.impulse =>
            {
                -1
            }
            _ => raw_class,
        };
        let class = self.class;
        if class == 1 && prev_class != 1 {
            let kind = if dir > 0.0 {
                "bull_impulse"
            } else {
                "bear_impulse"
            };
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("Motion DNA: live leg impulsive ({:.0})", leg.impulse),
                leg.impulse / 100.0,
            ));
        } else if class == -1 && prev_class != -1 {
            self.alerts.push(IndicatorAlert::new(
                "correction_started",
                format!("Motion DNA: live leg corrective ({:.0})", leg.correction),
                leg.correction / 100.0,
            ));
        } else if class == 0 && prev_class != 0 {
            self.alerts.push(IndicatorAlert::new(
                "character_lost",
                "Motion DNA: live leg back to neutral",
                0.3,
            ));
        }

        let mut extra = HashMap::new();
        extra.insert("correction".to_string(), leg.correction);
        extra.insert("confidence".to_string(), leg.confidence);
        extra.insert("maturity".to_string(), leg.maturity);
        extra.insert("bars".to_string(), leg.bars as f64);
        extra.insert("direction".to_string(), dir);
        extra.insert("class".to_string(), class as f64);
        extra.insert("leg_start".to_string(), start_price);
        for (name, v) in FEATURES.iter().zip(leg.features) {
            extra.insert(name.to_string(), v);
        }
        extra.insert("internal_swings".to_string(), leg.internal_swings as f64);
        extra.insert("avg_internal_swing_atr".to_string(), leg.avg_swing_atr);
        extra.insert("progression_ratio".to_string(), leg.progression);
        extra.insert("sub_leg_growth".to_string(), leg.growth);
        let state = match class {
            1 => "impulse",
            -1 => "correction",
            _ => "neutral",
        };
        Some(IndicatorOutput::with_extra(leg.impulse, extra).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
