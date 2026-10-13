//! Coil Release: a compressed, corrective coil and the break that resolves
//! it — a trend continuation (release) or a structural change out of a base (base break).
//!
//! | metric | definition over the last `len` bars |
//! |---|---|
//! | complexity | close-to-close moves symbolised into five states in steps of `symbol_atr`·ATR(14); `100 · (0.7 · entropy/ln 5 + 0.3 · transition rate)` |
//! | fractal dimension | box counting: `(ln(r1 + r2) − ln r3) / ln 2` from the ranges of the two half windows and the whole window, clamped to 1..2 |
//! | efficiency | `100 · |net move| / path` |
//! | chop | Choppiness Index over `chop_len` |
//!
//! `coil = 0.5 · complexity/100 + 0.3 · (fd − 1) + 0.2 · (1 − efficiency/100)`; the coil score
//! blends the strongest coil of the previous `break_len` bars (70 %) with the current one (30 %)
//! and is scaled down when the current coil is below `coil_now_min`. Release dynamics = mean of
//! complexity dropping and efficiency rising against their 3-bar means of the previous bar.
//! `setup = coil_score · (rel_floor + (1 − rel_floor) · release_dyn)`.
//!
//! The trend context is the EMA(`ema_len`) slope over `slope_len` bars in ATR(14) units
//! (±`slope_min`). The original reads it from a higher timeframe via `request.security`; the
//! streaming trait sees one timeframe, so the slope is measured on the chart's own bars — scale
//! `ema_len`/`slope_len` by the timeframe ratio for the higher-timeframe behaviour.
//!
//! `value` = setup score `0..=1`; `extra`: `complexity`, `fractal_dim`, `efficiency`, `chop`,
//! `coil`, `coil_score`, `release_dyn`, `trend` (−1/0/1), `corr_high`, `corr_low`, `was_base`
//! (1/0). `state`: `clean_trend`, `correction`, `base` or `mixed` (the regime colouring of the
//! original).
//!
//! Alerts (closed bars; the prefix names the expected move; `cooldown` bars per direction):
//!
//! | alert | condition |
//! |---|---|
//! | `bull_release` / `bear_release` | first close above (below) the `break_len` band of the previous bars, trend +1 (−1), setup ≥ `thr_cont`, chop < `chop_veto`, break impulse ≥ `break_atr_min` ATR |
//! | `bull_base_break` / `bear_base_break` | the same break against the trend out of a base (≥ `base_share` of the previous `break_len` bars with complexity > `complex_high` and efficiency < `eff_low`), setup ≥ `thr_base` |
//!
//! Conventions the original leaves open (also in the golden fixture): ATR Wilder average seeded
//! with an SMA, first true range `high − low`; EMA seeded with its first value; every quantity
//! needs all of its inputs (the complexity every ATR it reads); the base share needs all
//! `break_len` earlier classifications; a first break needs the previous bar's break state. A bar
//! reports once all of that exists.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/coil_release.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Parameters, defaults of the original.
#[derive(Debug, Clone)]
pub struct CoilReleaseConfig {
    pub len: usize,
    pub break_len: usize,
    pub symbol_atr: f64,
    pub ema_len: usize,
    pub slope_len: usize,
    pub slope_min: f64,
    pub complex_high: f64,
    pub complex_low: f64,
    pub eff_high: f64,
    pub eff_low: f64,
    pub rel_floor: f64,
    pub coil_now_min: f64,
    pub thr_cont: f64,
    pub thr_base: f64,
    pub base_share: f64,
    pub break_atr_min: f64,
    pub cooldown: usize,
    pub chop_len: usize,
    pub chop_veto: f64,
}

impl Default for CoilReleaseConfig {
    fn default() -> Self {
        Self {
            len: 34,
            break_len: 12,
            symbol_atr: 0.15,
            ema_len: 55,
            slope_len: 5,
            slope_min: 0.10,
            complex_high: 62.0,
            complex_low: 48.0,
            eff_high: 42.0,
            eff_low: 25.0,
            rel_floor: 0.35,
            coil_now_min: 0.45,
            thr_cont: 0.45,
            thr_base: 0.55,
            base_share: 0.35,
            break_atr_min: 0.10,
            cooldown: 8,
            chop_len: 14,
            chop_veto: 61.8,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    tr: f64,
    atr: Option<f64>,
    ema: f64,
    complexity: Option<f64>,
    eff: Option<f64>,
    coil: Option<f64>,
    break_up: Option<bool>,
    break_dn: Option<bool>,
}

/// Streaming Coil Release; see the module documentation.
pub struct CoilRelease {
    cfg: CoilReleaseConfig,
    atr: Rma,
    ema: Ema,
    rows: VecDeque<Row>,
    cap: usize,
    bars: usize,
    last_up: Option<usize>,
    last_dn: Option<usize>,
    alerts: Vec<IndicatorAlert>,
}

impl CoilRelease {
    pub fn new(cfg: CoilReleaseConfig) -> Self {
        let half = (cfg.len / 2).max(2);
        let cap = cfg
            .len
            .max(2 * half)
            .max(cfg.break_len + 4)
            .max(cfg.chop_len)
            .max(cfg.slope_len + 1)
            + 2;
        Self {
            atr: Rma::new(14),
            ema: Ema::new(cfg.ema_len),
            rows: VecDeque::with_capacity(cap + 1),
            cap,
            bars: 0,
            last_up: None,
            last_dn: None,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &CoilReleaseConfig {
        &self.cfg
    }

    fn at(&self, k: usize) -> Option<&Row> {
        let n = self.rows.len();
        (k < n).then(|| &self.rows[n - 1 - k])
    }

    /// Highest high and lowest low of `n` bars ending `from` bars back.
    fn range(&self, from: usize, n: usize) -> Option<(f64, f64)> {
        let mut hi = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        for k in from..from + n {
            let r = self.at(k)?;
            hi = hi.max(r.high);
            lo = lo.min(r.low);
        }
        Some((hi, lo))
    }

    fn complexity(&self) -> Option<f64> {
        let n = self.cfg.len;
        self.at(n - 1)?;
        let mut counts = [0usize; 5];
        let mut transitions = 0usize;
        let mut prev: Option<usize> = None;
        for i in 0..n - 1 {
            let r = self.at(i)?;
            let r_prev = self.at(i + 1)?;
            let mv = r.close - r_prev.close;
            let unit = r.atr? * self.cfg.symbol_atr;
            let s = if mv > unit {
                4
            } else if mv > 0.0 {
                3
            } else if mv < -unit {
                0
            } else if mv < 0.0 {
                1
            } else {
                2
            };
            counts[s] += 1;
            if prev.is_some_and(|p| p != s) {
                transitions += 1;
            }
            prev = Some(s);
        }
        let total = (n - 1).max(1) as f64;
        let mut entropy = 0.0;
        for &cnt in &counts {
            let p = cnt as f64 / total;
            if p > 0.0 {
                entropy += -p * p.ln();
            }
        }
        entropy /= 5f64.ln();
        let rate = transitions as f64 / (total - 1.0).max(1.0);
        Some(((entropy * 0.70 + rate * 0.30) * 100.0).clamp(0.0, 100.0))
    }

    fn fractal_dim(&self) -> Option<f64> {
        let n = self.cfg.len;
        let half = (n / 2).max(2);
        let (h1, l1) = self.range(0, half)?;
        let (h2, l2) = self.range(half, half)?;
        let (h3, l3) = self.range(0, n)?;
        let r1 = (h1 - l1) / half as f64;
        let r2 = (h2 - l2) / half as f64;
        let r3 = (h3 - l3) / n as f64;
        let d = if r1 + r2 > 0.0 && r3 > 0.0 {
            ((r1 + r2).ln() - r3.ln()) / 2f64.ln()
        } else {
            1.5
        };
        Some(d.clamp(1.0, 2.0))
    }

    fn efficiency(&self) -> Option<f64> {
        let n = self.cfg.len;
        let old = self.at(n - 1)?.close;
        let mut path = 0.0;
        for i in 0..n - 1 {
            path += (self.at(i)?.close - self.at(i + 1)?.close).abs();
        }
        let direct = (self.at(0)?.close - old).abs();
        Some(if path > 0.0 {
            (direct / path * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        })
    }

    fn chop(&self) -> Option<f64> {
        let n = self.cfg.chop_len;
        let (hi, lo) = self.range(0, n)?;
        let mut sum = 0.0;
        for k in (0..n).rev() {
            sum += self.at(k)?.tr;
        }
        let rng = hi - lo;
        Some(if rng > 0.0 {
            (100.0 * (sum / rng).log10() / (n as f64).log10()).clamp(0.0, 100.0)
        } else {
            0.0
        })
    }

    /// Mean of `f` over the 3 bars ending one bar back (oldest first).
    fn prev_sma3(&self, f: impl Fn(&Row) -> Option<f64>) -> Option<f64> {
        let mut sum = 0.0;
        for k in (1..=3).rev() {
            sum += f(self.at(k)?)?;
        }
        Some(sum / 3.0)
    }
}

impl Indicator for CoilRelease {
    fn name(&self) -> &str {
        "coil_release"
    }

    fn warmup_period(&self) -> usize {
        13 + self.cfg.len - 2 + self.cfg.break_len
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
        let ema = self.ema.update(bar.close).unwrap();
        let band = self.range(0, c.break_len);
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            tr,
            atr,
            ema,
            ..Row::default()
        });
        if self.rows.len() > self.cap {
            self.rows.pop_front();
        }
        self.bars += 1;
        let t = self.bars - 1;

        let complexity = self.complexity();
        let fd = self.fractal_dim();
        let eff = self.efficiency();
        let chop = self.chop();
        let coil = match (complexity, fd, eff) {
            (Some(cm), Some(f), Some(e)) => {
                Some((cm / 100.0 * 0.5 + (f - 1.0) * 0.3 + (1.0 - e / 100.0) * 0.2).clamp(0.0, 1.0))
            }
            _ => None,
        };
        let (break_up, break_dn) = match band {
            Some((h, l)) => (Some(bar.close > h), Some(bar.close < l)),
            None => (None, None),
        };
        let prev = self.at(1).copied();
        {
            let r = self.rows.back_mut().unwrap();
            r.complexity = complexity;
            r.eff = eff;
            r.coil = coil;
            r.break_up = break_up;
            r.break_dn = break_dn;
        }
        let trend = match (self.at(c.slope_len), atr) {
            (Some(p), Some(a)) => {
                let s = if a > 0.0 { (ema - p.ema) / a } else { 0.0 };
                Some(if s > c.slope_min {
                    1
                } else if s < -c.slope_min {
                    -1
                } else {
                    0
                })
            }
            _ => None,
        };

        let mut coil_recent = None;
        let mut base_bars = Some(0usize);
        for k in 1..=c.break_len {
            let r = self.at(k);
            let cv = r.and_then(|r| r.coil);
            coil_recent = match (k, coil_recent, cv) {
                (1, _, Some(v)) => Some(v),
                (_, Some(m), Some(v)) => Some(f64::max(m, v)),
                _ => None,
            };
            base_bars = match (base_bars, r.and_then(|r| r.complexity.zip(r.eff))) {
                (Some(b), Some((cm, e))) => {
                    Some(b + usize::from(cm > c.complex_high && e < c.eff_low))
                }
                _ => None,
            };
            if coil_recent.is_none() && base_bars.is_none() {
                break;
            }
        }
        let cmp_sma = self.prev_sma3(|r| r.complexity);
        let eff_sma = self.prev_sma3(|r| r.eff);

        let (
            Some(complexity),
            Some(fd),
            Some(eff),
            Some(chop),
            Some(trend),
            Some(coil),
            Some(coil_recent),
            Some(cmp_sma),
            Some(eff_sma),
            Some((corr_h, corr_l)),
            Some(atr),
            Some(prev),
            Some(base_bars),
        ) = (
            complexity,
            fd,
            eff,
            chop,
            trend,
            coil,
            coil_recent,
            cmp_sma,
            eff_sma,
            band,
            atr,
            prev,
            base_bars,
        )
        else {
            return None;
        };
        let (Some(prev_up), Some(prev_dn)) = (prev.break_up, prev.break_dn) else {
            return None;
        };

        let blend = (coil_recent * 0.70 + coil * 0.30).clamp(0.0, 1.0);
        let coil_score = if coil >= c.coil_now_min {
            blend
        } else {
            blend * (coil / c.coil_now_min.max(0.01)).clamp(0.0, 1.0)
        };
        let drop = ((cmp_sma - complexity) / 8.0).clamp(0.0, 1.0);
        let gain = ((eff - eff_sma) / 8.0).clamp(0.0, 1.0);
        let dyn_ = (drop + gain) / 2.0;
        let setup = (coil_score * (c.rel_floor + (1.0 - c.rel_floor) * dyn_)).clamp(0.0, 1.0);
        let was_base = base_bars as f64 >= (c.break_len as f64 * c.base_share).round();
        let first_up = bar.close > corr_h && !prev_up;
        let first_dn = bar.close < corr_l && !prev_dn;
        let impulse = if first_up {
            if atr > 0.0 {
                (bar.close - corr_h).abs() / atr
            } else {
                0.0
            }
        } else if first_dn {
            if atr > 0.0 {
                (bar.close - corr_l).abs() / atr
            } else {
                0.0
            }
        } else {
            0.0
        };
        let quality = chop < c.chop_veto && impulse >= c.break_atr_min;
        let up_ok = self.last_up.is_none_or(|l| t - l >= c.cooldown);
        let dn_ok = self.last_dn.is_none_or(|l| t - l >= c.cooldown);
        let rel_l = first_up && trend == 1 && setup >= c.thr_cont && quality && up_ok;
        let rel_s = first_dn && trend == -1 && setup >= c.thr_cont && quality && dn_ok;
        let bb_l = first_up && trend == -1 && was_base && setup >= c.thr_base && quality && up_ok;
        let bb_s = first_dn && trend == 1 && was_base && setup >= c.thr_base && quality && dn_ok;
        for (hit, kind) in [
            (rel_l, "bull_release"),
            (bb_l, "bull_base_break"),
            (rel_s, "bear_release"),
            (bb_s, "bear_base_break"),
        ] {
            if hit {
                self.alerts.push(IndicatorAlert::new(
                    kind,
                    format!(
                        "Coil Release {kind}: setup {setup:.2}, coil {coil:.2}, chop {chop:.1}"
                    ),
                    setup.clamp(0.05, 1.0),
                ));
            }
        }
        if rel_l || bb_l {
            self.last_up = Some(t);
        }
        if rel_s || bb_s {
            self.last_dn = Some(t);
        }

        let fd_score = (fd - 1.0) * 100.0;
        let base = complexity > c.complex_high && eff < c.eff_low;
        let state = if complexity < c.complex_low && fd_score < c.complex_low && eff > c.eff_high {
            "clean_trend"
        } else if trend != 0 && !base && coil > 0.55 {
            "correction"
        } else if base {
            "base"
        } else {
            "mixed"
        };
        let mut extra = HashMap::new();
        for (k, v) in [
            ("complexity", complexity),
            ("fractal_dim", fd),
            ("efficiency", eff),
            ("chop", chop),
            ("coil", coil),
            ("coil_score", coil_score),
            ("release_dyn", dyn_),
            ("trend", f64::from(trend)),
            ("corr_high", corr_h),
            ("corr_low", corr_l),
            ("was_base", if was_base { 1.0 } else { 0.0 }),
        ] {
            extra.insert(k.to_string(), v);
        }
        Some(IndicatorOutput::with_extra(setup, extra).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
