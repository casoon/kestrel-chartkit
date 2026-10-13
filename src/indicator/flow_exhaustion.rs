//! Flow Exhaustion: MFI flow context plus StochRSI timing, with scored regular divergences.
//!
//! MFI oscillator mode.
//!
//! * MFI of `hlc3` over `mfi_len` bars, smoothed with the open JMA approximation
//!   ([`Jma`], `jma_len`, phase, power). Adaptive zones: over the last `zone_lookback` MFI values
//!   `upper = lo + (hi - lo) * upper_frac`, `lower = lo + (hi - lo) * lower_frac`.
//! * StochRSI K: Wilder RSI of the close (`rsi_len`), stochastic over `stoch_len`, SMA
//!   `k_smooth`, JMA-smoothed like the MFI. The JMA may overshoot 0..100.
//!
//! `value` = smoothed MFI, `secondary` = K (also `extra["stoch_k"]`); `extra["upper"]`/`["lower"]`
//! once the zones exist, `extra["divergence_score"]` (signed: + bull, − bear) on a bar that emits a
//! divergence. `state`: `overbought` / `oversold` / `neutral`.
//!
//! Alerts (closed bars only; the prefix names the expected move):
//!
//! | alert | condition |
//! |---|---|
//! | `bull_exhaustion` | MFI below the lower zone on this and the previous `confirm_bars` bars, or crossing back above it, and K crosses above `stoch_os` — selling exhausted |
//! | `bear_exhaustion` | mirrored at the upper zone and `stoch_ob` |
//! | `bull_divergence` / `bear_divergence` | regular divergence between strict price pivots (`pivot_left`/`pivot_right`) and the MFI at the pivot bar, in the OS/OB area at either end, with minimum MFI delta and ATR swing; emitted once the reaction window (`react_bars` after the pivot) has passed, strength = final score / 100 |
//!
//! The divergence score (0..100) blends MFI delta, swing size against ATR, bars between pivots,
//! the price reaction after the pivot and the sequence of consecutive divergences, with the
//! original's weights. The reaction uses only bars up to the emitting bar — nothing looks ahead.
//!
//! Without real volume (no volume in the MFI window) there is no MFI and the indicator reports
//! nothing: only data that exists is evaluated. Not ported: the CCI oscillator modes, the
//! higher-timeframe MFI filter and the timeframe presets of the divergence parameters (the
//! defaults are the manual values).
//!
//! Conventions the original leaves open (also in the golden fixture): MFI flow counts from the
//! second bar; JMA seeded with its first value instead of a zero state; RSI and ATR Wilder
//! averages seeded with an SMA, first true range `high - low`; pivots strict on both sides; the
//! rolling zone window needs every value. A bar reports once MFI and K exist; until the zones
//! exist the divergence area check falls back to 25/75.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/flow_exhaustion.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Jma, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original (manual divergence values).
#[derive(Debug, Clone)]
pub struct FlowExhaustionConfig {
    pub mfi_len: usize,
    pub zone_lookback: usize,
    pub upper_frac: f64,
    pub lower_frac: f64,
    pub rsi_len: usize,
    pub stoch_len: usize,
    pub k_smooth: usize,
    pub stoch_os: f64,
    pub stoch_ob: f64,
    pub confirm_bars: usize,
    pub jma_len: usize,
    pub jma_phase: f64,
    pub jma_power: f64,
    pub pivot_left: usize,
    pub pivot_right: usize,
    pub min_delta: f64,
    pub buffer: f64,
    pub atr_len: usize,
    pub min_atr_move: f64,
    pub price_eps: f64,
    pub osc_eps: f64,
    pub react_bars: usize,
    pub react_atr: f64,
}

impl Default for FlowExhaustionConfig {
    fn default() -> Self {
        Self {
            mfi_len: 14,
            zone_lookback: 200,
            upper_frac: 0.85,
            lower_frac: 0.15,
            rsi_len: 14,
            stoch_len: 14,
            k_smooth: 3,
            stoch_os: 20.0,
            stoch_ob: 80.0,
            confirm_bars: 1,
            jma_len: 20,
            jma_phase: 0.0,
            jma_power: 2.0,
            pivot_left: 7,
            pivot_right: 7,
            min_delta: 15.0,
            buffer: 0.10,
            atr_len: 14,
            min_atr_move: 0.75,
            price_eps: 0.0010,
            osc_eps: 1.0,
            react_bars: 10,
            react_atr: 0.35,
        }
    }
}

const W_DELTA: f64 = 0.18 / 0.82;
const W_ATR: f64 = 0.16 / 0.82;
const W_TIME: f64 = 0.18 / 0.82;
const W_REACT: f64 = 0.18 / 0.82;
const W_SEQ: f64 = 0.12 / 0.82;

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    src: f64,
    volume: f64,
    mfi: Option<f64>,
    upper: Option<f64>,
    lower: Option<f64>,
    k: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    pivot_bar: usize,
    bull: bool,
    base: f64,
}

/// Streaming Flow Exhaustion; see the module documentation.
pub struct FlowExhaustion {
    cfg: FlowExhaustionConfig,
    mfi_jma: Jma,
    k_jma: Jma,
    rsi_up: Rma,
    rsi_down: Rma,
    rsi_window: VecDeque<f64>,
    k_sma: Sma,
    atr: Rma,
    mfi_window: VecDeque<Option<f64>>,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    last_pivot: [Option<(f64, Option<f64>, usize)>; 2],
    seq: [usize; 2],
    pending: VecDeque<Pending>,
    alerts: Vec<IndicatorAlert>,
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

impl FlowExhaustion {
    pub fn new(cfg: FlowExhaustionConfig) -> Self {
        let lag = cfg.pivot_right.saturating_sub(cfg.react_bars);
        let cap = (cfg.pivot_left + cfg.pivot_right + 1)
            .max(cfg.react_bars + lag + 1)
            .max(cfg.confirm_bars + 1)
            .max(cfg.mfi_len)
            + 2;
        Self {
            mfi_jma: Jma::new(cfg.jma_len, cfg.jma_phase, cfg.jma_power),
            k_jma: Jma::new(cfg.jma_len, cfg.jma_phase, cfg.jma_power),
            rsi_up: Rma::new(cfg.rsi_len),
            rsi_down: Rma::new(cfg.rsi_len),
            rsi_window: VecDeque::with_capacity(cfg.stoch_len + 1),
            k_sma: Sma::new(cfg.k_smooth),
            atr: Rma::new(cfg.atr_len),
            mfi_window: VecDeque::with_capacity(cfg.zone_lookback + 1),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            last_pivot: [None, None],
            seq: [0, 0],
            pending: VecDeque::new(),
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &FlowExhaustionConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    fn raw_mfi(&self) -> Option<f64> {
        let n = self.cfg.mfi_len;
        if self.bars < n {
            return None;
        }
        let (mut pos, mut neg, mut vol) = (0.0, 0.0, 0.0);
        for k in 0..n {
            let r = self.at(k)?;
            vol += r.volume;
            if let Some(p) = self.at(k + 1) {
                if r.src > p.src {
                    pos += r.src * r.volume;
                } else if r.src < p.src {
                    neg += r.src * r.volume;
                }
            }
        }
        if vol <= 0.0 {
            None
        } else if neg == 0.0 {
            Some(100.0)
        } else if pos == 0.0 {
            Some(0.0)
        } else {
            Some(100.0 - 100.0 / (1.0 + pos / neg))
        }
    }

    fn rsi(&mut self, change: Option<f64>) -> Option<f64> {
        let d = change?;
        let up = self.rsi_up.update(d.max(0.0));
        let down = self.rsi_down.update((-d).max(0.0));
        let (u, dn) = (up?, down?);
        Some(if dn == 0.0 {
            100.0
        } else if u == 0.0 {
            0.0
        } else {
            100.0 - 100.0 / (1.0 + u / dn)
        })
    }

    /// Strict pivot `pivot_right` bars back.
    fn pivot(&self, low_side: bool) -> Option<f64> {
        let c = &self.cfg;
        let centre = self.at(c.pivot_right)?;
        self.at(c.pivot_right + c.pivot_left)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..c.pivot_right).chain(c.pivot_right + 1..=c.pivot_right + c.pivot_left) {
            let r = self.at(k)?;
            let o = if low_side { r.low } else { r.high };
            if (low_side && v >= o) || (!low_side && v <= o) {
                return None;
            }
        }
        Some(v)
    }

    fn in_area(&self, m: f64, bull: bool) -> bool {
        let cur = self.at(0).unwrap();
        match (cur.upper, cur.lower) {
            (Some(u), Some(l)) => {
                let band = (u - l).max(1.0);
                if bull {
                    m <= l + band * self.cfg.buffer
                } else {
                    m >= u - band * self.cfg.buffer
                }
            }
            _ => {
                if bull {
                    m <= 25.0
                } else {
                    m >= 75.0
                }
            }
        }
    }

    fn zone_confirmed(&self, below: bool) -> bool {
        (0..=self.cfg.confirm_bars).all(|i| match self.at(i) {
            Some(r) => match (r.mfi, r.upper, r.lower) {
                (Some(m), _, Some(l)) if below => m < l,
                (Some(m), Some(u), _) if !below => m > u,
                _ => false,
            },
            None => false,
        })
    }

    fn line_cross(&self, over: bool, f: impl Fn(&Row) -> Option<(f64, f64)>) -> bool {
        match (self.at(0).and_then(&f), self.at(1).and_then(&f)) {
            (Some((a, b)), Some((pa, pb))) => {
                if over {
                    crossed_over(pa, pb, a, b)
                } else {
                    crossed_under(pa, pb, a, b)
                }
            }
            _ => false,
        }
    }

    /// Divergence check for the pivot confirmed on this bar, then the pivot is remembered.
    fn divergence(&mut self, bull: bool, atr: Option<f64>) {
        let c = self.cfg.clone();
        let Some(p2) = self.pivot(bull) else {
            return;
        };
        let x2 = self.bars - 1 - c.pivot_right;
        let m2 = self.at(c.pivot_right).and_then(|r| r.mfi);
        let side = usize::from(!bull);
        let mut is_div = false;
        if let (Some((p1, Some(m1), x1)), Some(m2)) = (self.last_pivot[side], m2) {
            let (area, d, price_ok, osc_ok) = if bull {
                (
                    self.in_area(m1, true) || self.in_area(m2, true),
                    m2 - m1,
                    p2 <= p1 * (1.0 + c.price_eps),
                    m2 >= m1 - c.osc_eps,
                )
            } else {
                (
                    self.in_area(m1, false) || self.in_area(m2, false),
                    m1 - m2,
                    p2 >= p1 * (1.0 - c.price_eps),
                    m2 <= m1 + c.osc_eps,
                )
            };
            let swing = atr.is_some_and(|a| (p1 - p2).abs() >= a * c.min_atr_move);
            is_div = price_ok && osc_ok && area && d >= c.min_delta && swing;
            if is_div {
                let a = atr.unwrap();
                self.seq[side] += 1;
                self.seq[1 - side] = 0;
                let dt = (x2 - x1).max(1) as f64;
                let base = W_DELTA * clamp01(d / (c.min_delta * 2.0))
                    + W_ATR * clamp01((p1 - p2).abs() / (a * c.min_atr_move).max(1e-10) / 2.0)
                    + W_TIME * clamp01(dt / 40.0)
                    + W_SEQ * clamp01(self.seq[side] as f64 / 3.0);
                self.pending.push_back(Pending {
                    pivot_bar: x2,
                    bull,
                    base,
                });
            }
        }
        if !is_div {
            self.seq[side] = 0;
        }
        self.last_pivot[side] = Some((p2, m2, x2));
    }
}

impl Indicator for FlowExhaustion {
    fn name(&self) -> &str {
        "flow_exhaustion"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        (c.mfi_len - 1).max(c.rsi_len + c.stoch_len - 1 + c.k_smooth - 1)
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let prev_close = self.at(0).map(|r| r.close);
        let tr = match prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            src: bar.typical_price(),
            volume: bar.volume,
            mfi: None,
            upper: None,
            lower: None,
            k: None,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;

        let mfi = self.raw_mfi().map(|m| self.mfi_jma.update(m));
        self.mfi_window.push_back(mfi);
        if self.mfi_window.len() > c.zone_lookback {
            self.mfi_window.pop_front();
        }
        let zones = if self.mfi_window.len() == c.zone_lookback {
            let vals: Option<Vec<f64>> = self.mfi_window.iter().copied().collect();
            vals.map(|v| {
                let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
                (lo + (hi - lo) * c.upper_frac, lo + (hi - lo) * c.lower_frac)
            })
        } else {
            None
        };

        let rsi = self.rsi(prev_close.map(|p| bar.close - p));
        let stoch = rsi.and_then(|r| {
            self.rsi_window.push_back(r);
            if self.rsi_window.len() > c.stoch_len {
                self.rsi_window.pop_front();
            }
            (self.rsi_window.len() == c.stoch_len).then(|| {
                let lo = self
                    .rsi_window
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min);
                let hi = self
                    .rsi_window
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max);
                100.0 * (r - lo) / (hi - lo).max(1e-10)
            })
        });
        let k = stoch
            .and_then(|s| self.k_sma.update(s))
            .map(|v| self.k_jma.update(v));
        {
            let r = self.rows.back_mut().unwrap();
            r.mfi = mfi;
            r.upper = zones.map(|z| z.0);
            r.lower = zones.map(|z| z.1);
            r.k = k;
        }

        self.divergence(true, atr);
        self.divergence(false, atr);

        let reported = mfi.is_some() && k.is_some();
        let lag = c.pivot_right.saturating_sub(c.react_bars);
        let mut div_score = None;
        while let Some(p) = self.pending.front().copied() {
            if t - p.pivot_bar < c.react_bars + lag {
                break;
            }
            self.pending.pop_front();
            let piv_close = self.at(c.react_bars + lag).unwrap().close;
            let window = (lag..lag + c.react_bars).map(|k| self.at(k).unwrap());
            let mv = if p.bull {
                window.map(|r| r.high).fold(f64::NEG_INFINITY, f64::max) - piv_close
            } else {
                piv_close - window.map(|r| r.low).fold(f64::INFINITY, f64::min)
            };
            let a = atr.unwrap_or(0.0);
            let react = clamp01(mv / (a * c.react_atr).max(1e-10));
            let score = (clamp01(p.base + W_REACT * react) * 100.0).round();
            if reported {
                let kind = if p.bull {
                    "bull_divergence"
                } else {
                    "bear_divergence"
                };
                self.alerts.push(IndicatorAlert::new(
                    kind,
                    format!("Flow Exhaustion {kind}: score {score:.0}"),
                    (score / 100.0).clamp(0.05, 1.0),
                ));
                div_score = Some(if p.bull { score } else { -score });
            }
        }

        if !reported {
            return None;
        }
        let (mfi, k) = (mfi.unwrap(), k.unwrap());
        let long_ctx =
            self.zone_confirmed(true) || self.line_cross(true, |r| Some((r.mfi?, r.lower?)));
        let short_ctx =
            self.zone_confirmed(false) || self.line_cross(false, |r| Some((r.mfi?, r.upper?)));
        let (os, ob) = (c.stoch_os, c.stoch_ob);
        if long_ctx && self.line_cross(true, |r| Some((r.k?, os))) {
            self.alerts.push(IndicatorAlert::new(
                "bull_exhaustion",
                format!(
                    "Flow Exhaustion bull_exhaustion: MFI {mfi:.1} oversold, K {k:.1} crosses {os}"
                ),
                0.6,
            ));
        }
        if short_ctx && self.line_cross(false, |r| Some((r.k?, ob))) {
            self.alerts.push(IndicatorAlert::new(
                "bear_exhaustion",
                format!("Flow Exhaustion bear_exhaustion: MFI {mfi:.1} overbought, K {k:.1} crosses {ob}"),
                0.6,
            ));
        }

        let mut extra = HashMap::new();
        extra.insert("stoch_k".to_string(), k);
        let mut state = "neutral";
        if let Some((u, l)) = zones {
            extra.insert("upper".to_string(), u);
            extra.insert("lower".to_string(), l);
            if mfi > u {
                state = "overbought";
            } else if mfi < l {
                state = "oversold";
            }
        }
        if let Some(s) = div_score {
            extra.insert("divergence_score".to_string(), s);
        }
        Some(
            IndicatorOutput::with_extra(mfi, extra)
                .with_secondary(k)
                .with_state(state),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_volume_nothing_is_reported() {
        let mut ind = FlowExhaustion::new(FlowExhaustionConfig::default());
        for i in 0..400 {
            let x = i as f64;
            let c = 100.0 + (x / 7.0).sin() * 3.0;
            let bar = Bar::new(i, c, c + 1.0, c - 1.0, c, 0.0);
            assert!(ind.on_bar(&bar).is_none());
        }
    }
}
