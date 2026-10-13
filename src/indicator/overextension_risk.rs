//! Overextension Risk: overextension / reversal-risk context from three role scores.
//!
//! Per side (top = upside overextension, bottom = downside), three role scores `0..=100`:
//!
//! | role | inputs |
//! |---|---|
//! | stretch (location) | close vs. EMA in ATR units, Bollinger z-score, proximity to the `structure_len` high/low, band breach |
//! | exhaustion (momentum) | RSI, StochRSI (SMA 3), WaveTrend wt1, an active WaveTrend divergence, money flow (MFI + volume spike) where real volume exists |
//! | reaction (trigger) | reclaim-fail of band or prior extreme, wt1/wt2 cross out of ±45, wick rejection |
//!
//! `score = (stretch·w_stretch + exhaustion·w_exhaust + reaction·w_react) / Σw · vol_factor`,
//! `vol_factor = 0.85 + vol_score/100 · 0.30` from ATR expansion and bar range. A regime (EMA
//! `regime_fast`/`regime_slow` stack plus ADX ≥ `adx_trend`) raises the threshold by
//! `regime_penalty` for a signal against the trend and names the signal.
//!
//! `value` = top score, `secondary` = bottom score; `extra`: `stretch_top`/`_bot`,
//! `exhaust_top`/`_bot`, `react_top`/`_bot`, `vol_factor`, `top_thresh`, `bot_thresh`, `regime`
//! (1 up, −1 down, 0 range), `adx`, `money_flow` (1 when real volume feeds the flow part),
//! `bear_div_active`, `bull_div_active`. `state`: `trend_up` / `trend_down` / `range`.
//!
//! Alerts (closed bars; `confirm` 1 = score at threshold and a reaction on the bar, 0 = score
//! first reaching the threshold; at least `min_gap` bars between same-side signals). The prefix
//! names the expected move; a top inside an uptrend is only a warning:
//!
//! | alert | condition |
//! |---|---|
//! | `do_not_chase_up` / `do_not_chase_down` | top in an uptrend / bottom in a downtrend ("continuation risk") |
//! | `bear_trend_exhaustion` / `bull_trend_exhaustion` | top in a downtrend / bottom in an uptrend |
//! | `bear_range_fade` / `bull_range_fade` | top / bottom without a trend regime |
//!
//! The stretch role keeps its own formula rather than [`super::price_average_relation`]'s
//! extension: that score is unsigned and measured against its own history, while the scanner
//! needs a signed location per side plus band and structure terms.
//!
//! Conventions the original leaves open (also in the golden fixture): EMAs seeded with the
//! first value; RSI, ATR and ADX Wilder averages seeded with an SMA (ADX via [`Adx`], from the
//! second bar); WaveTrend via [`WaveTrendEngine`]; population standard deviation; StochRSI not
//! available when the RSI range is zero; MFI flow from the second bar, 50 while its window has no
//! volume; pivots strict on both sides. A bar reports once every role input exists. Not ported:
//! market presets (the defaults are the Commodities weights), divergence-assisted thresholds,
//! zones and the dashboard.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/overextension_risk.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::adx::Adx;
use super::smoothing::{Ema, Rma, Sma};
use super::wavetrend::WaveTrendEngine;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original (preset Commodities, mode Normal).
#[derive(Debug, Clone)]
pub struct OverextensionRiskConfig {
    pub w_stretch: f64,
    pub w_exhaust: f64,
    pub w_react: f64,
    pub use_flow: bool,
    pub base_threshold: f64,
    pub confirm: bool,
    pub min_gap: usize,
    pub regime_fast: usize,
    pub regime_slow: usize,
    pub adx_len: usize,
    pub adx_trend: f64,
    pub regime_penalty: f64,
    pub ema_len: usize,
    pub atr_len: usize,
    pub bb_len: usize,
    pub bb_mult: f64,
    pub rsi_len: usize,
    pub stoch_len: usize,
    pub wt_n1: usize,
    pub wt_n2: usize,
    pub pivot_len: usize,
    pub div_memory: usize,
    pub structure_len: usize,
    pub near_atr: f64,
}

impl Default for OverextensionRiskConfig {
    fn default() -> Self {
        Self {
            w_stretch: 40.0,
            w_exhaust: 35.0,
            w_react: 25.0,
            use_flow: true,
            base_threshold: 40.0,
            confirm: true,
            min_gap: 8,
            regime_fast: 20,
            regime_slow: 100,
            adx_len: 14,
            adx_trend: 25.0,
            regime_penalty: 12.0,
            ema_len: 50,
            atr_len: 14,
            bb_len: 20,
            bb_mult: 2.0,
            rsi_len: 14,
            stoch_len: 14,
            wt_n1: 10,
            wt_n2: 21,
            pivot_len: 5,
            div_memory: 12,
            structure_len: 80,
            near_atr: 0.75,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    src: f64,
    volume: f64,
    wt1: Option<f64>,
    wt2: Option<f64>,
    recent_high: Option<f64>,
    recent_low: Option<f64>,
    top_extreme: bool,
    bot_extreme: bool,
}

/// Streaming Overextension Risk; see the module documentation.
pub struct OverextensionRisk {
    cfg: OverextensionRiskConfig,
    atr: Rma,
    ema: Ema,
    closes: VecDeque<f64>,
    rsi_up: Rma,
    rsi_down: Rma,
    rsi_window: VecDeque<f64>,
    stoch_sma: Sma,
    wt: WaveTrendEngine,
    vol_sma: Sma,
    cum_volume: f64,
    atr_sma: Sma,
    ema_fast: Ema,
    ema_slow: Ema,
    adx: Adx,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    last_ph: Option<(f64, Option<f64>)>,
    last_pl: Option<(f64, Option<f64>)>,
    bear_div: Option<(usize, f64)>,
    bull_div: Option<(usize, f64)>,
    last_top: Option<usize>,
    last_bot: Option<usize>,
    alerts: Vec<IndicatorAlert>,
}

fn sd(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

fn s01(x: f64) -> f64 {
    x.clamp(0.0, 1.0) * 100.0
}

fn flag(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

impl OverextensionRisk {
    pub fn new(cfg: OverextensionRiskConfig) -> Self {
        let cap = (2 * cfg.pivot_len + 1).max(cfg.structure_len).max(14) + 2;
        Self {
            atr: Rma::new(cfg.atr_len),
            ema: Ema::new(cfg.ema_len),
            closes: VecDeque::with_capacity(cfg.bb_len + 1),
            rsi_up: Rma::new(cfg.rsi_len),
            rsi_down: Rma::new(cfg.rsi_len),
            rsi_window: VecDeque::with_capacity(cfg.stoch_len + 1),
            stoch_sma: Sma::new(3),
            wt: WaveTrendEngine::new(cfg.wt_n1, cfg.wt_n2, 60.0, -60.0),
            vol_sma: Sma::new(30),
            cum_volume: 0.0,
            atr_sma: Sma::new(50),
            ema_fast: Ema::new(cfg.regime_fast),
            ema_slow: Ema::new(cfg.regime_slow),
            adx: Adx::new(cfg.adx_len, cfg.adx_len, 1, cfg.adx_trend),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            last_ph: None,
            last_pl: None,
            bear_div: None,
            bull_div: None,
            last_top: None,
            last_bot: None,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &OverextensionRiskConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    fn mfi(&self) -> f64 {
        let n = 14;
        if self.bars < n {
            return 50.0;
        }
        let (mut pos, mut neg, mut vol) = (0.0, 0.0, 0.0);
        for k in 0..n {
            let r = self.at(k).unwrap();
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
            50.0
        } else if neg == 0.0 {
            100.0
        } else if pos == 0.0 {
            0.0
        } else {
            100.0 - 100.0 / (1.0 + pos / neg)
        }
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

    fn update_divergences(&mut self) {
        let p = self.cfg.pivot_len;
        let t = self.bars - 1;
        let w = self.at(p).and_then(|r| r.wt1);
        if let Some(ph) = self.pivot(false) {
            if let (Some((lp, Some(lw))), Some(w)) = (self.last_ph, w) {
                if ph > lp && w < lw {
                    self.bear_div = Some((t, ph));
                }
            }
            self.last_ph = Some((ph, w));
        }
        if let Some(pl) = self.pivot(true) {
            if let (Some((lp, Some(lw))), Some(w)) = (self.last_pl, w) {
                if pl < lp && w > lw {
                    self.bull_div = Some((t, pl));
                }
            }
            self.last_pl = Some((pl, w));
        }
    }
}

impl Indicator for OverextensionRisk {
    fn name(&self) -> &str {
        "overextension_risk"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        c.structure_len
            .max(c.atr_len + 49)
            .max(c.rsi_len + c.stoch_len + 1)
            .max(2 * c.adx_len)
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
        let ema = self.ema.update(bar.close);
        self.closes.push_back(bar.close);
        if self.closes.len() > c.bb_len {
            self.closes.pop_front();
        }
        let bb = (self.closes.len() == c.bb_len).then(|| {
            let n = c.bb_len as f64;
            let m = self.closes.iter().sum::<f64>() / n;
            let dev = (self.closes.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n).sqrt();
            (m, dev)
        });

        let rsi = prev_close.and_then(|pc| {
            let d = bar.close - pc;
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
        });
        let stoch = rsi.and_then(|r| {
            self.rsi_window.push_back(r);
            if self.rsi_window.len() > c.stoch_len {
                self.rsi_window.pop_front();
            }
            if self.rsi_window.len() < c.stoch_len {
                return None;
            }
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
            (hi != lo).then(|| 100.0 * (r - lo) / (hi - lo))
        });
        let stoch = match stoch {
            Some(s) => self.stoch_sma.update(s),
            None => {
                if rsi.is_some() {
                    self.stoch_sma.reset();
                }
                None
            }
        };
        let wt = self.wt.on_bar(bar);
        let (wt1, wt2) = match &wt {
            Some(o) => (Some(o.extra["wt1"]), Some(o.extra["wt2"])),
            None => (None, None),
        };
        self.cum_volume += bar.volume;
        let vol_ma = self.vol_sma.update(bar.volume);
        let atr_ma = atr.and_then(|a| self.atr_sma.update(a));
        let ema_fast = self.ema_fast.update(bar.close).unwrap();
        let ema_slow = self.ema_slow.update(bar.close).unwrap();
        let adx = self.adx.on_bar(bar).map(|o| o.value);

        let n = c.structure_len;
        let (recent_high, recent_low) = if self.bars + 1 >= n {
            let mut hi = bar.high;
            let mut lo = bar.low;
            for k in 0..n - 1 {
                let r = self.at(k).unwrap();
                hi = hi.max(r.high);
                lo = lo.min(r.low);
            }
            (Some(hi), Some(lo))
        } else {
            (None, None)
        };
        let prior = self.at(0).copied();
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            src: bar.typical_price(),
            volume: bar.volume,
            wt1,
            wt2,
            recent_high,
            recent_low,
            top_extreme: false,
            bot_extreme: false,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;
        self.update_divergences();

        let bear_active = self
            .bear_div
            .is_some_and(|(b, p)| t - b <= c.div_memory && bar.high <= p);
        let bull_active = self
            .bull_div
            .is_some_and(|(b, p)| t - b <= c.div_memory && bar.low >= p);

        let wt1_2 = self.at(2).and_then(|r| r.wt1);
        let (
            Some(atr),
            Some(ema),
            Some((basis, dev)),
            Some(rsi),
            Some(stoch),
            Some(wt1),
            Some(wt2),
            Some(wt1_2),
            Some(atr_ma),
            Some(rh),
            Some(rl),
            Some(prior),
            Some(adx),
        ) = (
            atr,
            ema,
            bb,
            rsi,
            stoch,
            wt1,
            wt2,
            wt1_2,
            atr_ma,
            recent_high,
            recent_low,
            prior,
            adx,
        )
        else {
            return None;
        };
        let (Some(pwt1), Some(pwt2), Some(prh), Some(prl)) =
            (prior.wt1, prior.wt2, prior.recent_high, prior.recent_low)
        else {
            return None;
        };

        let upper = basis + dev * c.bb_mult;
        let lower = basis - dev * c.bb_mult;
        let dist_atr = sd(bar.close - ema, atr);
        let bb_z = sd(bar.close - basis, dev);
        let atr_exp = sd(atr, atr_ma);
        let range_atr = sd(bar.high - bar.low, atr);
        let near_high = rh - bar.close <= atr * c.near_atr;
        let near_low = bar.close - rl <= atr * c.near_atr;
        let rng = bar.high - bar.low;
        let upper_wick = sd(bar.high - bar.open.max(bar.close), rng);
        let lower_wick = sd(bar.open.min(bar.close) - bar.low, rng);
        let trend_up = ema_fast > ema_slow && bar.close > ema_slow;
        let trend_dn = ema_fast < ema_slow && bar.close < ema_slow;
        let strong = adx >= c.adx_trend;
        let regime_up = strong && trend_up;
        let regime_dn = strong && trend_dn;

        let stretch_top = s01(0.45 * sd(dist_atr, 3.0).max(sd(bb_z, 2.5))
            + 0.35 * flag(near_high)
            + 0.20 * flag(bar.close > upper));
        let stretch_bot = s01(0.45 * sd(-dist_atr, 3.0).max(sd(-bb_z, 2.5))
            + 0.35 * flag(near_low)
            + 0.20 * flag(bar.close < lower));
        let mom_top = s01(0.40 * sd(rsi - 60.0, 20.0)
            + 0.30 * sd(stoch - 75.0, 25.0)
            + 0.30 * sd(wt1 - 45.0, 25.0));
        let mom_bot = s01(0.40 * sd(40.0 - rsi, 20.0)
            + 0.30 * sd(25.0 - stoch, 25.0)
            + 0.30 * sd(-45.0 - wt1, 25.0));
        let div_top = 100.0 * flag(bear_active);
        let div_bot = 100.0 * flag(bull_active);
        let flow_on = c.use_flow && self.cum_volume > 0.0 && vol_ma.is_some_and(|m| m > 0.0);
        let (ex_top, ex_bot) = if flow_on {
            let mfi = self.mfi();
            let spike = sd(bar.volume, vol_ma.unwrap());
            let flow_top = s01(0.55 * sd(mfi - 75.0, 20.0) + 0.45 * sd(spike - 1.5, 1.5));
            let flow_bot = s01(0.55 * sd(25.0 - mfi, 20.0) + 0.45 * sd(spike - 1.5, 1.5));
            (
                0.55 * mom_top + 0.25 * flow_top + 0.20 * div_top,
                0.55 * mom_bot + 0.25 * flow_bot + 0.20 * div_bot,
            )
        } else {
            (
                0.70 * mom_top + 0.30 * div_top,
                0.70 * mom_bot + 0.30 * div_bot,
            )
        };

        let top_rf = (bar.high > upper && bar.close < upper) || (bar.high > prh && bar.close < prh);
        let bot_rf = (bar.low < lower && bar.close > lower) || (bar.low < prl && bar.close > prl);
        let wt_up = wt1 > wt2 && pwt1 <= pwt2;
        let wt_dn = wt1 < wt2 && pwt1 >= pwt2;
        let was_high = wt1.max(pwt1).max(wt1_2) >= 45.0;
        let was_low = wt1.min(pwt1).min(wt1_2) <= -45.0;
        let top_wt = wt_dn && was_high;
        let bot_wt = wt_up && was_low;
        let hl2 = (bar.high + bar.low) / 2.0;
        let top_weak = bar.close < bar.open && bar.close < hl2;
        let bot_weak = bar.close > bar.open && bar.close > hl2;
        let react_top = s01(0.45 * flag(top_rf)
            + 0.30 * flag(top_wt)
            + 0.25
                * if top_weak {
                    sd(upper_wick - 0.35, 0.35)
                } else {
                    0.0
                });
        let react_bot = s01(0.45 * flag(bot_rf)
            + 0.30 * flag(bot_wt)
            + 0.25
                * if bot_weak {
                    sd(lower_wick - 0.35, 0.35)
                } else {
                    0.0
                });
        let top_reacted = top_rf || top_wt || (top_weak && upper_wick >= 0.35);
        let bot_reacted = bot_rf || bot_wt || (bot_weak && lower_wick >= 0.35);

        let vol_score = s01(0.55 * sd(atr_exp - 1.1, 0.7) + 0.45 * sd(range_atr - 1.2, 1.2));
        let vf = 0.85 + sd(vol_score, 100.0) * 0.30;
        let role = (c.w_stretch + c.w_exhaust + c.w_react).max(1.0);
        let top = ((stretch_top * c.w_stretch + ex_top * c.w_exhaust + react_top * c.w_react)
            / role
            * vf)
            .clamp(0.0, 100.0);
        let bot = ((stretch_bot * c.w_stretch + ex_bot * c.w_exhaust + react_bot * c.w_react)
            / role
            * vf)
            .clamp(0.0, 100.0);
        let top_th =
            (c.base_threshold + if regime_up { c.regime_penalty } else { 0.0 }).clamp(20.0, 80.0);
        let bot_th =
            (c.base_threshold + if regime_dn { c.regime_penalty } else { 0.0 }).clamp(20.0, 80.0);

        let top_extreme = top >= top_th;
        let bot_extreme = bot >= bot_th;
        {
            let r = self.rows.back_mut().unwrap();
            r.top_extreme = top_extreme;
            r.bot_extreme = bot_extreme;
        }
        let (top_raw, bot_raw) = if c.confirm {
            (top_extreme && top_reacted, bot_extreme && bot_reacted)
        } else {
            (
                top_extreme && !prior.top_extreme,
                bot_extreme && !prior.bot_extreme,
            )
        };
        let gap_ok = |last: Option<usize>| c.min_gap == 0 || last.is_none_or(|l| t - l > c.min_gap);
        if top_raw && gap_ok(self.last_top) {
            self.last_top = Some(t);
            let kind = if regime_up {
                "do_not_chase_up"
            } else if regime_dn {
                "bear_trend_exhaustion"
            } else {
                "bear_range_fade"
            };
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("Overextension Risk {kind}: top score {top:.0} / threshold {top_th:.0}"),
                (top / 100.0).clamp(0.05, 1.0),
            ));
        }
        if bot_raw && gap_ok(self.last_bot) {
            self.last_bot = Some(t);
            let kind = if regime_dn {
                "do_not_chase_down"
            } else if regime_up {
                "bull_trend_exhaustion"
            } else {
                "bull_range_fade"
            };
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("Overextension Risk {kind}: bottom score {bot:.0} / threshold {bot_th:.0}"),
                (bot / 100.0).clamp(0.05, 1.0),
            ));
        }

        let regime = if regime_up {
            1.0
        } else if regime_dn {
            -1.0
        } else {
            0.0
        };
        let mut extra = HashMap::new();
        for (k, v) in [
            ("stretch_top", stretch_top),
            ("stretch_bot", stretch_bot),
            ("exhaust_top", ex_top),
            ("exhaust_bot", ex_bot),
            ("react_top", react_top),
            ("react_bot", react_bot),
            ("vol_factor", vf),
            ("top_thresh", top_th),
            ("bot_thresh", bot_th),
            ("regime", regime),
            ("adx", adx),
            ("money_flow", flag(flow_on)),
            ("bear_div_active", flag(bear_active)),
            ("bull_div_active", flag(bull_active)),
        ] {
            extra.insert(k.to_string(), v);
        }
        let state = if regime_up {
            "trend_up"
        } else if regime_dn {
            "trend_down"
        } else {
            "range"
        };
        Some(
            IndicatorOutput::with_extra(top, extra)
                .with_secondary(bot)
                .with_state(state),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
