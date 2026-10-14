//! Accumulation Phase: bottom formation (accumulation) and top formation (distribution) as
//! process states, scored from five components of `0..=2` each.
//!
//! Per side:
//!
//! | component | accumulation (distribution mirrored) |
//! |---|---|
//! | trend loss | EMA 20 below EMA 50 `lookback` bars ago, and now: EMAs within ATR/2 (+1), no bear BOS and no lower low (+0.5), EMA 20 slope (3 bars) within ±0.01 % of the close (+0.5) |
//! | range | efficiency ratio over `lookback` below `er_range` (+1), below half of it (+0.5), `lookback`-bar range 2..8 ATR wide (+0.5) — counted only while trend loss > 0 |
//! | volatility compression | Bollinger width (20, 2σ, relative to the basis) below `bbw_compress` × its `lookback` average (+1), ATR below `atr_compress` × average (+0.5), bar range below 0.7 × average (+0.5) — only while trend loss > 0 |
//! | liquidity | sweeps of the last swing low (+0.5 at one, +1 at two) and springs — sweep closing back above (+0.5 / +1); both counts rise on each new event and fall by one every `round(lookback/2)` bars |
//! | structure | higher low (+1), bull BOS (+1), RSI 14 above 40 after below 35 `lookback` bars ago (+0.5) |
//!
//! Swings are strict pivots (`pivot_left`/`pivot_right`). A side is active at a score of 3; the
//! phase counts bars since it became active. State: `none` (inactive), `transition` (fewer than
//! `min_phase_bars`), then `breakout_ready` (score ≥ 8 and structure ≥ 1.5), `mature` (≥ 6 with
//! liquidity ≥ 1 and range ≥ 1), `developing` (≥ 5) or `early`; distribution uses
//! `breakdown_ready`.
//!
//! `value`: the accumulation score `0..=10`; `secondary`: the distribution score. `extra`: the
//! five components of each side (`accum_trend_loss`, `accum_range`, `accum_compression`,
//! `accum_liquidity`, `accum_structure`, `dist_…`), `accum_bars`, `dist_bars`, `accum_state` and
//! `dist_state` (0 none, 1 transition, 2 early, 3 developing, 4 mature, 5 breakout/breakdown
//! ready), `efficiency`, `bbw_ratio`, `atr_ratio`, `range_atr`. `state`: `accumulation_<state>`
//! when accumulation is active, else `distribution_<state>`, else `none`.
//!
//! Alerts when a state is entered (prefix = expected move): `bull_accumulation_developing`,
//! `bull_accumulation_mature`, `bull_breakout_ready`, `bear_distribution_developing`,
//! `bear_distribution_mature`, `bear_breakdown_ready`.
//!
//! Conventions the original leaves open (also in the golden fixture): EMAs seeded with the first
//! close; ATR = Wilder average of the true range (first bar high - low), SMA-seeded; RSI = Wilder
//! averages of the close changes, SMA-seeded; population standard deviation; pivots strict on both
//! sides. The original evaluates the scores with `na` fallbacks during the warm-up; here the
//! sweep/spring counters run from the first bar (their decay keyed to the bar count, like
//! `bar_index`), while scores, phases and output start once every input exists (bar 39 with the
//! defaults).
//!
//! Golden reference: `reference/kestrel_reference/fixtures/accumulation_phase.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original.
#[derive(Debug, Clone)]
pub struct AccumulationPhaseConfig {
    pub atr_len: usize,
    pub lookback: usize,
    pub min_phase_bars: usize,
    pub pivot_left: usize,
    pub pivot_right: usize,
    pub er_range: f64,
    pub bbw_compress: f64,
    pub atr_compress: f64,
}

impl Default for AccumulationPhaseConfig {
    fn default() -> Self {
        Self {
            atr_len: 14,
            lookback: 20,
            min_phase_bars: 10,
            pivot_left: 3,
            pivot_right: 3,
            er_range: 0.3,
            bbw_compress: 0.7,
            atr_compress: 0.8,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    ema20: f64,
    ema50: f64,
    rsi: Option<f64>,
}

/// One side's running phase: (start bar, previous state code).
#[derive(Debug, Clone, Copy, Default)]
struct Phase {
    start: Option<usize>,
    prev_state: u8,
}

const STATE_NAMES: [&str; 6] = [
    "none",
    "transition",
    "early",
    "developing",
    "mature",
    "ready",
];

/// Streaming Accumulation Phase; see the module documentation.
pub struct AccumulationPhase {
    cfg: AccumulationPhaseConfig,
    atr: Rma,
    atr_avg: Sma,
    ema20: Ema,
    ema50: Ema,
    basis: Sma,
    bbw_avg: Sma,
    range_avg: Sma,
    gain: Rma,
    loss: Rma,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    swing_high: [Option<f64>; 2],
    swing_low: [Option<f64>; 2],
    prev_sweep: [bool; 4],
    counts: [usize; 4],
    accum: Phase,
    dist: Phase,
    alerts: Vec<IndicatorAlert>,
}

impl AccumulationPhase {
    pub fn new(cfg: AccumulationPhaseConfig) -> Self {
        let cap = (cfg.lookback + 2)
            .max(cfg.pivot_left + cfg.pivot_right + 2)
            .max(22);
        Self {
            atr: Rma::new(cfg.atr_len),
            atr_avg: Sma::new(cfg.lookback),
            ema20: Ema::new(20),
            ema50: Ema::new(50),
            basis: Sma::new(20),
            bbw_avg: Sma::new(cfg.lookback),
            range_avg: Sma::new(cfg.lookback),
            gain: Rma::new(14),
            loss: Rma::new(14),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            cfg,
            bars: 0,
            swing_high: [None; 2],
            swing_low: [None; 2],
            prev_sweep: [false; 4],
            counts: [0; 4],
            accum: Phase::default(),
            dist: Phase::default(),
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &AccumulationPhaseConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        self.rows.len().checked_sub(k + 1).map(|i| &self.rows[i])
    }

    fn pivot(&self, low_side: bool) -> Option<f64> {
        let (l, r) = (self.cfg.pivot_left, self.cfg.pivot_right);
        let centre = self.at(r)?;
        self.at(r + l)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..r).chain(r + 1..=r + l) {
            let b = self.at(k)?;
            let ok = if low_side { v < b.low } else { v > b.high };
            if !ok {
                return None;
            }
        }
        Some(v)
    }

    /// Advances one side's phase and returns (state code, bars in phase).
    fn phase(
        &self,
        ph: &mut Phase,
        t: usize,
        score: f64,
        structure: f64,
        liq: f64,
        range: f64,
    ) -> (u8, usize) {
        let active = score >= 3.0;
        if active && ph.start.is_none() {
            ph.start = Some(t);
        }
        if !active {
            ph.start = None;
        }
        let duration = ph.start.map_or(0, |s| t - s);
        let state = if !active {
            0
        } else if duration < self.cfg.min_phase_bars {
            1
        } else if score >= 8.0 && structure >= 1.5 {
            5
        } else if score >= 6.0 && liq >= 1.0 && range >= 1.0 {
            4
        } else if score >= 5.0 {
            3
        } else {
            2
        };
        (state, duration)
    }
}

impl Indicator for AccumulationPhase {
    fn name(&self) -> &str {
        "accumulation_phase"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        // ATR average, Bollinger-width average, and the RSI (first at bar 15) `lookback` bars back.
        (c.atr_len + c.lookback - 1)
            .max(20 + c.lookback - 1)
            .max(14 + 1 + c.lookback)
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let t = self.bars;
        self.bars += 1;
        let prev = self.rows.back().copied();
        let tr = match prev {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let atr_avg = atr.and_then(|a| self.atr_avg.update(a));
        let rsi = prev.and_then(|p| {
            let ch = bar.close - p.close;
            let g = self.gain.update(ch.max(0.0));
            let l = self.loss.update((-ch).max(0.0));
            match (g, l) {
                (Some(g), Some(l)) => Some(if g == 0.0 && l == 0.0 {
                    50.0
                } else if l == 0.0 {
                    100.0
                } else {
                    100.0 - 100.0 / (1.0 + g / l)
                }),
                _ => None,
            }
        });
        let ema20 = self.ema20.update(bar.close).expect("first-sample seed");
        let ema50 = self.ema50.update(bar.close).expect("first-sample seed");
        let range_avg = self.range_avg.update(bar.high - bar.low);
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            ema20,
            ema50,
            rsi,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        let basis = self.basis.update(bar.close);
        let bb_width = basis.map(|b| {
            let n = 20.0;
            let var = (0..20)
                .map(|k| (self.at(k).unwrap().close - b).powi(2))
                .sum::<f64>()
                / n;
            if b > 0.0 {
                4.0 * var.sqrt() / b
            } else {
                0.0
            }
        });
        let bbw_avg = bb_width.and_then(|w| self.bbw_avg.update(w));

        // Swings and liquidity events.
        if let Some(v) = self.pivot(false) {
            self.swing_high = [Some(v), self.swing_high[0]];
        }
        if let Some(v) = self.pivot(true) {
            self.swing_low = [Some(v), self.swing_low[0]];
        }
        let prev_close = prev.map(|p| p.close);
        let [last_high, prev_high] = self.swing_high;
        let [last_low, prev_low] = self.swing_low;
        let sweep_low = matches!(last_low, Some(l) if bar.low < l);
        let spring = matches!(last_low, Some(l) if bar.low < l && bar.close > l);
        let sweep_high = matches!(last_high, Some(h) if bar.high > h);
        let upthrust = matches!(last_high, Some(h) if bar.high > h && bar.close < h);
        let events = [sweep_low, spring, sweep_high, upthrust];
        let decay_every = (c.lookback as f64 / 2.0).round() as usize;
        for ((count, now), before) in self.counts.iter_mut().zip(events).zip(self.prev_sweep) {
            if now && !before {
                *count += 1;
            }
            if t.is_multiple_of(decay_every) {
                *count = count.saturating_sub(1);
            }
        }
        self.prev_sweep = events;

        let lb = c.lookback;
        let (Some(atr), Some(atr_avg), Some(bb_width), Some(bbw_avg), Some(range_avg), Some(rsi)) =
            (atr, atr_avg, bb_width, bbw_avg, range_avg, rsi)
        else {
            return None;
        };
        let back = self.at(lb)?;
        let rsi_back = back.rsi?;
        let (ema20_back, ema50_back) = (back.ema20, back.ema50);
        let ema20_3 = self.at(3)?.ema20;

        let atr_ratio = if atr_avg > 0.0 { atr / atr_avg } else { 1.0 };
        let bbw_ratio = if bbw_avg > 0.0 {
            bb_width / bbw_avg
        } else {
            1.0
        };
        let net = (bar.close - back.close).abs();
        let path: f64 = (0..lb)
            .map(|k| (self.at(k).unwrap().close - self.at(k + 1).unwrap().close).abs())
            .sum();
        let efficiency = if path > 0.0 { net / path } else { 1.0 };
        let candle_ratio = if range_avg > 0.0 {
            (bar.high - bar.low) / range_avg
        } else {
            1.0
        };
        let (hh, ll) = (0..lb).fold((f64::NEG_INFINITY, f64::INFINITY), |(h, l), k| {
            let r = self.at(k).unwrap();
            (h.max(r.high), l.min(r.low))
        });
        let range_atr = if atr > 0.0 { (hh - ll) / atr } else { 0.0 };

        let bull_bos =
            matches!((last_high, prev_close), (Some(h), Some(pc)) if bar.close > h && pc <= h);
        let bear_bos =
            matches!((last_low, prev_close), (Some(l), Some(pc)) if bar.close < l && pc >= l);
        let higher_low = matches!((last_low, prev_low), (Some(a), Some(b)) if a > b);
        let lower_low = matches!((last_low, prev_low), (Some(a), Some(b)) if a < b);
        let lower_high = matches!((last_high, prev_high), (Some(a), Some(b)) if a < b);
        let higher_high = matches!((last_high, prev_high), (Some(a), Some(b)) if a > b);
        let slope = (ema20 - ema20_3) / 3.0;
        let flattening = slope > -0.0001 * bar.close && slope < 0.0001 * bar.close;
        let now_flat = (ema20 - ema50).abs() < atr * 0.5;

        let mut range_score: f64 = 0.0;
        if efficiency < c.er_range {
            range_score += 1.0;
        }
        if efficiency < c.er_range * 0.5 {
            range_score += 0.5;
        }
        if range_atr > 2.0 && range_atr < 8.0 {
            range_score += 0.5;
        }
        let range_score = range_score.min(2.0);
        let mut comp: f64 = 0.0;
        if bbw_ratio < c.bbw_compress {
            comp += 1.0;
        }
        if atr_ratio < c.atr_compress {
            comp += 0.5;
        }
        if candle_ratio < 0.7 {
            comp += 0.5;
        }
        let comp = comp.min(2.0);
        let liq = |sweeps: usize, springs: usize| {
            let mut s: f64 = 0.0;
            for (n, k) in [(sweeps, 1), (sweeps, 2), (springs, 1), (springs, 2)] {
                if n >= k {
                    s += 0.5;
                }
            }
            s.min(2.0)
        };
        let trend_loss = |was: bool, against_bos: bool, against_swing: bool| {
            let mut s: f64 = 0.0;
            if was && now_flat {
                s += 1.0;
            }
            if was && !against_bos && !against_swing {
                s += 0.5;
            }
            if flattening && was {
                s += 0.5;
            }
            s.min(2.0)
        };

        let a_trend = trend_loss(ema20_back < ema50_back, bear_bos, lower_low);
        let a_liq = liq(self.counts[0], self.counts[1]);
        let mut a_struct: f64 = 0.0;
        if higher_low {
            a_struct += 1.0;
        }
        if bull_bos {
            a_struct += 1.0;
        }
        if rsi > 40.0 && rsi_back < 35.0 {
            a_struct += 0.5;
        }
        let a_struct = a_struct.min(2.0);
        let (a_range, a_comp) = if a_trend > 0.0 {
            (range_score, comp)
        } else {
            (0.0, 0.0)
        };
        let accum = a_trend + a_range + a_comp + a_liq + a_struct;

        let d_trend = trend_loss(ema20_back > ema50_back, bull_bos, higher_high);
        let d_liq = liq(self.counts[2], self.counts[3]);
        let mut d_struct: f64 = 0.0;
        if lower_high {
            d_struct += 1.0;
        }
        if bear_bos {
            d_struct += 1.0;
        }
        if rsi < 60.0 && rsi_back > 65.0 {
            d_struct += 0.5;
        }
        let d_struct = d_struct.min(2.0);
        let (d_range, d_comp) = if d_trend > 0.0 {
            (range_score, comp)
        } else {
            (0.0, 0.0)
        };
        let dist = d_trend + d_range + d_comp + d_liq + d_struct;

        let mut ap = self.accum;
        let (a_state, a_bars) = self.phase(&mut ap, t, accum, a_struct, a_liq, a_range);
        let mut dp = self.dist;
        let (d_state, d_bars) = self.phase(&mut dp, t, dist, d_struct, d_liq, d_range);
        for (state, prev, kinds) in [
            (
                a_state,
                ap.prev_state,
                [
                    "bull_accumulation_developing",
                    "bull_accumulation_mature",
                    "bull_breakout_ready",
                ],
            ),
            (
                d_state,
                dp.prev_state,
                [
                    "bear_distribution_developing",
                    "bear_distribution_mature",
                    "bear_breakdown_ready",
                ],
            ),
        ] {
            if (3..=5).contains(&state) && state != prev {
                let kind = kinds[(state - 3) as usize];
                self.alerts.push(IndicatorAlert::new(
                    kind,
                    format!("Accumulation Phase: {kind}"),
                    0.4 + 0.2 * (state - 3) as f64,
                ));
            }
        }
        ap.prev_state = a_state;
        dp.prev_state = d_state;
        self.accum = ap;
        self.dist = dp;

        let mut extra = HashMap::new();
        for (k, v) in [
            ("accum_trend_loss", a_trend),
            ("accum_range", a_range),
            ("accum_compression", a_comp),
            ("accum_liquidity", a_liq),
            ("accum_structure", a_struct),
            ("dist_trend_loss", d_trend),
            ("dist_range", d_range),
            ("dist_compression", d_comp),
            ("dist_liquidity", d_liq),
            ("dist_structure", d_struct),
            ("accum_bars", a_bars as f64),
            ("dist_bars", d_bars as f64),
            ("accum_state", a_state as f64),
            ("dist_state", d_state as f64),
            ("efficiency", efficiency),
            ("bbw_ratio", bbw_ratio),
            ("atr_ratio", atr_ratio),
            ("range_atr", range_atr),
        ] {
            extra.insert(k.to_string(), v);
        }
        let state = if a_state > 0 {
            let s = STATE_NAMES[a_state as usize];
            if a_state == 5 {
                "accumulation_breakout_ready".to_string()
            } else {
                format!("accumulation_{s}")
            }
        } else if d_state > 0 {
            let s = STATE_NAMES[d_state as usize];
            if d_state == 5 {
                "distribution_breakdown_ready".to_string()
            } else {
                format!("distribution_{s}")
            }
        } else {
            "none".to_string()
        };
        Some(
            IndicatorOutput::with_extra(accum, extra)
                .with_secondary(dist)
                .with_state(state),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const ACCUMULATION_PHASE_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "accumulation_phase",
        aliases: &[],
        description: "Accumulation Phase: accumulation (bottom) and distribution (top) as process states from five components 0..2 each (trend loss, range formation, volatility compression, liquidity sweeps/springs, early structure); value = accumulation score 0..10, secondary = distribution score; extra: components per side, accum/dist_bars, accum/dist_state (0 none .. 5 breakout/breakdown ready), efficiency, bbw_ratio, atr_ratio, range_atr; alerts bull_accumulation_developing/_mature, bull_breakout_ready, bear_distribution_developing/_mature, bear_breakdown_ready",
        params: &[
            super::definition::ParamSpec::count("atr_len", 14, 1, 500).meaning("bars in the Wilder ATR"),
            super::definition::ParamSpec::count("lookback", 20, 10, 500).meaning("bars of the trend-loss, range and compression comparisons"),
            super::definition::ParamSpec::count("min_phase_bars", 10, 3, 500).meaning("bars an active phase stays in transition"),
            super::definition::ParamSpec::count("pivot_left", 3, 1, 50).meaning("bars left of a swing pivot"),
            super::definition::ParamSpec::count("pivot_right", 3, 1, 50).meaning("bars right of a swing pivot"),
            super::definition::ParamSpec::number("er_range", 0.3, 0.05, 1.0).meaning("efficiency ratio below which price ranges"),
            super::definition::ParamSpec::number("bbw_compress", 0.7, 0.3, 5.0).meaning("Bollinger width against its average below which volatility compresses"),
            super::definition::ParamSpec::number("atr_compress", 0.8, 0.3, 5.0).meaning("ATR against its average below which volatility compresses"),
        ],
        checks: &[],
        build: |p| {
            let cfg = AccumulationPhaseConfig {
                atr_len: p.count("atr_len"),
                lookback: p.count("lookback"),
                min_phase_bars: p.count("min_phase_bars"),
                pivot_left: p.count("pivot_left"),
                pivot_right: p.count("pivot_right"),
                er_range: p.number("er_range"),
                bbw_compress: p.number("bbw_compress"),
                atr_compress: p.number("atr_compress"),
            };
            Ok(Box::new(AccumulationPhase::new(cfg)))
        },
    };
