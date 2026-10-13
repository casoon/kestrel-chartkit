//! Trend Break Risk: how close the prevailing trend's structure is to breaking, and against
//! which swing level.
//!
//! The prevailing trend is latched from an EMA (`bias_len`): close above it with a rising slope
//! (`ema - ema[5]`) sets it up, the mirror sets it down, anything else keeps the last direction.
//! In an uptrend the risk reads the top-break side (the last swing low is the break level), in a
//! downtrend the bottom-break side (the last swing high). Four evidence sensors, each `0..=1`:
//!
//! | sensor | uptrend (downtrend mirrored) |
//! |---|---|
//! | near | `1 - norm(distance, 0, near_atr)` while price is above the break level and the ATR distance shrinks |
//! | sfp | high above the last swing high, close back below it (failed breakout) |
//! | structure | on a pivot-high bar: a lower high, `norm((prev - last) / ATR, 0, 2)` |
//! | divergence | on a pivot-high bar: higher high in price, lower RSI at the pivot bars |
//!
//! Each sensor is held as `max(sensor, held[1] · (1 - 1/hold_bars))`. The raw risk is the
//! weighted sum over the reachable maximum (weight sum minus the smaller of the structure and
//! divergence weights, which exclude each other), capped at 100 and 0 without a trend; `value` is
//! its EMA (`smooth_len`). `extra`: `risk_raw`, `trend_dir`, `near`, `sfp`, `structure`,
//! `divergence` (held sensors), `rsi`, `ema`, and `break_level`/`distance_atr` while a break
//! level exists. `state`: `no_trend`, `structure_broken`, `break_pressure`, `critical`, `watch`
//! or `quiet`.
//!
//! A confirmed break of structure is judged against the trend of the previous bar: a close
//! through the break level (the previous close still on the other side) while the EMA context
//! has flipped too (close beyond the EMA, slope turned), or — when such a swing break stayed
//! pressure — on the later bar the latched trend flips while price is still beyond that
//! unconsumed level. A swing break without the context flip is break pressure.
//!
//! Alerts (closed bars; prefix = expected move, i.e. against the trend at risk):
//!
//! | alert | condition |
//! |---|---|
//! | `bear_break_pressure` / `bull_break_pressure` | risk crosses above `level_high` in an up-/downtrend |
//! | `bear_break_critical` / `bull_break_critical` | risk crosses above `level_critical` in an up-/downtrend |
//! | `bear_structure_break` / `bull_structure_break` | confirmed break of an up-/downtrend |
//! | `bear_divergence` / `bull_divergence` | the divergence sensor fires in an up-/downtrend |
//!
//! Conventions the original leaves open (also in the golden fixture): EMA seeded with the
//! first close; ATR = Wilder average of the true range (first bar high - low), SMA-seeded, floored
//! at `1e-12` for the mintick; RSI = Wilder averages of the close changes, SMA-seeded, 50 when
//! nothing moved; pivots strict on both sides. Pine's `na` would poison the held sensors while
//! ATR or RSI are not yet available; here a sensor that needs them is 0 on such a bar (and the
//! previous distance counts as missing). The calculation runs from the first bar, output starts
//! once ATR and RSI exist.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/trend_break_risk.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original.
#[derive(Debug, Clone)]
pub struct TrendBreakRiskConfig {
    pub bias_len: usize,
    pub pivot_len: usize,
    pub near_atr: f64,
    pub rsi_len: usize,
    pub atr_len: usize,
    pub hold_bars: usize,
    pub smooth_len: usize,
    pub w_near: f64,
    pub w_sfp: f64,
    pub w_structure: f64,
    pub w_divergence: f64,
    pub level_watch: f64,
    pub level_high: f64,
    pub level_critical: f64,
    pub require_context_flip: bool,
}

impl Default for TrendBreakRiskConfig {
    fn default() -> Self {
        Self {
            bias_len: 50,
            pivot_len: 5,
            near_atr: 1.5,
            rsi_len: 14,
            atr_len: 14,
            hold_bars: 10,
            smooth_len: 3,
            w_near: 25.0,
            w_sfp: 20.0,
            w_structure: 15.0,
            w_divergence: 10.0,
            level_watch: 35.0,
            level_high: 60.0,
            level_critical: 80.0,
            require_context_flip: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    ema: f64,
    rsi: Option<f64>,
}

/// Streaming Trend Break Risk; see the module documentation.
pub struct TrendBreakRisk {
    cfg: TrendBreakRiskConfig,
    ema: Ema,
    atr: Rma,
    gain: Rma,
    loss: Rma,
    risk_ema: Ema,
    rows: VecDeque<Row>,
    cap: usize,
    trend_dir: i8,
    last_ph: Option<f64>,
    prev_ph: Option<f64>,
    last_pl: Option<f64>,
    prev_pl: Option<f64>,
    last_rsi_ph: Option<f64>,
    prev_rsi_ph: Option<f64>,
    last_rsi_pl: Option<f64>,
    prev_rsi_pl: Option<f64>,
    prev_dist: Option<f64>,
    held: [f64; 4],
    consumed_pl: Option<f64>,
    consumed_ph: Option<f64>,
    prev_risk: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

fn norm(x: f64, lo: f64, hi: f64) -> f64 {
    if hi == lo {
        0.0
    } else {
        ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
    }
}

impl TrendBreakRisk {
    pub fn new(cfg: TrendBreakRiskConfig) -> Self {
        let cap = (2 * cfg.pivot_len + 1).max(6) + 1;
        Self {
            ema: Ema::new(cfg.bias_len),
            atr: Rma::new(cfg.atr_len),
            gain: Rma::new(cfg.rsi_len),
            loss: Rma::new(cfg.rsi_len),
            risk_ema: Ema::new(cfg.smooth_len),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            cfg,
            trend_dir: 0,
            last_ph: None,
            prev_ph: None,
            last_pl: None,
            prev_pl: None,
            last_rsi_ph: None,
            prev_rsi_ph: None,
            last_rsi_pl: None,
            prev_rsi_pl: None,
            prev_dist: None,
            held: [0.0; 4],
            consumed_pl: None,
            consumed_ph: None,
            prev_risk: None,
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &TrendBreakRiskConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        self.rows.len().checked_sub(k + 1).map(|i| &self.rows[i])
    }

    /// Strict pivot `pivot_len` bars back: (high or low, RSI at that bar).
    fn pivot(&self, low_side: bool) -> Option<(f64, Option<f64>)> {
        let p = self.cfg.pivot_len;
        let centre = self.at(p)?;
        self.at(2 * p)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..p).chain(p + 1..=2 * p) {
            let r = self.at(k)?;
            let ok = if low_side { v < r.low } else { v > r.high };
            if !ok {
                return None;
            }
        }
        Some((v, centre.rsi))
    }
}

impl Indicator for TrendBreakRisk {
    fn name(&self) -> &str {
        "trend_break_risk"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.rsi_len.max(self.cfg.atr_len - 1)
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let prev = self.rows.back().copied();
        let tr = match prev {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr).map(|a| a.max(1e-12));
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
        let ema = self.ema.update(bar.close).expect("first-sample seed");
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            ema,
            rsi,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }

        // Trend latch.
        let slope = self.at(5).map(|r| ema - r.ema);
        let prev_trend = self.trend_dir;
        match slope {
            Some(s) if bar.close > ema && s > 0.0 => self.trend_dir = 1,
            Some(s) if bar.close < ema && s < 0.0 => self.trend_dir = -1,
            _ => {}
        }
        let trend = self.trend_dir;

        // Swing levels and the RSI at each pivot.
        let ph = self.pivot(false);
        let pl = self.pivot(true);
        if let Some((v, r)) = ph {
            self.prev_ph = self.last_ph;
            self.last_ph = Some(v);
            self.prev_rsi_ph = self.last_rsi_ph;
            self.last_rsi_ph = r;
        }
        if let Some((v, r)) = pl {
            self.prev_pl = self.last_pl;
            self.last_pl = Some(v);
            self.prev_rsi_pl = self.last_rsi_pl;
            self.last_rsi_pl = r;
        }

        // Sensors, selected by the prevailing trend.
        let (mut near, mut sfp, mut structure, mut div) = (0.0, 0.0, 0.0, 0.0);
        let mut dist = None;
        let mut break_level = None;
        if trend != 0 {
            let up = trend == 1;
            let (level, opposite, prev_opposite) = if up {
                (self.last_pl, self.last_ph, self.prev_ph)
            } else {
                (self.last_ph, self.last_pl, self.prev_pl)
            };
            break_level = level;
            if let (Some(lv), Some(a)) = (level, atr) {
                let d = if up {
                    (bar.close - lv) / a
                } else {
                    (lv - bar.close) / a
                };
                dist = Some(d);
                let approaching = matches!(self.prev_dist, Some(p) if d < p);
                if d >= 0.0 && approaching {
                    near = 1.0 - norm(d, 0.0, c.near_atr);
                }
            }
            if let Some(o) = opposite {
                let failed = if up {
                    bar.high > o && bar.close < o
                } else {
                    bar.low < o && bar.close > o
                };
                if failed {
                    sfp = 1.0;
                }
            }
            let fresh = if up { ph.is_some() } else { pl.is_some() };
            if fresh {
                if let (Some(last), Some(prev), Some(a)) = (opposite, prev_opposite, atr) {
                    let eroded = if up { last < prev } else { last > prev };
                    if eroded {
                        structure = norm((last - prev).abs() / a, 0.0, 2.0);
                    }
                }
                let (lr, pr) = if up {
                    (self.last_rsi_ph, self.prev_rsi_ph)
                } else {
                    (self.last_rsi_pl, self.prev_rsi_pl)
                };
                if let (Some(last), Some(prev), Some(lr), Some(pr)) =
                    (opposite, prev_opposite, lr, pr)
                {
                    let diverges = if up {
                        last > prev && lr < pr
                    } else {
                        last < prev && lr > pr
                    };
                    if diverges {
                        div = 1.0;
                    }
                }
            }
        }
        self.prev_dist = dist;
        let decay = if c.hold_bars <= 1 {
            0.0
        } else {
            1.0 - 1.0 / c.hold_bars as f64
        };
        for (h, s) in self.held.iter_mut().zip([near, sfp, structure, div]) {
            *h = s.max(*h * decay);
        }
        let [near_h, sfp_h, struct_h, div_h] = self.held;

        // Confirmed break of structure, judged against the previous bar's trend.
        let prev_close = prev.map(|p| p.close);
        let raw_up = prev_trend == 1
            && matches!((self.last_pl, prev_close), (Some(l), Some(pc)) if bar.close < l && pc >= l);
        let raw_dn = prev_trend == -1
            && matches!((self.last_ph, prev_close), (Some(h), Some(pc)) if bar.close > h && pc <= h);
        let context_ok = !c.require_context_flip
            || (raw_up && bar.close < ema && matches!(slope, Some(s) if s < 0.0))
            || (raw_dn && bar.close > ema && matches!(slope, Some(s) if s > 0.0));
        let flip_up = c.require_context_flip
            && prev_trend == 1
            && trend == -1
            && !raw_up
            && matches!(self.last_pl, Some(l) if bar.close < l && self.consumed_pl != Some(l));
        let flip_dn = c.require_context_flip
            && prev_trend == -1
            && trend == 1
            && !raw_dn
            && matches!(self.last_ph, Some(h) if bar.close > h && self.consumed_ph != Some(h));
        let bos_up = (raw_up && context_ok) || flip_up;
        let bos_dn = (raw_dn && context_ok) || flip_dn;
        let bos_pressure = (raw_up || raw_dn) && !context_ok;
        if bos_up {
            self.consumed_pl = self.last_pl;
        }
        if bos_dn {
            self.consumed_ph = self.last_ph;
        }

        // Composite risk.
        let weight_sum = c.w_near + c.w_sfp + c.w_structure + c.w_divergence;
        let reachable = weight_sum - c.w_structure.min(c.w_divergence);
        let risk_raw = if trend != 0 && reachable > 0.0 {
            ((near_h * c.w_near
                + sfp_h * c.w_sfp
                + struct_h * c.w_structure
                + div_h * c.w_divergence)
                / reachable
                * 100.0)
                .min(100.0)
        } else {
            0.0
        };
        let risk = self.risk_ema.update(risk_raw).expect("first-sample seed");
        let prev_risk = self.prev_risk.replace(risk);

        if atr.is_none() || rsi.is_none() {
            return None;
        }

        let crossed = |level: f64| matches!(prev_risk, Some(p) if risk > level && p <= level);
        let side = if trend == 1 { "bear" } else { "bull" };
        let mut fire = |kind: String, note: String, strength: f64| {
            self.alerts.push(IndicatorAlert::new(kind, note, strength));
        };
        if trend != 0 && crossed(c.level_high) {
            fire(
                format!("{side}_break_pressure"),
                format!("SBR: break-pressure risk {risk:.0}"),
                risk / 100.0,
            );
        }
        if trend != 0 && crossed(c.level_critical) {
            fire(
                format!("{side}_break_critical"),
                format!("SBR: critical risk {risk:.0}"),
                risk / 100.0,
            );
        }
        if bos_up {
            fire(
                "bear_structure_break".to_string(),
                "SBR: uptrend structure broken".to_string(),
                1.0,
            );
        }
        if bos_dn {
            fire(
                "bull_structure_break".to_string(),
                "SBR: downtrend structure broken".to_string(),
                1.0,
            );
        }
        if div > 0.0 {
            fire(
                format!("{side}_divergence"),
                "SBR: price extends at the pivot, RSI does not".to_string(),
                0.6,
            );
        }

        let state = if trend == 0 {
            "no_trend"
        } else if bos_up || bos_dn {
            "structure_broken"
        } else if bos_pressure {
            "break_pressure"
        } else if risk >= c.level_critical {
            "critical"
        } else if risk >= c.level_high {
            "break_pressure"
        } else if risk >= c.level_watch {
            "watch"
        } else {
            "quiet"
        };
        let mut extra = HashMap::new();
        extra.insert("risk_raw".to_string(), risk_raw);
        extra.insert("trend_dir".to_string(), trend as f64);
        extra.insert("near".to_string(), near_h);
        extra.insert("sfp".to_string(), sfp_h);
        extra.insert("structure".to_string(), struct_h);
        extra.insert("divergence".to_string(), div_h);
        extra.insert("rsi".to_string(), rsi.unwrap());
        extra.insert("ema".to_string(), ema);
        if let Some(l) = break_level {
            extra.insert("break_level".to_string(), l);
        }
        if let Some(d) = dist {
            extra.insert("distance_atr".to_string(), d);
        }
        Some(IndicatorOutput::with_extra(risk, extra).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
