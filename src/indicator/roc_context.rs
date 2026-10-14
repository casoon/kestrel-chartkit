//! ROC Context: percentage rate of change with signal line, trend context, stall/absorption
//! detection and strong/weak stretch crosses.
//!
//! ```text
//! roc     = 100 · (close − close[roc_len]) / close[roc_len]      (0 when close[roc_len] = 0)
//! line    = MA(avg_len) of roc        signal = MA(sig_len) of line
//! context = MA(avg_len) of the ROC over ctx_len
//! stall   = line moved more than stall_roc_min over stall_len bars while the close moved less
//!           than stall_flat_atr · ATR(14): +1 when the line fell (selling absorbed), −1 when it rose
//! ```
//!
//! `value` = line, `secondary` = signal; `extra`: `hist`, `context`, `sentiment` (−100..100, how
//! deep the line sits in its stretch zone), `stall` (+1/−1/0), `divergence` (+1 line falling by
//! more than `div_min` over `div_len` bars while the context rises, −1 mirrored), `quality` on a
//! stretch-cross bar (stretch depth 50 % + context agreement 25 % + no stall against it 25 %).
//!
//! Alerts (closed bars; prefix = expected move):
//!
//! | alert | condition |
//! |---|---|
//! | `bull_cross` / `bear_cross` | line crosses its signal up (down) after touching `os_level` (`ob_level`) within `lookback_extreme` bars, no stall against it, context on the same side of zero |
//! | `bull_cross_weak` / `bear_cross_weak` | the same cross with a stall against it or a context on the other side of zero ("counter-trend") — replaces the strong kind |
//! | `bull_zero_cross` / `bear_zero_cross` | line crosses the midline |
//! | `bull_stall` / `bear_stall` | first bar of a stall (+1 / −1) |
//!
//! The raw ROC itself is [`super::momentum_indicators`]' `roc`; this indicator adds the context
//! layer. `ma_kind`: 0 EMA (default), 1 SMA, 2 RMA; the WMA/SuperSmoother/T3/KAMA/JMA
//! choices are not offered.
//!
//! Conventions the original leaves open (also in the golden fixture): EMAs seeded with the
//! first ROC value; ATR Wilder average seeded with an SMA. A bar reports once line, signal,
//! context, ATR and every lookback it reads exist.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/roc_context.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Rma, Smoother, SmootherKind};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original.
#[derive(Debug, Clone)]
pub struct RocContextConfig {
    pub ma_kind: SmootherKind,
    pub roc_len: usize,
    pub avg_len: usize,
    pub sig_len: usize,
    pub mid_line: f64,
    pub ob_level: f64,
    pub os_level: f64,
    pub stall_len: usize,
    pub stall_flat_atr: f64,
    pub stall_roc_min: f64,
    pub ctx_len: usize,
    pub lookback_extreme: usize,
    pub div_len: usize,
    pub div_min: f64,
}

impl Default for RocContextConfig {
    fn default() -> Self {
        Self {
            ma_kind: SmootherKind::Ema,
            roc_len: 12,
            avg_len: 3,
            sig_len: 9,
            mid_line: 0.0,
            ob_level: 5.0,
            os_level: -5.0,
            stall_len: 5,
            stall_flat_atr: 0.5,
            stall_roc_min: 3.0,
            ctx_len: 50,
            lookback_extreme: 5,
            div_len: 4,
            div_min: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Row {
    close: f64,
    line: Option<f64>,
    signal: Option<f64>,
    ctx: Option<f64>,
}

/// Streaming ROC Context; see the module documentation.
pub struct RocContext {
    cfg: RocContextConfig,
    line_ma: Box<dyn Smoother>,
    sig_ma: Box<dyn Smoother>,
    ctx_ma: Box<dyn Smoother>,
    atr: Rma,
    rows: VecDeque<Row>,
    cap: usize,
    prev_stall: i8,
    alerts: Vec<IndicatorAlert>,
}

impl RocContext {
    pub fn new(cfg: RocContextConfig) -> Self {
        let cap = cfg
            .roc_len
            .max(cfg.ctx_len)
            .max(cfg.stall_len)
            .max(cfg.div_len)
            .max(cfg.lookback_extreme)
            + 2;
        Self {
            line_ma: cfg.ma_kind.build(cfg.avg_len),
            sig_ma: cfg.ma_kind.build(cfg.sig_len),
            ctx_ma: cfg.ma_kind.build(cfg.avg_len),
            atr: Rma::new(14),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            prev_stall: 0,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &RocContextConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    /// ROC over `len` for a new close, from the stored closes (before the new row is pushed).
    fn roc(&self, close: f64, len: usize) -> Option<f64> {
        let old = self.at(len - 1)?.close;
        Some(if old != 0.0 {
            100.0 * (close - old) / old
        } else {
            0.0
        })
    }
}

impl Indicator for RocContext {
    fn name(&self) -> &str {
        "roc_context"
    }

    fn warmup_period(&self) -> usize {
        let c = &self.cfg;
        // Samples a smoother needs for its first value (`0` = emits from the first one).
        let first = |s: &dyn Smoother| s.warmup_period().max(1);
        // ROC over `len` first exists at bar `len + 1`.
        let line = c.roc_len + first(self.line_ma.as_ref());
        let signal = line + first(self.sig_ma.as_ref()) - 1;
        let ctx = c.ctx_len + first(self.ctx_ma.as_ref());
        signal
            .max(ctx + c.div_len)
            .max(line + c.stall_len.max(c.div_len))
            .max(line + c.lookback_extreme.max(1) - 1)
            .max(c.stall_len + 1)
            .max(14)
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
        let line = self
            .roc(bar.close, c.roc_len)
            .and_then(|r| self.line_ma.update(r));
        let signal = line.and_then(|l| self.sig_ma.update(l));
        let ctx = self
            .roc(bar.close, c.ctx_len)
            .and_then(|r| self.ctx_ma.update(r));
        let prev = self.at(0).copied();
        self.rows.push_back(Row {
            close: bar.close,
            line,
            signal,
            ctx,
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }

        let mut lo_ext = f64::INFINITY;
        let mut hi_ext = f64::NEG_INFINITY;
        for k in 0..c.lookback_extreme {
            let v = self.at(k).and_then(|r| r.line)?;
            lo_ext = lo_ext.min(v);
            hi_ext = hi_ext.max(v);
        }
        let (
            Some(line),
            Some(signal),
            Some(ctx),
            Some(atr),
            Some(line_s),
            Some(ctx_d),
            Some(line_d),
            Some(close_s),
        ) = (
            line,
            signal,
            ctx,
            atr,
            self.at(c.stall_len).and_then(|r| r.line),
            self.at(c.div_len).and_then(|r| r.ctx),
            self.at(c.div_len).and_then(|r| r.line),
            self.at(c.stall_len).map(|r| r.close),
        )
        else {
            return None;
        };

        let roc_chg = line - line_s;
        let price_chg = if atr != 0.0 {
            (bar.close - close_s) / atr
        } else {
            0.0
        };
        let flat = price_chg.abs() < c.stall_flat_atr;
        let stall_bull = roc_chg < -c.stall_roc_min && flat;
        let stall_bear = roc_chg > c.stall_roc_min && flat;
        let stall: i8 = if stall_bull {
            1
        } else if stall_bear {
            -1
        } else {
            0
        };
        let ctx_bull = ctx > c.mid_line;
        let ctx_bear = ctx < c.mid_line;
        let (bull_x, bear_x, bull_mid, bear_mid) = match prev {
            Some(Row {
                line: Some(pl),
                signal: Some(ps),
                ..
            }) => (
                crossed_over(pl, ps, line, signal),
                crossed_under(pl, ps, line, signal),
                crossed_over(pl, c.mid_line, line, c.mid_line),
                crossed_under(pl, c.mid_line, line, c.mid_line),
            ),
            _ => (false, false, false, false),
        };
        let bull_ext = bull_x && lo_ext <= c.os_level;
        let bear_ext = bear_x && hi_ext >= c.ob_level;
        let bull_depth = ((c.os_level - lo_ext) / (c.mid_line - c.os_level).abs()).clamp(0.0, 1.0);
        let bear_depth = ((hi_ext - c.ob_level) / (c.ob_level - c.mid_line).abs()).clamp(0.0, 1.0);
        let bull_q = (bull_depth * 50.0
            + if ctx_bull { 25.0 } else { 0.0 }
            + if stall_bear { 0.0 } else { 25.0 })
        .min(100.0);
        let bear_q = (bear_depth * 50.0
            + if ctx_bear { 25.0 } else { 0.0 }
            + if stall_bull { 0.0 } else { 25.0 })
        .min(100.0);

        let mut fired: Vec<(&str, f64)> = Vec::new();
        if bull_ext {
            let weak = stall_bear || ctx_bear;
            fired.push((
                if weak {
                    "bull_cross_weak"
                } else {
                    "bull_cross"
                },
                bull_q,
            ));
        }
        if bear_ext {
            let weak = stall_bull || ctx_bull;
            fired.push((
                if weak {
                    "bear_cross_weak"
                } else {
                    "bear_cross"
                },
                bear_q,
            ));
        }
        if bull_mid {
            fired.push(("bull_zero_cross", 50.0));
        }
        if bear_mid {
            fired.push(("bear_zero_cross", 50.0));
        }
        if stall == 1 && self.prev_stall != 1 {
            fired.push(("bull_stall", 50.0));
        }
        if stall == -1 && self.prev_stall != -1 {
            fired.push(("bear_stall", 50.0));
        }
        self.prev_stall = stall;
        for (kind, q) in fired {
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("ROC Context {kind}: line {line:+.2}, context {ctx:+.2}"),
                (q / 100.0).clamp(0.05, 1.0),
            ));
        }

        let bull_pct = (line - c.mid_line).max(0.0) / (c.ob_level - c.mid_line) * 100.0;
        let bear_pct = (c.mid_line - line).max(0.0) / (c.mid_line - c.os_level) * 100.0;
        let fast_dir = line - line_d;
        let slow_dir = ctx - ctx_d;
        let div = if fast_dir < -c.div_min && slow_dir > 0.0 {
            1.0
        } else if fast_dir > c.div_min && slow_dir < 0.0 {
            -1.0
        } else {
            0.0
        };
        let mut extra = HashMap::new();
        extra.insert("hist".to_string(), line - signal);
        extra.insert("context".to_string(), ctx);
        extra.insert(
            "sentiment".to_string(),
            (bull_pct - bear_pct).clamp(-100.0, 100.0),
        );
        extra.insert("stall".to_string(), f64::from(stall));
        extra.insert("divergence".to_string(), div);
        if bull_ext {
            extra.insert("quality".to_string(), bull_q);
        } else if bear_ext {
            extra.insert("quality".to_string(), bear_q);
        }
        Some(IndicatorOutput::with_extra(line, extra).with_secondary(signal))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const ROC_CONTEXT_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "roc_context",
        aliases: &[],
        description: "ROC Context: percentage ROC (roc_len) smoothed (avg_len, ma_kind 0 EMA, 1 SMA, 2 RMA) with a signal line (sig_len) and a slow context ROC (ctx_len); value = line, secondary = signal; extra: hist, context, sentiment (-100..100 stretch depth), stall (+1 line falling while price holds, -1 rising), divergence (fast vs. context), quality on stretch-cross bars; alerts bull/bear_cross (signal cross out of the stretch zone, context agrees, no stall against), bull/bear_cross_weak (with stall against or counter-context), bull/bear_zero_cross, bull/bear_stall (first stall bar); prefix = expected move",
        params: &[
            super::definition::ParamSpec::count("ma_kind", 0, 0, 2).meaning("how the ROC is smoothed: 0 EMA, 1 SMA, 2 RMA"),
            super::definition::ParamSpec::count("roc_len", 12, 1, 1000).meaning("bars in the rate of change"),
            super::definition::ParamSpec::count("avg_len", 3, 1, 200).meaning("bars smoothing the ROC (the line)"),
            super::definition::ParamSpec::count("sig_len", 9, 1, 200).meaning("bars in the signal line"),
            super::definition::ParamSpec::number("ob_level", 5.0, 0.01, 1000.0).meaning("upper stretch level, in percent"),
            super::definition::ParamSpec::number("os_level", -5.0, -1000.0, -0.01).meaning("lower stretch level, in percent"),
            super::definition::ParamSpec::count("ctx_len", 50, 2, 2000).meaning("bars in the slow context ROC"),
            super::definition::ParamSpec::count("stall_len", 5, 1, 200).meaning("bars a stall is measured over"),
        ],
        checks: &[],
        build: |p| {
            let ma_kind = match p.count("ma_kind") {
                0 => SmootherKind::Ema,
                1 => SmootherKind::Sma,
                _ => SmootherKind::Rma,
            };
            Ok(Box::new(RocContext::new(RocContextConfig {
                ma_kind,
                roc_len: p.count("roc_len"),
                avg_len: p.count("avg_len"),
                sig_len: p.count("sig_len"),
                ob_level: p.number("ob_level"),
                os_level: p.number("os_level"),
                ctx_len: p.count("ctx_len"),
                stall_len: p.count("stall_len"),
                ..RocContextConfig::default()
            })))
        },
    };
