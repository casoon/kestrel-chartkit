//! Sweep Reversal Score: liquidity-sweep reversals with an ATR gate and a two-point quality
//! score.
//!
//! Configured as the chart timeframe alone can carry: the 15-minute lockout off and the
//! higher-timeframe gates F (EMA 50/200 via `request.security`), G and H (WaveTrend on 1H/4H) off,
//! as are the optional gates B (close position), C (body size), E (spike filter) and I (session) —
//! all of them off by default in the original except F. A long signal needs, on a closed bar:
//!
//! - **sweep**: low below the lowest low of the previous `swing_len` bars, close back above it;
//! - **ATR gate**: `ATR(atr_len) > SMA(ATR, atr_base_len) · atr_low_factor`;
//! - **quality** `≥ min_quality_long`: +1 for `RSI > 50`, +1 when the last two confirmed swing
//!   lows are not a lower low.
//!
//! A short mirrors the long exactly: the sweep (high above the highest high, close back below),
//! +1 for `RSI < 50` and +1 for no higher high, against `min_quality_short` (default 2 like the
//! long). The original scores the short with `RSI > 50` as well and needs only 1 point — one-sided,
//! so the short fired about three times as often as the long on the same, mirrored data; this
//! port treats both sides alike. After a signal, the next one needs more than `cooldown` bars.
//!
//! `value`: `1` on a long signal, `-1` on a short, `0` otherwise (a bar with both reports `1`
//! and fires both alerts). `extra`: `long_quality`, `short_quality`, `rsi`, `atr`, `atr_base`,
//! `swing_low`, `swing_high` (the swept references), and on a signal bar `entry` (the close),
//! `stop` (beyond the sweep extreme or the reference by `sl_atr_buffer` ATR) and `take`
//! (`tp_r_mult` times that risk) — `long_*`/`short_*` when both fire.
//!
//! Alerts (prefix = expected move): `bull_sweep_reversal`, `bear_sweep_reversal`.
//!
//! Conventions the original leaves open (also in the golden fixture): ATR = Wilder average of
//! the true range (first bar high - low), SMA-seeded; RSI = Wilder averages of the close changes,
//! SMA-seeded, 50 when nothing moved; pivots strict on both sides. Output starts once the ATR
//! base, the RSI and the sweep window exist.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/sweep_reversal_score.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original — except `min_quality_short`, which follows the long.
#[derive(Debug, Clone)]
pub struct SweepReversalScoreConfig {
    pub swing_len: usize,
    pub atr_len: usize,
    pub atr_base_len: usize,
    pub atr_low_factor: f64,
    pub rsi_len: usize,
    pub pivot_len: usize,
    pub min_quality_long: u8,
    pub min_quality_short: u8,
    pub cooldown: usize,
    pub sl_atr_buffer: f64,
    pub tp_r_mult: f64,
}

impl Default for SweepReversalScoreConfig {
    fn default() -> Self {
        Self {
            swing_len: 10,
            atr_len: 14,
            atr_base_len: 50,
            atr_low_factor: 0.60,
            rsi_len: 14,
            pivot_len: 5,
            min_quality_long: 2,
            min_quality_short: 2,
            cooldown: 5,
            sl_atr_buffer: 0.30,
            tp_r_mult: 1.50,
        }
    }
}

/// Streaming Sweep Reversal Score; see the module documentation.
pub struct SweepReversalScore {
    cfg: SweepReversalScoreConfig,
    atr: Rma,
    atr_base: Sma,
    gain: Rma,
    loss: Rma,
    rows: VecDeque<Bar>,
    cap: usize,
    bars: usize,
    highs: [Option<f64>; 2],
    lows: [Option<f64>; 2],
    last_signal: Option<usize>,
    alerts: Vec<IndicatorAlert>,
}

impl SweepReversalScore {
    pub fn new(cfg: SweepReversalScoreConfig) -> Self {
        let cap = (cfg.swing_len + 1).max(2 * cfg.pivot_len + 1) + 1;
        Self {
            atr: Rma::new(cfg.atr_len),
            atr_base: Sma::new(cfg.atr_base_len),
            gain: Rma::new(cfg.rsi_len),
            loss: Rma::new(cfg.rsi_len),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            cfg,
            bars: 0,
            highs: [None; 2],
            lows: [None; 2],
            last_signal: None,
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &SweepReversalScoreConfig {
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
}

impl Indicator for SweepReversalScore {
    fn name(&self) -> &str {
        "sweep_reversal_score"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        (c.atr_len + c.atr_base_len - 2)
            .max(c.rsi_len)
            .max(c.swing_len)
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
        let atr_base = atr.and_then(|a| self.atr_base.update(a));
        let rsi = prev.as_ref().and_then(|p| {
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
        self.rows.push_back(bar.clone());
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        if let Some(v) = self.pivot(false) {
            self.highs = [Some(v), self.highs[0]];
        }
        if let Some(v) = self.pivot(true) {
            self.lows = [Some(v), self.lows[0]];
        }

        let (Some(atr), Some(atr_base), Some(rsi)) = (atr, atr_base, rsi) else {
            return None;
        };
        self.at(c.swing_len)?;
        let window = (1..=c.swing_len).map(|k| self.at(k).expect("inside history"));
        let (swing_low, swing_high) = window
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), b| {
                (l.min(b.low), h.max(b.high))
            });

        let atr_ok = atr > atr_base * c.atr_low_factor;
        let ll = matches!(self.lows, [Some(a), Some(b)] if a < b);
        let hh = matches!(self.highs, [Some(a), Some(b)] if a > b);
        let long_quality = u8::from(rsi > 50.0) + u8::from(!ll);
        let short_quality = u8::from(rsi < 50.0) + u8::from(!hh);
        let sweep_long = bar.low < swing_low && bar.close > swing_low;
        let sweep_short = bar.high > swing_high && bar.close < swing_high;
        let cooldown_ok = self.last_signal.is_none_or(|s| t - s > c.cooldown);
        let long = sweep_long && atr_ok && long_quality >= c.min_quality_long && cooldown_ok;
        let short = sweep_short && atr_ok && short_quality >= c.min_quality_short && cooldown_ok;
        if long || short {
            self.last_signal = Some(t);
        }

        let mut extra = HashMap::new();
        extra.insert("long_quality".to_string(), long_quality as f64);
        extra.insert("short_quality".to_string(), short_quality as f64);
        extra.insert("rsi".to_string(), rsi);
        extra.insert("atr".to_string(), atr);
        extra.insert("atr_base".to_string(), atr_base);
        extra.insert("swing_low".to_string(), swing_low);
        extra.insert("swing_high".to_string(), swing_high);
        let both = long && short;
        if long {
            let stop = bar.low.min(swing_low) - atr * c.sl_atr_buffer;
            let take = bar.close + (bar.close - stop) * c.tp_r_mult;
            let pre = if both { "long_" } else { "" };
            extra.insert(format!("{pre}entry"), bar.close);
            extra.insert(format!("{pre}stop"), stop);
            extra.insert(format!("{pre}take"), take);
            self.alerts.push(IndicatorAlert::new(
                "bull_sweep_reversal",
                format!("RES: sweep of {swing_low:.5} reclaimed, quality {long_quality}/2"),
                long_quality as f64 / 2.0,
            ));
        }
        if short {
            let stop = bar.high.max(swing_high) + atr * c.sl_atr_buffer;
            let take = bar.close - (stop - bar.close) * c.tp_r_mult;
            let pre = if both { "short_" } else { "" };
            extra.insert(format!("{pre}entry"), bar.close);
            extra.insert(format!("{pre}stop"), stop);
            extra.insert(format!("{pre}take"), take);
            self.alerts.push(IndicatorAlert::new(
                "bear_sweep_reversal",
                format!("RES: sweep of {swing_high:.5} rejected, quality {short_quality}/2"),
                short_quality as f64 / 2.0,
            ));
        }
        let value = if long {
            1.0
        } else if short {
            -1.0
        } else {
            0.0
        };
        Some(IndicatorOutput::with_extra(value, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
