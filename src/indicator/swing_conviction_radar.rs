//! Swing Conviction Radar: how hard each pivot-to-pivot leg drove price, and divergences between
//! successive price extremes and that conviction.
//!
//! Port of the Pine Script "Swing Conviction Radar" (WavesUnchained, v1.2.2). When a pivot is
//! confirmed (`pivot_right` bars after it), the leg from the opposite pivot to it is scored
//! `0..=100` from three parts:
//!
//! | part | meaning |
//! |---|---|
//! | speed | leg distance per bar in ATR, divided by `speed_ref` and capped at 1 |
//! | cleanliness | share of the leg's bars that closed in the leg's direction |
//! | volume gradient | average volume of the later half of the leg against the earlier half, `ratio - 0.5` clamped to `0..=1` |
//!
//! The score is the weighted mean `(speed·w_speed + clean·w_clean + vol·w_volume) / Σw · 100`.
//! Without real volume (no bar so far carried volume) the volume weight is 0 and the other two
//! renormalize. At most `max_leg` bars of a leg are scanned, counted back from the pivot.
//!
//! `value` is the conviction of the last up-leg (confirmed at a pivot high), `secondary` the
//! negated conviction of the last down-leg (as the Pine step lines); `extra`: `bull_last`,
//! `bull_prev`, `bear_last`, `bear_prev` (each only once it exists) and, on a bar that fires an
//! alert, `pivot_price` — the pivot, the natural stop. Output starts once both legs exist.
//!
//! Alerts on the confirmation bar (prefix = expected move):
//!
//! | alert | condition |
//! |---|---|
//! | `bear_conviction_divergence` | pivot high above the previous scored pivot high, up-leg conviction below the previous one |
//! | `bull_conviction_divergence` | pivot low below the previous scored pivot low, down-leg conviction below the previous one |
//!
//! Conventions where Pine leaves them open (also in the golden fixture): pivots strict on both
//! sides; on a bar confirming both a high and a low the high is processed first (the low's leg
//! then has length 0 and is not scored); ATR = Wilder average of the true range (first bar
//! high - low), SMA-seeded — while it is not yet available, speed counts as 0 (Pine's `na > 0` is
//! false); "real volume" = the cumulative volume so far is positive — tick volume cannot be told
//! apart here, a caller without exchange volume passes 0.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/swing_conviction_radar.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults as in the Pine script.
#[derive(Debug, Clone)]
pub struct SwingConvictionConfig {
    pub pivot_left: usize,
    pub pivot_right: usize,
    pub max_leg: usize,
    pub w_speed: f64,
    pub w_clean: f64,
    pub w_volume: f64,
    pub atr_len: usize,
    pub speed_ref: f64,
}

impl Default for SwingConvictionConfig {
    fn default() -> Self {
        Self {
            pivot_left: 5,
            pivot_right: 5,
            max_leg: 100,
            w_speed: 0.4,
            w_clean: 0.4,
            w_volume: 0.2,
            atr_len: 14,
            speed_ref: 1.0,
        }
    }
}

/// Streaming Swing Conviction Radar; see the module documentation.
pub struct SwingConvictionRadar {
    cfg: SwingConvictionConfig,
    atr: Rma,
    rows: VecDeque<Bar>,
    cap: usize,
    bars: usize,
    cum_volume: f64,
    last_low: Option<(usize, f64)>,
    last_high: Option<(usize, f64)>,
    bull_last: Option<(f64, f64)>,
    bull_prev: Option<f64>,
    bear_last: Option<(f64, f64)>,
    bear_prev: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl SwingConvictionRadar {
    pub fn new(cfg: SwingConvictionConfig) -> Self {
        let cap = cfg.pivot_left + cfg.pivot_right + cfg.max_leg + 3;
        Self {
            atr: Rma::new(cfg.atr_len),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            cfg,
            bars: 0,
            cum_volume: 0.0,
            last_low: None,
            last_high: None,
            bull_last: None,
            bull_prev: None,
            bear_last: None,
            bear_prev: None,
            alerts: Vec::new(),
        }
    }

    pub fn config(&self) -> &SwingConvictionConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Bar> {
        self.rows.len().checked_sub(k + 1).map(|i| &self.rows[i])
    }

    /// Strict pivot `pivot_right` bars back.
    fn pivot(&self, low_side: bool) -> Option<f64> {
        let c = &self.cfg;
        let centre = self.at(c.pivot_right)?;
        self.at(c.pivot_right + c.pivot_left)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..c.pivot_right).chain(c.pivot_right + 1..=c.pivot_right + c.pivot_left) {
            let b = self.at(k)?;
            let ok = if low_side { v < b.low } else { v > b.high };
            if !ok {
                return None;
            }
        }
        Some(v)
    }

    fn leg_score(&self, bull: bool, leg_bars: usize, leg_dist: f64, atr: Option<f64>) -> f64 {
        let c = &self.cfg;
        let speed_raw = match atr {
            Some(a) if leg_bars > 0 && a > 0.0 => leg_dist / leg_bars as f64 / a,
            _ => 0.0,
        };
        let speed = (speed_raw / c.speed_ref).min(1.0);
        let max_i = (leg_bars - 1).min(c.max_leg - 1);
        let (mut on, mut v1, mut v2, mut n1, mut n2) = (0usize, 0.0, 0.0, 0usize, 0usize);
        for i in 0..=max_i {
            let off = c.pivot_right + i;
            let cur = self.at(off).expect("leg inside history");
            let prev = self.at(off + 1).expect("leg inside history");
            let is_on = if bull {
                cur.close > prev.close
            } else {
                cur.close < prev.close
            };
            if is_on {
                on += 1;
            }
            if i * 2 <= max_i {
                v1 += cur.volume;
                n1 += 1;
            } else {
                v2 += cur.volume;
                n2 += 1;
            }
        }
        let clean = on as f64 / (max_i + 1) as f64;
        let a1 = if n1 > 0 { v1 / n1 as f64 } else { 0.0 };
        let a2 = if n2 > 0 { v2 / n2 as f64 } else { 1.0 };
        let grad = if a2 > 0.0 { a1 / a2 - 1.0 } else { 0.0 };
        let vol = (grad + 0.5).clamp(0.0, 1.0);
        let wv = if self.cum_volume > 0.0 {
            c.w_volume
        } else {
            0.0
        };
        let total = c.w_speed + c.w_clean + wv;
        if total > 0.0 {
            (speed * c.w_speed + clean * c.w_clean + vol * wv) / total * 100.0
        } else {
            0.0
        }
    }
}

impl Indicator for SwingConvictionRadar {
    fn name(&self) -> &str {
        "swing_conviction_radar"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.pivot_left + self.cfg.pivot_right + 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let t = self.bars;
        self.bars += 1;
        let tr = match self.rows.back() {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        self.cum_volume += bar.volume;
        self.rows.push_back(bar.clone());
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        let r = self.cfg.pivot_right;
        let mut pivot_price = None;

        if let Some(ph) = self.pivot(false) {
            let piv_bar = t - r;
            if let Some((low_bar, low_val)) = self.last_low {
                let leg_bars = piv_bar - low_bar;
                if leg_bars > 0 {
                    let conv = self.leg_score(true, leg_bars, (ph - low_val).abs(), atr);
                    if let Some((last_conv, last_price)) = self.bull_last {
                        if ph > last_price && conv < last_conv {
                            pivot_price = Some(ph);
                            self.alerts.push(IndicatorAlert::new(
                                "bear_conviction_divergence",
                                format!("SCR: higher high {ph:.5} on a weaker up-leg ({conv:.0} < {last_conv:.0})"),
                                ((last_conv - conv) / 100.0).clamp(0.05, 1.0),
                            ));
                        }
                    }
                    self.bull_prev = self.bull_last.map(|(c, _)| c);
                    self.bull_last = Some((conv, ph));
                }
            }
            self.last_high = Some((piv_bar, ph));
        }

        if let Some(pl) = self.pivot(true) {
            let piv_bar = t - r;
            if let Some((high_bar, high_val)) = self.last_high {
                let leg_bars = piv_bar - high_bar;
                if leg_bars > 0 {
                    let conv = self.leg_score(false, leg_bars, (pl - high_val).abs(), atr);
                    if let Some((last_conv, last_price)) = self.bear_last {
                        if pl < last_price && conv < last_conv {
                            pivot_price = Some(pl);
                            self.alerts.push(IndicatorAlert::new(
                                "bull_conviction_divergence",
                                format!("SCR: lower low {pl:.5} on a weaker down-leg ({conv:.0} < {last_conv:.0})"),
                                ((last_conv - conv) / 100.0).clamp(0.05, 1.0),
                            ));
                        }
                    }
                    self.bear_prev = self.bear_last.map(|(c, _)| c);
                    self.bear_last = Some((conv, pl));
                }
            }
            self.last_low = Some((piv_bar, pl));
        }

        let (bull, bear) = (self.bull_last?, self.bear_last?);
        let mut extra = HashMap::new();
        extra.insert("bull_last".to_string(), bull.0);
        extra.insert("bear_last".to_string(), bear.0);
        if let Some(v) = self.bull_prev {
            extra.insert("bull_prev".to_string(), v);
        }
        if let Some(v) = self.bear_prev {
            extra.insert("bear_prev".to_string(), v);
        }
        if let Some(p) = pivot_price {
            extra.insert("pivot_price".to_string(), p);
        }
        Some(IndicatorOutput::with_extra(bull.0, extra).with_secondary(-bear.0))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
