//! Tradability Filter: a market-quality and no-trade filter with an eight-state
//! machine.
//!
//! Defaults follow the original's Balanced preset. It produces no entries of its own; it says
//! whether the market is worth trading right now.
//!
//! Four scores, most components ranked against their own last `rank_len` values
//! (`ta.percentrank`):
//!
//! | score | components |
//! |---|---|
//! | tradability `0..=100` | efficiency ratio, ADX gated by the efficiency rank, baseline slope, directional consistency, baseline cleanliness, candle quality, volatility expansion, participation (only with real volume; its weight is renormalised away otherwise) |
//! | balance `0..=100` | low efficiency, low ADX, frequent baseline crosses, stable range boundaries, range width inside `max_range_atr` |
//! | compression `0..=100` | ATR and Bollinger-bandwidth contraction, compact range, stable boundaries, low efficiency, shrinking bodies / growing wicks |
//! | direction `−100..=100` | baseline slope, displacement from the baseline, DI dominance, signed consistency, position in the range |
//!
//! States: `no_trade`, `balance`, `compression`, `ready` (compression, energy building off its
//! trough, directional pressure near a boundary), `breakout` (close beyond the previous
//! `range_len` high/low plus a buffer, held for `accept_bars`, with direction, tradability,
//! volatility expansion and body confirmation), `trend`, `aftermath` (a recent impulse whose fast
//! efficiency has collapsed while ADX is still high), `exhaustion` (an extended trend losing
//! quality). A candidate replaces the confirmed state after `confirm_bars` disagreeing bars
//! (one more for `no_trade`, one for `breakout`) and `min_state_bars` in the current state;
//! a breakout is held `hold_bars`, and balance is locked for `lock_bars` after a breakout.
//!
//! `value` = tradability, `secondary` = direction; `extra`: `balance`, `compression`,
//! `direction`, `state_code` (0 no trade, 1 balance, 2 compression, 3 ready, 4 breakout, 5 trend,
//! 6 exhaustion, 7 aftermath), `candidate_code`, `efficiency`, `fast_efficiency`, `adx`,
//! `range_high`, `range_low`, `range_position`, `atr_ratio`. `state`: the confirmed state name.
//!
//! Alerts on entering a state (closed bars; the prefix names the expected move, warnings carry
//! none): `no_trade`, `compression`, `aftermath`; `bull_ready`/`bear_ready` (direction sign),
//! `bull_breakout`/`bear_breakout`, `bull_trend`/`bear_trend` (direction sign),
//! `bear_exhaustion`/`bull_exhaustion` (`bear_` = an up-move tiring, by the active direction).
//!
//! Conventions the original leaves open (also in the golden fixture): EMAs seeded with their first
//! value; ATR and DMI Wilder averages seeded with an SMA (DMI via [`Adx`], from the second bar);
//! population standard deviation; percentrank needs the current and all `rank_len` previous
//! values; the bar-range floor (`syminfo.mintick`) is `1e-12`; the direction step and baseline
//! cross of the first bar are 0. A bar is ready once the original's data-ready bar has passed and
//! every score, the last `impulse_lookback` scores and every lookback the states read exist;
//! before that the candidate is `no_trade` and no breakout counts, while the state-machine
//! counters run on every bar. Only ready bars report. Not ported: the other presets (the lengths
//! are parameters), the v2 beta build, calibration logging and the dashboard.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/tradability_filter.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::adx::Adx;
use super::smoothing::{Ema, PercentRank, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original (preset Balanced).
#[derive(Debug, Clone)]
pub struct TradabilityFilterConfig {
    pub quality_len: usize,
    pub range_len: usize,
    pub baseline_len: usize,
    pub atr_len: usize,
    pub adx_len: usize,
    pub rank_len: usize,
    pub balance_threshold: f64,
    pub compression_threshold: f64,
    pub max_range_atr: f64,
    pub ready_direction: f64,
    pub breakout_buffer_atr: f64,
    pub breakout_quality: f64,
    pub accept_bars: usize,
    pub require_volume: bool,
    pub volume_factor: f64,
    pub confirm_bars: usize,
    pub min_state_bars: usize,
    pub hold_bars: usize,
    pub lock_bars: usize,
    pub impulse_lookback: usize,
    pub impulse_direction: f64,
    pub impulse_tradability: f64,
    pub aftermath_adx_min: f64,
    pub aftermath_eff_max: f64,
    pub fast_eff_len: usize,
}

impl Default for TradabilityFilterConfig {
    fn default() -> Self {
        Self {
            quality_len: 20,
            range_len: 30,
            baseline_len: 34,
            atr_len: 14,
            adx_len: 14,
            rank_len: 150,
            balance_threshold: 58.0,
            compression_threshold: 62.0,
            max_range_atr: 7.0,
            ready_direction: 24.0,
            breakout_buffer_atr: 0.12,
            breakout_quality: 46.0,
            accept_bars: 2,
            require_volume: false,
            volume_factor: 1.05,
            confirm_bars: 2,
            min_state_bars: 3,
            hold_bars: 3,
            lock_bars: 10,
            impulse_lookback: 10,
            impulse_direction: 55.0,
            impulse_tradability: 60.0,
            aftermath_adx_min: 25.0,
            aftermath_eff_max: 0.12,
            fast_eff_len: 8,
        }
    }
}

const NO_TRADE: u8 = 0;
const BALANCE: u8 = 1;
const COMPRESSION: u8 = 2;
const READY: u8 = 3;
const BREAKOUT: u8 = 4;
const TREND: u8 = 5;
const EXHAUSTION: u8 = 6;
const AFTERMATH: u8 = 7;

fn state_name(s: u8) -> &'static str {
    match s {
        BALANCE => "balance",
        COMPRESSION => "compression",
        READY => "ready",
        BREAKOUT => "breakout",
        TREND => "trend",
        EXHAUSTION => "exhaustion",
        AFTERMATH => "aftermath",
        _ => "no_trade",
    }
}

fn sdiv(n: f64, d: f64, fallback: f64) -> f64 {
    if d.abs() > 0.0000001 {
        n / d
    } else {
        fallback
    }
}

fn scale(v: f64, lo: f64, hi: f64) -> f64 {
    let span = hi - lo;
    if span != 0.0 {
        ((v - lo) / span).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    base: f64,
    step: f64,
    cross: f64,
    atr_ratio: Option<f64>,
    bw_ratio: Option<f64>,
    er: Option<f64>,
    avg_body: f64,
    trad: Option<f64>,
    dir: Option<f64>,
}

struct Ranks {
    atr: PercentRank,
    bw: PercentRank,
    width: PercentRank,
    vol: PercentRank,
    er: PercentRank,
    cons: PercentRank,
    adx: PercentRank,
    slope: PercentRank,
    cross: PercentRank,
    body: PercentRank,
    wick: PercentRank,
    bar_body: PercentRank,
}

/// Streaming Tradability Filter; see the module documentation.
pub struct TradabilityFilter {
    cfg: TradabilityFilterConfig,
    atr: Rma,
    atr_ref: Ema,
    base: Ema,
    adx: Adx,
    avg_body: Ema,
    avg_wick: Ema,
    bw_ref: Ema,
    vol_ref: Ema,
    cum_volume: f64,
    ranks: Ranks,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    state: u8,
    streak: usize,
    state_bars: usize,
    hold_left: usize,
    active_dir: i8,
    since_breakout: usize,
    bull_streak: usize,
    bear_streak: usize,
    bull_level: f64,
    bear_level: f64,
    alerts: Vec<IndicatorAlert>,
}

impl TradabilityFilter {
    pub fn new(cfg: TradabilityFilterConfig) -> Self {
        let r = cfg.rank_len;
        let cap = (2 * cfg.range_len)
            .max(cfg.quality_len + 1)
            .max(cfg.impulse_lookback)
            .max(cfg.fast_eff_len + 1)
            + 2;
        Self {
            atr: Rma::new(cfg.atr_len),
            atr_ref: Ema::new(cfg.quality_len * 2),
            base: Ema::new(cfg.baseline_len),
            adx: Adx::new(cfg.adx_len, cfg.adx_len, 1, 20.0),
            avg_body: Ema::new(cfg.quality_len),
            avg_wick: Ema::new(cfg.quality_len),
            bw_ref: Ema::new(cfg.quality_len * 2),
            vol_ref: Ema::new(cfg.quality_len),
            cum_volume: 0.0,
            ranks: Ranks {
                atr: PercentRank::new(r),
                bw: PercentRank::new(r),
                width: PercentRank::new(r),
                vol: PercentRank::new(r),
                er: PercentRank::new(r),
                cons: PercentRank::new(r),
                adx: PercentRank::new(r),
                slope: PercentRank::new(r),
                cross: PercentRank::new(r),
                body: PercentRank::new(r),
                wick: PercentRank::new(r),
                bar_body: PercentRank::new(r),
            },
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            state: NO_TRADE,
            streak: 0,
            state_bars: 0,
            hold_left: 0,
            active_dir: 0,
            since_breakout: 100_000,
            bull_streak: 0,
            bear_streak: 0,
            bull_level: 0.0,
            bear_level: 0.0,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &TradabilityFilterConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    /// Efficiency ratio over `len` bars ending now.
    fn efficiency(&self, len: usize) -> Option<f64> {
        let old = self.at(len)?.close;
        let mut travel = 0.0;
        for k in (0..len).rev() {
            travel += (self.at(k)?.close - self.at(k + 1)?.close).abs();
        }
        Some(sdiv((self.at(0)?.close - old).abs(), travel, 0.0))
    }

    /// Highest high and lowest low of `n` bars ending `from` bars back.
    fn range(&self, from: usize, n: usize) -> Option<(f64, f64)> {
        let mut hi = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        for k in from..from + n {
            let r = self.at(k)?;
            hi = hi.max(r.high);
            lo = lo.min(r.low);
        }
        Some((hi, lo))
    }

    /// Sum of `f` over the last `n` rows, oldest first.
    fn sum(&self, n: usize, f: impl Fn(&Row) -> f64) -> Option<f64> {
        let mut s = 0.0;
        for k in (0..n).rev() {
            s += f(self.at(k)?);
        }
        Some(s)
    }
}

impl Indicator for TradabilityFilter {
    fn name(&self) -> &str {
        "tradability_filter"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        let q = c.quality_len;
        // The range bands cover the bars before the current one; a percent rank needs its input
        // on the current and `rank_len` previous bars.
        let scores = (c
            .atr_len
            .max(q + 1)
            .max(c.range_len + 1)
            .max(self.adx.warmup_period())
            + c.rank_len)
            .max(2 * c.range_len + 1);
        let short = (q / 4).max(3);
        let data_ready = (2 * c.range_len + 5).max(3 * q + 5).max(c.rank_len) + 2;
        data_ready
            .max(scores + c.impulse_lookback.max(1) - 1)
            .max(c.atr_len + 3)
            .max(q + 1 + short)
            .max(c.atr_len.max(q) + short - 1)
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let q = c.quality_len;
        let prev = self.at(0).copied();
        let tr = match prev {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let atr_ratio = atr.map(|a| {
            let r = self.atr_ref.update(a).unwrap();
            sdiv(a, r, 1.0)
        });
        let base = self.base.update(bar.close).unwrap();
        let dmi = self.adx.on_bar(bar);
        let (adx, di_plus, di_minus) = match &dmi {
            Some(o) => (
                Some(o.value),
                Some(o.extra["di_plus"]),
                Some(o.extra["di_minus"]),
            ),
            None => (None, None, None),
        };
        let rng = (bar.high - bar.low).max(1e-12);
        let body = sdiv((bar.close - bar.open).abs(), rng, 0.0);
        let wick = sdiv(
            (bar.high - bar.open.max(bar.close)) + (bar.open.min(bar.close) - bar.low),
            rng,
            0.0,
        );
        let avg_body = self.avg_body.update(body).unwrap();
        let avg_wick = self.avg_wick.update(wick).unwrap();
        let (step, cross) = match prev {
            Some(p) => (
                if bar.close > p.close {
                    1.0
                } else if bar.close < p.close {
                    -1.0
                } else {
                    0.0
                },
                if (bar.close > base && p.close <= p.base)
                    || (bar.close < base && p.close >= p.base)
                {
                    1.0
                } else {
                    0.0
                },
            ),
            None => (0.0, 0.0),
        };
        let band = self.range(0, c.range_len);
        let prev_band = self.range(c.range_len, c.range_len);
        self.cum_volume += bar.volume;
        let vol_real = self.cum_volume > 0.0;
        let vol_ref = self.vol_ref.update(bar.volume).unwrap();
        let vol_ratio = if vol_real {
            sdiv(bar.volume, vol_ref, 1.0)
        } else {
            1.0
        };
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            base,
            step,
            cross,
            atr_ratio,
            avg_body,
            ..Row::default()
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;

        let er = self.efficiency(q);
        let fer = self.efficiency(c.fast_eff_len);
        let slope = self.at(q).map(|r| base - r.base);
        let nslope = match (slope, atr) {
            (Some(s), Some(a)) => Some(sdiv(s, a * (q as f64).sqrt(), 0.0)),
            _ => None,
        };
        let step_sum = self.sum(q, |r| r.step);
        let consistency = step_sum.map(|s| s.abs() / q as f64);
        let signed = step_sum.map(|s| s / q as f64);
        let cross_rate = self.sum(q, |r| r.cross).map(|s| s / q as f64);
        let disp = atr.map(|a| sdiv(bar.close - base, a, 0.0));
        let width_atr = match (band, atr) {
            (Some((h, l)), Some(a)) => Some(sdiv(h - l, a, c.max_range_atr * 2.0)),
            _ => None,
        };
        let pos = band.map(|(h, l)| sdiv(bar.close - l, h - l, 0.5).clamp(0.0, 1.0));
        let stability = match (band, prev_band, atr) {
            (Some((h, l)), Some((ph, pl)), Some(a)) => Some(
                1.0 - ((sdiv((h - ph).abs(), a, 0.0) + sdiv((l - pl).abs(), a, 0.0)) * 0.5)
                    .clamp(0.0, 1.0),
            ),
            _ => None,
        };
        let bw = (self.rows.len() >= q).then(|| {
            let mut sum = 0.0;
            for k in (0..q).rev() {
                sum += self.at(k).unwrap().close;
            }
            let mean = sum / q as f64;
            let mut var = 0.0;
            for k in (0..q).rev() {
                var += (self.at(k).unwrap().close - mean).powi(2);
            }
            let dev = (var / q as f64).sqrt();
            sdiv(dev * 4.0, mean, 0.0)
        });
        let bw_ratio = bw.map(|b| {
            let r = self.bw_ref.update(b).unwrap();
            sdiv(b, r, 1.0)
        });
        {
            let r = self.rows.back_mut().unwrap();
            r.bw_ratio = bw_ratio;
            r.er = er;
        }

        let rk = &mut self.ranks;
        let r_atr = rk.atr.update(atr_ratio);
        let r_bw = rk.bw.update(bw_ratio);
        let r_width = rk.width.update(width_atr);
        let r_vol = rk.vol.update(Some(vol_ratio));
        let r_er = rk.er.update(er);
        let r_cons = rk.cons.update(consistency);
        let r_adx = rk.adx.update(adx);
        let r_slope = rk.slope.update(nslope.map(f64::abs));
        let r_cross = rk.cross.update(cross_rate);
        let r_body = rk.body.update(Some(avg_body));
        let r_wick = rk.wick.update(Some(avg_wick));
        let r_bar_body = rk.bar_body.update(Some(body));

        let mut scores = None;
        if let (
            Some(r_atr),
            Some(r_bw),
            Some(r_width),
            Some(r_er),
            Some(r_cons),
            Some(r_adx),
            Some(r_slope),
            Some(r_cross),
            Some(r_body),
            Some(r_wick),
            Some(stability),
            Some(pos),
            Some(disp),
            Some(dip),
            Some(dim),
        ) = (
            r_atr, r_bw, r_width, r_er, r_cons, r_adx, r_slope, r_cross, r_body, r_wick, stability,
            pos, disp, di_plus, di_minus,
        ) {
            if !vol_real || r_vol.is_some() {
                let width = width_atr.unwrap();
                let bal = ((100.0 - r_er) * 0.26
                    + (100.0 - r_adx) * 0.18
                    + r_cross * 0.18
                    + stability * 100.0 * 0.24
                    + (1.0 - scale(width, c.max_range_atr * 0.65, c.max_range_atr * 1.25))
                        * 100.0
                        * 0.14)
                    .clamp(0.0, 100.0);
                let energy = (100.0 - r_body) * 0.5 + r_wick * 0.5;
                let comp = ((100.0 - r_atr) * 0.27
                    + (100.0 - r_bw) * 0.27
                    + (100.0 - r_width) * 0.16
                    + stability * 100.0 * 0.12
                    + (100.0 - r_er) * 0.06
                    + energy * 0.12)
                    .clamp(0.0, 100.0);
                let di_dir = sdiv(dip - dim, dip + dim, 0.0);
                let dir = ((nslope.unwrap() / 0.75).clamp(-1.0, 1.0) * 0.28
                    + (disp / 2.0).clamp(-1.0, 1.0) * 0.20
                    + di_dir * 0.20
                    + signed.unwrap() * 0.18
                    + ((pos - 0.5) * 2.0).clamp(-1.0, 1.0) * 0.14)
                    * 100.0;
                let dir = dir.clamp(-100.0, 100.0);
                let cq = r_body * 0.65 + (100.0 - r_wick) * 0.35;
                let wsum = if vol_real { 1.00 } else { 0.96 };
                let trad = ((r_er * 0.24
                    + r_adx * (r_er / 100.0) * 0.16
                    + r_slope * 0.15
                    + r_cons * 0.13
                    + (100.0 - r_cross) * 0.12
                    + cq * 0.10
                    + r_atr * 0.06
                    + if vol_real { r_vol.unwrap() * 0.04 } else { 0.0 })
                    / wsum)
                    .clamp(0.0, 100.0);
                scores = Some((trad, bal, comp, dir));
            }
        }
        {
            let r = self.rows.back_mut().unwrap();
            r.trad = scores.map(|s| s.0);
            r.dir = scores.map(|s| s.3);
        }

        let short = (q / 4).max(3);
        let data_ready = t > (2 * c.range_len + 5).max(3 * q + 5).max(c.rank_len);
        let lookbacks_ok = (0..c.impulse_lookback)
            .all(|k| self.at(k).is_some_and(|r| r.trad.is_some()))
            && self.at(3).is_some_and(|r| r.atr_ratio.is_some())
            && self.at(short).is_some_and(|r| r.er.is_some())
            && r_bar_body.is_some()
            && (0..short).all(|k| {
                self.at(k)
                    .is_some_and(|r| r.atr_ratio.is_some() && r.bw_ratio.is_some())
            });
        let ready = data_ready && scores.is_some() && lookbacks_ok;

        let mut cand = NO_TRADE;
        let (mut est_bal, mut est_comp, mut ready_c, mut aftermath_c) =
            (false, false, false, false);
        if ready {
            let (_, bal, comp, dir) = scores.unwrap();
            let pos = pos.unwrap();
            est_bal = self.since_breakout > c.lock_bars
                && bal >= c.balance_threshold
                && width_atr.unwrap() <= c.max_range_atr;
            est_comp = est_bal && comp >= c.compression_threshold;
            let bull_p = dir >= c.ready_direction && pos >= 0.68;
            let bear_p = dir <= -c.ready_direction && pos <= 0.32;
            let mut atr_tr = f64::INFINITY;
            let mut bw_tr = f64::INFINITY;
            for k in (0..short).rev() {
                let r = self.at(k).unwrap();
                atr_tr = atr_tr.min(r.atr_ratio.unwrap());
                bw_tr = bw_tr.min(r.bw_ratio.unwrap());
            }
            let (ar, br) = (atr_ratio.unwrap(), bw_ratio.unwrap());
            let energy = ar > atr_tr * 1.03 || br > bw_tr * 1.03;
            ready_c = est_comp && energy && (bull_p || bear_p);
            let mut max_dir = f64::NEG_INFINITY;
            let mut max_trad = f64::NEG_INFINITY;
            for k in 0..c.impulse_lookback {
                let r = self.at(k).unwrap();
                max_dir = max_dir.max(r.dir.unwrap().abs());
                max_trad = max_trad.max(r.trad.unwrap());
            }
            let impulse = max_dir >= c.impulse_direction && max_trad >= c.impulse_tradability;
            aftermath_c = impulse
                && adx.unwrap() >= c.aftermath_adx_min
                && fer.unwrap() <= c.aftermath_eff_max
                && ar <= self.at(3).unwrap().atr_ratio.unwrap();
        }

        let buf = atr.map_or(0.0, |a| a * c.breakout_buffer_atr);
        let (r_hi, r_lo) = band.unwrap_or((f64::NAN, f64::NAN));
        let bull_ref = if self.bull_streak > 0 {
            self.bull_level
        } else {
            r_hi
        };
        let bear_ref = if self.bear_streak > 0 {
            self.bear_level
        } else {
            r_lo
        };
        let raw_bull = ready && bar.close > bull_ref + buf;
        let raw_bear = ready && bar.close < bear_ref - buf;
        if raw_bull && self.bull_streak == 0 {
            self.bull_level = r_hi;
        }
        if raw_bear && self.bear_streak == 0 {
            self.bear_level = r_lo;
        }
        self.bull_streak = if raw_bull { self.bull_streak + 1 } else { 0 };
        self.bear_streak = if raw_bear { self.bear_streak + 1 } else { 0 };
        let (mut bull_bo, mut bear_bo) = (false, false);
        let mut exh_c = false;
        if ready {
            let (trad, _, _, dir) = scores.unwrap();
            let vol_ok = !c.require_volume || !vol_real || vol_ratio >= c.volume_factor;
            let common = trad >= c.breakout_quality
                && r_atr.unwrap() >= 35.0
                && r_bar_body.unwrap() >= 60.0
                && vol_ok;
            bull_bo = raw_bull && self.bull_streak >= c.accept_bars && dir > 0.0 && common;
            bear_bo = raw_bear && self.bear_streak >= c.accept_bars && dir < 0.0 && common;
        }
        let bo = bull_bo || bear_bo;
        if bull_bo {
            self.bull_streak = 0;
        }
        if bear_bo {
            self.bear_streak = 0;
        }
        self.since_breakout = if bo { 0 } else { self.since_breakout + 1 };
        if ready {
            let (trad, _, _, dir) = scores.unwrap();
            let e = er.unwrap();
            let bull_t = trad >= 55.0 && dir >= 28.0 && e >= 0.20 && bar.close > base;
            let bear_t = trad >= 55.0 && dir <= -28.0 && e >= 0.20 && bar.close < base;
            let trend_c = bull_t || bear_t;
            let ext = sdiv(bar.close - base, atr.unwrap(), 0.0).abs();
            let past = self.at(short).unwrap();
            let decay = e < past.er.unwrap() && avg_body < past.avg_body;
            exh_c = trend_c
                && ext >= 2.6
                && (decay || r_wick.unwrap() >= 80.0 || r_atr.unwrap() >= 90.0);
            cand = if bo {
                BREAKOUT
            } else if exh_c {
                EXHAUSTION
            } else if trend_c {
                TREND
            } else if ready_c {
                READY
            } else if aftermath_c {
                AFTERMATH
            } else if est_comp {
                COMPRESSION
            } else if est_bal {
                BALANCE
            } else {
                NO_TRADE
            };
        }

        self.streak = if cand != self.state {
            self.streak + 1
        } else {
            0
        };
        let dir_now = scores.map(|s| s.3);
        if bull_bo {
            self.active_dir = 1;
        } else if bear_bo {
            self.active_dir = -1;
        } else if self.state == TREND {
            if let Some(d) = dir_now {
                if d >= 20.0 {
                    self.active_dir = 1;
                } else if d <= -20.0 {
                    self.active_dir = -1;
                }
            }
        }
        if bo {
            self.hold_left = c.hold_bars;
        }
        if self.hold_left > 0 {
            self.hold_left -= 1;
        }
        let hold = self.state == BREAKOUT && self.hold_left > 0 && !exh_c;
        let may_leave =
            self.state_bars >= c.min_state_bars || cand == BREAKOUT || self.state == BREAKOUT;
        let required = if cand == BREAKOUT {
            1
        } else if cand == NO_TRADE {
            c.confirm_bars + 1
        } else {
            c.confirm_bars
        };
        let prev_state = self.state;
        if hold {
            self.state = BREAKOUT;
            self.state_bars += 1;
        } else if self.streak >= required && may_leave && cand != self.state {
            self.state = cand;
            self.state_bars = 0;
        } else {
            self.state_bars += 1;
        }
        if self.state != prev_state {
            let d = dir_now.unwrap_or(0.0);
            let kind = match self.state {
                NO_TRADE => Some("no_trade"),
                COMPRESSION => Some("compression"),
                READY if d > 0.0 => Some("bull_ready"),
                READY if d < 0.0 => Some("bear_ready"),
                BREAKOUT if self.active_dir == 1 => Some("bull_breakout"),
                BREAKOUT if self.active_dir == -1 => Some("bear_breakout"),
                TREND if d > 0.0 => Some("bull_trend"),
                TREND => Some("bear_trend"),
                AFTERMATH => Some("aftermath"),
                EXHAUSTION if self.active_dir >= 0 => Some("bear_exhaustion"),
                EXHAUSTION => Some("bull_exhaustion"),
                _ => None,
            };
            if let Some(kind) = kind {
                let trad = scores.map_or(0.0, |s| s.0);
                self.alerts.push(IndicatorAlert::new(
                    kind,
                    format!("Tradability Filter {kind}: tradability {trad:.0}, direction {d:+.0}"),
                    (trad / 100.0).clamp(0.05, 1.0),
                ));
            }
        }

        if !ready {
            return None;
        }
        let (trad, bal, comp, dir) = scores.unwrap();
        let mut extra = HashMap::new();
        for (k, v) in [
            ("balance", bal),
            ("compression", comp),
            ("direction", dir),
            ("state_code", f64::from(self.state)),
            ("candidate_code", f64::from(cand)),
            ("efficiency", er.unwrap()),
            ("fast_efficiency", fer.unwrap()),
            ("adx", adx.unwrap()),
            ("range_high", r_hi),
            ("range_low", r_lo),
            ("range_position", pos.unwrap()),
            ("atr_ratio", atr_ratio.unwrap()),
        ] {
            extra.insert(k.to_string(), v);
        }
        Some(
            IndicatorOutput::with_extra(trad, extra)
                .with_secondary(dir)
                .with_state(state_name(self.state)),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
