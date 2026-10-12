//! Smooth Trend Radar: a double-smoothed supertrend baseline with slope-hysteresis trend,
//! baseline rejections and statistical overextension.
//!
//! Port of the Pine Script "Smooth Trend Radar" (WavesUnchained).
//!
//! ```text
//! lower/upper = supertrend bands (hl2 ∓ factor·ATR(atr_len), ratcheting as in Pine)
//! baseline    = EMA(ema_len) of WMA(wma_len) of (lower + upper) / 2
//! slope       = baseline − baseline[slope_lookback]
//! trend       = +1 if slope > ATR·slope_threshold, −1 if slope < −ATR·slope_threshold, else held
//! strength    = percentrank(|slope| / ATR, strength_lookback)      (strong above strength_threshold)
//! overext     = percentrank(|close − baseline|, overext_lookback)  (above overext_threshold)
//! ```
//!
//! `value` = baseline; `extra`: `trend`, `st_lower`, `st_upper`, `adx` (once available),
//! `strength_rank`, `distance_rank` (once their windows exist), `overextension` (+1 price far
//! above the baseline, −1 far below, 0 otherwise). `state`: `uptrend` / `downtrend` / `neutral`.
//!
//! Alerts (closed bars; the prefix names the expected move):
//!
//! | alert | condition |
//! |---|---|
//! | `bull_flip` / `bear_flip` | trend turns to +1 / −1 while ADX(14) > `adx_threshold` |
//! | `bull_rejection` / `bear_rejection` | low (high) touches the baseline, close back on the trend side, baseline rising (falling) over `rejection_slope` bars, at least `touches` touching bars of the trend side within the last `touches` bars |
//! | `bull_rejection_strong` / `bear_rejection_strong` | the same rejection while the trend strength rank exceeds `strength_threshold` — replaces the plain rejection on that bar |
//!
//! Not ported: auto timeframe scaling (the defaults are the Pine manual values), the
//! volatility-adaptive band width, polarity and volume filters (off in Pine) and the SL/TP trade
//! overlay, which is execution logic rather than an indicator.
//!
//! Conventions where Pine leaves them open (also in the golden fixture): ATR and ADX Wilder
//! averages seeded with an SMA (ADX via [`Adx`]); the supertrend bands start raw on the first bar
//! with an ATR; the EMA seeded with its first value; percentrank = share of the previous `n`
//! values less than or equal to the current one, needing all of them. A bar reports once the
//! baseline and ATR exist.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/smooth_trend_radar.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::adx::Adx;
use super::smoothing::{Ema, Rma, Wma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults as in the Pine script (manual values).
#[derive(Debug, Clone)]
pub struct SmoothTrendRadarConfig {
    pub factor: f64,
    pub atr_len: usize,
    pub wma_len: usize,
    pub ema_len: usize,
    pub adx_threshold: f64,
    pub slope_lookback: usize,
    pub slope_threshold: f64,
    pub strength_lookback: usize,
    pub strength_threshold: f64,
    pub touches: usize,
    pub rejection_slope: usize,
    pub overext_lookback: usize,
    pub overext_threshold: f64,
}

impl Default for SmoothTrendRadarConfig {
    fn default() -> Self {
        Self {
            factor: 12.0,
            atr_len: 90,
            wma_len: 40,
            ema_len: 14,
            adx_threshold: 25.0,
            slope_lookback: 3,
            slope_threshold: 0.05,
            strength_lookback: 100,
            strength_threshold: 70.0,
            touches: 2,
            rejection_slope: 5,
            overext_lookback: 100,
            overext_threshold: 85.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    base: Option<f64>,
    trend: i8,
}

/// Rolling percent rank: share of the previous `n` values `<=` the newest one.
#[derive(Debug, Clone)]
struct PercentRank {
    n: usize,
    window: VecDeque<Option<f64>>,
}

impl PercentRank {
    fn new(n: usize) -> Self {
        Self {
            n,
            window: VecDeque::with_capacity(n + 2),
        }
    }

    fn update(&mut self, v: Option<f64>) -> Option<f64> {
        self.window.push_back(v);
        if self.window.len() > self.n + 1 {
            self.window.pop_front();
        }
        if self.window.len() < self.n + 1 {
            return None;
        }
        let cur = v?;
        let mut count = 0usize;
        for p in self.window.iter().take(self.n) {
            if (*p)? <= cur {
                count += 1;
            }
        }
        Some(100.0 * count as f64 / self.n as f64)
    }
}

/// Streaming Smooth Trend Radar; see the module documentation.
pub struct SmoothTrendRadar {
    cfg: SmoothTrendRadarConfig,
    atr: Rma,
    wma: Wma,
    ema: Ema,
    adx: Adx,
    bands: Option<(f64, f64)>,
    prev_close: Option<f64>,
    trend: i8,
    strength: PercentRank,
    distance: PercentRank,
    rows: VecDeque<Row>,
    cap: usize,
    alerts: Vec<IndicatorAlert>,
}

impl SmoothTrendRadar {
    pub fn new(cfg: SmoothTrendRadarConfig) -> Self {
        let cap = cfg.slope_lookback.max(cfg.rejection_slope).max(cfg.touches) + 2;
        Self {
            atr: Rma::new(cfg.atr_len),
            wma: Wma::new(cfg.wma_len),
            ema: Ema::new(cfg.ema_len),
            adx: Adx::new(14, 14, 1, cfg.adx_threshold),
            bands: None,
            prev_close: None,
            trend: 0,
            strength: PercentRank::new(cfg.strength_lookback),
            distance: PercentRank::new(cfg.overext_lookback),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &SmoothTrendRadarConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }
}

impl Indicator for SmoothTrendRadar {
    fn name(&self) -> &str {
        "smooth_trend_radar"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.atr_len + self.cfg.wma_len - 2
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let tr = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let adx = self.adx.on_bar(bar).map(|o| o.value);
        let mut base = None;
        if let Some(a) = atr {
            let hl2 = (bar.high + bar.low) / 2.0;
            let mut lower = hl2 - c.factor * a;
            let mut upper = hl2 + c.factor * a;
            if let (Some((pl, pu)), Some(pc)) = (self.bands, self.prev_close) {
                if !(lower > pl || pc < pl) {
                    lower = pl;
                }
                if !(upper < pu || pc > pu) {
                    upper = pu;
                }
            }
            self.bands = Some((lower, upper));
            base = self
                .wma
                .update((lower + upper) / 2.0)
                .and_then(|w| self.ema.update(w));
        }
        self.prev_close = Some(bar.close);

        let slope = match (base, self.at(c.slope_lookback - 1).and_then(|r| r.base)) {
            (Some(b), Some(p)) => Some(b - p),
            _ => None,
        };
        let prev_trend = self.at(0).map_or(0, |r| r.trend);
        if let (Some(s), Some(a)) = (slope, atr) {
            if s > a * c.slope_threshold {
                self.trend = 1;
            } else if s < -a * c.slope_threshold {
                self.trend = -1;
            }
        }
        let strength = self
            .strength
            .update(slope.zip(atr).map(|(s, a)| s.abs() / a));
        let distance = self.distance.update(base.map(|b| (bar.close - b).abs()));
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            base,
            trend: self.trend,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }

        let (Some(base), Some(_)) = (base, atr) else {
            return None;
        };
        let trend = self.trend;
        let trending = adx.is_some_and(|a| a > c.adx_threshold);
        let mut fired: Vec<(&str, f64)> = Vec::new();
        if trend == 1 && prev_trend != 1 && trending {
            fired.push(("bull_flip", 0.7));
        }
        if trend == -1 && prev_trend != -1 && trending {
            fired.push(("bear_flip", 0.7));
        }
        let (mut bull_t, mut bear_t) = (0, 0);
        for i in 0..c.touches {
            if let Some(r) = self.at(i) {
                if let Some(b) = r.base {
                    if r.trend == 1 && r.low <= b {
                        bull_t += 1;
                    }
                    if r.trend == -1 && r.high >= b {
                        bear_t += 1;
                    }
                }
            }
        }
        let strong = strength.is_some_and(|s| s > c.strength_threshold);
        if let Some(past) = self.at(c.rejection_slope).and_then(|r| r.base) {
            if bar.low <= base
                && bar.close > base
                && trend == 1
                && base > past
                && bull_t >= c.touches
            {
                if strong {
                    fired.push(("bull_rejection_strong", 0.8));
                } else {
                    fired.push(("bull_rejection", 0.5));
                }
            }
            if bar.high >= base
                && bar.close < base
                && trend == -1
                && base < past
                && bear_t >= c.touches
            {
                if strong {
                    fired.push(("bear_rejection_strong", 0.8));
                } else {
                    fired.push(("bear_rejection", 0.5));
                }
            }
        }

        for (kind, strength) in fired {
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("Smooth Trend Radar {kind}: baseline {base:.4}"),
                strength,
            ));
        }
        let (lower, upper) = self.bands.unwrap();
        let mut extra = HashMap::new();
        extra.insert("trend".to_string(), f64::from(trend));
        extra.insert("st_lower".to_string(), lower);
        extra.insert("st_upper".to_string(), upper);
        if let Some(a) = adx {
            extra.insert("adx".to_string(), a);
        }
        if let Some(s) = strength {
            extra.insert("strength_rank".to_string(), s);
        }
        let mut overext = 0.0;
        if let Some(d) = distance {
            extra.insert("distance_rank".to_string(), d);
            if d > c.overext_threshold && bar.close > base {
                overext = 1.0;
            } else if d > c.overext_threshold && bar.close < base {
                overext = -1.0;
            }
        }
        extra.insert("overextension".to_string(), overext);
        let state = match trend {
            1 => "uptrend",
            -1 => "downtrend",
            _ => "neutral",
        };
        Some(IndicatorOutput::with_extra(base, extra).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
