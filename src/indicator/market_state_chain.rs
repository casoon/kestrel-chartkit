//! Market State Chain: six market states as a first-order Markov chain, read as "when this
//! regime ends, where does it go?".
//!
//! A regime resolution map, not a price forecast.
//!
//! States (classification priority top to bottom): 5 expansion/chaos (ATR rank > `chaos_rank`,
//! efficiency < `eff_th`), 2 compression (ATR rank and Bollinger-width rank < `comp_rank`,
//! efficiency < `eff_th`), 0 up trend / 3 down trend (context plus ADX > `trend_th` and
//! efficiency > `eff_th`), 1 up weak / 4 down weak (context only), otherwise 2. Context = close
//! vs. EMA(`ema_len`) plus the EMA slope over 5 bars in ATR units beyond ±`slope_th`. Ranks are
//! min–max positions over `rank_len` bars. A raw state is committed after `min_dwell` bars.
//!
//! Over the last `lookback` bars every bar in the same context state (compression split by
//! up/down/neutral context) counts the state that followed it; self-transitions give the
//! persistence, the others Laplace-smoothed (`laplace`) exit probabilities with the current state
//! excluded: bull = up trend + up weak, bear = down trend + down weak, neutral = compression +
//! chaos. Reliability = `min(1, exits / min_samples)` · (1 − normalised 3-way entropy). The bias
//! (+1/−1/0) flips only when a side leads the others by `bias_margin` points. Maturity = bars in
//! state ÷ the typical dwell `1 / (1 − persistence)`.
//!
//! `value` = resolution bias `p_bull − p_bear`; `extra`: `state_code`, `context_state`,
//! `p_bull`, `p_bear`, `p_neutral`, `reliability`, `bias`, `exits`, `bars_in_state`,
//! `persistence` and `dwell_ratio` when they exist, `atr_rank`, `bb_rank`, `efficiency`, `adx`.
//! `state`: `up_trend`, `up_weak`, `compression`, `down_trend`, `down_weak`, `chaos`.
//!
//! Alerts (closed bars; the prefix names the expected move): an edge appears when the bias is
//! confirmed by confidence ≥ `conf_th` and reliability ≥ `rel_th` and the market is not already
//! in that trend state, typed by the state it comes out of — `bull_breakout` / `bear_breakout`
//! (out of compression or chaos: the resolution of a squeeze), `bull_reversal` / `bear_reversal`
//! (out of an opposing state), `bull_continuation` / `bear_continuation` (out of the same-side
//! weak state); `regime_change` on every committed state change and `state_extended` when the
//! dwell ratio first reaches 1.5 (both without direction). Every count uses past bars only.
//!
//! Conventions the original leaves open (also in the golden fixture): EMA seeded with its
//! first value; ATR and DMI Wilder averages seeded with an SMA (DMI via [`Adx`], from the second
//! bar); population standard deviation; a bar is classified once every input exists, earlier
//! bars carry no state and never match; the debounced state starts at compression; bars in state
//! is 0 until the first state change.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/market_state_chain.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::adx::Adx;
use super::smoothing::{Ema, ExtremeWindow, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original.
#[derive(Debug, Clone)]
pub struct MarketStateChainConfig {
    pub ema_len: usize,
    pub atr_len: usize,
    pub eff_len: usize,
    pub adx_len: usize,
    pub rank_len: usize,
    pub bb_len: usize,
    pub trend_th: f64,
    pub eff_th: f64,
    pub chaos_rank: f64,
    pub comp_rank: f64,
    pub slope_th: f64,
    pub lookback: usize,
    pub laplace: f64,
    pub min_samples: usize,
    pub min_dwell: usize,
    pub conf_th: f64,
    pub rel_th: f64,
    pub bias_margin: f64,
}

impl Default for MarketStateChainConfig {
    fn default() -> Self {
        Self {
            ema_len: 55,
            atr_len: 14,
            eff_len: 34,
            adx_len: 14,
            rank_len: 100,
            bb_len: 20,
            trend_th: 22.0,
            eff_th: 38.0,
            chaos_rank: 70.0,
            comp_rank: 35.0,
            slope_th: 0.05,
            lookback: 500,
            laplace: 1.0,
            min_samples: 20,
            min_dwell: 2,
            conf_th: 55.0,
            rel_th: 40.0,
            bias_margin: 8.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    close: f64,
    ema: f64,
    state: Option<u8>,
    ctx: Option<u8>,
}

fn state_name(s: u8) -> &'static str {
    match s {
        0 => "up_trend",
        1 => "up_weak",
        3 => "down_trend",
        4 => "down_weak",
        5 => "chaos",
        _ => "compression",
    }
}

/// Streaming Market State Chain; see the module documentation.
pub struct MarketStateChain {
    cfg: MarketStateChainConfig,
    ema: Ema,
    atr: Rma,
    adx: Adx,
    atr_window: ExtremeWindow,
    bb_window: ExtremeWindow,
    closes_bb: VecDeque<f64>,
    rows: VecDeque<Row>,
    cap: usize,
    raw_prev: Option<u8>,
    raw_run: usize,
    current: u8,
    bias: i8,
    prev_edge: (bool, bool),
    prev_coiled: bool,
    bars: usize,
    last_change: Option<usize>,
    alerts: Vec<IndicatorAlert>,
}

impl MarketStateChain {
    pub fn new(cfg: MarketStateChainConfig) -> Self {
        let cap = cfg.lookback.max(cfg.eff_len).max(6) + 2;
        Self {
            ema: Ema::new(cfg.ema_len),
            atr: Rma::new(cfg.atr_len),
            adx: Adx::new(cfg.adx_len, cfg.adx_len, 1, cfg.trend_th),
            atr_window: ExtremeWindow::new(cfg.rank_len),
            bb_window: ExtremeWindow::new(cfg.rank_len),
            closes_bb: VecDeque::with_capacity(cfg.bb_len + 1),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            raw_prev: None,
            raw_run: 0,
            current: 2,
            bias: 0,
            prev_edge: (false, false),
            prev_coiled: false,
            bars: 0,
            last_change: None,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &MarketStateChainConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }
}

impl Indicator for MarketStateChain {
    fn name(&self) -> &str {
        "market_state_chain"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        (c.bb_len + c.rank_len - 1)
            .max(c.atr_len + c.rank_len - 1)
            .max(self.adx.warmup_period())
            .max(c.eff_len)
            .max(6)
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
        let atr_ext = atr.and_then(|a| self.atr_window.push(a));
        let ema = self.ema.update(bar.close).unwrap();
        let adx = self.adx.on_bar(bar).map(|o| o.value);
        self.closes_bb.push_back(bar.close);
        if self.closes_bb.len() > c.bb_len {
            self.closes_bb.pop_front();
        }
        let bb_width = (self.closes_bb.len() == c.bb_len).then(|| {
            let n = c.bb_len as f64;
            let m = self.closes_bb.iter().sum::<f64>() / n;
            let dev = (self.closes_bb.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n).sqrt();
            if m != 0.0 {
                dev / m * 100.0
            } else {
                0.0
            }
        });
        let bb_ext = bb_width.and_then(|w| self.bb_window.push(w));
        let prev_state = self.at(0).and_then(|r| r.state);
        self.rows.push_back(Row {
            close: bar.close,
            ema,
            state: None,
            ctx: None,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;

        let ema5 = self.at(5).map(|r| r.ema);
        let (
            Some(atr),
            Some(adx),
            Some(ema5),
            Some((a_lo, a_hi)),
            Some((b_lo, b_hi)),
            Some(bbw),
            true,
        ) = (
            atr,
            adx,
            ema5,
            atr_ext,
            bb_ext,
            bb_width,
            t + 1 >= c.eff_len,
        )
        else {
            self.raw_prev = None;
            return None;
        };
        let slope = if atr != 0.0 { (ema - ema5) / atr } else { 0.0 };
        let a_rank = if a_hi != a_lo {
            (atr - a_lo) / (a_hi - a_lo) * 100.0
        } else {
            50.0
        };
        let b_rank = if b_hi != b_lo {
            (bbw - b_lo) / (b_hi - b_lo) * 100.0
        } else {
            50.0
        };
        let mut path = 0.0;
        for i in 0..c.eff_len - 1 {
            path += (self.at(i).unwrap().close - self.at(i + 1).unwrap().close).abs();
        }
        let direct = (bar.close - self.at(c.eff_len - 1).unwrap().close).abs();
        let eff = if path > 0.0 {
            (direct / path * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let up = bar.close > ema && slope > c.slope_th;
        let dn = bar.close < ema && slope < -c.slope_th;
        let high_trend = adx > c.trend_th && eff > c.eff_th;
        let raw = if a_rank > c.chaos_rank && eff < c.eff_th {
            5
        } else if a_rank < c.comp_rank && b_rank < c.comp_rank && eff < c.eff_th {
            2
        } else if up && high_trend {
            0
        } else if dn && high_trend {
            3
        } else if up {
            1
        } else if dn {
            4
        } else {
            2
        };
        self.raw_run = if self.raw_prev == Some(raw) {
            self.raw_run + 1
        } else {
            1
        };
        self.raw_prev = Some(raw);
        if self.raw_run >= c.min_dwell {
            self.current = raw;
        }
        let cur = self.current;
        let ctx = if cur == 2 {
            if up {
                6
            } else if dn {
                7
            } else {
                2
            }
        } else {
            cur
        };
        {
            let r = self.rows.back_mut().unwrap();
            r.state = Some(cur);
            r.ctx = Some(ctx);
        }

        let mut counts = [0.0f64; 6];
        let mut c_self = 0.0;
        for i in 1..=c.lookback {
            let Some(r) = self.at(i) else { break };
            if r.ctx == Some(ctx) {
                let to = self.at(i - 1).unwrap().state.unwrap();
                if to == cur {
                    c_self += 1.0;
                } else {
                    counts[usize::from(to)] += 1.0;
                }
            }
        }
        let total: f64 = counts.iter().sum();
        let total_all = total + c_self;
        let persistence = (total_all > 0.0).then(|| c_self / total_all * 100.0);
        let denom = total + 5.0 * c.laplace;
        let p: [f64; 6] = std::array::from_fn(|k| {
            if k == usize::from(cur) {
                0.0
            } else {
                (counts[k] + c.laplace) / denom * 100.0
            }
        });
        let (p_bull, p_bear, p_neu) = (p[0] + p[1], p[3] + p[4], p[2] + p[5]);
        let term = |x: f64| {
            let q = x / 100.0;
            if q > 0.0 {
                -q * q.ln()
            } else {
                0.0
            }
        };
        let entropy = term(p_bull) + term(p_bear) + term(p_neu);
        let predictability = (1.0 - entropy / 3f64.ln()).clamp(0.0, 1.0);
        let reliability = (total / c.min_samples as f64).min(1.0) * predictability * 100.0;
        let dprob = p_bull.max(p_bear.max(p_neu));
        if p_bull - p_bear.max(p_neu) >= c.bias_margin {
            self.bias = 1;
        } else if p_bear - p_bull.max(p_neu) >= c.bias_margin {
            self.bias = -1;
        } else if p_neu - p_bull.max(p_bear) >= c.bias_margin {
            self.bias = 0;
        }
        let ok_sig = dprob >= c.conf_th && reliability >= c.rel_th;
        let bull_edge = self.bias == 1 && ok_sig && cur != 0;
        let bear_edge = self.bias == -1 && ok_sig && cur != 3;
        let mut fired: Vec<&str> = Vec::new();
        if bull_edge && !self.prev_edge.0 {
            fired.push(match cur {
                3 | 4 => "bull_reversal",
                2 | 5 => "bull_breakout",
                _ => "bull_continuation",
            });
        }
        if bear_edge && !self.prev_edge.1 {
            fired.push(match cur {
                0 | 1 => "bear_reversal",
                2 | 5 => "bear_breakout",
                _ => "bear_continuation",
            });
        }
        self.prev_edge = (bull_edge, bear_edge);
        if prev_state.is_some_and(|s| s != cur) {
            fired.push("regime_change");
            self.last_change = Some(t);
        }
        let bars_in = self.last_change.map_or(0, |l| t - l);
        let pf = persistence.unwrap_or(0.0) / 100.0;
        let typical = (pf > 0.0 && pf < 0.999).then(|| 1.0 / (1.0 - pf));
        let dwell = typical.filter(|&d| d > 0.0).map(|d| bars_in as f64 / d);
        let coiled = dwell.is_some_and(|d| d >= 1.5);
        if coiled && !self.prev_coiled {
            fired.push("state_extended");
        }
        self.prev_coiled = coiled;
        for kind in fired {
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!(
                    "Markov {kind}: state {}, bull {p_bull:.1} %, bear {p_bear:.1} %, reliability {reliability:.1} %",
                    state_name(cur)
                ),
                (reliability / 100.0).clamp(0.05, 1.0),
            ));
        }

        let mut extra = HashMap::new();
        for (k, v) in [
            ("state_code", f64::from(cur)),
            ("context_state", f64::from(ctx)),
            ("p_bull", p_bull),
            ("p_bear", p_bear),
            ("p_neutral", p_neu),
            ("reliability", reliability),
            ("bias", f64::from(self.bias)),
            ("exits", total),
            ("bars_in_state", bars_in as f64),
            ("atr_rank", a_rank),
            ("bb_rank", b_rank),
            ("efficiency", eff),
            ("adx", adx),
        ] {
            extra.insert(k.to_string(), v);
        }
        if let Some(pv) = persistence {
            extra.insert("persistence".to_string(), pv);
        }
        if let Some(d) = dwell {
            extra.insert("dwell_ratio".to_string(), d);
        }
        Some(IndicatorOutput::with_extra(p_bull - p_bear, extra).with_state(state_name(cur)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const MARKET_STATE_CHAIN_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "market_state_chain",
        aliases: &[],
        description: "Market State Chain: six debounced states (up/down trend, up/down weak, compression, chaos) as a first-order Markov chain; over lookback bars the exits of the current (context) state give Laplace-smoothed bull/bear/neutral exit probabilities, persistence, reliability (sample adequacy x predictability) and a hysteresis bias; value = p_bull - p_bear; extra: state_code, context_state, p_bull, p_bear, p_neutral, reliability, bias, exits, bars_in_state, persistence, dwell_ratio, atr_rank, bb_rank, efficiency, adx; alerts bull/bear_breakout (edge out of compression or chaos), bull/bear_reversal, bull/bear_continuation (prefix = expected move), regime_change, state_extended",
        params: &[
            super::definition::ParamSpec::count("ema_len", 55, 10, 1000).meaning("bars in the trend EMA"),
            super::definition::ParamSpec::count("rank_len", 100, 20, 2000).meaning("bars the ATR and Bollinger width are ranked against"),
            super::definition::ParamSpec::count("lookback", 500, 50, 1500).meaning("bars whose state exits give the probabilities"),
            super::definition::ParamSpec::count("min_dwell", 2, 1, 50).meaning("bars a new state must hold before it counts"),
            super::definition::ParamSpec::count("min_samples", 20, 5, 1000).meaning("exits of the context state for full reliability"),
            super::definition::ParamSpec::number("conf_th", 55.0, 0.0, 100.0).meaning("exit probability an alert needs, in percent"),
            super::definition::ParamSpec::number("rel_th", 40.0, 0.0, 100.0).meaning("reliability an alert needs, in percent"),
            super::definition::ParamSpec::number("bias_margin", 8.0, 0.0, 100.0).meaning("probability lead that turns the bias, in percent"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(MarketStateChain::new(MarketStateChainConfig {
                ema_len: p.count("ema_len"),
                rank_len: p.count("rank_len"),
                lookback: p.count("lookback"),
                min_dwell: p.count("min_dwell"),
                min_samples: p.count("min_samples"),
                conf_th: p.number("conf_th"),
                rel_th: p.number("rel_th"),
                bias_margin: p.number("bias_margin"),
                ..MarketStateChainConfig::default()
            })))
        },
    };
