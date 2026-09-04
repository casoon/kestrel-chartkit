use super::{SignalEvaluationRecord, TradeOutcome};
use crate::model::Bar;
use crate::signal::TriggerAction;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Active setup tracking record for lookahead-free forward evaluation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ActiveSetup {
    pub id: u64,
    pub timestamp: i64,
    pub trigger: TriggerAction,
    pub score: f64,
    pub confidence: f64,
    pub entry_price: f64,
    pub target_price: f64,
    pub stop_price: f64,
    pub max_favorable_excursion: f64,
    pub max_adverse_excursion: f64,
    pub duration_bars: usize,
    pub max_duration_bars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntrabarFillPolicy {
    #[default]
    StopFirst,
    TargetFirst,
    NearestToOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordSetupError {
    UnsupportedTrigger,
    NonFiniteValue,
    InvalidPriceGeometry,
    ZeroDuration,
}

impl fmt::Display for RecordSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedTrigger => "only buy and sell setups can be recorded",
            Self::NonFiniteValue => "setup values must be finite",
            Self::InvalidPriceGeometry => "target, entry, and stop are not ordered for the trigger",
            Self::ZeroDuration => "maximum duration must be greater than zero",
        })
    }
}

impl std::error::Error for RecordSetupError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutcomeExcursion {
    pub setup_id: u64,
    pub max_favorable_excursion: f64,
    pub max_adverse_excursion: f64,
}

/// Bar-by-bar lookahead-free evaluation recorder for active strategy setups.
#[derive(Debug, Clone, Default)]
pub struct OutcomeRecorder {
    next_id: u64,
    active_setups: Vec<ActiveSetup>,
    completed_records: Vec<SignalEvaluationRecord>,
    completed_excursions: Vec<OutcomeExcursion>,
    intrabar_fill_policy: IntrabarFillPolicy,
}

impl OutcomeRecorder {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            active_setups: Vec::new(),
            completed_records: Vec::new(),
            completed_excursions: Vec::new(),
            intrabar_fill_policy: IntrabarFillPolicy::StopFirst,
        }
    }

    pub fn with_intrabar_fill_policy(policy: IntrabarFillPolicy) -> Self {
        Self {
            intrabar_fill_policy: policy,
            ..Self::new()
        }
    }

    /// Registers a new active trade setup for forward evaluation.
    #[allow(clippy::too_many_arguments)]
    pub fn record_setup(
        &mut self,
        timestamp: i64,
        trigger: TriggerAction,
        score: f64,
        confidence: f64,
        entry_price: f64,
        target_price: f64,
        stop_price: f64,
        max_duration_bars: usize,
    ) -> Result<u64, RecordSetupError> {
        if !matches!(trigger, TriggerAction::Buy | TriggerAction::Sell) {
            return Err(RecordSetupError::UnsupportedTrigger);
        }
        if ![score, confidence, entry_price, target_price, stop_price]
            .into_iter()
            .all(f64::is_finite)
        {
            return Err(RecordSetupError::NonFiniteValue);
        }
        let valid_geometry = match trigger {
            TriggerAction::Buy => stop_price < entry_price && entry_price < target_price,
            TriggerAction::Sell => target_price < entry_price && entry_price < stop_price,
            _ => false,
        };
        if !valid_geometry {
            return Err(RecordSetupError::InvalidPriceGeometry);
        }
        if max_duration_bars == 0 {
            return Err(RecordSetupError::ZeroDuration);
        }
        let id = self.next_id;
        self.next_id += 1;

        self.active_setups.push(ActiveSetup {
            id,
            timestamp,
            trigger,
            score,
            confidence,
            entry_price,
            target_price,
            stop_price,
            max_favorable_excursion: 0.0,
            max_adverse_excursion: 0.0,
            duration_bars: 0,
            max_duration_bars,
        });

        Ok(id)
    }

    /// Processes an incoming price bar, updating MFE/MAE and checking for target/stop hits.
    pub fn on_bar(&mut self, bar: &Bar) {
        let mut unresolved = Vec::new();

        for mut setup in self.active_setups.drain(..) {
            setup.duration_bars += 1;

            let (favorable, adverse) = match setup.trigger {
                TriggerAction::Buy => (bar.high - setup.entry_price, setup.entry_price - bar.low),
                TriggerAction::Sell => (setup.entry_price - bar.low, bar.high - setup.entry_price),
                _ => (0.0, 0.0),
            };

            setup.max_favorable_excursion = setup.max_favorable_excursion.max(favorable);
            setup.max_adverse_excursion = setup.max_adverse_excursion.max(adverse);

            let initial_risk = (setup.entry_price - setup.stop_price).abs().max(1e-8);

            let hit_target = match setup.trigger {
                TriggerAction::Buy => bar.high >= setup.target_price,
                TriggerAction::Sell => bar.low <= setup.target_price,
                _ => false,
            };

            let hit_stop = match setup.trigger {
                TriggerAction::Buy => bar.low <= setup.stop_price,
                TriggerAction::Sell => bar.high >= setup.stop_price,
                _ => false,
            };

            if hit_target || hit_stop || setup.duration_bars >= setup.max_duration_bars {
                let target_wins = if hit_target && hit_stop {
                    match self.intrabar_fill_policy {
                        IntrabarFillPolicy::StopFirst => false,
                        IntrabarFillPolicy::TargetFirst => true,
                        IntrabarFillPolicy::NearestToOpen => {
                            (bar.open - setup.target_price).abs()
                                < (bar.open - setup.stop_price).abs()
                        }
                    }
                } else {
                    hit_target
                };
                let outcome = if target_wins {
                    TradeOutcome::Win
                } else if hit_stop {
                    TradeOutcome::Loss
                } else {
                    TradeOutcome::Expired
                };

                let exit_price = if target_wins {
                    setup.target_price
                } else if hit_stop {
                    setup.stop_price
                } else {
                    bar.close
                };

                let realized_r = match setup.trigger {
                    TriggerAction::Buy => (exit_price - setup.entry_price) / initial_risk,
                    TriggerAction::Sell => (setup.entry_price - exit_price) / initial_risk,
                    _ => 0.0,
                };

                self.completed_records.push(SignalEvaluationRecord {
                    timestamp: setup.timestamp,
                    trigger: setup.trigger,
                    score: setup.score,
                    confidence: setup.confidence,
                    entry_price: setup.entry_price,
                    exit_price,
                    realized_r_multiple: realized_r,
                    duration_bars: setup.duration_bars as u32,
                    outcome,
                });
                self.completed_excursions.push(OutcomeExcursion {
                    setup_id: setup.id,
                    max_favorable_excursion: setup.max_favorable_excursion,
                    max_adverse_excursion: setup.max_adverse_excursion,
                });
            } else {
                unresolved.push(setup);
            }
        }

        self.active_setups = unresolved;
    }

    pub fn completed_records(&self) -> &[SignalEvaluationRecord] {
        &self.completed_records
    }

    pub fn active_setups(&self) -> &[ActiveSetup] {
        &self.active_setups
    }

    pub fn completed_excursions(&self) -> &[OutcomeExcursion] {
        &self.completed_excursions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outcome_recorder_win() {
        let mut recorder = OutcomeRecorder::new();
        let id = recorder
            .record_setup(1000, TriggerAction::Buy, 0.8, 0.9, 100.0, 105.0, 95.0, 10)
            .unwrap();
        assert_eq!(id, 1);

        // Bar 1 hits target 105.0
        let bar = Bar::new(1000, 100.0, 106.0, 99.0, 105.0, 1000.0);
        recorder.on_bar(&bar);

        assert!(recorder.active_setups().is_empty());
        assert_eq!(recorder.completed_records().len(), 1);
        let rec = &recorder.completed_records()[0];
        assert_eq!(rec.outcome, TradeOutcome::Win);
        assert_eq!(rec.realized_r_multiple, 1.0); // (105 - 100) / (100 - 95) = 5 / 5 = 1.0
        assert_eq!(
            recorder.completed_excursions()[0].max_favorable_excursion,
            6.0
        );
    }

    #[test]
    fn same_bar_stop_and_target_use_explicit_policy() {
        let bar = Bar::new(1, 100.0, 106.0, 94.0, 100.0, 1.0);
        let mut conservative = OutcomeRecorder::new();
        conservative
            .record_setup(0, TriggerAction::Buy, 1.0, 1.0, 100.0, 105.0, 95.0, 10)
            .unwrap();
        conservative.on_bar(&bar);
        assert_eq!(
            conservative.completed_records()[0].outcome,
            TradeOutcome::Loss
        );

        let mut optimistic =
            OutcomeRecorder::with_intrabar_fill_policy(IntrabarFillPolicy::TargetFirst);
        optimistic
            .record_setup(0, TriggerAction::Buy, 1.0, 1.0, 100.0, 105.0, 95.0, 10)
            .unwrap();
        optimistic.on_bar(&bar);
        assert_eq!(optimistic.completed_records()[0].outcome, TradeOutcome::Win);
    }

    #[test]
    fn rejects_non_directional_and_invalid_setups() {
        let mut recorder = OutcomeRecorder::new();
        assert_eq!(
            recorder.record_setup(0, TriggerAction::Hold, 0.0, 0.0, 100.0, 105.0, 95.0, 10),
            Err(RecordSetupError::UnsupportedTrigger)
        );
        assert_eq!(
            recorder.record_setup(0, TriggerAction::Buy, 0.0, 0.0, 100.0, 95.0, 105.0, 10),
            Err(RecordSetupError::InvalidPriceGeometry)
        );
    }
}
