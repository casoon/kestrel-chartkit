//! Structural Wave Field: multi-horizon structural flow, the internal wave inside it, wave
//! segmentation, maturity, re-synchronization, exhaustion and a nine-phase read.
//!
//! Port of the computation of the Pine Script "Structural Wave Field" (WavesUnchained, v1.0.2)
//! with the "Balanced" preset and "Relative Volume" participation; the gradient field, labels
//! and dashboard (drawing) are not ported.
//!
//! **Structural flow.** Three horizons (24, 48, 96 bars) each score `0.35 · displacement` (net
//! move over 3 ATR, squashed `x / (1 + |x|)`) `+ 0.30 · signed efficiency + 0.20 · persistence`
//! (directional path share, each bar's move measured against that bar's own net direction)
//! `+ 0.15 · location` (close against the horizon EMA in ATR, squashed). The drive `0.20 fast +
//! 0.35 medium + 0.45 slow` is scaled by horizon sign agreement; the direction enters at ±0.34,
//! holds above ±0.14 (else transition) and flips at ∓0.42. Confidence = agreement × (1 - 0.65 ·
//! horizon spread).
//!
//! **Internal wave.** Momentum (10-bar move over 1.75 ATR), EMA-10 slope and candle impulse (EMA
//! 5 of close - open over 0.65 ATR), each squashed, blend into a raw momentum, EMA 5 of it is the
//! core; velocity and acceleration are squashed 2-bar differences ×3. Internal wave =
//! `0.55 core + 0.28 velocity + 0.17 acceleration`, aligned wave = internal wave × direction.
//! Wave quality (12 bars) blends efficiency, persistence and smoothness; counter pressure grows
//! with the counter-aligned energy, quality, retracement from the leg extreme and counter
//! persistence; structural integrity falls with counter pressure, retracement and stress.
//!
//! **Segments.** An aligned (counter) segment starts when the aligned wave reaches +0.18
//! (−0.18) and ends at the opposite threshold or when the energy falls to 0.08; segments of at
//! least 2 bars are archived (polarity, duration, displacement, peak, energy integral, quality,
//! efficiency, participation, retracement; 30 kept) — the archive and the counters reset with each
//! flow change. Structural legs (duration, displacement, cycles) are archived on each flow change
//! (30 kept); their medians normalize maturity: age, extension, cycles, multi-metric wave decay
//! against the previous aligned segment, efficiency loss and price/pressure mismatch.
//!
//! **Re-Sync, exhaustion, phases.** A completed, qualified counter segment (≥ 3 context bars,
//! counter peak ≥ 0.24 or retracement ≥ 0.10) is owned by the next confirmed Re-Sync (aligned
//! wave > 0.16, velocity recovering, integrity > 0.5, strength > 0.22, confidence > 0.35, quality
//! ≥ 0.38, 15-bar cooldown); stages 1 counter decay, 2 neutralizing, 3 re-engaging, 4 confirmed.
//! Exhaustion (from maturity 0.58) combines decay evidence and continuation failure (mismatch,
//! failed expansion: an aligned segment ending below 62 % of the previous peak and 70 % of its
//! displacement). Five-segment impulse / three-segment correction contexts are quality gated
//! (≥ 0.44). The phase (0 transition, 1 initiation, 2 acceleration, 3 expansion, 4 correction, 5
//! compression, 6 re-sync, 7 late expansion, 8 exhaustion, 9 flow flip) changes after two bars
//! of persistence, a flip and a transition immediately.
//!
//! `value`: the structural drive; `secondary`: the aligned wave. `extra`: `direction`,
//! `strength`, `confidence`, `internal_wave`, `wave_quality`, `counter_pressure`, `integrity`,
//! `retracement`, `maturity`, `compression`, `resync_quality`, `resync_stage`, `exhaustion`,
//! `deterioration`, `wave_decay`, `impulse_context`, `correction_context`, `wave_context` (0
//! unclassified, 1 developing, 2 three-segment correction, 3 five-segment impulse, 4 dominant
//! middle expansion, 5 late impulse), `phase`, `phase_confidence`. `state`: the phase name.
//!
//! Alerts (closed bars; prefix = expected move = the flow direction, warnings without one):
//! `bull_flow_initiated` / `bear_flow_initiated`, `flow_transition`, `bull_flow_flip` /
//! `bear_flow_flip`, `bull_expansion` / `bear_expansion` (expansion phase entered),
//! `correction_start`, `compression`, `bull_early_resync` / `bear_early_resync` (stage 1
//! entered), `bull_resync` / `bear_resync` (confirmed), `late_expansion`, `exhaustion` (phase
//! entered), `failed_expansion`, `bull_dominant_expansion` / `bear_dominant_expansion` (an aligned
//! segment ending above 120 % of the previous peak), `weakening_waves` (latest aligned peak below
//! 80 % of the previous in a mature leg).
//!
//! Conventions where Pine leaves them open (also in the golden fixture): EMAs seeded with the
//! first available value; ATR = Wilder average of the true range (first bar high - low),
//! SMA-seeded, floored at `1e-12` (also for the mintick floors); population standard deviation;
//! Pine's `array.median`; relative volume 1 while the 20-bar volume average is missing or 0.
//! Every series starts once its inputs exist (Pine's safe division falls back to 0 on a missing
//! ATR during the warm-up); the state machines and output start with the first bar on which the
//! slow horizon's persistence exists (bar 191), before which Pine's flow direction stays 0 and
//! every flow-dependent quantity is inert: previous-bar values from before that bar are the
//! resting values (phase 0, Re-Sync stage 0, counter pressure 0), the previous structural
//! strength is missing (Pine compares against `na`, false).
//!
//! Golden reference: `reference/kestrel_reference/fixtures/structural_wave_field.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

const LENS: [usize; 3] = [24, 48, 96];
const ENTRY: f64 = 0.34;
const HOLD: f64 = 0.14;
const FLIP: f64 = 0.42;
const DISP_TARGET: f64 = 3.0;
const WAVE_LEN: usize = 10;
const VEL_LAG: usize = 2;
const ACC_LAG: usize = 2;
const QUALITY_LEN: usize = 12;
const WAVE_START: f64 = 0.18;
const WAVE_END: f64 = 0.08;
const MIN_WAVE_BARS: usize = 2;
const RECOVERY_LEN: usize = 8;
const CONTEXT_MIN: f64 = 0.44;
const RS_MIN_BARS: usize = 3;
const RS_MIN_PEAK: f64 = 0.24;
const RS_MIN_RET: f64 = 0.10;
const RS_MIN_QUALITY: f64 = 0.38;
const RS_COOLDOWN: usize = 15;
const RS_CONFIRM: f64 = 0.16;
const MEMORY: usize = 30;
const EXH_MATURITY: f64 = 0.58;
const FAILED_PEAK: f64 = 0.62;
const FAILED_DISP: f64 = 0.70;
const PART_WEIGHT: f64 = 0.10;
const PHASE_CONFIRM: usize = 2;
const HISTORY: usize = 2 * 96 + 2;

const PHASES: [&str; 10] = [
    "transition",
    "initiation",
    "acceleration",
    "expansion",
    "correction",
    "compression",
    "resync",
    "late_expansion",
    "exhaustion",
    "flow_flip",
];

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

fn safe_div(n: f64, d: f64, fallback: f64) -> f64 {
    if d.abs() > 1e-10 {
        n / d
    } else {
        fallback
    }
}

fn squash(x: f64) -> f64 {
    x / (1.0 + x.abs())
}

fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn decay(current: f64, reference: f64, floor: f64) -> f64 {
    clamp(1.0 - safe_div(current, reference.max(floor), 1.0), 0.0, 1.0)
}

fn median(a: &VecDeque<f64>, fallback: f64) -> f64 {
    if a.is_empty() {
        return fallback;
    }
    let mut v: Vec<f64> = a.iter().copied().collect();
    v.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn push<T>(a: &mut VecDeque<T>, v: T, max: usize) {
    a.push_back(v);
    if a.len() > max {
        a.pop_front();
    }
}

/// One archived wave segment.
#[derive(Debug, Clone, Copy)]
struct Segment {
    polarity: i8,
    duration: f64,
    displacement: f64,
    peak: f64,
    integral: f64,
    quality: f64,
    efficiency: f64,
    participation: f64,
    retracement: f64,
}

/// The `nth` newest archived segment of `polarity`.
fn nth(a: &VecDeque<Segment>, polarity: i8, n: usize) -> Option<Segment> {
    a.iter()
        .rev()
        .filter(|s| s.polarity == polarity)
        .nth(n)
        .copied()
}

/// One horizon: its EMA and the per-bar directional-path terms.
struct Horizon {
    len: usize,
    ema: Ema,
    terms: VecDeque<Option<f64>>,
}

/// The running (open) segment.
#[derive(Debug, Clone, Copy, Default)]
struct OpenSegment {
    polarity: i8,
    start_bar: usize,
    start_price: f64,
    peak: f64,
    integral: f64,
    quality_sum: f64,
    efficiency_sum: f64,
    participation_sum: f64,
    max_retracement: f64,
    max_move: f64,
    last_atr: Option<f64>,
}

/// Re-Sync bookkeeping.
#[derive(Debug, Clone, Copy)]
struct Resync {
    seen: bool,
    stage: u8,
    hold: usize,
    context_bars: usize,
    context_start: Option<usize>,
    peak_pressure: f64,
    peak_retracement: f64,
    last_bar: Option<usize>,
    pending_serial: Option<usize>,
    consumed_serial: Option<usize>,
    pending_end: Option<usize>,
}

impl Default for Resync {
    fn default() -> Self {
        Self {
            seen: false,
            stage: 0,
            hold: 99,
            context_bars: 0,
            context_start: None,
            peak_pressure: 0.0,
            peak_retracement: 0.0,
            last_bar: None,
            pending_serial: None,
            consumed_serial: None,
            pending_end: None,
        }
    }
}

impl Resync {
    fn clear_context(&mut self) {
        self.seen = false;
        self.context_bars = 0;
        self.context_start = None;
        self.peak_pressure = 0.0;
        self.peak_retracement = 0.0;
    }
}

/// Previous-bar values of the core section; before the first core bar the state machines rest
/// (phase 0, stage 0, no weakening) and the strength is missing.
#[derive(Debug, Clone, Copy, Default)]
struct Prev {
    strength: Option<f64>,
    phase: usize,
    resync_stage: u8,
    weakening: bool,
}

/// Streaming Structural Wave Field; see the module documentation.
pub struct StructuralWaveField {
    atr: Rma,
    closes: VecDeque<f64>,
    changes: VecDeque<Option<f64>>,
    horizons: Vec<Horizon>,
    ema_wave: Ema,
    prev_ema_wave: Option<f64>,
    ema_body: Ema,
    momentum: Ema,
    momentum_hist: VecDeque<Option<f64>>,
    velocity_hist: VecDeque<Option<f64>>,
    energy_prev: Option<f64>,
    signed_moves: VecDeque<Option<f64>>,
    vol_base: Sma,
    part_ema: Ema,
    part_prev: Option<f64>,
    eff_ema: Ema,
    eff_prev: Option<f64>,
    ppe_ema: Ema,
    ppe_prev: Option<f64>,
    recovery_ema: Ema,
    counter_hist: VecDeque<f64>,
    cum_volume: f64,
    bars: usize,
    // Core state.
    dir: i8,
    leg_dur: VecDeque<f64>,
    leg_disp: VecDeque<f64>,
    leg_cycles: VecDeque<f64>,
    leg_start_bar: Option<usize>,
    leg_start_price: Option<f64>,
    leg_extreme: Option<f64>,
    leg_atr_sum: f64,
    leg_atr_bars: usize,
    leg_aligned: usize,
    leg_corrections: usize,
    waves: VecDeque<Segment>,
    seg: OpenSegment,
    serial: usize,
    rs: Resync,
    last_impulse_serial: Option<usize>,
    last_correction_serial: Option<usize>,
    phase: usize,
    pending_phase: usize,
    pending_bars: usize,
    prev: Prev,
    alerts: Vec<IndicatorAlert>,
}

impl Default for StructuralWaveField {
    fn default() -> Self {
        Self::new()
    }
}

impl StructuralWaveField {
    pub fn new() -> Self {
        Self {
            atr: Rma::new(14),
            closes: VecDeque::with_capacity(HISTORY + 1),
            changes: VecDeque::with_capacity(HISTORY + 1),
            horizons: LENS
                .iter()
                .map(|&len| Horizon {
                    len,
                    ema: Ema::new(len),
                    terms: VecDeque::with_capacity(len + 1),
                })
                .collect(),
            ema_wave: Ema::new(WAVE_LEN),
            prev_ema_wave: None,
            ema_body: Ema::new(5),
            momentum: Ema::new(5),
            momentum_hist: VecDeque::with_capacity(VEL_LAG + 2),
            velocity_hist: VecDeque::with_capacity(ACC_LAG + 2),
            energy_prev: None,
            signed_moves: VecDeque::with_capacity(QUALITY_LEN + 1),
            vol_base: Sma::new(20),
            part_ema: Ema::new(5),
            part_prev: None,
            eff_ema: Ema::new(5),
            eff_prev: None,
            ppe_ema: Ema::new(5),
            ppe_prev: None,
            recovery_ema: Ema::new(RECOVERY_LEN),
            counter_hist: VecDeque::with_capacity(VEL_LAG + 2),
            cum_volume: 0.0,
            bars: 0,
            dir: 0,
            leg_dur: VecDeque::new(),
            leg_disp: VecDeque::new(),
            leg_cycles: VecDeque::new(),
            leg_start_bar: None,
            leg_start_price: None,
            leg_extreme: None,
            leg_atr_sum: 0.0,
            leg_atr_bars: 0,
            leg_aligned: 0,
            leg_corrections: 0,
            waves: VecDeque::new(),
            seg: OpenSegment::default(),
            serial: 0,
            rs: Resync::default(),
            last_impulse_serial: None,
            last_correction_serial: None,
            phase: 0,
            pending_phase: 0,
            pending_bars: 0,
            prev: Prev::default(),
            alerts: Vec::new(),
        }
    }

    /// `close[k]`, `None` before the series start.
    fn close_at(&self, k: usize) -> Option<f64> {
        self.closes.len().checked_sub(k + 1).map(|i| self.closes[i])
    }

    /// Sum of the last `len` changes mapped by `f`, `None` unless all exist.
    fn change_sum(&self, len: usize, f: impl Fn(f64) -> f64) -> Option<f64> {
        if self.changes.len() < len {
            return None;
        }
        let mut s = 0.0;
        for c in self.changes.iter().rev().take(len) {
            s += f((*c)?);
        }
        Some(s)
    }
}

impl Indicator for StructuralWaveField {
    fn name(&self) -> &str {
        "structural_wave_field"
    }

    fn warmup_period(&self) -> usize {
        HISTORY - 3
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let t = self.bars;
        self.bars += 1;
        let (o, h, l, c) = (bar.open, bar.high, bar.low, bar.close);
        let prev_close = self.closes.back().copied();
        let tr = match prev_close {
            Some(pc) => (h - l).max((h - pc).abs()).max((l - pc).abs()),
            None => h - l,
        };
        let atr = self.atr.update(tr).map(|a| a.max(1e-12));
        let change = prev_close.map(|pc| c - pc);
        push(&mut self.closes, c, HISTORY);
        push(&mut self.changes, change, HISTORY);

        // Horizon scores.
        let mut scores = [None; 3];
        for (i, score) in scores.iter_mut().enumerate() {
            let len = self.horizons[i].len;
            let ema_ref = self.horizons[i].ema.update(c).unwrap();
            let d = self.close_at(len).map(|p| c - p);
            let term = match (d, change) {
                (Some(d), Some(ch)) => Some((ch * sign(d)).max(0.0)),
                _ => None,
            };
            push(&mut self.horizons[i].terms, term, len);
            let path = self.change_sum(len, f64::abs);
            let hz = &self.horizons[i];
            let dir_path = if hz.terms.len() == len {
                hz.terms.iter().try_fold(0.0, |s, x| x.map(|v| s + v))
            } else {
                None
            };
            if let (Some(d), Some(path), Some(dir_path), Some(a)) = (d, path, dir_path, atr) {
                let eff = safe_div(d.abs(), path, 0.0);
                let disp = squash(safe_div(d, a * DISP_TARGET, 0.0));
                let persistence = safe_div(dir_path, path, 0.0) * sign(d);
                let location = squash(safe_div(c - ema_ref, a, 0.0));
                *score =
                    Some(0.35 * disp + 0.30 * sign(d) * eff + 0.20 * persistence + 0.15 * location);
            }
        }

        // Internal wave series.
        let ema_wave = self.ema_wave.update(c).unwrap();
        let prev_ema_wave = self.prev_ema_wave.replace(ema_wave);
        let body_ema = self.ema_body.update(c - o).unwrap();
        let raw_momentum = match (atr, self.close_at(WAVE_LEN), prev_ema_wave) {
            (Some(a), Some(cw), Some(pw)) => {
                let price_mom = squash(safe_div(c - cw, a * 1.75, 0.0));
                let slope = squash(safe_div((ema_wave - pw) * WAVE_LEN as f64, a * 1.25, 0.0));
                let impulse = squash(safe_div(body_ema, a * 0.65, 0.0));
                Some(0.52 * price_mom + 0.30 * slope + 0.18 * impulse)
            }
            _ => None,
        };
        let core = raw_momentum.map(|r| self.momentum.update(r).unwrap());
        push(&mut self.momentum_hist, core, VEL_LAG + 1);
        let velocity = match (core, self.momentum_hist.front().copied().flatten()) {
            (Some(m), Some(mp)) if self.momentum_hist.len() == VEL_LAG + 1 => {
                Some(squash((m - mp) * 3.0))
            }
            _ => None,
        };
        push(&mut self.velocity_hist, velocity, ACC_LAG + 1);
        let acceleration = match (velocity, self.velocity_hist.front().copied().flatten()) {
            (Some(v), Some(vp)) if self.velocity_hist.len() == ACC_LAG + 1 => {
                Some(squash((v - vp) * 3.0))
            }
            _ => None,
        };
        let internal = match (core, velocity, acceleration) {
            (Some(m), Some(v), Some(acc)) => {
                Some(clamp((0.55 * m + 0.28 * v + 0.17 * acc) / 1.0, -1.0, 1.0))
            }
            _ => None,
        };
        let energy = internal.map(f64::abs);
        let energy_prev = std::mem::replace(&mut self.energy_prev, energy);

        // Quality series (direction-independent parts).
        let q_path = self.change_sum(QUALITY_LEN, f64::abs);
        let q_eff = match (q_path, self.close_at(QUALITY_LEN)) {
            (Some(p), Some(cq)) => Some(clamp(safe_div((c - cq).abs(), p, 0.0), 0.0, 1.0)),
            _ => None,
        };
        let noise = if self.changes.len() >= QUALITY_LEN {
            let w: Option<Vec<f64>> = self
                .changes
                .iter()
                .rev()
                .take(QUALITY_LEN)
                .copied()
                .collect();
            w.map(|w| {
                let m = w.iter().sum::<f64>() / QUALITY_LEN as f64;
                (w.iter().map(|x| (x - m).powi(2)).sum::<f64>() / QUALITY_LEN as f64).sqrt()
            })
        } else {
            None
        };
        let eff_ema = q_eff.map(|e| self.eff_ema.update(e).unwrap());
        let eff_prev = std::mem::replace(&mut self.eff_prev, eff_ema);
        let efficiency_loss = match (eff_ema, eff_prev) {
            (Some(e), Some(p)) => Some(clamp((p - e) * 4.0, 0.0, 1.0)),
            _ => None,
        };

        // Participation (relative volume).
        self.cum_volume += bar.volume;
        let real = self.cum_volume > 0.0;
        let rel_vol = match self.vol_base.update(bar.volume) {
            Some(b) => safe_div(bar.volume, b, 1.0),
            None => 1.0,
        };
        let participation = clamp((rel_vol - 0.5) / 1.5, 0.0, 1.0);
        let part_ema = self.part_ema.update(participation).unwrap();
        let part_prev = self.part_prev.replace(part_ema);
        let participation_loss = if real {
            part_prev.map_or(0.0, |p| clamp(p - part_ema, 0.0, 1.0))
        } else {
            0.0
        };
        let recovery_ema = self.recovery_ema.update(c).unwrap();

        // Gate: the core runs once every input exists. Before, the flow direction is 0 and the
        // flow-dependent series rest at 0.
        let (
            Some(h_fast),
            Some(h_med),
            Some(h_slow),
            Some(a),
            Some(internal),
            Some(energy),
            Some(velocity),
            Some(acceleration),
            Some(q_eff),
            Some(noise),
            Some(efficiency_loss),
        ) = (
            scores[0],
            scores[1],
            scores[2],
            atr,
            internal,
            energy,
            velocity,
            acceleration,
            q_eff,
            noise,
            efficiency_loss,
        )
        else {
            push(&mut self.signed_moves, change.map(|_| 0.0), QUALITY_LEN);
            if change.is_some() && atr.is_some() {
                self.ppe_prev = Some(self.ppe_ema.update(0.0).unwrap());
            }
            if q_eff.is_some() && atr.is_some() {
                push(&mut self.counter_hist, 0.0, VEL_LAG);
            }
            return None;
        };

        // Structural flow.
        let raw_drive = 0.20 * h_fast + 0.35 * h_med + 0.45 * h_slow;
        let s = |x: f64| {
            if x > 0.05 {
                1
            } else if x < -0.05 {
                -1
            } else {
                0
            }
        };
        let (sf, sm, ss) = (s(h_fast), s(h_med), s(h_slow));
        let agreement = if sf == sm && sm == ss {
            1.0
        } else if sf == sm || sf == ss || sm == ss {
            0.68
        } else {
            0.34
        };
        let spread =
            ((h_fast - h_med).abs() + (h_med - h_slow).abs() + (h_fast - h_slow).abs()) / 3.0;
        let confidence = clamp(agreement * (1.0 - 0.65 * spread), 0.0, 1.0);
        let drive = raw_drive * (0.72 + 0.28 * agreement);
        let strength = clamp(drive.abs(), 0.0, 1.0);
        let prev_dir = self.dir;
        match self.dir {
            0 => {
                if drive > ENTRY {
                    self.dir = 1;
                } else if drive < -ENTRY {
                    self.dir = -1;
                }
            }
            1 => {
                if drive < -FLIP {
                    self.dir = -1;
                } else if drive < HOLD {
                    self.dir = 0;
                }
            }
            _ => {
                if drive > FLIP {
                    self.dir = 1;
                } else if drive > -HOLD {
                    self.dir = 0;
                }
            }
        }
        let dir = self.dir;
        let dirf = dir as f64;
        let flip = prev_dir != 0 && dir != 0 && dir != prev_dir;
        let exit = prev_dir != 0 && dir == 0;
        let initiated = prev_dir == 0 && dir != 0;
        let reset = flip || exit;

        // Structural legs.
        if initiated || (dir != 0 && self.leg_start_bar.is_none()) {
            self.leg_start_bar = Some(t);
            self.leg_start_price = Some(c);
            self.leg_extreme = Some(c);
            self.leg_atr_sum = 0.0;
            self.leg_atr_bars = 0;
            self.leg_aligned = 0;
            self.leg_corrections = 0;
        }
        if reset {
            if let (Some(sb), Some(sp), Some(ex)) =
                (self.leg_start_bar, self.leg_start_price, self.leg_extreme)
            {
                let dur = ((t - sb) as f64).max(1.0);
                let leg_atr = safe_div(self.leg_atr_sum, (self.leg_atr_bars as f64).max(1.0), a);
                push(&mut self.leg_dur, dur, MEMORY);
                push(
                    &mut self.leg_disp,
                    (ex - sp).abs() / leg_atr.max(1e-12),
                    MEMORY,
                );
                let cycles = self.leg_aligned as f64 + 0.5 * self.leg_corrections as f64;
                push(&mut self.leg_cycles, cycles.max(1.0), MEMORY);
                self.leg_start_bar = flip.then_some(t);
                self.leg_start_price = flip.then_some(c);
                self.leg_extreme = flip.then_some(c);
                self.leg_atr_sum = 0.0;
                self.leg_atr_bars = 0;
                self.leg_aligned = 0;
                self.leg_corrections = 0;
            }
        }
        if dir == 1 {
            self.leg_extreme = Some(self.leg_extreme.map_or(h, |x| x.max(h)));
        } else if dir == -1 {
            self.leg_extreme = Some(self.leg_extreme.map_or(l, |x| x.min(l)));
        }
        if dir != 0 && self.leg_start_bar.is_some() {
            self.leg_atr_sum += a;
            self.leg_atr_bars += 1;
        }
        let leg_range = match (self.leg_extreme, self.leg_start_price) {
            (Some(x), Some(p)) => Some((x - p).abs()),
            _ => None,
        };
        let retracement = match (dir, self.leg_extreme, leg_range) {
            (1, Some(x), Some(r)) => clamp(safe_div(x - c, r.max(a * 0.25), 0.0), 0.0, 1.5),
            (-1, Some(x), Some(r)) => clamp(safe_div(c - x, r.max(a * 0.25), 0.0), 0.0, 1.5),
            _ => 0.0,
        };

        // Wave quality, counter pressure, integrity.
        let aligned = if dir == 0 { 0.0 } else { internal * dirf };
        let aligned_acc = acceleration * dirf;
        let signed_move = change.map(|ch| ch * dirf);
        push(&mut self.signed_moves, signed_move, QUALITY_LEN);
        let moves: Vec<f64> = self
            .signed_moves
            .iter()
            .map(|m| m.expect("changes exist past the warm-up"))
            .collect();
        let aligned_path: f64 = moves.iter().map(|m| m.max(0.0)).sum();
        let counter_path: f64 = moves.iter().map(|m| (-m).max(0.0)).sum();
        let path_total = (aligned_path + counter_path).max(1e-12);
        let q_persistence = clamp(aligned_path.max(counter_path) / path_total, 0.0, 1.0);
        let q_smooth = clamp(1.0 - safe_div(noise, a * 0.90, 1.0), 0.0, 1.0);
        let quality = clamp(
            0.45 * q_eff + 0.30 * q_persistence + 0.25 * q_smooth,
            0.0,
            1.0,
        );
        let counter_energy = (-aligned).max(0.0);
        let counter_persistence = clamp(counter_path / path_total, 0.0, 1.0);
        let counter = clamp(
            counter_energy
                * (0.42
                    + 0.22 * quality
                    + 0.18 * clamp(retracement, 0.0, 1.0)
                    + 0.18 * counter_persistence),
            0.0,
            1.0,
        );
        let stress = clamp((-drive * dirf).max(0.0) * 2.0, 0.0, 1.0);
        let integrity = clamp(
            1.0 - (0.52 * counter + 0.30 * clamp(retracement, 0.0, 1.0) + 0.18 * stress),
            0.0,
            1.0,
        );
        let participation_active = real;

        // Segmentation.
        let prior_peak = nth(&self.waves, 1, 0).map(|s| s.peak);
        let prior_disp = nth(&self.waves, 1, 0).map(|s| s.displacement);
        let desired = if aligned >= WAVE_START {
            1
        } else if aligned <= -WAVE_START {
            -1
        } else {
            0
        };
        if reset {
            self.seg = OpenSegment::default();
            self.waves.clear();
        }
        let start = |polarity: i8| OpenSegment {
            polarity,
            start_bar: t,
            start_price: c,
            peak: energy,
            integral: energy,
            quality_sum: quality,
            efficiency_sum: q_eff,
            participation_sum: participation,
            max_retracement: retracement,
            max_move: 0.0,
            last_atr: Some(a),
        };
        let mut ended: Option<Segment> = None;
        if self.seg.polarity == 0 {
            if desired != 0 {
                self.seg = start(desired);
            }
        } else {
            let opposite = desired == -self.seg.polarity;
            let died = energy <= WAVE_END;
            if opposite || died {
                let sg = self.seg;
                let dur = (t - sg.start_bar).max(1);
                if dur >= MIN_WAVE_BARS {
                    let durf = dur as f64;
                    let seg = Segment {
                        polarity: sg.polarity,
                        duration: durf,
                        displacement: sg.max_move / sg.last_atr.unwrap_or(a).max(1e-12),
                        peak: sg.peak,
                        integral: sg.integral,
                        quality: safe_div(sg.quality_sum, durf, quality),
                        efficiency: safe_div(sg.efficiency_sum, durf, q_eff),
                        participation: safe_div(sg.participation_sum, durf, participation),
                        retracement: sg.max_retracement,
                    };
                    push(&mut self.waves, seg, MEMORY);
                    ended = Some(seg);
                }
                self.seg = if opposite {
                    start(desired)
                } else {
                    OpenSegment::default()
                };
            } else {
                let sd = dirf * self.seg.polarity as f64;
                let sg = &mut self.seg;
                sg.peak = sg.peak.max(energy);
                sg.integral += energy;
                sg.quality_sum += quality;
                sg.efficiency_sum += q_eff;
                sg.participation_sum += participation;
                sg.max_retracement = sg.max_retracement.max(retracement);
                sg.max_move = sg.max_move.max(((c - sg.start_price) * sd).max(0.0));
                sg.last_atr = Some(a);
            }
        }
        if let Some(e) = ended {
            if e.polarity == 1 {
                self.leg_aligned += 1;
            } else if e.polarity == -1 {
                self.leg_corrections += 1;
            }
        }
        if reset {
            self.serial = 0;
        } else if ended.is_some() {
            self.serial += 1;
        }

        // Decay against the previous aligned segment.
        let last = nth(&self.waves, 1, 0);
        let prevseg = nth(&self.waves, 1, 1);
        let seg = self.seg;
        let active = seg.polarity == 1;
        let live_bars = if active {
            ((t - seg.start_bar + 1) as f64).max(1.0)
        } else {
            last.map_or(1.0, |s| s.duration)
        };
        let live_disp = if active {
            seg.max_move / a
        } else {
            last.map_or(0.0, |s| s.displacement)
        };
        let live_integral = if active {
            seg.integral
        } else {
            last.map_or(0.0, |s| s.integral)
        };
        let live_quality = if active {
            safe_div(seg.quality_sum, live_bars, quality)
        } else {
            last.map_or(quality, |s| s.quality)
        };
        let live_eff = if active {
            safe_div(seg.efficiency_sum, live_bars, q_eff)
        } else {
            last.map_or(q_eff, |s| s.efficiency)
        };
        let live_part = if active {
            safe_div(seg.participation_sum, live_bars, participation)
        } else {
            last.map_or(participation, |s| s.participation)
        };
        let current_peak = if active {
            seg.peak
        } else {
            last.map_or(aligned.max(0.0), |s| s.peak)
        };
        let reference = |f: fn(&Segment) -> f64, live: f64, floor: f64| -> f64 {
            if active {
                last.map_or(live.max(floor), |s| f(&s))
            } else {
                prevseg.or(last).map_or(live.max(floor), |s| f(&s))
            }
        };
        let ref_peak = reference(|s| s.peak, current_peak, 0.01);
        let ref_disp = reference(|s| s.displacement, live_disp, 0.01);
        let ref_integral = reference(|s| s.integral, live_integral, 0.01);
        let ref_duration = reference(|s| s.duration, live_bars, 1.0);
        let ref_quality = reference(|s| s.quality, live_quality, 0.05);
        let ref_eff = reference(|s| s.efficiency, live_eff, 0.05);
        let ref_part = reference(|s| s.participation, live_part, 0.05);
        let part_decay = if participation_active {
            decay(live_part, ref_part, 0.05)
        } else {
            0.0
        };
        let part_w = if participation_active { 0.05 } else { 0.0 };
        let wave_decay = clamp(
            (0.35 * decay(current_peak, ref_peak, 0.05)
                + 0.18 * decay(live_disp, ref_disp, 0.05)
                + 0.17 * decay(live_integral, ref_integral, 0.05)
                + 0.08 * decay(live_bars, ref_duration, 1.0)
                + 0.10 * decay(live_quality, ref_quality, 0.05)
                + 0.07 * decay(live_eff, ref_eff, 0.05)
                + part_w * part_decay)
                / (0.95 + part_w),
            0.0,
            1.0,
        );
        let failed = matches!((ended, prior_peak), (Some(e), Some(pp)) if e.polarity == 1
            && e.peak < pp * FAILED_PEAK
            && prior_disp.is_none_or(|pd| e.displacement < pd * FAILED_DISP));
        let dominant = matches!((ended, prior_peak), (Some(e), Some(pp)) if e.polarity == 1 && e.peak > pp * 1.20);

        // Maturity and compression.
        let hist_bars = median(&self.leg_dur, 24.0 * 3.0);
        let hist_disp = median(&self.leg_disp, 4.0);
        let hist_cycles = median(&self.leg_cycles, 3.0);
        let leg_bars = self
            .leg_start_bar
            .map_or(0.0, |sb| ((t - sb + 1) as f64).max(1.0));
        let (extension, progress) = if dir == 0 {
            (0.0, 0.0)
        } else {
            let sp = self.leg_start_price.unwrap();
            (
                safe_div(((self.leg_extreme.unwrap() - sp) * dirf).max(0.0), a, 0.0),
                safe_div(((c - sp) * dirf).max(0.0), a, 0.0),
            )
        };
        let cycle_count = self.leg_aligned as f64
            + 0.5 * self.leg_corrections as f64
            + match seg.polarity {
                1 => 0.5,
                -1 => 0.25,
                _ => 0.0,
            };
        let age_score = clamp(safe_div(leg_bars, hist_bars.max(1.0), 0.0), 0.0, 1.0);
        let ext_score = clamp(safe_div(extension, hist_disp.max(0.5), 0.0), 0.0, 1.0);
        let cycle_score = clamp(safe_div(cycle_count, hist_cycles.max(1.0), 0.0), 0.0, 1.0);
        let price_progress = clamp(safe_div(progress, hist_disp.max(0.5), 0.0), 0.0, 1.5);
        let max_peak = self
            .waves
            .iter()
            .filter(|s| s.polarity == 1)
            .map(|s| s.peak)
            .fold(None, |m: Option<f64>, p| Some(m.map_or(p, |x| x.max(p))))
            .unwrap_or(current_peak);
        let pressure_progress = clamp(safe_div(current_peak, max_peak.max(0.05), 1.0), 0.0, 1.5);
        let mismatch = clamp(price_progress - pressure_progress, 0.0, 1.0);
        let maturity = clamp(
            (0.20 * age_score
                + 0.25 * ext_score
                + 0.15 * cycle_score
                + 0.20 * wave_decay
                + 0.10 * efficiency_loss
                + 0.10 * mismatch)
                / 1.0,
            0.0,
            1.0,
        );
        let seg_bars = if seg.polarity == -1 {
            ((t - seg.start_bar + 1) as f64).max(1.0)
        } else {
            0.0
        };
        let corr_norm = clamp(
            safe_div(seg_bars, (hist_bars * 0.25).max(2.0), 0.0),
            0.0,
            1.0,
        );
        let compression = if seg.polarity == -1 {
            clamp(
                corr_norm * strength * (1.0 - clamp(retracement, 0.0, 1.0)) * (1.0 - counter),
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let ppe = clamp(
            safe_div(
                signed_move.unwrap_or(0.0).max(0.0) / a,
                aligned.max(0.0).max(0.05),
                0.0,
            ) / 2.0,
            0.0,
            1.0,
        );
        let ppe_ema = self.ppe_ema.update(ppe).unwrap();
        let ppe_prev = self.ppe_prev.replace(ppe_ema);
        let ppe_loss = ppe_prev.map_or(0.0, |p| clamp(p - ppe_ema, 0.0, 1.0));

        // Re-Sync.
        let close_recovery = self.close_at(RECOVERY_LEN);
        let rs = &mut self.rs;
        if reset {
            *rs = Resync::default();
        }
        if !rs.seen {
            rs.stage = 0;
        }
        let counter_active = aligned <= -WAVE_END || counter > 0.18;
        if counter_active && matches!(rs.pending_serial, Some(p) if self.serial > p) {
            rs.clear_context();
            rs.pending_serial = None;
            rs.pending_end = None;
        }
        if counter_active {
            if !rs.seen {
                rs.hold = 99;
                rs.context_bars = 0;
                rs.context_start = Some(t);
                rs.peak_pressure = 0.0;
                rs.peak_retracement = 0.0;
            }
            rs.seen = true;
            rs.context_bars += 1;
            rs.peak_pressure = rs.peak_pressure.max(counter);
            rs.peak_retracement = rs.peak_retracement.max(retracement);
        }
        let fresh = rs.seen
            && matches!(rs.context_start, Some(cs) if counter_active || t - cs <= WAVE_LEN * 3);
        if rs.seen && !fresh {
            rs.clear_context();
        }
        if matches!(ended, Some(e) if e.polarity == -1) {
            let qualified = rs.context_bars >= RS_MIN_BARS
                && (rs.peak_pressure >= RS_MIN_PEAK || rs.peak_retracement >= RS_MIN_RET);
            if qualified {
                rs.pending_serial = Some(self.serial);
                rs.pending_end = Some(t);
            } else {
                rs.clear_context();
            }
        }
        let pending_fresh = rs.pending_serial.is_some()
            && matches!(rs.pending_end, Some(e) if t - e <= WAVE_LEN * 2);
        if rs.pending_serial.is_some() && !pending_fresh {
            rs.pending_serial = None;
            rs.pending_end = None;
            rs.clear_context();
        }
        // `counter_hist` holds the previous VEL_LAG values; read before pushing this bar's.
        let counter_lag = if self.counter_hist.len() == VEL_LAG {
            self.counter_hist[0]
        } else {
            0.0
        };
        let counter_decay = clamp((counter_lag - counter) * 3.5, 0.0, 1.0);
        let accel_trend = clamp(aligned_acc * 1.8, 0.0, 1.0);
        let velocity_recovery = clamp(velocity * dirf * 2.0, 0.0, 1.0);
        let recovery_location = match dir {
            1 if c > recovery_ema => 1.0,
            -1 if c < recovery_ema => 1.0,
            _ => 0.0,
        };
        let recovery_disp = if dir == 0 {
            0.0
        } else {
            match close_recovery {
                Some(cr) => clamp(squash(safe_div((c - cr) * dirf, a, 0.0)) * 1.5, 0.0, 1.0),
                None => 0.0,
            }
        };
        let recovery_score = 0.45 * recovery_location + 0.55 * recovery_disp;
        let recovery_evidence = 0.55 * recovery_score + 0.45 * velocity_recovery;
        let resync_raw = if rs.seen {
            (0.30 * counter_decay
                + 0.30 * accel_trend
                + 0.20 * recovery_evidence
                + 0.20 * integrity)
                / 1.0
        } else {
            0.0
        };
        let resync_quality = clamp(
            resync_raw * (0.72 + 0.28 * strength) * (1.0 + 0.20 * compression),
            0.0,
            1.0,
        );
        let owned = pending_fresh
            && (rs.consumed_serial.is_none() || rs.pending_serial != rs.consumed_serial);
        let cooldown_ready = rs.last_bar.is_none_or(|b| t - b >= RS_COOLDOWN);
        let confirmed = !reset
            && owned
            && cooldown_ready
            && aligned > RS_CONFIRM
            && velocity_recovery > 0.08
            && integrity > 0.50
            && strength > 0.22
            && confidence > 0.35
            && resync_quality >= RS_MIN_QUALITY;
        if !rs.seen {
            rs.stage = 0;
        } else {
            let mut detected = if counter_decay > 0.20 { 1 } else { 0 };
            let developed = rs.context_bars >= RS_MIN_BARS.saturating_sub(1).max(1)
                && (rs.peak_pressure >= RS_MIN_PEAK * 0.75
                    || rs.peak_retracement >= RS_MIN_RET * 0.75);
            if !developed {
                detected = 0;
            }
            if developed && aligned.abs() < WAVE_END && counter_decay > 0.15 {
                detected = 2;
            }
            if developed
                && aligned_acc > 0.08
                && (velocity_recovery > 0.05 || recovery_score > 0.25)
            {
                detected = 3;
            }
            if confirmed {
                detected = 4;
            }
            rs.stage = rs.stage.max(detected);
        }
        if confirmed {
            rs.hold = 0;
            rs.last_bar = Some(t);
            rs.consumed_serial = rs.pending_serial;
            rs.pending_serial = None;
            rs.pending_end = None;
            rs.clear_context();
        } else if !reset {
            rs.hold = (rs.hold + 1).min(99);
        }
        let resync_glow = if rs.hold <= 5 {
            1.0 - rs.hold as f64 / 6.0
        } else {
            0.0
        };
        let resync_stage = rs.stage;
        push(&mut self.counter_hist, counter, VEL_LAG);

        // Sequence context.
        let pols: Vec<i8> = self.waves.iter().map(|s| s.polarity).collect();
        let n = pols.len();
        let impulse_pattern = n >= 5 && pols[n - 5..] == [1, -1, 1, -1, 1];
        let correction_pattern = n >= 3 && pols[n - 3..] == [-1, 1, -1];
        let al = |k: usize| nth(&self.waves, 1, k);
        let (s1, s2, s3) = (al(2), al(1), al(0));
        let hier = |f: fn(&Segment) -> f64| -> Option<f64> {
            match (s1, s2, s3) {
                (Some(a1), Some(a2), Some(a3)) => {
                    let (x1, x2, x3) = (f(&a1), f(&a2), f(&a3));
                    Some(clamp(
                        (x2 / x1.max(0.05) - 0.8) * 0.8 + (1.0 - x3 / x2.max(0.05)) * 0.6,
                        0.0,
                        1.0,
                    ))
                }
                _ => None,
            }
        };
        let exp_h = hier(|s| s.peak).unwrap_or(0.0);
        let disp_h = hier(|s| s.displacement).unwrap_or(exp_h);
        let int_h = hier(|s| s.integral).unwrap_or(exp_h);
        let seq_h = clamp(0.50 * exp_h + 0.25 * disp_h + 0.25 * int_h, 0.0, 1.0);
        let mean3 = |f: fn(&Segment) -> f64, fb: f64| match (s1, s2, s3) {
            (Some(a1), Some(a2), Some(a3)) => (f(&a1) + f(&a2) + f(&a3)) / 3.0,
            _ => fb,
        };
        let q_mean = mean3(|s| s.quality, quality);
        let e_mean = mean3(|s| s.efficiency, q_eff);
        let p_mean = mean3(|s| s.participation, participation);
        let seq_pw = if participation_active { 0.10 } else { 0.0 };
        let seq_quality = clamp(
            (0.72 * q_mean + 0.28 * e_mean + seq_pw * p_mean) / (1.0 + seq_pw),
            0.0,
            1.0,
        );
        let last_corr_ret = nth(&self.waves, -1, 0).map(|s| s.retracement);
        let correction_control = last_corr_ret.map_or(integrity, |r| clamp(1.0 - r, 0.0, 1.0));
        let impulse_seq = if impulse_pattern {
            clamp(
                0.30 * seq_h + 0.30 * seq_quality + 0.20 * correction_control + 0.20 * integrity,
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let recent = |k: usize, f: fn(&Segment) -> f64, fb: f64| {
            if self.waves.len() >= k {
                self.waves.iter().rev().take(k).map(f).sum::<f64>() / k as f64
            } else {
                fb
            }
        };
        let impulse_context = if impulse_pattern {
            clamp(
                0.55 * impulse_seq
                    + 0.25 * recent(5, |s| s.quality, quality)
                    + 0.20 * recent(5, |s| s.efficiency, q_eff),
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let impulse_ok = impulse_pattern && impulse_context >= CONTEXT_MIN;
        let depth = last_corr_ret.map_or(counter, |r| clamp(r / 0.35, 0.0, 1.0));
        let correction_context = if correction_pattern {
            clamp(
                0.40 * recent(3, |s| s.quality, quality)
                    + 0.25 * recent(3, |s| s.efficiency, q_eff)
                    + 0.20 * integrity
                    + 0.15 * depth,
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let correction_ok = correction_pattern && correction_context >= CONTEXT_MIN;
        if reset {
            self.last_impulse_serial = None;
            self.last_correction_serial = None;
        }
        let late_impulse = if impulse_ok {
            clamp(
                maturity * (0.45 * wave_decay + 0.30 * seq_h + 0.25 * impulse_context),
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let wave_context = if impulse_ok {
            if late_impulse > 0.55 {
                5.0
            } else if seq_h > 0.45 {
                4.0
            } else {
                3.0
            }
        } else if correction_ok {
            2.0
        } else if self.leg_aligned >= 2 {
            1.0
        } else {
            0.0
        };

        // Exhaustion and deterioration.
        let decay_evidence = clamp(
            (0.42 * wave_decay
                + 0.28 * efficiency_loss
                + 0.20 * ppe_loss
                + PART_WEIGHT * participation_loss)
                / (0.90 + PART_WEIGHT),
            0.0,
            1.0,
        );
        let continuation_failure = clamp(
            0.53 * mismatch + 0.47 * if failed { 1.0 } else { 0.0 },
            0.0,
            1.0,
        );
        let exhaustion = if maturity >= EXH_MATURITY {
            clamp(
                maturity * (0.55 * decay_evidence + 0.45 * continuation_failure) * 1.35,
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let deterioration = clamp(
            0.45 * wave_decay + 0.30 * mismatch + 0.25 * continuation_failure,
            0.0,
            1.0,
        );
        let weakening_state =
            matches!((s2, s3), (Some(p2), Some(p3)) if p3.peak < p2.peak * 0.80) && maturity > 0.45;
        let weakening = weakening_state && !self.prev.weakening;

        // Phase.
        let transition = dir == 0 || strength < 0.16 || confidence < 0.18;
        let initiation = !transition
            && leg_bars <= 4.0f64.max(hist_bars * 0.18)
            && matches!(self.prev.strength, Some(p) if strength > p);
        let accelerating = !transition
            && aligned > 0.08
            && aligned_acc > 0.10
            && matches!(energy_prev, Some(p) if energy > p);
        let correcting = !transition && (seg.polarity == -1 || counter > 0.22) && integrity > 0.28;
        let compressing = correcting && compression > 0.34;
        let resyncing = resync_stage >= 2 || resync_glow > 0.0;
        let late = !transition
            && aligned > 0.06
            && maturity > 0.56
            && (wave_decay > 0.18 || late_impulse > 0.35);
        let exhausted = !transition && exhaustion > 0.42;
        let expanding = !transition && aligned > 0.12 && quality > 0.25 && counter < 0.28;
        let candidate = if flip {
            9
        } else if transition {
            0
        } else if exhausted {
            8
        } else if resyncing {
            6
        } else if compressing {
            5
        } else if correcting {
            4
        } else if late {
            7
        } else if expanding {
            3
        } else if accelerating {
            2
        } else if initiation {
            1
        } else {
            0
        };
        if flip {
            self.phase = 9;
            self.pending_phase = 9;
            self.pending_bars = 0;
        } else if exit || dir == 0 {
            self.phase = 0;
            self.pending_phase = 0;
            self.pending_bars = 0;
        } else if self.phase == 9 {
            self.phase = candidate;
            self.pending_phase = candidate;
            self.pending_bars = 0;
        } else if candidate == self.phase {
            self.pending_phase = self.phase;
            self.pending_bars = 0;
        } else {
            if candidate != self.pending_phase {
                self.pending_phase = candidate;
                self.pending_bars = 1;
            } else {
                self.pending_bars += 1;
            }
            if self.pending_bars >= PHASE_CONFIRM {
                self.phase = self.pending_phase;
                self.pending_bars = 0;
            }
        }
        let phase = self.phase;
        let phase_confidence = match phase {
            1 => clamp(0.5 * strength + 0.5 * (1.0 - age_score), 0.0, 1.0),
            2 => clamp(
                0.45 * aligned.max(0.0) + 0.35 * accel_trend + 0.20 * quality,
                0.0,
                1.0,
            ),
            3 => clamp(
                0.35 * strength + 0.35 * aligned.max(0.0) + 0.30 * quality,
                0.0,
                1.0,
            ),
            4 => clamp(0.55 * counter + 0.45 * integrity, 0.0, 1.0),
            5 => compression,
            6 => resync_quality.max(resync_glow),
            7 => clamp(0.55 * maturity + 0.45 * aligned.max(0.0), 0.0, 1.0),
            8 => exhaustion,
            9 => 1.0,
            _ => clamp(1.0 - confidence, 0.0, 1.0),
        };

        // Context ownership (labels in Pine; kept for parity of the serial state).
        if ended.is_some()
            && impulse_ok
            && self
                .last_impulse_serial
                .is_none_or(|s| self.serial - s >= 5)
        {
            self.last_impulse_serial = Some(self.serial);
        }
        if ended.is_some()
            && correction_ok
            && self
                .last_correction_serial
                .is_none_or(|s| self.serial - s >= 3)
        {
            self.last_correction_serial = Some(self.serial);
        }

        // Alerts.
        let side = if dir == 1 { "bull" } else { "bear" };
        let prev_phase = self.prev.phase;
        let entered = |p: usize| phase == p && prev_phase != p;
        let mut kinds: Vec<String> = Vec::new();
        if initiated {
            kinds.push(format!("{side}_flow_initiated"));
        }
        if exit {
            kinds.push("flow_transition".into());
        }
        if flip {
            kinds.push(format!("{side}_flow_flip"));
        }
        if entered(3) {
            kinds.push(format!("{side}_expansion"));
        }
        if entered(4) {
            kinds.push("correction_start".into());
        }
        if entered(5) {
            kinds.push("compression".into());
        }
        if resync_stage == 1 && self.prev.resync_stage == 0 {
            kinds.push(format!("{side}_early_resync"));
        }
        if confirmed {
            kinds.push(format!("{side}_resync"));
        }
        if entered(7) {
            kinds.push("late_expansion".into());
        }
        if entered(8) {
            kinds.push("exhaustion".into());
        }
        if failed {
            kinds.push("failed_expansion".into());
        }
        if dominant {
            kinds.push(format!("{side}_dominant_expansion"));
        }
        if weakening {
            kinds.push("weakening_waves".into());
        }
        for kind in kinds {
            self.alerts.push(IndicatorAlert::new(
                kind.clone(),
                format!("Structural wave field: {kind} (phase {})", PHASES[phase]),
                clamp(strength, 0.05, 1.0),
            ));
        }
        self.prev = Prev {
            strength: Some(strength),
            phase,
            resync_stage,
            weakening: weakening_state,
        };

        let mut extra = HashMap::new();
        for (k, v) in [
            ("direction", dirf),
            ("strength", strength),
            ("confidence", confidence),
            ("internal_wave", internal),
            ("wave_quality", quality),
            ("counter_pressure", counter),
            ("integrity", integrity),
            ("retracement", retracement),
            ("maturity", maturity),
            ("compression", compression),
            ("resync_quality", resync_quality),
            ("resync_stage", resync_stage as f64),
            ("exhaustion", exhaustion),
            ("deterioration", deterioration),
            ("wave_decay", wave_decay),
            ("impulse_context", impulse_context),
            ("correction_context", correction_context),
            ("wave_context", wave_context),
            ("phase", phase as f64),
            ("phase_confidence", phase_confidence),
        ] {
            extra.insert(k.to_string(), v);
        }
        Some(
            IndicatorOutput::with_extra(drive, extra)
                .with_secondary(aligned)
                .with_state(PHASES[phase]),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
