//! Bottom and top formation as a sequence of stages (Kestrel plan/60 point 4, plan/71).
//!
//! A bottom is a process, not a point. Four stages, each with what does **not** follow from it
//! yet:
//!
//! | Stage | Observation | Does not yet follow |
//! |---|---|---|
//! | Attempt | a new low below the last swing low of a falling sequence, reclaimed on the same bar | that no further lows come |
//! | Stabilized | a confirmed higher low above the attempt low | that the higher downtrend ends |
//! | Structure shift | a close above the intermediate high | that the breakout holds |
//! | Confirmed | a retest of the intermediate high holds, then a new high | that the entry is still attractive |
//!
//! A close below the attempt low fails the run; so does a close below the stabilizing low once
//! the structure shifted. Each stage expires after `max_stage_bars`. Only one run per side at a
//! time: an attempt only counts when no run is open, so the attempt depends on the whole
//! sequence, not on the attempt condition alone. The top is the same sequence on the mirrored
//! price.
//!
//! Everything is causal: swing points count from the bar that confirms them (`pivot_len` bars
//! late, non-strict on both sides like [`crate::structure::confirmed_pivots`]), RSI 14 and ATR 14
//! (Wilder, seeded with the mean of the first 14 true ranges) only from bars up to the current
//! one. Moved here from Kestrel's `analytics::bodenbildung` so the study, the live indicator and
//! a registered hypothesis read the same sequence.
//!
//! [`runs`] returns every run over a series; [`BottomFormation`] streams the same sequence for
//! both sides. Alerts: `bull_bottom_attempt`, `bull_bottom_stabilized`, `bull_structure_shift`,
//! `bull_bottom_confirmed`, and `bear_top_attempt`, `bear_top_stabilized`,
//! `bear_structure_shift`, `bear_top_confirmed`. Failure and expiry end a run silently. On event
//! bars `extra["wick"]` is the run's invalidation (attempt low of a bottom, attempt high of a
//! top) — the key `WickStop` consumers read; if both sides report an event on the same bar it
//! belongs to the bottom. `extra["bottom_stage"]`/`["top_stage"]` carry the open stage (0 none,
//! 1 attempt … 4 confirmed) on every bar. `value` is `+n`/`-n` for the stage a bottom/top
//! reached on this bar (bottom first), else `0`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::rsi::Rsi;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// RSI and ATR length.
const LEN: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Bottom,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Attempt,
    Stabilized,
    StructureShift,
    Confirmed,
}

impl Stage {
    fn code(self) -> i32 {
        match self {
            Stage::Attempt => 1,
            Stage::Stabilized => 2,
            Stage::StructureShift => 3,
            Stage::Confirmed => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// Still running at the end of the series.
    Open,
    /// Closed beyond the attempt extreme (or the stabilizing extreme after the shift).
    Failed,
    /// A stage waited too long.
    Expired,
    Confirmed,
}

/// One run with the bar indices at which it reached its stages. Prices always in real
/// direction (not mirrored).
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub side: Side,
    pub attempt: usize,
    /// Extreme of the attempt (bottom: low, top: high) — the invalidation.
    pub attempt_extreme: f64,
    /// The swing extreme that was broken and reclaimed.
    pub old_extreme: f64,
    /// RSI at the attempt beyond the one at the old extreme (bottom: higher) — weaker momentum.
    pub divergence: bool,
    /// Bar at which the higher low (lower high) was known, and its price.
    pub stabilized: Option<(usize, f64)>,
    /// Intermediate high (bottom) or low (top) whose break is the structure shift.
    pub break_level: Option<f64>,
    pub structure_shift: Option<usize>,
    pub retest: Option<usize>,
    pub confirmed: Option<usize>,
    pub end: RunEnd,
    /// Bar at which the run ended (or the last bar).
    pub end_index: usize,
}

impl Run {
    pub fn stage(&self) -> Stage {
        if self.confirmed.is_some() {
            Stage::Confirmed
        } else if self.structure_shift.is_some() {
            Stage::StructureShift
        } else if self.stabilized.is_some() {
            Stage::Stabilized
        } else {
            Stage::Attempt
        }
    }

    /// Bar at which `stage` was reached.
    pub fn index(&self, stage: Stage) -> Option<usize> {
        match stage {
            Stage::Attempt => Some(self.attempt),
            Stage::Stabilized => self.stabilized.map(|s| s.0),
            Stage::StructureShift => self.structure_shift,
            Stage::Confirmed => self.confirmed,
        }
    }

    fn mirrored(mut self) -> Self {
        self.attempt_extreme = -self.attempt_extreme;
        self.old_extreme = -self.old_extreme;
        self.stabilized = self.stabilized.map(|(i, p)| (i, -p));
        self.break_level = self.break_level.map(|p| -p);
        self
    }
}

fn mirror(bar: &Bar) -> Bar {
    Bar {
        open: -bar.open,
        high: -bar.low,
        low: -bar.high,
        close: -bar.close,
        ..bar.clone()
    }
}

/// What a bar did to the run of one side.
#[derive(Debug, Clone, Default, PartialEq)]
struct Step {
    /// Stage reached on this bar.
    reached: Option<Stage>,
    /// A run that ended on this bar (or `None`).
    finished: Option<Run>,
}

/// The sequence for one side, always computed as a bottom (the top gets mirrored bars).
#[derive(Debug, Clone)]
struct Machine {
    side: Side,
    pivot_len: usize,
    max_stage_bars: usize,
    retest_atr: f64,
    /// Bars seen so far; the current bar has index `count - 1`.
    count: usize,
    window: VecDeque<Bar>,
    rsi: Rsi,
    /// RSI of the last `pivot_len + 1` bars, newest last.
    rsi_recent: VecDeque<Option<f64>>,
    prev_close: Option<f64>,
    atr: Option<f64>,
    tr_sum: f64,
    /// The last two confirmed swing lows: (bar index, low, RSI at that bar).
    lows: VecDeque<(usize, f64, Option<f64>)>,
    run: Option<Run>,
    /// Bars since the attempt of the open run, the attempt bar first.
    history: Vec<Bar>,
}

impl Machine {
    fn new(side: Side, pivot_len: usize, max_stage_bars: usize, retest_atr: f64) -> Self {
        Self {
            side,
            pivot_len: pivot_len.max(1),
            max_stage_bars,
            retest_atr,
            count: 0,
            window: VecDeque::new(),
            rsi: rsi(),
            rsi_recent: VecDeque::new(),
            prev_close: None,
            atr: None,
            tr_sum: 0.0,
            lows: VecDeque::new(),
            run: None,
            history: Vec::new(),
        }
    }

    fn update(&mut self, bar: &Bar) -> Step {
        let i = self.count;
        self.count += 1;

        // ATR 14, Wilder, seeded with the mean of the first 14 true ranges.
        let tr = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);
        let n = LEN as f64;
        self.atr = match self.atr {
            Some(a) => Some((a * (n - 1.0) + tr) / n),
            None => {
                self.tr_sum += tr;
                (i + 1 == LEN).then_some(self.tr_sum / n)
            }
        };
        let rsi_now = self.rsi.on_bar(bar).map(|o| o.value);
        self.rsi_recent.push_back(rsi_now);
        if self.rsi_recent.len() > self.pivot_len + 1 {
            self.rsi_recent.pop_front();
        }

        // Swing low confirmed on this bar: the bar `pivot_len` back, no lower bar on either side.
        self.window.push_back(bar.clone());
        if self.window.len() > 2 * self.pivot_len + 1 {
            self.window.pop_front();
        }
        if self.window.len() == 2 * self.pivot_len + 1 {
            let low = self.window[self.pivot_len].low;
            if self.window.iter().all(|b| b.low >= low) {
                let rsi_at = self.rsi_recent.front().copied().flatten();
                self.lows.push_back((i - self.pivot_len, low, rsi_at));
                if self.lows.len() > 2 {
                    self.lows.pop_front();
                }
            }
        }

        if let Some(mut run) = self.run.take() {
            self.history.push(bar.clone());
            let stage_since = run.index(run.stage()).unwrap_or(run.attempt);
            let end = |mut run: Run, end: RunEnd| {
                run.end = end;
                run.end_index = i;
                Step {
                    reached: (end == RunEnd::Confirmed).then_some(Stage::Confirmed),
                    finished: Some(run),
                }
            };
            if bar.close < run.attempt_extreme {
                return end(run, RunEnd::Failed);
            }
            if i - stage_since > self.max_stage_bars {
                return end(run, RunEnd::Expired);
            }
            let mut reached = None;
            match run.stage() {
                Stage::Attempt => {
                    // A confirmed low after the attempt, above its low.
                    if let Some(&(idx, price, _)) = self.lows.back() {
                        if idx > run.attempt && price > run.attempt_extreme {
                            run.stabilized = Some((i, price));
                            // Intermediate high: the highest high from the attempt to the
                            // higher low.
                            run.break_level = self.history[..=idx - run.attempt]
                                .iter()
                                .map(|x| x.high)
                                .reduce(f64::max);
                            reached = Some(Stage::Stabilized);
                        }
                    }
                }
                Stage::Stabilized => {
                    if run.break_level.is_some_and(|h| bar.close > h) {
                        run.structure_shift = Some(i);
                        reached = Some(Stage::StructureShift);
                    }
                }
                Stage::StructureShift => {
                    let level = run.break_level.expect("a shift has a break level");
                    let shift = run.structure_shift.expect("set");
                    let stabilizing = run.stabilized.expect("set").1;
                    let peak = self.history[shift - run.attempt..i - run.attempt]
                        .iter()
                        .map(|x| x.high)
                        .reduce(f64::max);
                    let near = self.atr.map(|a| self.retest_atr * a).unwrap_or(0.0);
                    if bar.close < stabilizing {
                        return end(run, RunEnd::Failed);
                    }
                    if run.retest.is_none() && i > shift && bar.low <= level + near {
                        run.retest = Some(i);
                    } else if run.retest.is_some() && peak.is_some_and(|p| bar.close > p) {
                        run.confirmed = Some(i);
                        return end(run, RunEnd::Confirmed);
                    }
                }
                Stage::Confirmed => {}
            }
            self.run = Some(run);
            return Step {
                reached,
                finished: None,
            };
        }

        // No run: an attempt is a new low below the last swing low of a falling sequence,
        // reclaimed on the same bar.
        let (Some(&(_, before, _)), Some(&(_, last, last_rsi))) = (
            self.lows.len().checked_sub(2).map(|k| &self.lows[k]),
            self.lows.back(),
        ) else {
            return Step::default();
        };
        if !(last < before && bar.low < last && bar.close > last) {
            return Step::default();
        }
        let divergence = match (rsi_now, last_rsi) {
            (Some(now), Some(then)) => now > then,
            _ => false,
        };
        self.history = vec![bar.clone()];
        self.run = Some(Run {
            side: self.side,
            attempt: i,
            attempt_extreme: bar.low,
            old_extreme: last,
            divergence,
            stabilized: None,
            break_level: None,
            structure_shift: None,
            retest: None,
            confirmed: None,
            end: RunEnd::Open,
            end_index: i,
        });
        Step {
            reached: Some(Stage::Attempt),
            finished: None,
        }
    }

    /// The open run in real direction.
    fn open_run(&self) -> Option<Run> {
        let run = self.run.clone()?;
        Some(match self.side {
            Side::Bottom => run,
            Side::Top => run.mirrored(),
        })
    }
}

fn rsi() -> Rsi {
    Rsi::new(LEN, 1, 1, 50.0, 70.0, 30.0, 5, false, 100, 4, 10.0)
}

/// Default swing length, stage timeout and retest distance (in ATR).
pub const DEFAULT_PIVOT_LEN: usize = 5;
pub const DEFAULT_MAX_STAGE_BARS: usize = 30;
pub const DEFAULT_RETEST_ATR: f64 = 0.5;

/// Every run of one side over `bars` (ascending), with the default parameters. At most one runs
/// at a time; a new one starts only after the previous ended. A run still open at the end is
/// returned with [`RunEnd::Open`] and `end_index` the last bar.
pub fn runs(bars: &[Bar], side: Side) -> Vec<Run> {
    runs_with(
        bars,
        side,
        DEFAULT_PIVOT_LEN,
        DEFAULT_MAX_STAGE_BARS,
        DEFAULT_RETEST_ATR,
    )
}

/// [`runs`] with explicit parameters.
pub fn runs_with(
    bars: &[Bar],
    side: Side,
    pivot_len: usize,
    max_stage_bars: usize,
    retest_atr: f64,
) -> Vec<Run> {
    if bars.len() < 2 * pivot_len + 2 {
        return Vec::new();
    }
    let mut machine = Machine::new(side, pivot_len, max_stage_bars, retest_atr);
    let mut out = Vec::new();
    for bar in bars {
        let bar = match side {
            Side::Bottom => bar.clone(),
            Side::Top => mirror(bar),
        };
        if let Some(run) = machine.update(&bar).finished {
            out.push(match side {
                Side::Bottom => run,
                Side::Top => run.mirrored(),
            });
        }
    }
    if let Some(mut run) = machine.open_run() {
        run.end_index = bars.len() - 1;
        out.push(run);
    }
    out
}

/// Both sides of the sequence as a streaming indicator. See the module documentation.
#[derive(Debug, Clone)]
pub struct BottomFormation {
    bottom: Machine,
    top: Machine,
    bottom_reached: Option<Stage>,
    top_reached: Option<Stage>,
    wick: Option<f64>,
}

impl BottomFormation {
    pub fn new(pivot_len: usize, max_stage_bars: usize, retest_atr: f64) -> Self {
        Self {
            bottom: Machine::new(Side::Bottom, pivot_len, max_stage_bars, retest_atr),
            top: Machine::new(Side::Top, pivot_len, max_stage_bars, retest_atr),
            bottom_reached: None,
            top_reached: None,
            wick: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            DEFAULT_PIVOT_LEN,
            DEFAULT_MAX_STAGE_BARS,
            DEFAULT_RETEST_ATR,
        )
    }

    /// The open run of a side, if any, in real direction.
    pub fn open_run(&self, side: Side) -> Option<Run> {
        match side {
            Side::Bottom => self.bottom.open_run(),
            Side::Top => self.top.open_run(),
        }
    }
}

/// Invalidation of a run that reached a stage on this bar: the open run's extreme, or that of
/// the run that just confirmed.
fn wick_of(machine: &Machine, step: &Step) -> Option<f64> {
    step.finished
        .as_ref()
        .map(|run| run.attempt_extreme)
        .or_else(|| machine.run.as_ref().map(|run| run.attempt_extreme))
}

impl Indicator for BottomFormation {
    fn name(&self) -> &str {
        "bottom_formation"
    }

    fn warmup_period(&self) -> usize {
        2 * self.bottom.pivot_len + 1
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let bottom = self.bottom.update(bar);
        let top = self.top.update(&mirror(bar));
        self.bottom_reached = bottom.reached;
        self.top_reached = top.reached;
        self.wick = if bottom.reached.is_some() {
            wick_of(&self.bottom, &bottom)
        } else if top.reached.is_some() {
            wick_of(&self.top, &top).map(|w| -w)
        } else {
            None
        };

        let stage = |m: &Machine| m.run.as_ref().map_or(0, |r| r.stage().code());
        let mut extra = HashMap::new();
        extra.insert("bottom_stage".to_string(), f64::from(stage(&self.bottom)));
        extra.insert("top_stage".to_string(), f64::from(stage(&self.top)));
        if let Some(wick) = self.wick {
            extra.insert("wick".to_string(), wick);
        }
        let value = match (self.bottom_reached, self.top_reached) {
            (Some(s), _) => s.code(),
            (None, Some(s)) => -s.code(),
            (None, None) => 0,
        };
        Some(IndicatorOutput::with_extra(f64::from(value), extra))
    }

    fn reset(&mut self) {
        let (p, m, r) = (
            self.bottom.pivot_len,
            self.bottom.max_stage_bars,
            self.bottom.retest_atr,
        );
        *self = Self::new(p, m, r);
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if let Some(stage) = self.bottom_reached {
            let (kind, note, strength) = match stage {
                Stage::Attempt => ("bull_bottom_attempt", "BOTTOM · ATTEMPT", 0.4),
                Stage::Stabilized => ("bull_bottom_stabilized", "BOTTOM · HIGHER LOW", 0.6),
                Stage::StructureShift => ("bull_structure_shift", "BOTTOM · STRUCTURE SHIFT", 0.8),
                Stage::Confirmed => ("bull_bottom_confirmed", "BOTTOM · CONFIRMED", 1.0),
            };
            out.push(IndicatorAlert::new(kind, note, strength));
        }
        if let Some(stage) = self.top_reached {
            let (kind, note, strength) = match stage {
                Stage::Attempt => ("bear_top_attempt", "TOP · ATTEMPT", 0.4),
                Stage::Stabilized => ("bear_top_stabilized", "TOP · LOWER HIGH", 0.6),
                Stage::StructureShift => ("bear_structure_shift", "TOP · STRUCTURE SHIFT", 0.8),
                Stage::Confirmed => ("bear_top_confirmed", "TOP · CONFIRMED", 1.0),
            };
            out.push(IndicatorAlert::new(kind, note, strength));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(i: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(1_700_000_000 + i * 3600, open, high, low, close, 0.0)
    }

    /// Falling swing lows, then a bar that undercuts the last one and closes back above it, a
    /// higher low, a close above the intermediate high, a retest and a new high: one run through
    /// all four stages, with the indicator firing each stage once and `wick` at the attempt low.
    fn lehrbuch() -> Vec<Bar> {
        let mut closes: Vec<f64> = Vec::new();
        // Two falling legs with swing lows at 95 and 90.
        for k in 0..12 {
            closes.push(100.0 - k as f64 * 0.5);
        }
        for k in 0..8 {
            closes.push(95.0 + k as f64 * 0.6);
        }
        for k in 0..14 {
            closes.push(99.2 - k as f64 * 0.7);
        }
        for k in 0..8 {
            closes.push(90.0 + k as f64 * 0.5);
        }
        for k in 0..10 {
            closes.push(94.0 - k as f64 * 0.4);
        }
        let mut bars: Vec<Bar> = closes
            .iter()
            .enumerate()
            .map(|(i, &c)| bar(i as i64, c, c + 0.3, c - 0.3, c))
            .collect();
        let n = bars.len() as i64;
        // Attempt: undercuts the swing low at 90 (89.7), closes above it.
        bars.push(bar(n, 90.2, 90.6, 88.0, 90.4));
        // Up to an intermediate high, back down to a higher low, up through the high.
        let mut c = 90.4;
        for (k, step) in [
            0.8, 0.8, 0.8, 0.6, -0.5, -0.5, -0.5, -0.4, -0.3, 0.2, 0.2, 0.2, 0.3, 0.3, 0.6, 0.8,
            0.8, 0.8, 0.8, -0.9, -0.9, 0.8, 0.9, 1.0, 1.0,
        ]
        .into_iter()
        .enumerate()
        {
            c += step;
            bars.push(bar(n + 1 + k as i64, c - step / 2.0, c + 0.3, c - 0.3, c));
        }
        bars
    }

    #[test]
    fn a_textbook_bottom_reaches_every_stage() {
        let bars = lehrbuch();
        let runs = runs(&bars, Side::Bottom);
        let attempt = runs
            .iter()
            .find(|r| r.attempt_extreme == 88.0)
            .expect("the undercut bar starts a run");
        assert_eq!(attempt.old_extreme, 89.7);
        assert!(attempt.stabilized.is_some(), "{attempt:?}");
        assert!(attempt.structure_shift.is_some(), "{attempt:?}");

        let mut ind = BottomFormation::with_defaults();
        let mut kinds = Vec::new();
        for b in &bars {
            let out = ind.on_bar(b).unwrap();
            for a in ind.alerts() {
                if a.kind == "bull_bottom_attempt" {
                    assert_eq!(out.extra["wick"], 88.0);
                    assert_eq!(out.value, 1.0);
                }
                kinds.push(a.kind);
            }
        }
        assert!(kinds.contains(&"bull_bottom_attempt".to_string()));
        assert!(kinds.contains(&"bull_bottom_stabilized".to_string()));
        assert!(kinds.contains(&"bull_structure_shift".to_string()));
    }

    /// The top is the bottom on the mirrored price: same indices, prices negated back.
    #[test]
    fn the_top_is_the_mirrored_bottom() {
        let bars = lehrbuch();
        let mirrored: Vec<Bar> = bars.iter().map(mirror).collect();
        let bottoms = runs(&bars, Side::Bottom);
        let tops = runs(&mirrored, Side::Top);
        assert_eq!(bottoms.len(), tops.len());
        for (b, t) in bottoms.iter().zip(&tops) {
            assert_eq!(b.attempt, t.attempt);
            assert_eq!(b.end, t.end);
            assert_eq!(b.attempt_extreme, -t.attempt_extreme);
            assert_eq!(b.break_level.map(|x| -x), t.break_level);
        }
    }

    /// The streaming indicator fires an attempt exactly where the batch function starts a run.
    #[test]
    fn stream_and_batch_agree() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut c = 100.0;
        let bars: Vec<Bar> = (0..3000)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let step = ((seed >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 2.0;
                let o = c;
                c += step;
                let wick = ((seed >> 20) & 0xff) as f64 / 255.0;
                bar(i, o, o.max(c) + wick, o.min(c) - wick * 0.8, c)
            })
            .collect();
        let mut ind = BottomFormation::with_defaults();
        let mut bottom_attempts = Vec::new();
        let mut top_attempts = Vec::new();
        for (i, b) in bars.iter().enumerate() {
            ind.on_bar(b);
            for a in ind.alerts() {
                match a.kind.as_str() {
                    "bull_bottom_attempt" => bottom_attempts.push(i),
                    "bear_top_attempt" => top_attempts.push(i),
                    _ => {}
                }
            }
        }
        let batch = |side| {
            runs(&bars, side)
                .iter()
                .map(|r| r.attempt)
                .collect::<Vec<_>>()
        };
        assert!(bottom_attempts.len() > 10, "{}", bottom_attempts.len());
        assert_eq!(bottom_attempts, batch(Side::Bottom));
        assert_eq!(top_attempts, batch(Side::Top));
    }
}
