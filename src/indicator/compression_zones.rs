//! Compression Zones: compression zones from a weighted multi-module score, with a breakout watch
//! after a valid zone ends.
//!
//! Defaults follow the original's Balanced sensitivity.
//!
//! Per bar a score `0..=80` over the last `lookback` bars:
//!
//! | module | points |
//! |---|---|
//! | ATR compression | `25 · (1 − range/ATR ÷ range_atr)` while range/ATR ≤ `range_atr` |
//! | ADX chop | `15 · (1 − ADX ÷ adx_max)` while ADX ≤ `adx_max` |
//! | volume dry-up | 15 when volume is below its MA (and the half-length MA is too), or when there is no real volume |
//! | impulse-in | 15 when the close moved more than `impulse_atr`·ATR over `impulse_lookback` bars |
//! | historical S/R | 10 when a range edge lies within `hist_tol`·ATR of a pivot (or an earlier zone edge) from before the range window, at most `hist_lookback` bars old |
//!
//! A bar is a zone candidate when range compression and the boundary-touch structure (each edge
//! retested at least once, `min_touches` in all, the bar setting the extreme not counted) agree
//! and the score reaches `min_score`. A zone widens while candidates continue; inside it upper
//! and lower edge tests are counted and read as bull/bear pressure (EMA structure, ADX strength)
//! or continuation (impulse). When a zone of at least `min_zone_bars` ends, a breakout watch of
//! `watch_bars` arms.
//!
//! `value` = score; `extra`: `range_atr`, `in_zone` (1/0), `zone_high`/`zone_low` while in a
//! zone, `upper_touches`, `lower_touches`, `read` (1 bull pressure, 0.5 bull continuation,
//! −0.5/−1 bear, 0 balance or none), `armed` (1/0), `hist_reuse` (1/0). `state`:
//! `bull_pressure`, `bear_pressure`, `bull_continuation`, `bear_continuation`, `balance`,
//! `armed` or `no_zone`. The zone fields describe the bar before an ending zone is cleared.
//!
//! Alerts (closed bars; prefix = expected move): `bull_breakout` / `bear_breakout` — while armed,
//! a close beyond the zone edge plus 0.05 ATR, body ≥ 0.12 ATR, close in the outer 40 % of the
//! bar, volume above its MA where real volume exists, and the zone read not pointing the other
//! way. A breakout or the end of the watch disarms.
//!
//! Conventions the original leaves open (also in the golden fixture): ATR and ADX Wilder
//! averages seeded with an SMA (ADX via [`Adx`]); EMAs seeded with their first value; on ties the
//! range extreme is the most recent bar; pivots strict on both sides; the historical level list
//! expires from its front only. A bar reports once ATR, ADX, the range window and the
//! close `impulse_lookback` bars back exist; volume counts as unavailable until its MA exists.
//! Not ported: the other sensitivity presets (the parameters cover them), module switches and
//! the old-zone boxes.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/compression_zones.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::adx::Adx;
use super::smoothing::{Ema, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original (Balanced, breakout quality Confirmed).
#[derive(Debug, Clone)]
pub struct CompressionZonesConfig {
    pub lookback: usize,
    pub min_zone_bars: usize,
    pub range_atr: f64,
    pub adx_max: f64,
    pub min_score: f64,
    pub min_touches: usize,
    pub atr_len: usize,
    pub touch_tol: f64,
    pub vol_len: usize,
    pub impulse_lookback: usize,
    pub impulse_atr: f64,
    pub hist_lookback: usize,
    pub hist_tol: f64,
    pub pivot_len: usize,
    pub watch_bars: usize,
    pub breakout_buffer: f64,
    pub min_body: f64,
    pub min_close_pos: f64,
}

impl Default for CompressionZonesConfig {
    fn default() -> Self {
        Self {
            lookback: 16,
            min_zone_bars: 5,
            range_atr: 4.2,
            adx_max: 27.0,
            min_score: 35.0,
            min_touches: 2,
            atr_len: 14,
            touch_tol: 0.25,
            vol_len: 20,
            impulse_lookback: 12,
            impulse_atr: 1.8,
            hist_lookback: 250,
            hist_tol: 0.5,
            pivot_len: 5,
            watch_bars: 20,
            breakout_buffer: 0.05,
            min_body: 0.12,
            min_close_pos: 0.60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Read {
    None,
    Balance,
    BullPressure,
    BearPressure,
    BullContinuation,
    BearContinuation,
}

impl Read {
    fn code(self) -> f64 {
        match self {
            Read::BullPressure => 1.0,
            Read::BearPressure => -1.0,
            Read::BullContinuation => 0.5,
            Read::BearContinuation => -0.5,
            Read::None | Read::Balance => 0.0,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Read::None => "no_zone",
            Read::Balance => "balance",
            Read::BullPressure => "bull_pressure",
            Read::BearPressure => "bear_pressure",
            Read::BullContinuation => "bull_continuation",
            Read::BearContinuation => "bear_continuation",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
}

/// Streaming Compression Zones; see the module documentation.
pub struct CompressionZones {
    cfg: CompressionZonesConfig,
    atr: Rma,
    adx: Adx,
    vol_ma: Sma,
    vol_ma_short: Sma,
    ema_fast: Ema,
    ema_slow: Ema,
    cum_volume: f64,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    hist: VecDeque<(f64, usize)>,
    in_zone: bool,
    zone_start: usize,
    zone_high: f64,
    zone_low: f64,
    upper_touches: usize,
    lower_touches: usize,
    read: Read,
    armed: bool,
    armed_until: usize,
    armed_high: f64,
    armed_low: f64,
    armed_read: Read,
    alerts: Vec<IndicatorAlert>,
}

impl CompressionZones {
    pub fn new(cfg: CompressionZonesConfig) -> Self {
        let cap = cfg
            .lookback
            .max(cfg.impulse_lookback + 1)
            .max(2 * cfg.pivot_len + 1)
            + 2;
        let fast = ((cfg.lookback as f64 / 2.0).round() as usize).max(2);
        Self {
            atr: Rma::new(cfg.atr_len),
            adx: Adx::new(14, 14, 1, cfg.adx_max),
            vol_ma: Sma::new(cfg.vol_len),
            vol_ma_short: Sma::new((cfg.vol_len / 2).max(3)),
            ema_fast: Ema::new(fast),
            ema_slow: Ema::new(cfg.lookback),
            cum_volume: 0.0,
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            hist: VecDeque::new(),
            in_zone: false,
            zone_start: 0,
            zone_high: 0.0,
            zone_low: 0.0,
            upper_touches: 0,
            lower_touches: 0,
            read: Read::None,
            armed: false,
            armed_until: 0,
            armed_high: 0.0,
            armed_low: 0.0,
            armed_read: Read::None,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &CompressionZonesConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    fn pivot(&self, low_side: bool) -> Option<f64> {
        let p = self.cfg.pivot_len;
        let centre = self.at(p)?;
        self.at(2 * p)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..p).chain(p + 1..=2 * p) {
            let r = self.at(k)?;
            let o = if low_side { r.low } else { r.high };
            if (low_side && v >= o) || (!low_side && v <= o) {
                return None;
            }
        }
        Some(v)
    }
}

impl Indicator for CompressionZones {
    fn name(&self) -> &str {
        "compression_zones"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        self.adx
            .warmup_period()
            .max(c.atr_len)
            .max(c.lookback)
            .max(c.impulse_lookback + 1)
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let tr = match self.at(0) {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let adx = self.adx.on_bar(bar).map(|o| o.value);
        let vol_ma = self.vol_ma.update(bar.volume);
        let vol_ma_short = self.vol_ma_short.update(bar.volume);
        let ema_fast = self.ema_fast.update(bar.close).unwrap();
        let ema_slow = self.ema_slow.update(bar.close).unwrap();
        self.cum_volume += bar.volume;
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;

        if let Some(ph) = self.pivot(false) {
            self.hist.push_back((ph, t - c.pivot_len));
        }
        if let Some(pl) = self.pivot(true) {
            self.hist.push_back((pl, t - c.pivot_len));
        }
        while self
            .hist
            .front()
            .is_some_and(|&(_, b)| t - b > c.hist_lookback)
        {
            self.hist.pop_front();
        }

        let n = c.lookback;
        let (Some(a), Some(adx), Some(_), Some(old)) = (
            atr,
            adx,
            self.at(n - 1),
            self.at(c.impulse_lookback).map(|r| r.close),
        ) else {
            return None;
        };
        let (mut r_hi, mut r_lo) = (f64::NEG_INFINITY, f64::INFINITY);
        let (mut hi_off, mut lo_off) = (0, 0);
        for i in 0..n {
            let r = self.at(i).unwrap();
            if r.high > r_hi {
                r_hi = r.high;
                hi_off = i;
            }
            if r.low < r_lo {
                r_lo = r.low;
                lo_off = i;
            }
        }
        let ratio = (a > 0.0).then(|| (r_hi - r_lo) / a);
        let vol_avail = self.cum_volume > 0.0 && bar.volume > 0.0 && vol_ma.is_some();
        let drying = vol_avail
            && bar.volume < vol_ma.unwrap()
            && vol_ma_short.is_some_and(|s| s < vol_ma.unwrap());
        let mv = bar.close - old;
        let impulse = if mv > a * c.impulse_atr {
            1
        } else if mv < -a * c.impulse_atr {
            -1
        } else {
            0
        };
        let range_ok = ratio.is_some_and(|r| r <= c.range_atr);
        let tol = a * c.touch_tol;
        let (mut ut, mut lt) = (0, 0);
        for i in 0..n {
            let r = self.at(i).unwrap();
            if i != hi_off && r.high >= r_hi - tol {
                ut += 1;
            }
            if i != lo_off && r.low <= r_lo + tol {
                lt += 1;
            }
        }
        let touch_ok = ut > 0 && lt > 0 && ut + lt >= c.min_touches;
        let (mut near_top, mut near_bot) = (false, false);
        for &(lvl, b) in &self.hist {
            if b + n > t {
                continue;
            }
            if (lvl - r_hi).abs() <= a * c.hist_tol {
                near_top = true;
            }
            if (lvl - r_lo).abs() <= a * c.hist_tol {
                near_bot = true;
            }
        }
        let reuse = near_top || near_bot;

        let mut score = 0.0;
        if range_ok {
            score += 25.0 * (1.0 - (ratio.unwrap() / c.range_atr).min(1.0));
        }
        if adx <= c.adx_max {
            score += 15.0 * (1.0 - (adx / c.adx_max).min(1.0));
        }
        if !vol_avail || drying {
            score += 15.0;
        }
        if impulse != 0 {
            score += 15.0;
        }
        if reuse {
            score += 10.0;
        }
        let candidate = range_ok && touch_ok && score >= c.min_score;

        let zone_age = if self.in_zone {
            t + 1 - self.zone_start
        } else {
            0
        };
        let end = !candidate && self.in_zone;
        if candidate && !self.in_zone {
            self.in_zone = true;
            self.zone_start = t + 1 - n;
            self.zone_high = r_hi;
            self.zone_low = r_lo;
            self.upper_touches = 0;
            self.lower_touches = 0;
        } else if candidate {
            self.zone_high = self.zone_high.max(r_hi);
            self.zone_low = self.zone_low.min(r_lo);
        }
        if self.in_zone {
            if bar.high >= self.zone_high - tol {
                self.upper_touches += 1;
            }
            if bar.low <= self.zone_low + tol {
                self.lower_touches += 1;
            }
            let p = self.upper_touches as i64 - self.lower_touches as i64;
            let need = if adx >= 20.0 { 1 } else { 2 };
            self.read = if p >= 3 || (p >= need && ema_fast > ema_slow) {
                Read::BullPressure
            } else if p <= -3 || (p <= -need && ema_fast < ema_slow) {
                Read::BearPressure
            } else if impulse == 1 {
                Read::BullContinuation
            } else if impulse == -1 {
                Read::BearContinuation
            } else {
                Read::Balance
            };
        }
        let mut extra = HashMap::new();
        extra.insert("in_zone".to_string(), if self.in_zone { 1.0 } else { 0.0 });
        if self.in_zone {
            extra.insert("zone_high".to_string(), self.zone_high);
            extra.insert("zone_low".to_string(), self.zone_low);
        }
        extra.insert("upper_touches".to_string(), self.upper_touches as f64);
        extra.insert("lower_touches".to_string(), self.lower_touches as f64);
        extra.insert("read".to_string(), self.read.code());
        let zone_read = self.read;
        if end {
            if zone_age >= c.min_zone_bars {
                self.armed = true;
                self.armed_until = t + c.watch_bars;
                self.armed_high = self.zone_high;
                self.armed_low = self.zone_low;
                self.armed_read = self.read;
                self.hist.push_back((self.zone_high, t));
                self.hist.push_back((self.zone_low, t));
            }
            self.in_zone = false;
            self.upper_touches = 0;
            self.lower_touches = 0;
            self.read = Read::None;
        }
        if self.armed && t > self.armed_until {
            self.armed = false;
            self.armed_read = Read::None;
        }
        let vol_ok = !vol_avail || bar.volume > vol_ma.unwrap();
        let rng = bar.high - bar.low;
        let pos = if rng > 0.0 {
            (bar.close - bar.low) / rng
        } else {
            0.5
        };
        let body_ok = a <= 0.0 || (bar.close - bar.open).abs() >= a * c.min_body;
        let buf = a * c.breakout_buffer;
        let read_up = !matches!(self.armed_read, Read::BearPressure | Read::BearContinuation);
        let read_dn = !matches!(self.armed_read, Read::BullPressure | Read::BullContinuation);
        let up = self.armed
            && bar.close > self.armed_high + buf
            && vol_ok
            && body_ok
            && pos >= c.min_close_pos
            && read_up;
        let dn = self.armed
            && bar.close < self.armed_low - buf
            && vol_ok
            && body_ok
            && pos <= 1.0 - c.min_close_pos
            && read_dn;
        if up || dn {
            let kind = if up { "bull_breakout" } else { "bear_breakout" };
            let edge = if up { self.armed_high } else { self.armed_low };
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("Compression Zones {kind}: close beyond zone edge {edge:.4}"),
                0.7,
            ));
            self.armed = false;
            self.armed_read = Read::None;
        }
        if let Some(r) = ratio {
            extra.insert("range_atr".to_string(), r);
        }
        extra.insert("armed".to_string(), if self.armed { 1.0 } else { 0.0 });
        extra.insert("hist_reuse".to_string(), if reuse { 1.0 } else { 0.0 });
        let state = if zone_read != Read::None {
            zone_read.name()
        } else if self.armed {
            "armed"
        } else {
            "no_zone"
        };
        Some(IndicatorOutput::with_extra(score, extra).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const COMPRESSION_ZONES_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "compression_zones",
        aliases: &[],
        description: "Compression Zones: compression zones from a 0..80 score (ATR compression, ADX chop, volume dry-up where real volume exists, impulse-in, historical S/R reuse) plus range compression and boundary-touch structure; value = score; extra: range_atr, in_zone, zone_high/zone_low, upper_touches, lower_touches, read (bull/bear pressure or continuation), armed, hist_reuse; alerts bull/bear_breakout (close beyond the zone edge within watch_bars after a zone of at least min_zone_bars, with body, close position and volume checks; prefix = expected move)",
        params: &[
            super::definition::ParamSpec::count("lookback", 16, 8, 100).meaning("bars the range compression is measured over"),
            super::definition::ParamSpec::count("min_zone_bars", 5, 3, 50).meaning("bars a zone must last before its breakout alerts"),
            super::definition::ParamSpec::number("range_atr", 4.2, 0.5, 20.0).meaning("widest zone range, in ATRs"),
            super::definition::ParamSpec::number("adx_max", 27.0, 5.0, 60.0).meaning("ADX above which the market is not compressed"),
            super::definition::ParamSpec::number("min_score", 35.0, 0.0, 80.0).meaning("score a zone needs"),
            super::definition::ParamSpec::count("min_touches", 2, 2, 20).meaning("boundary touches a zone needs"),
            super::definition::ParamSpec::count("watch_bars", 20, 1, 200).meaning("bars after the zone a breakout still alerts"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(CompressionZones::new(CompressionZonesConfig {
                lookback: p.count("lookback"),
                min_zone_bars: p.count("min_zone_bars"),
                range_atr: p.number("range_atr"),
                adx_max: p.number("adx_max"),
                min_score: p.number("min_score"),
                min_touches: p.count("min_touches"),
                watch_bars: p.count("watch_bars"),
                ..CompressionZonesConfig::default()
            })))
        },
    };
