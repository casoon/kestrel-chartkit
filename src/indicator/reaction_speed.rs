//! Reaction Speed: how quickly and cleanly price reacts after a structure event
//! (break of structure, liquidity sweep, order-block tap), scored against the volatility regime.
//!
//! One event is tracked at a time (priority BOS > sweep > OB tap):
//!
//! * BOS: close beyond the `swing_len` high/low of the previous bars (the bar before was not).
//! * Sweep: wick beyond that high/low, close back inside; the direction is the expected reversal.
//! * OB tap: the bar overlaps the last order block — the most recent opposite candle within
//!   `ob_search` bars before a displacement candle (body > `disp_body`·ATR closing beyond the
//!   previous bar).
//!
//! From the next bar on the first reaction candle in the event direction is awaited (bull: close
//! above open or a higher low). `speed = 100 · (1 − bars_to_react / (react_bars − 1))`. From the
//! reaction on, for `valid_bars` bars: A = a close beyond the reaction candle's extreme, B =
//! progress of the current bar's extreme from the event close toward `atr_move`·ATR (event-bar
//! ATR). `validation = 100 · (w_a·A + w_b·B)`, `reaction = w_speed·speed + w_val·validation`
//! (weights normalised). Without a reaction within `react_bars` all scores are 0 and the event
//! expires. Every score at a bar uses only bars up to it.
//!
//! `value` = activity score `50 + 50·tanh((ATR / SMA(ATR, atr_base) − 1)·activity_k)`;
//! `extra` while an event is active and on the bar it closes: `reaction`, `speed`, `validation`,
//! `event_dir` (±1), `event_type` (1 BOS, 2 sweep, 3 OB); on the closing bar also
//! `final_score`. `state`: `tracking_bos` / `tracking_sweep` / `tracking_ob` or `idle`.
//!
//! No alerts: the original has none, and a finished score describes a reaction that has already
//! happened rather than an expected move. Break-of-structure, sweep and order-block events with
//! alerts of their own are in [`super::bos_choch`], [`super::liquidity_sweeps`] and
//! [`super::order_block`].
//!
//! The original's windows are minutes converted per timeframe; the streaming trait has no
//! timeframe, so the defaults are the 1h preset tier in bars (reaction 8, validation 20, ATR move
//! 1.6, weights 0.7/0.3 and 0.40/0.60).
//!
//! Conventions the original leaves open (also in the golden fixture): ATR Wilder average seeded
//! with an SMA, first true range `high − low`; an event needs an ATR; a bar reports once the ATR
//! base exists; after a no-reaction expiry the zero score is reported on the closing bar only (the
//! original keeps plotting 0 until the next event).
//!
//! Golden reference: `reference/kestrel_reference/fixtures/reaction_speed.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults = the 1h preset tier in bars.
#[derive(Debug, Clone)]
pub struct ReactionSpeedConfig {
    pub atr_len: usize,
    pub atr_base: usize,
    pub activity_k: f64,
    pub swing_len: usize,
    pub react_bars: usize,
    pub valid_bars: usize,
    pub atr_move: f64,
    pub w_a: f64,
    pub w_b: f64,
    pub w_speed: f64,
    pub w_val: f64,
    pub disp_body: f64,
    pub ob_search: usize,
}

impl Default for ReactionSpeedConfig {
    fn default() -> Self {
        Self {
            atr_len: 14,
            atr_base: 50,
            activity_k: 1.2,
            swing_len: 10,
            react_bars: 8,
            valid_bars: 20,
            atr_move: 1.6,
            w_a: 0.7,
            w_b: 0.3,
            w_speed: 0.40,
            w_val: 0.60,
            disp_body: 1.0,
            ob_search: 12,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Debug, Clone, Copy)]
struct Event {
    kind: u8,
    dir: i8,
    bar: usize,
    reference: f64,
    atr: f64,
    reaction: Option<(usize, f64, f64)>,
    val_a: bool,
    speed: Option<f64>,
    validation: Option<f64>,
    score: Option<f64>,
}

/// Streaming Reaction Speed; see the module documentation.
pub struct ReactionSpeed {
    cfg: ReactionSpeedConfig,
    atr: Rma,
    atr_base: Sma,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    bull_ob: Option<(f64, f64)>,
    bear_ob: Option<(f64, f64)>,
    event: Option<Event>,
    active: bool,
}

impl ReactionSpeed {
    pub fn new(cfg: ReactionSpeedConfig) -> Self {
        let cap = (cfg.swing_len + 2).max(cfg.ob_search + 1) + 2;
        Self {
            atr: Rma::new(cfg.atr_len),
            atr_base: Sma::new(cfg.atr_base),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            bull_ob: None,
            bear_ob: None,
            event: None,
            active: false,
            cfg,
        }
    }

    pub fn config(&self) -> &ReactionSpeedConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    /// Highest high and lowest low of `swing_len` bars ending `from` bars back.
    fn swing(&self, from: usize) -> Option<(f64, f64)> {
        let mut hi = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        for k in from..from + self.cfg.swing_len {
            let r = self.at(k)?;
            hi = hi.max(r.high);
            lo = lo.min(r.low);
        }
        Some((hi, lo))
    }

    /// The most recent candle within `ob_search` bars back that closed down (`bearish`) or up.
    fn last_opposite(&self, bearish: bool) -> Option<(f64, f64)> {
        (1..=self.cfg.ob_search).find_map(|i| {
            let r = self.at(i)?;
            let hit = if bearish {
                r.close < r.open
            } else {
                r.close > r.open
            };
            hit.then_some((r.high, r.low))
        })
    }
}

impl Indicator for ReactionSpeed {
    fn name(&self) -> &str {
        "reaction_speed"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.atr_len + self.cfg.atr_base - 2
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let c = self.cfg.clone();
        let tr = match self.at(0) {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let atr_base = atr.and_then(|a| self.atr_base.update(a));
        self.rows.push_back(Row {
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;
        let prev = self.at(1).copied();

        if let (Some(a), Some(p)) = (atr, prev) {
            let body = (bar.close - bar.open).abs();
            if bar.close > bar.open && body > c.disp_body * a && bar.close > p.high {
                if let Some(ob) = self.last_opposite(true) {
                    self.bull_ob = Some(ob);
                }
            }
            if bar.close < bar.open && body > c.disp_body * a && bar.close < p.low {
                if let Some(ob) = self.last_opposite(false) {
                    self.bear_ob = Some(ob);
                }
            }
        }
        let bull_tap = self
            .bull_ob
            .is_some_and(|(h, l)| bar.low <= h && bar.high >= l);
        let bear_tap = self
            .bear_ob
            .is_some_and(|(h, l)| bar.high >= l && bar.low <= h);
        let (mut bos_up, mut bos_dn, mut sweep_up, mut sweep_dn) = (false, false, false, false);
        if let Some((lsh, lsl)) = self.swing(1) {
            let p = prev.unwrap();
            let (plsh, plsl) = self.swing(2).unwrap_or((lsh, lsl));
            bos_up = bar.close > lsh && p.close <= plsh;
            bos_dn = bar.close < lsl && p.close >= plsl;
            sweep_up = bar.high > lsh && bar.close < lsh;
            sweep_dn = bar.low < lsl && bar.close > lsl;
        }
        let fire = if bos_up || bos_dn {
            Some((1u8, if bos_up { 1i8 } else { -1 }))
        } else if sweep_up || sweep_dn {
            Some((2, if sweep_dn { 1 } else { -1 }))
        } else if bull_tap || bear_tap {
            let up = bull_tap && !(bear_tap && bar.close < bar.open);
            Some((3, if up { 1 } else { -1 }))
        } else {
            None
        };

        let mut final_score = None;
        if let (false, Some((kind, dir)), Some(a)) = (self.active, fire, atr) {
            self.active = true;
            self.event = Some(Event {
                kind,
                dir,
                bar: t,
                reference: bar.close,
                atr: a,
                reaction: None,
                val_a: false,
                speed: None,
                validation: None,
                score: None,
            });
        }
        if self.active {
            let mut ev = self.event.unwrap();
            let age = t - ev.bar;
            if ev.reaction.is_none() && age >= 1 {
                let p = prev.unwrap();
                let bull_r = bar.close > bar.open || bar.low >= p.low;
                let bear_r = bar.close < bar.open || bar.high <= p.high;
                if (ev.dir == 1 && bull_r) || (ev.dir == -1 && bear_r) {
                    ev.reaction = Some((t, bar.high, bar.low));
                }
            }
            if let Some((rb, rh, rl)) = ev.reaction {
                if ev.speed.is_none() {
                    let to_react = (rb - ev.bar - 1) as f64;
                    ev.speed = Some(
                        100.0
                            * (1.0 - to_react / ((c.react_bars as f64) - 1.0).max(1.0))
                                .clamp(0.0, 1.0),
                    );
                }
                let since = t - rb;
                if !ev.val_a && since <= c.valid_bars {
                    ev.val_a = if ev.dir == 1 {
                        bar.close > rh
                    } else {
                        bar.close < rl
                    };
                }
                let target = c.atr_move * ev.atr;
                let mv = if ev.dir == 1 {
                    bar.high - ev.reference
                } else {
                    ev.reference - bar.low
                };
                let progress = (if target > 0.0 { mv / target } else { 0.0 }).clamp(0.0, 1.0);
                let sum_v = (c.w_a + c.w_b).max(0.0001);
                let sum_f = (c.w_speed + c.w_val).max(0.0001);
                let val = 100.0
                    * (c.w_a / sum_v * if ev.val_a { 1.0 } else { 0.0 } + c.w_b / sum_v * progress);
                ev.validation = Some(val);
                ev.score = Some(
                    (c.w_speed / sum_f * ev.speed.unwrap() + c.w_val / sum_f * val)
                        .clamp(0.0, 100.0),
                );
                if since > c.valid_bars {
                    self.active = false;
                    final_score = ev.score;
                }
            }
            if ev.reaction.is_none() && age > c.react_bars {
                ev.speed = Some(0.0);
                ev.validation = Some(0.0);
                ev.score = Some(0.0);
                self.active = false;
                final_score = Some(0.0);
            }
            self.event = Some(ev);
        }

        let (a, base) = (atr?, atr_base?);
        let ratio = if base > 0.0 { a / base } else { 1.0 };
        let activity = (50.0 + 50.0 * ((ratio - 1.0) * c.activity_k).tanh()).clamp(0.0, 100.0);
        let mut extra = HashMap::new();
        let mut state = "idle";
        if self.active || final_score.is_some() {
            let ev = self.event.unwrap();
            if let (Some(r), Some(s), Some(v)) = (ev.score, ev.speed, ev.validation) {
                extra.insert("reaction".to_string(), r);
                extra.insert("speed".to_string(), s);
                extra.insert("validation".to_string(), v);
            }
            extra.insert("event_dir".to_string(), f64::from(ev.dir));
            extra.insert("event_type".to_string(), f64::from(ev.kind));
            if let Some(f) = final_score {
                extra.insert("final_score".to_string(), f);
            }
            if self.active {
                state = match ev.kind {
                    1 => "tracking_bos",
                    2 => "tracking_sweep",
                    _ => "tracking_ob",
                };
            }
        }
        Some(IndicatorOutput::with_extra(activity, extra).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}
