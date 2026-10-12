//! Market–Average Relationship Engine (MARE): how price relates to one moving average.
//!
//! Port of the Pine Script "Market–Average Relationship Engine" (WavesUnchained, v1.3.1). The
//! moving average is a market reference, not a crossover signal. Six diagnostic scores, each
//! `0..=100`, describe the relationship:
//!
//! | score | meaning |
//! |---|---|
//! | trend | MA slope, persistence of price side and slope, price efficiency, agreement |
//! | respect | hit rate of earlier touches of the MA, confidence-weighted by sample count |
//! | extension | ATR distance from the MA blended with its rarity against its own history |
//! | compression | share of recent bars near the MA plus ATR contraction |
//! | acceleration | separation velocity, MA-slope acceleration, return acceleration |
//! | exhaustion | extension gated by efficiency decay, opposing wicks, stalled progress, fading acceleration |
//!
//! `value` is the relationship oscillator (`-100..=100`): signed trend quality reduced by an
//! extension/exhaustion penalty of at most 50 % — extension and exhaustion degrade the
//! relationship but never flip its sign. `secondary` is its signal line (EMA 5 of the EMA-3
//! smoothed oscillator), also in `extra["signal"]`. `extra` carries the six scores plus `ma`, `distance_atr`,
//! `respect_samples`, `respect_hit_rate` and `direction`; `state` one of twelve states.
//!
//! Alerts (closed bars, cooldown per family counted from that family's last fired alert). The
//! `bull_`/`bear_` prefix names the expected move, not the trend the condition fires in; the
//! do-not-chase warnings carry no direction:
//!
//! | alert | condition |
//! |---|---|
//! | `bull_pullback` / `bear_pullback` | established, respected trend; prior extension in trend direction; rejection candle at the MA; bounded exhaustion |
//! | `bull_momentum_release` / `bear_momentum_release` | compression in the last `compression_len` bars, distance crosses the band, slope, acceleration and efficiency agree |
//! | `do_not_chase_up` / `do_not_chase_down` | extension crosses the chase threshold inside a trend — a warning, not a reversal |
//! | `bear_exhaustion` / `bull_exhaustion` | exhaustion crosses its threshold; `bear_*` = an up-move tiring |
//! | `bear_relationship_break` / `bull_relationship_break` | an established up-/downtrend: sustained closes on the other side of the MA and a flattening slope (transition only) |
//! | `bull_divergence` / `bear_divergence` | regular divergence between strict price pivots and the oscillator at the pivot bar |
//! | `bull_turn` / `bear_turn` | the oscillator touched ±`turn_level` within `turn_lookback` bars and now exceeds each of its previous `turn_confirm` values (transition only) |
//!
//! Unlike the Pine original, whose `bullish exhaustion` fires in an uptrend, the names here follow
//! the expected move: `bear_exhaustion` fires when an uptrend tires.
//!
//! Conventions where Pine leaves them open (also in the golden fixture): EMA seeded with the first
//! value; ATR = Wilder average of the true range, SMA-seeded; population standard deviation; the
//! `syminfo.mintick` floor on ATR is `1e-12`; pivots strict on both sides. Every quantity
//! propagates "not available", a bar reports only when the oscillator, its signal and all six
//! scores exist, and alerts are evaluated only on reported bars and need every input they read
//! (including the previous bar's condition for transitions). The Pine VWMA/HMA/WMA/DEMA/TEMA
//! families are not offered; the MA is any [`SmootherKind`].
//!
//! Golden reference: `reference/kestrel_reference/fixtures/market_average_relationship.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Rma, Smoother, SmootherKind};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults as in the Pine script.
#[derive(Debug, Clone)]
pub struct MareConfig {
    pub ma_kind: SmootherKind,
    pub ma_len: usize,
    pub atr_len: usize,
    pub slope_len: usize,
    pub full_slope_atr: f64,
    pub persistence_len: usize,
    pub efficiency_len: usize,
    pub statistics_len: usize,
    pub extension_normal_atr: f64,
    pub extension_extreme_atr: f64,
    pub proximity_atr: f64,
    pub respect_lookback: usize,
    pub respect_response: usize,
    pub compression_len: usize,
    pub compression_band_atr: f64,
    pub acceleration_len: usize,
    pub exhaustion_len: usize,
    pub cooldown: usize,
    pub minimum_trend: f64,
    pub minimum_respect: f64,
    pub pullback_prior_extension: f64,
    pub maximum_pullback_exhaustion: f64,
    pub acceleration_threshold: f64,
    pub compression_threshold: f64,
    pub chase_extension: f64,
    pub exhaustion_threshold: f64,
    pub break_confirmation: usize,
    pub pivot_left: usize,
    pub pivot_right: usize,
    pub turn_lookback: usize,
    pub turn_level: f64,
    pub turn_confirm: usize,
}

impl Default for MareConfig {
    fn default() -> Self {
        Self {
            ma_kind: SmootherKind::Ema,
            ma_len: 50,
            atr_len: 14,
            slope_len: 10,
            full_slope_atr: 0.50,
            persistence_len: 20,
            efficiency_len: 14,
            statistics_len: 100,
            extension_normal_atr: 0.75,
            extension_extreme_atr: 2.50,
            proximity_atr: 0.35,
            respect_lookback: 80,
            respect_response: 4,
            compression_len: 20,
            compression_band_atr: 0.45,
            acceleration_len: 5,
            exhaustion_len: 8,
            cooldown: 8,
            minimum_trend: 55.0,
            minimum_respect: 55.0,
            pullback_prior_extension: 0.70,
            maximum_pullback_exhaustion: 65.0,
            acceleration_threshold: 60.0,
            compression_threshold: 65.0,
            chase_extension: 75.0,
            exhaustion_threshold: 68.0,
            break_confirmation: 3,
            pivot_left: 5,
            pivot_right: 5,
            turn_lookback: 30,
            turn_level: 55.0,
            turn_confirm: 3,
        }
    }
}

/// One bar of history. `None` = not available yet.
#[derive(Debug, Clone, Default)]
struct Row {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    ma: Option<f64>,
    atr: Option<f64>,
    distance: Option<f64>,
    slope: Option<f64>,
    eff: Option<f64>,
    trend: Option<f64>,
    direction: Option<f64>,
    respect: Option<f64>,
    extension: Option<f64>,
    compression: Option<f64>,
    acceleration: Option<f64>,
    exhaustion: Option<f64>,
    rel: Option<f64>,
    broken_up: Option<bool>,
    broken_down: Option<bool>,
    turn_bull: Option<bool>,
    turn_bear: Option<bool>,
}

const FAMILIES: usize = 7;
const PULLBACK: usize = 0;
const MOMENTUM: usize = 1;
const CHASE: usize = 2;
const EXHAUSTION: usize = 3;
const BROKEN: usize = 4;
const DIVERGENCE: usize = 5;
const TURN: usize = 6;

/// Streaming MARE; see the module documentation.
pub struct MarketAverageRelationship {
    cfg: MareConfig,
    ma: Box<dyn Smoother>,
    atr: Rma,
    rel_ema: Option<f64>,
    signal_ema: Option<f64>,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    last_fired: [Option<usize>; FAMILIES],
    last_pivot_low: Option<(f64, f64)>,
    last_pivot_high: Option<(f64, f64)>,
    alerts: Vec<IndicatorAlert>,
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

fn safe_div(n: f64, d: f64, fallback: f64) -> f64 {
    if d.abs() > 1e-10 {
        n / d
    } else {
        fallback
    }
}

fn norm(v: f64, lo: f64, hi: f64) -> f64 {
    if hi > lo {
        clamp((v - lo) / (hi - lo), 0.0, 1.0)
    } else {
        0.0
    }
}

fn sign(v: f64, dead: f64) -> f64 {
    if v > dead {
        1.0
    } else if v < -dead {
        -1.0
    } else {
        0.0
    }
}

fn ema_step(prev: Option<f64>, v: f64, len: usize) -> f64 {
    let a = 2.0 / (len as f64 + 1.0);
    match prev {
        Some(p) => a * v + (1.0 - a) * p,
        None => v,
    }
}

impl MarketAverageRelationship {
    pub fn new(cfg: MareConfig) -> Self {
        let cap = [
            cfg.statistics_len,
            cfg.respect_lookback + 2,
            cfg.turn_lookback + cfg.turn_confirm + 2,
            cfg.compression_len + 2,
            cfg.pivot_left + cfg.pivot_right + 2,
            cfg.acceleration_len * 2 + cfg.slope_len + 2,
            cfg.exhaustion_len + 2,
            cfg.persistence_len + 2,
            cfg.efficiency_len + 2,
            cfg.break_confirmation + 2,
        ]
        .into_iter()
        .max()
        .unwrap_or(1)
            + 2;
        Self {
            ma: cfg.ma_kind.build(cfg.ma_len),
            atr: Rma::new(cfg.atr_len),
            cfg,
            rel_ema: None,
            signal_ema: None,
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            last_fired: [None; FAMILIES],
            last_pivot_low: None,
            last_pivot_high: None,
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &MareConfig {
        &self.cfg
    }

    /// The row `k` bars back (0 = current), `None` before the series start.
    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    fn col(&self, k: usize, f: impl Fn(&Row) -> Option<f64>) -> Option<f64> {
        self.at(k).and_then(f)
    }

    /// Mean of `f(row k, row k+1)` over `k = 0..len`, `None` unless every term exists.
    fn sma(&self, len: usize, f: impl Fn(&Row, Option<&Row>) -> Option<f64>) -> Option<f64> {
        let mut sum = 0.0;
        for k in 0..len {
            sum += f(self.at(k)?, self.at(k + 1))?;
        }
        Some(sum / len as f64)
    }

    fn stdev(&self, len: usize, f: impl Fn(&Row) -> Option<f64>) -> Option<f64> {
        let vals: Option<Vec<f64>> = (0..len).map(|k| self.at(k).and_then(&f)).collect();
        let vals = vals?;
        let mean = vals.iter().sum::<f64>() / len as f64;
        Some((vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / len as f64).sqrt())
    }

    fn respect(&self) -> Option<(f64, f64, f64)> {
        let c = &self.cfg;
        self.col(c.respect_lookback + 1, |r| r.atr)?;
        let (mut succ, mut fail, mut weighted) = (0.0, 0.0, 0.0);
        for i in c.respect_response..=c.respect_lookback {
            let r = self.at(i)?;
            let before = self.at(i + 1)?;
            let (ma, atr) = (r.ma?, r.atr?);
            let (ma_b, atr_b) = (before.ma?, before.atr?);
            let touched = (r.close - ma).abs() <= atr * c.proximity_atr;
            let touched_before = (before.close - ma_b).abs() <= atr_b * c.proximity_atr;
            if touched && !touched_before {
                let trend_side = if ma - ma_b >= 0.0 { 1.0 } else { -1.0 };
                let out = self.at(i - c.respect_response)?;
                let progress = safe_div((out.close - r.close) * trend_side, out.atr?, 0.0);
                let held = (out.close - out.ma?) * trend_side > 0.0;
                if held && progress > 0.0 {
                    succ += 1.0;
                } else {
                    fail += 1.0;
                }
                weighted += clamp(progress, -1.0, 1.0);
            }
        }
        let samples: f64 = succ + fail;
        let hit = if samples > 0.0 { succ / samples } else { 0.5 };
        let quality = if samples > 0.0 {
            norm(weighted / samples, -0.35, 0.50)
        } else {
            0.5
        };
        let confidence = norm(samples, 2.0, 10.0);
        let raw = 100.0 * (0.70 * hit + 0.30 * quality);
        Some((
            clamp(50.0 + (raw - 50.0) * confidence, 0.0, 100.0),
            samples,
            100.0 * hit,
        ))
    }

    fn pullback(&self, bull: bool) -> Option<bool> {
        let c = &self.cfg;
        let cur = self.at(0)?;
        let prev = self.at(1)?;
        let (prev_dir, prev_trend) = (prev.direction?, prev.trend?);
        let (dir, trend, respect, exh, ma, atr) = (
            cur.direction?,
            cur.trend?,
            cur.respect?,
            cur.exhaustion?,
            cur.ma?,
            cur.atr?,
        );
        let len = (c.acceleration_len + 1).max(2);
        let mut hi = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        for k in 1..=len {
            let d = self.col(k, |r| r.distance)?;
            hi = hi.max(d);
            lo = lo.min(d);
        }
        let s = if bull { 1.0 } else { -1.0 };
        let prior_ext = if bull { hi } else { -lo };
        let mid = (cur.high + cur.low) * 0.5;
        let rejection = if bull {
            cur.low <= ma + atr * c.proximity_atr
                && cur.close > ma
                && cur.close > cur.open
                && cur.close >= mid
        } else {
            cur.high >= ma - atr * c.proximity_atr
                && cur.close < ma
                && cur.close < cur.open
                && cur.close <= mid
        };
        Some(
            prev_dir * s > 0.0
                && prev_trend >= c.minimum_trend
                && dir * s > 0.0
                && trend >= c.minimum_trend
                && respect >= c.minimum_respect
                && prior_ext >= c.pullback_prior_extension
                && rejection
                && exh <= c.maximum_pullback_exhaustion,
        )
    }

    fn momentum(&self, bull: bool) -> Option<bool> {
        let c = &self.cfg;
        let mut was = f64::NEG_INFINITY;
        for k in 1..=c.compression_len {
            was = was.max(self.col(k, |r| r.compression)?);
        }
        let cur = self.at(0)?;
        let (d, d_prev) = (cur.distance?, self.col(1, |r| r.distance)?);
        let (slope, acc, eff, exh) = (cur.slope?, cur.acceleration?, cur.eff?, cur.exhaustion?);
        let band = c.compression_band_atr;
        let (cross, slope_ok) = if bull {
            (d > band && d_prev <= band, slope > 0.0)
        } else {
            (d < -band && d_prev >= -band, slope < 0.0)
        };
        Some(
            was >= c.compression_threshold
                && cross
                && slope_ok
                && acc >= c.acceleration_threshold
                && eff >= 0.35
                && exh < c.exhaustion_threshold,
        )
    }

    fn crossing(
        &self,
        bull: bool,
        needs_trend: bool,
        f: fn(&Row) -> Option<f64>,
        level: f64,
    ) -> Option<bool> {
        let cur = self.at(0)?;
        let (dir, now, before) = (cur.direction?, f(cur)?, self.col(1, f)?);
        let trend_ok = if needs_trend {
            cur.trend? >= self.cfg.minimum_trend
        } else {
            true
        };
        let s = if bull { 1.0 } else { -1.0 };
        Some(dir * s > 0.0 && trend_ok && now > level && before <= level)
    }

    fn broken_raw(&self, was_up: bool) -> Option<bool> {
        let c = &self.cfg;
        let past = self.at(c.break_confirmation)?;
        let (past_dir, past_trend) = (past.direction?, past.trend?);
        let cur = self.at(0)?;
        let (rel, slope) = (cur.rel?, cur.slope?);
        let s = if was_up { 1.0 } else { -1.0 };
        let established = past_dir * s > 0.0 && past_trend >= c.minimum_trend;
        let flat = c.full_slope_atr * 0.10;
        if was_up {
            let other = self.sma(c.break_confirmation, |r, _| {
                Some(if r.close < r.ma? { 1.0 } else { 0.0 })
            })?;
            Some(established && other >= 0.80 && rel < 10.0 && slope <= flat)
        } else {
            let other = self.sma(c.break_confirmation, |r, _| {
                Some(if r.close > r.ma? { 1.0 } else { 0.0 })
            })?;
            Some(established && other >= 0.80 && rel > -10.0 && slope >= -flat)
        }
    }

    fn turn_raw(&self, bull: bool) -> Option<bool> {
        let c = &self.cfg;
        let rel = self.col(0, |r| r.rel)?;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for k in 0..c.turn_lookback {
            let v = self.col(k, |r| r.rel)?;
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let mut beyond = true;
        for k in 1..=c.turn_confirm {
            let p = self.col(k, |r| r.rel)?;
            beyond &= if bull { rel > p } else { rel < p };
        }
        Some(if bull {
            lo <= -c.turn_level && beyond
        } else {
            hi >= c.turn_level && beyond
        })
    }

    /// Strict pivot `pivot_right` bars back: (low or high) of that bar.
    fn pivot(&self, low_side: bool) -> Option<f64> {
        let c = &self.cfg;
        let centre = self.at(c.pivot_right)?;
        self.at(c.pivot_right + c.pivot_left)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..c.pivot_right).chain(c.pivot_right + 1..=c.pivot_right + c.pivot_left) {
            let r = self.at(k)?;
            let o = if low_side { r.low } else { r.high };
            let ok = if low_side { v < o } else { v > o };
            if !ok {
                return None;
            }
        }
        Some(v)
    }

    /// `s` = trend, direction, respect, extension, compression, acceleration, exhaustion.
    fn state_name(&self, s: [f64; 7]) -> &'static str {
        let [trend, dir, respect, ext, comp, acc, exh] = s;
        let c = &self.cfg;
        let trending = trend >= c.minimum_trend;
        let compression = comp >= c.compression_threshold && !trending;
        let accel = trending && acc >= c.acceleration_threshold && exh < c.exhaustion_threshold;
        let overextended = trending && ext >= c.chase_extension && exh < c.exhaustion_threshold;
        let exhausted = trending && exh >= c.exhaustion_threshold;
        let healthy = trending
            && respect >= c.minimum_respect
            && ext < c.chase_extension
            && exh < c.exhaustion_threshold
            && !accel;
        let up = dir > 0.0;
        let down = dir < 0.0;
        if compression {
            "ma_compression"
        } else if exhausted && up {
            "bullish_exhaustion_risk"
        } else if exhausted && down {
            "bearish_exhaustion_risk"
        } else if overextended && up {
            "bullish_do_not_chase"
        } else if overextended && down {
            "bearish_do_not_chase"
        } else if accel && up {
            "bullish_acceleration"
        } else if accel && down {
            "bearish_acceleration"
        } else if healthy && up {
            "healthy_bullish"
        } else if healthy && down {
            "healthy_bearish"
        } else if up && trend >= 35.0 {
            "developing_bullish"
        } else if down && trend >= 35.0 {
            "developing_bearish"
        } else {
            "neutral"
        }
    }
}

impl Indicator for MarketAverageRelationship {
    fn name(&self) -> &str {
        "market_average_relationship"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        (c.atr_len - 1 + c.statistics_len - 1).max(c.respect_lookback + c.atr_len)
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let t = self.bars;
        self.bars += 1;
        let prev_close = self.rows.back().map(|r| r.close);
        let tr = match prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        let row = Row {
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            ma: self.ma.update(bar.close),
            atr: self.atr.update(tr).map(|a| a.max(1e-12)),
            ..Row::default()
        };
        self.rows.push_back(row);
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        let c = self.cfg.clone();

        // Core series.
        let (close, ma, atr) = (bar.close, self.at(0).unwrap().ma, self.at(0).unwrap().atr);
        let distance = match (ma, atr) {
            (Some(m), Some(a)) => Some(safe_div(close - m, a, 0.0)),
            _ => None,
        };
        let slope = match (ma, self.col(c.slope_len, |r| r.ma), atr) {
            (Some(m), Some(mp), Some(a)) => Some(safe_div(m - mp, a, 0.0)),
            _ => None,
        };
        let path = self.sma(c.efficiency_len, |r, p| Some((r.close - p?.close).abs()));
        let eff = match (path, self.at(c.efficiency_len)) {
            (Some(p), Some(old)) => Some(clamp(
                safe_div((close - old.close).abs(), p * c.efficiency_len as f64, 0.0),
                0.0,
                1.0,
            )),
            _ => None,
        };
        {
            let r = self.rows.back_mut().unwrap();
            r.distance = distance;
            r.slope = slope;
            r.eff = eff;
        }

        let side = match (ma, atr) {
            (Some(m), Some(a)) => Some(sign(close - m, a * 0.03)),
            _ => None,
        };
        let ma_dir = slope.map(|s| sign(s, c.full_slope_atr * 0.05));
        let bull_side = self.sma(c.persistence_len, |r, _| {
            Some(if r.close > r.ma? { 1.0 } else { 0.0 })
        });
        let bear_side = self.sma(c.persistence_len, |r, _| {
            Some(if r.close < r.ma? { 1.0 } else { 0.0 })
        });
        let bull_slope = self.sma(c.persistence_len, |r, p| {
            Some(if r.ma? > p?.ma? { 1.0 } else { 0.0 })
        });
        let bear_slope = self.sma(c.persistence_len, |r, p| {
            Some(if r.ma? < p?.ma? { 1.0 } else { 0.0 })
        });

        let mut trend = None;
        let mut direction = None;
        if let (Some(s), Some(d), Some(bs), Some(rs), Some(bsl), Some(rsl), Some(e), Some(sl)) = (
            side, ma_dir, bull_side, bear_side, bull_slope, bear_slope, eff, slope,
        ) {
            let side_persist = if s > 0.0 {
                bs
            } else if s < 0.0 {
                rs
            } else {
                0.5
            };
            let slope_persist = if d > 0.0 {
                bsl
            } else if d < 0.0 {
                rsl
            } else {
                0.5
            };
            let agree = if s != 0.0 && d != 0.0 && s == d {
                1.0
            } else if s != 0.0 && d == 0.0 {
                0.45
            } else if s == 0.0 {
                0.25
            } else {
                0.0
            };
            let strength = 100.0 * norm(sl.abs(), 0.0, c.full_slope_atr);
            trend = Some(
                100.0
                    * clamp(
                        0.30 * strength / 100.0
                            + 0.25 * side_persist
                            + 0.20 * slope_persist
                            + 0.15 * e
                            + 0.10 * agree,
                        0.0,
                        1.0,
                    ),
            );
            direction = Some(if agree >= 1.0 {
                s
            } else if d != 0.0 {
                d
            } else {
                s
            });
        }
        {
            let r = self.rows.back_mut().unwrap();
            r.trend = trend;
            r.direction = direction;
        }

        // Respect.
        let respect_stats = self.respect();
        let flip = self.sma(c.persistence_len, |r, p| {
            let p = p?;
            let (m, mp) = (r.ma?, p.ma?);
            Some(
                if (r.close > m && p.close <= mp) || (r.close < m && p.close >= mp) {
                    1.0
                } else {
                    0.0
                },
            )
        });
        let respect = match (respect_stats, flip) {
            (Some((r, _, _)), Some(f)) => Some(clamp(
                0.80 * r + 0.20 * 100.0 * (1.0 - clamp(f * 4.0, 0.0, 1.0)),
                0.0,
                100.0,
            )),
            _ => None,
        };

        // Extension.
        let abs_d = |r: &Row| r.distance.map(f64::abs);
        let mean = self.sma(c.statistics_len, |r, _| abs_d(r));
        let dev = self.stdev(c.statistics_len, abs_d);
        let extension = match (distance, mean, dev) {
            (Some(d), Some(m), Some(sd)) => {
                let a = d.abs();
                let ext_atr = 100.0 * norm(a, c.extension_normal_atr, c.extension_extreme_atr);
                let rarity = 100.0 * norm(safe_div(a - m, sd, 0.0), 0.0, 2.5);
                Some(clamp(0.70 * ext_atr + 0.30 * rarity, 0.0, 100.0))
            }
            _ => None,
        };

        // Compression.
        let band = c.compression_band_atr;
        let near = self.sma(c.compression_len, |r, _| {
            abs_d(r).map(|a| if a <= band { 1.0 } else { 0.0 })
        });
        let avg = self.sma(c.compression_len, |r, _| abs_d(r));
        let base = self.sma(c.statistics_len, |r, _| r.atr);
        let compression = match (near, avg, atr, base) {
            (Some(n), Some(av), Some(a), Some(b)) => Some(
                100.0
                    * clamp(
                        0.50 * n
                            + 0.30 * (1.0 - norm(av, band * 0.40, band * 1.80))
                            + 0.20 * (1.0 - norm(safe_div(a, b, 1.0), 0.70, 1.20)),
                        0.0,
                        1.0,
                    ),
            ),
            _ => None,
        };

        // Acceleration.
        let l = c.acceleration_len;
        let acceleration = (|| {
            let d = direction?;
            let directed = distance? * d;
            let past = self.at(l)?;
            let directed_past = past.distance? * past.direction?;
            let a = atr?;
            let ma_l = past.ma?;
            let ma_ls = self.col(l + c.slope_len, |r| r.ma)?;
            let atr_l = past.atr?;
            let c2l = self.at(2 * l)?.close;
            let sep_vel = (directed - directed_past) / l as f64;
            let cur_sv = slope? / c.slope_len as f64 * d;
            let prior_slope = safe_div(ma_l - ma_ls, atr_l, 0.0);
            let slope_acc = cur_sv - prior_slope / c.slope_len as f64 * d;
            let recent = safe_div(close - past.close, a, 0.0) * d;
            let prior = safe_div(past.close - c2l, atr_l, 0.0) * d;
            Some(clamp(
                0.50 * 100.0 * norm(sep_vel, 0.0, 0.25)
                    + 0.20 * 100.0 * norm(slope_acc, 0.0, 0.04)
                    + 0.30 * 100.0 * norm(recent - prior, 0.0, 1.50),
                0.0,
                100.0,
            ))
        })();
        {
            let r = self.rows.back_mut().unwrap();
            r.respect = respect;
            r.extension = extension;
            r.compression = compression;
            r.acceleration = acceleration;
        }

        // Exhaustion.
        let x = c.exhaustion_len;
        let wick = self.sma(x, |r, _| {
            let d = r.direction?;
            let range = (r.high - r.low).max(1e-12);
            Some(if d > 0.0 {
                (r.high - r.open.max(r.close)) / range
            } else if d < 0.0 {
                (r.open.min(r.close) - r.low) / range
            } else {
                0.0
            })
        });
        let exhaustion = (|| {
            let past = self.at(x)?;
            let decay = norm(past.eff? - eff?, 0.0, 0.35);
            let progress = safe_div((close - past.close) * direction?, atr?, 0.0);
            let fail = 1.0 - norm(progress, 0.0, 1.50);
            let loss = norm(past.acceleration? - acceleration?, 0.0, 50.0);
            Some(
                100.0
                    * clamp(
                        extension? / 100.0
                            * (0.35 * decay + 0.25 * wick? + 0.25 * fail + 0.15 * loss),
                        0.0,
                        1.0,
                    ),
            )
        })();

        // Oscillator.
        if let (Some(tq), Some(d), Some(e), Some(ex)) = (trend, direction, extension, exhaustion) {
            let penalty = clamp(
                0.18 * norm(e, c.chase_extension, 100.0)
                    + 0.32 * norm(ex, c.exhaustion_threshold, 100.0),
                0.0,
                0.50,
            );
            let raw = tq * d * (1.0 - penalty);
            let rel = ema_step(self.rel_ema, raw, 3);
            self.rel_ema = Some(rel);
            self.signal_ema = Some(ema_step(self.signal_ema, rel, 5));
        }
        let rel = trend
            .and(direction)
            .and(extension)
            .and(exhaustion)
            .and(self.rel_ema);
        let signal = rel.and(self.signal_ema);
        {
            let r = self.rows.back_mut().unwrap();
            r.exhaustion = exhaustion;
            r.rel = rel;
        }
        let broken_up = self.broken_raw(true);
        let broken_down = self.broken_raw(false);
        let turn_bull = self.turn_raw(true);
        let turn_bear = self.turn_raw(false);
        let (prev_bu, prev_bd, prev_tb, prev_ts) = match self.at(1) {
            Some(p) => (p.broken_up, p.broken_down, p.turn_bull, p.turn_bear),
            None => (None, None, None, None),
        };
        {
            let r = self.rows.back_mut().unwrap();
            r.broken_up = broken_up;
            r.broken_down = broken_down;
            r.turn_bull = turn_bull;
            r.turn_bear = turn_bear;
        }

        let reported = matches!(
            (
                rel,
                signal,
                trend,
                respect,
                extension,
                compression,
                acceleration,
                exhaustion
            ),
            (
                Some(_),
                Some(_),
                Some(_),
                Some(_),
                Some(_),
                Some(_),
                Some(_),
                Some(_)
            )
        );

        // Divergence pivots are remembered even before the first reported bar.
        let pivot_low = self.pivot(true);
        let pivot_high = self.pivot(false);
        let rel_at_pivot = self.col(c.pivot_right, |r| r.rel);
        let mut bull_div = None;
        let mut bear_div = None;
        if reported {
            bull_div = Some(match (pivot_low, self.last_pivot_low, rel_at_pivot) {
                (Some(p), Some((lp, lr)), Some(r)) => p < lp && r > lr,
                _ => false,
            });
            bear_div = Some(match (pivot_high, self.last_pivot_high, rel_at_pivot) {
                (Some(p), Some((lp, lr)), Some(r)) => p > lp && r < lr,
                _ => false,
            });
        }
        if let (Some(p), Some(r)) = (pivot_low, rel_at_pivot) {
            self.last_pivot_low = Some((p, r));
        }
        if let (Some(p), Some(r)) = (pivot_high, rel_at_pivot) {
            self.last_pivot_high = Some((p, r));
        }

        if !reported {
            return None;
        }
        let (rel, signal) = (rel.unwrap(), signal.unwrap());
        let (tq, dir) = (trend.unwrap(), direction.unwrap());
        let (resp, ext, comp, acc, exh) = (
            respect.unwrap(),
            extension.unwrap(),
            compression.unwrap(),
            acceleration.unwrap(),
            exhaustion.unwrap(),
        );

        let transition = |now: Option<bool>, before: Option<bool>| match (now, before) {
            (Some(n), Some(b)) => Some(n && !b),
            _ => None,
        };
        let candidates: [(usize, &'static str, Option<bool>); 14] = [
            (PULLBACK, "bull_pullback", self.pullback(true)),
            (PULLBACK, "bear_pullback", self.pullback(false)),
            (MOMENTUM, "bull_momentum_release", self.momentum(true)),
            (MOMENTUM, "bear_momentum_release", self.momentum(false)),
            (
                CHASE,
                "do_not_chase_up",
                self.crossing(true, true, |r| r.extension, c.chase_extension),
            ),
            (
                CHASE,
                "do_not_chase_down",
                self.crossing(false, true, |r| r.extension, c.chase_extension),
            ),
            (
                EXHAUSTION,
                "bear_exhaustion",
                self.crossing(true, false, |r| r.exhaustion, c.exhaustion_threshold),
            ),
            (
                EXHAUSTION,
                "bull_exhaustion",
                self.crossing(false, false, |r| r.exhaustion, c.exhaustion_threshold),
            ),
            (
                BROKEN,
                "bear_relationship_break",
                transition(broken_up, prev_bu),
            ),
            (
                BROKEN,
                "bull_relationship_break",
                transition(broken_down, prev_bd),
            ),
            (DIVERGENCE, "bull_divergence", bull_div),
            (DIVERGENCE, "bear_divergence", bear_div),
            (TURN, "bull_turn", transition(turn_bull, prev_tb)),
            (TURN, "bear_turn", transition(turn_bear, prev_ts)),
        ];
        let allowed: [bool; FAMILIES] = std::array::from_fn(|f| match self.last_fired[f] {
            Some(last) => t - last - 1 > c.cooldown,
            None => true,
        });
        let mut fired = [false; FAMILIES];
        for (family, kind, cond) in candidates {
            if cond == Some(true) && allowed[family] {
                fired[family] = true;
                self.alerts.push(IndicatorAlert::new(
                    kind,
                    format!("MARE {kind}: relationship {rel:+.1}, trend {tq:.0}, extension {ext:.0}, exhaustion {exh:.0}"),
                    clamp(rel.abs() / 100.0, 0.05, 1.0),
                ));
            }
        }
        for (f, hit) in fired.iter().enumerate() {
            if *hit {
                self.last_fired[f] = Some(t);
            }
        }

        let (samples, hit_rate) = respect_stats.map(|(_, s, h)| (s, h)).unwrap();
        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), signal);
        extra.insert("trend".to_string(), tq);
        extra.insert("respect".to_string(), resp);
        extra.insert("extension".to_string(), ext);
        extra.insert("compression".to_string(), comp);
        extra.insert("acceleration".to_string(), acc);
        extra.insert("exhaustion".to_string(), exh);
        extra.insert("ma".to_string(), ma.unwrap());
        extra.insert("distance_atr".to_string(), distance.unwrap());
        extra.insert("respect_samples".to_string(), samples);
        extra.insert("respect_hit_rate".to_string(), hit_rate);
        extra.insert("direction".to_string(), dir);
        let state = self.state_name([tq, dir, resp, ext, comp, acc, exh]);
        Some(
            IndicatorOutput::with_extra(rel, extra)
                .with_secondary(signal)
                .with_state(state),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
