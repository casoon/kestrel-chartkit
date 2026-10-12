//! Liquidity Hunter: ranked equal-high/equal-low liquidity levels with scored sweep, breakout,
//! stop-hunt and reclaim events.
//!
//! Port of the Pine Script "Liquidity Hunter" (WavesUnchained) without the parts chartkit
//! already has or the script keeps off by default: fair value gaps (see `liquidity_fvg`), the
//! regime/premium-discount bonus, the level flip, the session filter (off means every bar is "in
//! session", so the session boost of 10 is always added, as in Pine) and the WaveTrend/MFI
//! exhaustion layer. Unlike `liquidity_pools` (percent tolerance, no scoring) levels cluster
//! within an ATR tolerance, are ranked, and events carry volume and score conditions.
//!
//! **Levels.** A confirmed pivot high joins the first buy-side level (BSL) within
//! `eq_tolerance_atr · ATR` of it — touches + 1, price = running mean of its pivots — or starts a
//! new one; at most `max_tracked` per side (the oldest drops). Pivot lows form sell-side levels
//! (SSL). Each bar, levels untouched for more than `max_age` bars or further than
//! `max_distance_atr` ATR from the close are dropped.
//!
//! **Events** (levels with at least `min_touches` touches, formed before this bar). The first
//! time the high reaches `price + min_sweep_atr · ATR` the level is swept: a close at or beyond
//! that mark is a *breakout*, otherwise a *sweep*; a sweep with an upper wick of at least
//! `hunt_wick_min` of the range, the close in the lower `hunt_close_pos_max` of the range and
//! volume ≥ `hunt_volume_mult` × its 20-bar average is a *stop hunt*. Within `reclaim_bars` bars
//! of the sweep (the sweep bar included), a close at least `min_close_back_atr` ATR back below
//! the level with the high at or above it and volume ≥ `min_volume_ratio` × average is a
//! *reclaim*, scored
//! `min(100, (12·min(touches,5) + 18·min(wick ATR,1.5) + 24·min(close-back ATR,1.5) + volume
//! term) · scale + 10 (session) + 10 (close beyond the previous low))`, volume term
//! `12·max(0, min(ratio,3) - 1)`; without real volume the volume term and conditions drop and the
//! core scales by 147/123. SSL mirrors everything.
//!
//! **Ranking.** Unswept levels with enough touches on the right side of the close score
//! `max(0, 20·min(touches,5) + 18 - 18·min(age/max_age,1) - 7·min(distance ATR, max_distance))`;
//! the highest score (strictly above 0) is primary, the next secondary.
//!
//! `value`: the bias (Pine's dashboard summary): ±2 for a reclaim this bar, else ±1 when a recent
//! reclaim (within `event_memory` bars, among the last `max_events`) scored ≥ `premium_score`; ±1
//! when the stronger primary level is within `alert_distance_atr` (SSL +, BSL −). `extra`:
//! `primary_bsl`, `primary_bsl_score`, `primary_bsl_touches`, `secondary_bsl`,
//! `secondary_bsl_score` and the same for SSL (each while it exists), `bsl_count`, `ssl_count`
//! (ranked levels), `recent_bull_score`, `recent_bear_score`, `volume_ratio`, and
//! `bear_reclaim_score` / `bull_reclaim_score` on a reclaim bar.
//!
//! Alerts (closed bars; prefix = expected move, warnings without one):
//!
//! | alert | condition |
//! |---|---|
//! | `bear_bsl_sweep` / `bull_ssl_sweep` | a level swept and closed back |
//! | `bull_bsl_breakout` / `bear_ssl_breakout` | a level broken with the close beyond it |
//! | `bear_bsl_stop_hunt` / `bull_ssl_stop_hunt` | a sweep with a rejection candle and a volume spike |
//! | `bear_reclaim` / `bull_reclaim` | a reclaim |
//! | `bear_premium_reclaim` / `bull_premium_reclaim` | a reclaim scoring ≥ `premium_score` |
//! | `primary_bsl_near` / `primary_ssl_near` | the close comes within `alert_distance_atr` of the primary level |
//!
//! Conventions where Pine leaves them open (also in the golden fixture): ATR(14) = Wilder average
//! of the true range (first bar high - low), SMA-seeded; while it is missing no level merges,
//! nothing is pruned or swept (Pine's `na` comparisons are false); the `2 · mintick` floor of the
//! tolerance and the mintick floor of the bar range are dropped (range floor `1e-12`); volume
//! ratio 1 while the 20-bar average is missing or 0; "real volume" = the cumulative volume so far
//! is positive; pivots strict on both sides. Output starts once the ATR exists.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/liquidity_hunter.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults as in the Pine script.
#[derive(Debug, Clone)]
pub struct LiquidityHunterConfig {
    pub pivot_len: usize,
    pub atr_len: usize,
    pub eq_tolerance_atr: f64,
    pub min_touches: u32,
    pub max_tracked: usize,
    pub max_age: usize,
    pub max_distance_atr: f64,
    pub min_sweep_atr: f64,
    pub reclaim_bars: usize,
    pub min_close_back_atr: f64,
    pub min_volume_ratio: f64,
    pub max_events: usize,
    pub hunt_volume_mult: f64,
    pub hunt_wick_min: f64,
    pub hunt_close_pos_max: f64,
    pub alert_distance_atr: f64,
    pub event_memory: usize,
    pub premium_score: f64,
}

impl Default for LiquidityHunterConfig {
    fn default() -> Self {
        Self {
            pivot_len: 8,
            atr_len: 14,
            eq_tolerance_atr: 0.12,
            min_touches: 2,
            max_tracked: 30,
            max_age: 400,
            max_distance_atr: 8.0,
            min_sweep_atr: 0.05,
            reclaim_bars: 3,
            min_close_back_atr: 0.10,
            min_volume_ratio: 1.2,
            max_events: 6,
            hunt_volume_mult: 1.5,
            hunt_wick_min: 0.45,
            hunt_close_pos_max: 0.45,
            alert_distance_atr: 1.0,
            event_memory: 30,
            premium_score: 65.0,
        }
    }
}

#[derive(Debug, Clone)]
struct Level {
    price: f64,
    pivot_bar: usize,
    last_touch: usize,
    touches: u32,
    swept: bool,
    sweep_bar: usize,
    reclaimed: bool,
}

/// The two best-ranked levels of one side: (price, score, touches).
type Ranked = (Option<(f64, f64, u32)>, Option<(f64, f64, u32)>, usize);

/// Streaming Liquidity Hunter; see the module documentation.
pub struct LiquidityHunter {
    cfg: LiquidityHunterConfig,
    atr: Rma,
    avg_volume: Sma,
    rows: VecDeque<Bar>,
    cap: usize,
    bars: usize,
    cum_volume: f64,
    bsl: Vec<Level>,
    ssl: Vec<Level>,
    /// Recent reclaims, newest first: (bar, direction, score).
    events: VecDeque<(usize, i8, f64)>,
    prev_near: (bool, bool),
    alerts: Vec<IndicatorAlert>,
}

impl LiquidityHunter {
    pub fn new(cfg: LiquidityHunterConfig) -> Self {
        let cap = 2 * cfg.pivot_len + 2;
        Self {
            atr: Rma::new(cfg.atr_len),
            avg_volume: Sma::new(20),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            cfg,
            bars: 0,
            cum_volume: 0.0,
            bsl: Vec::new(),
            ssl: Vec::new(),
            events: VecDeque::new(),
            prev_near: (false, false),
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &LiquidityHunterConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Bar> {
        self.rows.len().checked_sub(k + 1).map(|i| &self.rows[i])
    }

    fn pivot(&self, low_side: bool) -> Option<f64> {
        let p = self.cfg.pivot_len;
        let centre = self.at(p)?;
        self.at(2 * p)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..p).chain(p + 1..=2 * p) {
            let b = self.at(k)?;
            let ok = if low_side { v < b.low } else { v > b.high };
            if !ok {
                return None;
            }
        }
        Some(v)
    }

    fn add_pivot(&mut self, buy_side: bool, price: f64, pivot_bar: usize, tol: Option<f64>) {
        let max = self.cfg.max_tracked;
        let levels = if buy_side {
            &mut self.bsl
        } else {
            &mut self.ssl
        };
        let hit = tol.and_then(|tol| levels.iter_mut().find(|l| (l.price - price).abs() <= tol));
        match hit {
            Some(l) => {
                l.touches += 1;
                l.last_touch = pivot_bar;
                l.price = (l.price * (l.touches - 1) as f64 + price) / l.touches as f64;
            }
            None => levels.push(Level {
                price,
                pivot_bar,
                last_touch: pivot_bar,
                touches: 1,
                swept: false,
                sweep_bar: 0,
                reclaimed: false,
            }),
        }
        if levels.len() > max {
            levels.remove(0);
        }
    }

    fn level_score(&self, touches: u32, age: usize, dist: f64) -> f64 {
        let c = &self.cfg;
        let touch = touches.min(5) as f64 * 20.0;
        let age_pen = (age as f64 / c.max_age as f64).min(1.0) * 18.0;
        let dist_pen = dist.min(c.max_distance_atr) * 7.0;
        (touch + 18.0 - age_pen - dist_pen).max(0.0)
    }

    fn rank(&self, buy_side: bool, close: f64, atr: f64, t: usize) -> Ranked {
        let levels = if buy_side { &self.bsl } else { &self.ssl };
        let mut primary: Option<(f64, f64, u32)> = None;
        let mut secondary: Option<(f64, f64, u32)> = None;
        let mut count = 0;
        for l in levels {
            let right_side = if buy_side {
                l.price > close
            } else {
                l.price < close
            };
            if l.touches < self.cfg.min_touches
                || l.swept
                || l.reclaimed
                || !right_side
                || atr <= 0.0
            {
                continue;
            }
            let score =
                self.level_score(l.touches, t - l.last_touch, (l.price - close).abs() / atr);
            count += 1;
            if score > primary.map_or(0.0, |p| p.1) {
                secondary = primary;
                primary = Some((l.price, score, l.touches));
            } else if score > secondary.map_or(0.0, |s| s.1) {
                secondary = Some((l.price, score, l.touches));
            }
        }
        (primary, secondary, count)
    }
}

impl Indicator for LiquidityHunter {
    fn name(&self) -> &str {
        "liquidity_hunter"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.atr_len - 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let t = self.bars;
        self.bars += 1;
        let prev = self.rows.back().cloned();
        let tr = match &prev {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        self.cum_volume += bar.volume;
        let volume_real = self.cum_volume > 0.0;
        let vol_ratio = match self.avg_volume.update(bar.volume) {
            Some(a) if a > 0.0 => bar.volume / a,
            _ => 1.0,
        };
        self.rows.push_back(bar.clone());
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }

        // Levels.
        let tol = atr.map(|a| a * c.eq_tolerance_atr);
        let p = c.pivot_len;
        if let Some(ph) = self.pivot(false) {
            self.add_pivot(true, ph, t - p, tol);
        }
        if let Some(pl) = self.pivot(true) {
            self.add_pivot(false, pl, t - p, tol);
        }
        if let Some(a) = atr.filter(|a| *a > 0.0) {
            let keep = |l: &Level| {
                t - l.last_touch <= c.max_age
                    && (bar.close - l.price).abs() / a <= c.max_distance_atr
            };
            self.bsl.retain(keep);
            self.ssl.retain(keep);
        }

        // Events.
        let range = (bar.high - bar.low).max(1e-12);
        let upper_wick = (bar.high - bar.open.max(bar.close)) / range;
        let lower_wick = (bar.open.min(bar.close) - bar.low) / range;
        let close_pos = (bar.close - bar.low) / range;
        let volume_ok = |min: f64| !volume_real || vol_ratio >= min;
        let event_score = |touches: u32, wick: f64, back: f64, structure: f64| {
            let vol = if volume_real {
                (vol_ratio.min(3.0) - 1.0).max(0.0) * 12.0
            } else {
                0.0
            };
            let scale = if volume_real { 1.0 } else { 147.0 / 123.0 };
            let core =
                touches.min(5) as f64 * 12.0 + wick.min(1.5) * 18.0 + back.min(1.5) * 24.0 + vol;
            (core * scale + 10.0 + structure).min(100.0)
        };
        let mut flags = [false; 6]; // sweep, breakout, hunt for BSL then SSL
        let mut reclaim_score: [Option<f64>; 2] = [None, None]; // bear, bull
        if let Some(a) = atr {
            let sweep_abs = a * c.min_sweep_atr;
            let back_abs = a * c.min_close_back_atr;
            let prev_low = prev.as_ref().map(|b| b.low);
            let prev_high = prev.as_ref().map(|b| b.high);
            for (side, levels) in [(0usize, &mut self.bsl), (1, &mut self.ssl)] {
                let buy = side == 0;
                for l in levels.iter_mut() {
                    if l.touches < c.min_touches || l.pivot_bar >= t {
                        continue;
                    }
                    let (wick_atr, back_atr) = if a > 0.0 {
                        if buy {
                            (
                                (bar.high - l.price).max(0.0) / a,
                                (l.price - bar.close).max(0.0) / a,
                            )
                        } else {
                            (
                                (l.price - bar.low).max(0.0) / a,
                                (bar.close - l.price).max(0.0) / a,
                            )
                        }
                    } else {
                        (0.0, 0.0)
                    };
                    let touched = if buy {
                        bar.high >= l.price + sweep_abs
                    } else {
                        bar.low <= l.price - sweep_abs
                    };
                    if !l.swept && touched {
                        l.swept = true;
                        l.sweep_bar = t;
                        let breakout = if buy {
                            bar.close >= l.price + sweep_abs
                        } else {
                            bar.close <= l.price - sweep_abs
                        };
                        if breakout {
                            flags[side * 3 + 1] = true;
                        } else {
                            flags[side * 3] = true;
                            let (wick, pos) = if buy {
                                (upper_wick, close_pos)
                            } else {
                                (lower_wick, 1.0 - close_pos)
                            };
                            if wick >= c.hunt_wick_min
                                && pos <= c.hunt_close_pos_max
                                && volume_ok(c.hunt_volume_mult)
                            {
                                flags[side * 3 + 2] = true;
                            }
                        }
                    }
                    let can_reclaim = l.swept && !l.reclaimed && t <= l.sweep_bar + c.reclaim_bars;
                    let reclaimed = if buy {
                        bar.close <= l.price - back_abs && bar.high >= l.price
                    } else {
                        bar.close >= l.price + back_abs && bar.low <= l.price
                    };
                    if can_reclaim && reclaimed && volume_ok(c.min_volume_ratio) {
                        l.reclaimed = true;
                        let beyond = if buy {
                            matches!(prev_low, Some(pl) if bar.close < pl)
                        } else {
                            matches!(prev_high, Some(ph) if bar.close > ph)
                        };
                        let score = event_score(
                            l.touches,
                            wick_atr,
                            back_atr,
                            if beyond { 10.0 } else { 0.0 },
                        );
                        let slot = &mut reclaim_score[side];
                        *slot = Some(slot.map_or(score, |s: f64| s.max(score)));
                        self.events.push_front((t, if buy { -1 } else { 1 }, score));
                        if self.events.len() > c.max_events {
                            self.events.pop_back();
                        }
                    }
                }
            }
        }
        self.events.retain(|e| t - e.0 <= c.event_memory);

        let a = atr?;
        let (p_bsl, s_bsl, n_bsl) = self.rank(true, bar.close, a, t);
        let (p_ssl, s_ssl, n_ssl) = self.rank(false, bar.close, a, t);
        let near_bsl =
            a > 0.0 && matches!(p_bsl, Some(p) if (p.0 - bar.close) / a <= c.alert_distance_atr);
        let near_ssl =
            a > 0.0 && matches!(p_ssl, Some(p) if (bar.close - p.0) / a <= c.alert_distance_atr);
        let recent = |dir: i8| {
            self.events
                .iter()
                .filter(|e| e.1 == dir)
                .fold(0.0f64, |m, e| m.max(e.2))
        };
        let (recent_bull, recent_bear) = (recent(1), recent(-1));
        let p_bsl_score = p_bsl.map_or(0.0, |p| p.1);
        let p_ssl_score = p_ssl.map_or(0.0, |p| p.1);
        let mut bias = 0i32;
        bias += if reclaim_score[1].is_some() {
            2
        } else {
            i32::from(recent_bull >= c.premium_score)
        };
        bias -= if reclaim_score[0].is_some() {
            2
        } else {
            i32::from(recent_bear >= c.premium_score)
        };
        bias += i32::from(p_ssl_score > p_bsl_score && near_ssl);
        bias -= i32::from(p_bsl_score > p_ssl_score && near_bsl);

        let names = [
            ("bear_bsl_sweep", "BSL swept, close back below"),
            ("bull_bsl_breakout", "BSL broken, close above"),
            ("bear_bsl_stop_hunt", "BSL stop hunt"),
            ("bull_ssl_sweep", "SSL swept, close back above"),
            ("bear_ssl_breakout", "SSL broken, close below"),
            ("bull_ssl_stop_hunt", "SSL stop hunt"),
        ];
        for (hit, (kind, note)) in flags.iter().zip(names) {
            if *hit {
                self.alerts
                    .push(IndicatorAlert::new(kind, format!("LQH: {note}"), 0.6));
            }
        }
        for (side, (plain, premium)) in [
            ("bear_reclaim", "bear_premium_reclaim"),
            ("bull_reclaim", "bull_premium_reclaim"),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(s) = reclaim_score[side] {
                self.alerts.push(IndicatorAlert::new(
                    plain,
                    format!("LQH: reclaim, score {s:.0}"),
                    s / 100.0,
                ));
                if s >= c.premium_score {
                    self.alerts.push(IndicatorAlert::new(
                        premium,
                        format!("LQH: premium reclaim, score {s:.0}"),
                        s / 100.0,
                    ));
                }
            }
        }
        if near_bsl && !self.prev_near.0 {
            self.alerts.push(IndicatorAlert::new(
                "primary_bsl_near",
                "LQH: close within reach of the primary buy-side level",
                0.4,
            ));
        }
        if near_ssl && !self.prev_near.1 {
            self.alerts.push(IndicatorAlert::new(
                "primary_ssl_near",
                "LQH: close within reach of the primary sell-side level",
                0.4,
            ));
        }
        self.prev_near = (near_bsl, near_ssl);

        let mut extra = HashMap::new();
        for (name, lv) in [
            ("primary_bsl", p_bsl),
            ("secondary_bsl", s_bsl),
            ("primary_ssl", p_ssl),
            ("secondary_ssl", s_ssl),
        ] {
            if let Some((price, score, touches)) = lv {
                extra.insert(name.to_string(), price);
                extra.insert(format!("{name}_score"), score);
                if name.starts_with("primary") {
                    extra.insert(format!("{name}_touches"), touches as f64);
                }
            }
        }
        extra.insert("bsl_count".to_string(), n_bsl as f64);
        extra.insert("ssl_count".to_string(), n_ssl as f64);
        extra.insert("recent_bull_score".to_string(), recent_bull);
        extra.insert("recent_bear_score".to_string(), recent_bear);
        extra.insert("volume_ratio".to_string(), vol_ratio);
        if let Some(s) = reclaim_score[0] {
            extra.insert("bear_reclaim_score".to_string(), s);
        }
        if let Some(s) = reclaim_score[1] {
            extra.insert("bull_reclaim_score".to_string(), s);
        }
        Some(IndicatorOutput::with_extra(bias as f64, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
