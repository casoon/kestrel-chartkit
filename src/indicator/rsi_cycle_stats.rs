//! RSI Cycle Statistics: an explicit RSI cycle state machine and empirical, sample-size-
//! qualified completion frequencies from past cycles.
//!
//! A statistics engine, not a trading signal — but every number at a bar uses only cycles
//! completed before it, so it streams without looking ahead.
//!
//! Cycle model: a fresh entry into overbought starts an OB → OS cycle (oversold mirrored). Once
//! the oscillator has left the origin zone the cycle is armed. Reaching the opposite zone is a
//! success (and starts the opposite cycle), returning to the origin zone while armed is a
//! failure (and starts a new attempt from there), `max_cycle` bars without either is a timeout
//! (and the machine waits for a fresh zone entry). Each completed cycle is stored (at most
//! `max_samples`): direction, outcome, duration, end bar, peak progress and the first age at
//! which each 10-point progress bucket was reached.
//!
//! Conditional frequency for the live cycle: among same-direction cycles that ended within
//! `lookback` bars, had reached the cycle's progress bucket by its current age and were still
//! unresolved after that age, the share that completed. Edge = conditional − the direction's
//! plain base rate over the same lookback; Wilson interval (z = 1.96) for the sample.
//!
//! `value` = RSI; `extra`: `cycle_dir` (−1 OB → OS, +1 OS → OB, 0 idle) and, during a cycle,
//! `progress`, `max_progress`, `age`, `cond_n`, plus `conditional`, `base_rate`, `edge`,
//! `ci_low`, `ci_high` and `median_duration` when they exist. `state`: `idle` or the live label
//! word `low_data`, `stalled`, `uncertain`, `late`, `opportunity`, `watch`, `wait` (in
//! priority order, with the gates of the opportunity marker).
//!
//! Alerts (closed bars): `bull_cycle_opportunity` / `bear_cycle_opportunity` — at most once per
//! cycle, at a progress-bucket crossing inside `zone_min`..`zone_max` %, when the conditional
//! sample has at least `min_samples` cycles, the edge reaches `edge_th` points and the Wilson
//! width is at most `max_ci` points; the prefix is the cycle direction (up cycle = expected rise
//! of the oscillator toward overbought). `cycle_success`, `cycle_failure`, `cycle_timeout` —
//! the raw cycle outcomes, without direction.
//!
//! Not ported: the other ten oscillators, the regime-conditioned statistics, the
//! recent/long lookback comparison, MFE/MAE distributions, the walk-forward reliability table
//! and all drawings.
//!
//! Conventions the original leaves open (also in the golden fixture): RSI Wilder averages seeded
//! with an SMA; the first RSI bar counts as entering a zone it is in; the live state word uses the
//! progress of the cycle active after the bar (the original reuses the pre-transition value on a
//! bar where the cycle changes). A bar reports once the RSI exists.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/rsi_cycle_stats.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

const BUCKETS: usize = 10;

/// Parameters, defaults of the original (RSI mode).
#[derive(Debug, Clone)]
pub struct RsiCycleStatsConfig {
    pub rsi_len: usize,
    pub overbought: f64,
    pub oversold: f64,
    pub max_cycle: usize,
    pub lookback: usize,
    pub max_samples: usize,
    pub min_samples: usize,
    pub edge_th: f64,
    pub zone_min: f64,
    pub zone_max: f64,
    pub max_ci: f64,
}

impl Default for RsiCycleStatsConfig {
    fn default() -> Self {
        Self {
            rsi_len: 14,
            overbought: 70.0,
            oversold: 30.0,
            max_cycle: 150,
            lookback: 1000,
            max_samples: 1500,
            min_samples: 20,
            edge_th: 15.0,
            zone_min: 30.0,
            zone_max: 65.0,
            max_ci: 40.0,
        }
    }
}

#[derive(Debug, Clone)]
struct Sample {
    dir: i8,
    ok: bool,
    duration: usize,
    end: usize,
    max_progress: f64,
    reach: [i64; BUCKETS],
}

#[derive(Debug, Clone)]
struct Cycle {
    dir: i8,
    start: usize,
    armed: bool,
    max_progress: f64,
    reach: [i64; BUCKETS],
    marked: bool,
}

impl Cycle {
    fn new(dir: i8, start: usize) -> Self {
        Self {
            dir,
            start,
            armed: false,
            max_progress: 0.0,
            reach: [-1; BUCKETS],
            marked: false,
        }
    }
}

struct Conditional {
    success: usize,
    total: usize,
    median_duration: Option<f64>,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[m]
    } else {
        (v[m - 1] + v[m]) * 0.5
    })
}

fn wilson(success: usize, total: usize) -> Option<(f64, f64)> {
    if total == 0 {
        return None;
    }
    let z = 1.96f64;
    let n = total as f64;
    let p = success as f64 / n;
    let z2 = z * z;
    let center = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let margin = z / (1.0 + z2 / n) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    Some((
        100.0 * (center - margin).max(0.0),
        100.0 * (center + margin).min(1.0),
    ))
}

/// Streaming RSI Cycle Statistics; see the module documentation.
pub struct RsiCycleStats {
    cfg: RsiCycleStatsConfig,
    up: Rma,
    down: Rma,
    prev_close: Option<f64>,
    prev_osc: Option<f64>,
    samples: VecDeque<Sample>,
    cycle: Option<Cycle>,
    bars: usize,
    alerts: Vec<IndicatorAlert>,
}

impl RsiCycleStats {
    pub fn new(cfg: RsiCycleStatsConfig) -> Self {
        Self {
            up: Rma::new(cfg.rsi_len),
            down: Rma::new(cfg.rsi_len),
            prev_close: None,
            prev_osc: None,
            samples: VecDeque::new(),
            cycle: None,
            bars: 0,
            alerts: Vec::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &RsiCycleStatsConfig {
        &self.cfg
    }

    fn progress(&self, dir: i8, v: f64) -> f64 {
        let (ob, os) = (self.cfg.overbought, self.cfg.oversold);
        if dir == -1 {
            ((ob - v) / (ob - os)).clamp(0.0, 1.0)
        } else {
            ((v - os) / (ob - os)).clamp(0.0, 1.0)
        }
    }

    fn conditional(&self, t: usize, dir: i8, progress: f64, age: usize) -> Conditional {
        let bucket = (progress.clamp(0.0, 1.0) * BUCKETS as f64 + 0.000001).floor() as usize;
        let bucket_progress = bucket as f64 / BUCKETS as f64;
        let (mut success, mut total) = (0, 0);
        let mut durations = Vec::new();
        for s in &self.samples {
            if t - s.end > self.cfg.lookback || s.dir != dir {
                continue;
            }
            let reached =
                bucket == 0 || (s.reach[bucket - 1] >= 0 && s.reach[bucket - 1] <= age as i64);
            if s.max_progress >= bucket_progress && s.duration > age && reached {
                total += 1;
                if s.ok {
                    success += 1;
                    durations.push(s.duration as f64);
                }
            }
        }
        Conditional {
            success,
            total,
            median_duration: median(durations),
        }
    }

    fn base_rate(&self, t: usize, dir: i8) -> Option<f64> {
        let (mut success, mut total) = (0usize, 0usize);
        for s in &self.samples {
            if t - s.end <= self.cfg.lookback && s.dir == dir {
                success += usize::from(s.ok);
                total += 1;
            }
        }
        (total > 0).then(|| 100.0 * success as f64 / total as f64)
    }

    fn store(&mut self, cycle: &Cycle, ok: bool, t: usize) {
        self.samples.push_back(Sample {
            dir: cycle.dir,
            ok,
            duration: t - cycle.start,
            end: t,
            max_progress: cycle.max_progress,
            reach: cycle.reach,
        });
        if self.samples.len() > self.cfg.max_samples {
            self.samples.pop_front();
        }
    }

    fn push_alert(&mut self, kind: &str, note: String, strength: f64) {
        self.alerts.push(IndicatorAlert::new(kind, note, strength));
    }
}

impl Indicator for RsiCycleStats {
    fn name(&self) -> &str {
        "rsi_cycle_stats"
    }

    fn warmup_period(&self) -> usize {
        self.cfg.rsi_len
    }

    fn reset(&mut self) {
        *self = Self::new(self.cfg.clone());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let c = self.cfg.clone();
        let t = self.bars;
        self.bars += 1;
        let change = self.prev_close.map(|p| bar.close - p);
        self.prev_close = Some(bar.close);
        let rsi = change.and_then(|d| {
            let u = self.up.update(d.max(0.0));
            let dn = self.down.update((-d).max(0.0));
            let (u, dn) = (u?, dn?);
            Some(if dn == 0.0 {
                100.0
            } else if u == 0.0 {
                0.0
            } else {
                100.0 - 100.0 / (1.0 + u / dn)
            })
        });
        let v = rsi?;
        let (ob, os) = (c.overbought, c.oversold);
        let pv = self.prev_osc.replace(v);
        let entered_ob = v >= ob && pv.unwrap_or(ob - 1.0) < ob;
        let entered_os = v <= os && pv.unwrap_or(os + 1.0) > os;

        // Progress buckets and the opportunity check run on the cycle as it entered the bar.
        if let Some(mut cy) = self.cycle.take() {
            let prog = self.progress(cy.dir, v);
            let age = t - cy.start;
            let target = (cy.dir == -1 && v <= os) || (cy.dir == 1 && v >= ob);
            let timeout = age >= c.max_cycle;
            if prog > cy.max_progress {
                cy.max_progress = prog;
            }
            for b in 0..BUCKETS {
                let th = (b as f64 + 1.0) / BUCKETS as f64;
                if prog >= th && cy.reach[b] < 0 {
                    cy.reach[b] = age as i64;
                    if !cy.marked
                        && !target
                        && !timeout
                        && th >= c.zone_min / 100.0
                        && th <= c.zone_max / 100.0
                    {
                        let cond = self.conditional(t, cy.dir, th, age);
                        let p = (cond.total > 0)
                            .then(|| 100.0 * cond.success as f64 / cond.total as f64);
                        let edge = p.zip(self.base_rate(t, cy.dir)).map(|(p, b)| p - b);
                        let width = wilson(cond.success, cond.total).map(|(l, u)| u - l);
                        if cond.total >= c.min_samples
                            && edge.is_some_and(|e| e >= c.edge_th)
                            && width.is_some_and(|w| w <= c.max_ci)
                        {
                            cy.marked = true;
                            let kind = if cy.dir == 1 {
                                "bull_cycle_opportunity"
                            } else {
                                "bear_cycle_opportunity"
                            };
                            let e = edge.unwrap();
                            self.push_alert(
                                kind,
                                format!(
                                    "Cycle {kind}: conditional {:.1} % (n {}), edge {e:+.1} pp",
                                    p.unwrap(),
                                    cond.total
                                ),
                                (e / 100.0).clamp(0.05, 1.0),
                            );
                        }
                    }
                }
            }
            self.cycle = Some(cy);
        }

        // State machine.
        match self.cycle.take() {
            None => {
                if entered_ob {
                    self.cycle = Some(Cycle::new(-1, t));
                } else if entered_os {
                    self.cycle = Some(Cycle::new(1, t));
                }
            }
            Some(mut cy) => {
                let d = cy.dir;
                let age = t - cy.start;
                if !cy.armed && ((d == -1 && v < ob) || (d == 1 && v > os)) {
                    cy.armed = true;
                }
                if (d == -1 && v <= os) || (d == 1 && v >= ob) {
                    self.store(&cy, true, t);
                    self.push_alert(
                        "cycle_success",
                        format!("Cycle success after {age} bars"),
                        0.5,
                    );
                    self.cycle = Some(Cycle::new(-d, t));
                } else if cy.armed && ((d == -1 && v >= ob) || (d == 1 && v <= os)) {
                    self.store(&cy, false, t);
                    self.push_alert(
                        "cycle_failure",
                        format!("Cycle failure after {age} bars"),
                        0.5,
                    );
                    self.cycle = Some(Cycle::new(d, t));
                } else if age >= c.max_cycle {
                    self.store(&cy, false, t);
                    self.push_alert(
                        "cycle_timeout",
                        format!("Cycle timeout after {age} bars"),
                        0.5,
                    );
                } else {
                    self.cycle = Some(cy);
                }
            }
        }

        let mut extra = HashMap::new();
        let Some(cy) = self.cycle.as_ref() else {
            extra.insert("cycle_dir".to_string(), 0.0);
            return Some(IndicatorOutput::with_extra(v, extra).with_state("idle"));
        };
        let age = t - cy.start;
        let cond = self.conditional(t, cy.dir, cy.max_progress, age);
        let p = (cond.total > 0).then(|| 100.0 * cond.success as f64 / cond.total as f64);
        let base = self.base_rate(t, cy.dir);
        let edge = p.zip(base).map(|(p, b)| p - b);
        let ci = wilson(cond.success, cond.total);
        let width = ci.map(|(l, u)| u - l);
        let cur = self.progress(cy.dir, v);
        let in_zone = cur >= c.zone_min / 100.0 && cur <= c.zone_max / 100.0;
        let word = match edge {
            Some(e) if cond.total >= c.min_samples => {
                if cond.median_duration.is_some_and(|m| age as f64 > m) {
                    "stalled"
                } else if width.is_none_or(|w| w > c.max_ci) {
                    "uncertain"
                } else if cur > c.zone_max / 100.0 {
                    "late"
                } else if e >= c.edge_th && in_zone {
                    "opportunity"
                } else if e >= c.edge_th / 2.0 {
                    "watch"
                } else {
                    "wait"
                }
            }
            _ => "low_data",
        };
        extra.insert("cycle_dir".to_string(), f64::from(cy.dir));
        extra.insert("progress".to_string(), cur);
        extra.insert("max_progress".to_string(), cy.max_progress);
        extra.insert("age".to_string(), age as f64);
        extra.insert("cond_n".to_string(), cond.total as f64);
        let optional = [
            ("conditional", p),
            ("base_rate", base),
            ("edge", edge),
            ("ci_low", ci.map(|x| x.0)),
            ("ci_high", ci.map(|x| x.1)),
            ("median_duration", cond.median_duration),
        ];
        for (k, val) in optional {
            if let Some(x) = val {
                extra.insert(k.to_string(), x);
            }
        }
        Some(IndicatorOutput::with_extra(v, extra).with_state(word))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
