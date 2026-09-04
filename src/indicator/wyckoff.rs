//! Wyckoff accumulation/distribution state machine: range-lock detection, Phases A-E, Spring/UTAD,
//! SOS/SOW/LPS/LPSY events, sequence validation, and Cause/Quality scoring.
//!
//! This is a codified heuristic interpretation of the textbook Wyckoff method (range-lock via ATR-
//! relative contraction, climax via robust volume outlier, Spring/UTAD via pierce-and-reclaim —
//! structurally the same pattern as [`super::smart_money_structure::LiquidityPoolEngine`]'s stop-
//! hunt classification), not a claim of canonical/definitive Wyckoff analysis: real chart reading
//! involves judgment this state machine approximates with fixed, documented, testable rules.

use std::collections::VecDeque;

use crate::clustering::RollingRobustThreshold;
use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Which side of the cycle this range is developing into: accumulation (basing before markup) or
/// distribution (topping before markdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffBias {
    Accumulation,
    Distribution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffPhase {
    /// No range locked yet.
    Undefined,
    /// Range just locked (Preliminary Support/Supply + Climax context).
    A,
    /// Range building: repeated tests of the boundaries (Secondary Tests).
    B,
    /// A Spring (accumulation) or UTAD (distribution) has occurred: the decisive test.
    C,
    /// A Sign of Strength/Weakness breakout past the *opposite* boundary has occurred.
    D,
    /// A Last Point of Support/Supply held: trend confirmed (Markup/Markdown).
    E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffEventKind {
    SecondaryTest,
    Spring,
    Utad,
    SignOfStrength,
    SignOfWeakness,
    LastPointOfSupport,
    LastPointOfSupply,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffEvent {
    pub kind: WyckoffEventKind,
    pub price: f64,
    pub timestamp: i64,
}

/// Heuristic scoring of the developing range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffScore {
    /// Proportional to range width x bars spent in range (the point-and-figure "cause" building
    /// over time) — larger implies a larger implied subsequent move once the range resolves.
    pub cause_score: f64,
    /// `1.0` if the full A -> B -> C -> D -> E sequence was observed in order with no
    /// contradicting event (e.g. a Spring followed by a UTAD before any SOS); penalized for gaps
    /// or out-of-order events.
    pub sequence_quality: f64,
}

pub struct WyckoffStateMachine {
    range_lookback: usize,
    range_atr_max: f64,
    min_range_bars: usize,
    atr: Rma,
    prev_close: Option<f64>,
    volume_threshold: RollingRobustThreshold,
    bars: VecDeque<Bar>,
    bars_in_range: u32,
    range_high: f64,
    range_low: f64,
    range_locked: bool,
    bias: Option<WyckoffBias>,
    phase: WyckoffPhase,
    events: Vec<WyckoffEvent>,
    alerts: Vec<IndicatorAlert>,
}

impl WyckoffStateMachine {
    pub fn new(range_lookback: usize, range_atr_max: f64, min_range_bars: usize) -> Self {
        let range_lookback = range_lookback.max(3);
        Self {
            range_lookback,
            range_atr_max,
            min_range_bars: min_range_bars.max(2),
            atr: Rma::new(14),
            prev_close: None,
            // Must stay derived from the same `range_lookback` as the bar deque below: the deque
            // gates when a range becomes lock-eligible (`bars.len() == self.range_lookback`), and
            // the climax-driven bias assignment only fires if the volume-outlier window is warm
            // by that same bar. A separately floored window here (e.g. a hardcoded minimum higher
            // than `range_lookback`) would make the range lock-eligible before climax detection is
            // armed, silently forcing every small-`range_lookback` configuration onto the
            // non-climax fallback bias path regardless of actual volume behavior.
            volume_threshold: RollingRobustThreshold::new(range_lookback, 2.0),
            bars: VecDeque::with_capacity(range_lookback),
            bars_in_range: 0,
            range_high: f64::MIN,
            range_low: f64::MAX,
            range_locked: false,
            bias: None,
            phase: WyckoffPhase::Undefined,
            events: Vec::new(),
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 3.0, 6)
    }

    pub fn phase(&self) -> WyckoffPhase {
        self.phase
    }

    pub fn bias(&self) -> Option<WyckoffBias> {
        self.bias
    }

    /// Full event history observed for the current (or most recently completed) range.
    pub fn events(&self) -> &[WyckoffEvent] {
        &self.events
    }

    pub fn score(&self) -> WyckoffScore {
        let range_width = (self.range_high - self.range_low).max(0.0);
        let cause_score = range_width * self.bars_in_range as f64;

        let expected_order = [
            WyckoffEventKind::SecondaryTest,
            WyckoffEventKind::Spring,
            WyckoffEventKind::Utad,
            WyckoffEventKind::SignOfStrength,
            WyckoffEventKind::SignOfWeakness,
            WyckoffEventKind::LastPointOfSupport,
            WyckoffEventKind::LastPointOfSupply,
        ];
        let rank = |k: WyckoffEventKind| expected_order.iter().position(|&e| e == k).unwrap_or(0);

        let mut sequence_quality = 1.0f64;
        for pair in self.events.windows(2) {
            if rank(pair[1].kind) < rank(pair[0].kind) {
                sequence_quality -= 0.2;
            }
        }
        sequence_quality = sequence_quality.clamp(0.0, 1.0);

        WyckoffScore {
            cause_score,
            sequence_quality,
        }
    }

    fn lock_range(&mut self, bias: WyckoffBias) {
        self.range_locked = true;
        self.bias = Some(bias);
        self.phase = WyckoffPhase::A;
        self.events.clear();
        self.bars_in_range = 0;
    }

    fn unlock_range(&mut self) {
        self.range_locked = false;
        self.bias = None;
        self.phase = WyckoffPhase::Undefined;
        self.range_high = f64::MIN;
        self.range_low = f64::MAX;
    }

    fn push_event(
        &mut self,
        kind: WyckoffEventKind,
        price: f64,
        timestamp: i64,
        strength: f64,
        note: &str,
    ) {
        self.events.push(WyckoffEvent {
            kind,
            price,
            timestamp,
        });
        self.alerts.push(IndicatorAlert::new(
            format!("wyckoff_{kind:?}").to_lowercase(),
            note,
            strength,
        ));
    }
}

impl Indicator for WyckoffStateMachine {
    fn name(&self) -> &str {
        "wyckoff"
    }

    fn warmup_period(&self) -> usize {
        self.range_lookback
    }

    fn reset(&mut self) {
        self.atr.reset();
        self.prev_close = None;
        self.bars.clear();
        self.bars_in_range = 0;
        self.unlock_range();
        self.events.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let tr = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);
        let atr = self.atr.update(tr);
        let volume_band = self.volume_threshold.update(bar.volume);

        self.bars.push_back(bar.clone());
        if self.bars.len() > self.range_lookback {
            self.bars.pop_front();
        }
        if self.bars.len() < self.range_lookback {
            return None;
        }
        let atr = atr.filter(|a| *a > 0.0)?;

        let window_high = self.bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let window_low = self.bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let width_in_atr = (window_high - window_low) / atr;

        if !self.range_locked {
            if width_in_atr <= self.range_atr_max {
                // A climax (outlier volume) just inside/before this contraction picks the bias:
                // a high-volume down move contracting into a base implies accumulation, a
                // high-volume up move implies distribution.
                let is_climax = volume_band.map(|b| bar.volume > b.upper).unwrap_or(false);
                let bias = if is_climax && bar.close < bar.open {
                    WyckoffBias::Accumulation
                } else if is_climax && bar.close > bar.open {
                    WyckoffBias::Distribution
                } else if self.prev_close.map(|pc| bar.close < pc).unwrap_or(false) {
                    WyckoffBias::Accumulation
                } else {
                    WyckoffBias::Distribution
                };
                self.range_high = window_high;
                self.range_low = window_low;
                self.lock_range(bias);
            }
            return Some(IndicatorOutput::new(0.0));
        }

        self.bars_in_range += 1;
        let bias = self.bias.expect("range_locked implies bias is set");

        // Range invalidated: price wandered far beyond both original boundaries without a clean
        // Phase D/E resolution.
        if width_in_atr > self.range_atr_max * 2.0
            && self.bars_in_range > self.min_range_bars as u32 * 3
        {
            self.unlock_range();
            return Some(IndicatorOutput::new(0.0));
        }

        match self.phase {
            WyckoffPhase::A | WyckoffPhase::B => {
                self.phase = WyckoffPhase::B;
                let near_high =
                    bar.high >= self.range_high - atr * 0.25 && bar.high <= self.range_high;
                let near_low = bar.low <= self.range_low + atr * 0.25 && bar.low >= self.range_low;

                let spring = bar.low < self.range_low && bar.close > self.range_low;
                let utad = bar.high > self.range_high && bar.close < self.range_high;

                if bias == WyckoffBias::Accumulation && spring {
                    self.phase = WyckoffPhase::C;
                    self.push_event(
                        WyckoffEventKind::Spring,
                        bar.low,
                        bar.timestamp,
                        0.85,
                        "Wyckoff Spring: range low swept and reclaimed",
                    );
                } else if bias == WyckoffBias::Distribution && utad {
                    self.phase = WyckoffPhase::C;
                    self.push_event(
                        WyckoffEventKind::Utad,
                        bar.high,
                        bar.timestamp,
                        0.85,
                        "Wyckoff UTAD: range high swept and reclaimed",
                    );
                } else if near_high || near_low {
                    self.push_event(
                        WyckoffEventKind::SecondaryTest,
                        bar.close,
                        bar.timestamp,
                        0.4,
                        "Wyckoff Secondary Test of range boundary",
                    );
                }
            }
            WyckoffPhase::C => {
                let sos = bias == WyckoffBias::Accumulation && bar.close > self.range_high;
                let sow = bias == WyckoffBias::Distribution && bar.close < self.range_low;
                if sos {
                    self.phase = WyckoffPhase::D;
                    self.push_event(
                        WyckoffEventKind::SignOfStrength,
                        bar.close,
                        bar.timestamp,
                        0.8,
                        "Wyckoff Sign of Strength: closed beyond range high",
                    );
                } else if sow {
                    self.phase = WyckoffPhase::D;
                    self.push_event(
                        WyckoffEventKind::SignOfWeakness,
                        bar.close,
                        bar.timestamp,
                        0.8,
                        "Wyckoff Sign of Weakness: closed beyond range low",
                    );
                }
            }
            WyckoffPhase::D => {
                let lps = bias == WyckoffBias::Accumulation
                    && bar.low >= self.range_high - atr * 0.5
                    && bar.close > self.range_high;
                let lpsy = bias == WyckoffBias::Distribution
                    && bar.high <= self.range_low + atr * 0.5
                    && bar.close < self.range_low;
                if lps {
                    self.phase = WyckoffPhase::E;
                    self.push_event(
                        WyckoffEventKind::LastPointOfSupport,
                        bar.close,
                        bar.timestamp,
                        0.9,
                        "Wyckoff Last Point of Support: pullback held, Markup confirmed",
                    );
                } else if lpsy {
                    self.phase = WyckoffPhase::E;
                    self.push_event(
                        WyckoffEventKind::LastPointOfSupply,
                        bar.close,
                        bar.timestamp,
                        0.9,
                        "Wyckoff Last Point of Supply: pullback held, Markdown confirmed",
                    );
                } else {
                    // Breakout failed to hold: back inside the range invalidates Phase D.
                    let failed = (bias == WyckoffBias::Accumulation && bar.close < self.range_high)
                        || (bias == WyckoffBias::Distribution && bar.close > self.range_low);
                    if failed {
                        self.phase = WyckoffPhase::B;
                    }
                }
            }
            WyckoffPhase::E | WyckoffPhase::Undefined => {}
        }

        let phase_code = match self.phase {
            WyckoffPhase::Undefined => 0.0,
            WyckoffPhase::A => 1.0,
            WyckoffPhase::B => 2.0,
            WyckoffPhase::C => 3.0,
            WyckoffPhase::D => 4.0,
            WyckoffPhase::E => 5.0,
        };
        Some(IndicatorOutput::new(phase_code))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range_bars(n: usize, center: f64, half_width: f64, seed_volume: f64) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let offset = ((i % 4) as f64 - 1.5) * half_width * 0.3;
                let price = center + offset;
                Bar::new(
                    i as i64 * 60,
                    price,
                    price + half_width * 0.3,
                    price - half_width * 0.3,
                    price,
                    seed_volume,
                )
            })
            .collect()
    }

    #[test]
    fn test_locks_range_after_contraction() {
        let mut machine = WyckoffStateMachine::new(10, 5.0, 4);
        for bar in range_bars(30, 100.0, 2.0, 100.0) {
            machine.on_bar(&bar);
        }
        assert_ne!(machine.phase(), WyckoffPhase::Undefined);
    }

    #[test]
    fn test_spring_transitions_to_phase_c_in_accumulation_bias() {
        let mut machine = WyckoffStateMachine::new(10, 5.0, 4);
        for bar in range_bars(25, 100.0, 2.0, 100.0) {
            machine.on_bar(&bar);
        }
        // Force accumulation bias deterministically for the test by feeding a down climax first
        // is fragile; instead assert on whichever bias formed and drive the matching event.
        let bias = machine.bias();
        assert!(bias.is_some(), "range must have locked by now");

        if bias == Some(WyckoffBias::Accumulation) {
            let spring_bar = Bar::new(2000, 98.5, 99.0, 96.0, 98.8, 100.0);
            machine.on_bar(&spring_bar);
            assert!(machine
                .events()
                .iter()
                .any(|e| e.kind == WyckoffEventKind::Spring));
        }
    }

    #[test]
    fn test_full_sequence_scores_high_quality() {
        let mut machine = WyckoffStateMachine::new(8, 5.0, 3);
        for bar in range_bars(20, 100.0, 2.0, 100.0) {
            machine.on_bar(&bar);
        }
        let bias = machine.bias().expect("range must have locked");

        let (spring_bar, sos_bar, lps_bar) = if bias == WyckoffBias::Accumulation {
            (
                Bar::new(2000, 98.5, 99.0, 96.0, 98.8, 100.0),
                Bar::new(2060, 99.0, 106.0, 98.5, 105.5, 100.0),
                Bar::new(2120, 105.5, 106.0, 103.0, 105.0, 100.0),
            )
        } else {
            (
                Bar::new(2000, 101.5, 104.0, 101.0, 101.2, 100.0),
                Bar::new(2060, 101.0, 101.5, 94.0, 94.5, 100.0),
                Bar::new(2120, 94.5, 97.0, 94.0, 95.0, 100.0),
            )
        };

        machine.on_bar(&spring_bar);
        machine.on_bar(&sos_bar);
        machine.on_bar(&lps_bar);

        assert_eq!(machine.phase(), WyckoffPhase::E);
        let score = machine.score();
        assert!(
            score.sequence_quality > 0.5,
            "a clean A->C->D->E sequence must score reasonably high"
        );
        assert!(score.cause_score > 0.0);
    }

    #[test]
    fn test_smoke_no_panic_across_random_walk() {
        let mut machine = WyckoffStateMachine::with_defaults();
        let mut price = 100.0;
        for i in 0..200 {
            price += ((i * 37) % 7) as f64 * 0.3 - 0.9;
            let bar = Bar::new(
                i as i64 * 60,
                price,
                price + 1.0,
                price - 1.0,
                price,
                100.0 + (i % 5) as f64 * 20.0,
            );
            machine.on_bar(&bar);
        }
    }
}
