//! Generic composite scenario state machine: a reusable multi-stage progression with per-stage
//! expiry ("Ablauf") and explicit invalidation, generic over any caller-defined stage enum —
//! rather than one hand-rolled state machine per scenario shape. Ships with three concrete
//! presets matching the doc's named examples: Edge -> Setup -> Watch -> Trigger, Armed Balance ->
//! Breakout -> Aftermath, and Direct/Pullback/Failure.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioStatus {
    Active,
    Expired,
    Invalidated,
    /// Reached a configured terminal stage.
    Completed,
}

/// Per-stage configuration: how many bars may elapse in this stage before it expires
/// (`None` = no expiry).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageConfig<S> {
    pub stage: S,
    pub max_bars: Option<u32>,
}

/// A generic multi-stage scenario progression.
pub struct ScenarioStateMachine<S: Copy + PartialEq> {
    stages: Vec<StageConfig<S>>,
    terminal_stages: Vec<S>,
    current: S,
    bars_in_stage: u32,
    status: ScenarioStatus,
    history: Vec<(S, i64)>,
}

impl<S: Copy + PartialEq> ScenarioStateMachine<S> {
    pub fn new(
        stages: Vec<StageConfig<S>>,
        terminal_stages: Vec<S>,
        initial: S,
        started_at: i64,
    ) -> Self {
        Self {
            stages,
            terminal_stages,
            current: initial,
            bars_in_stage: 0,
            status: ScenarioStatus::Active,
            history: vec![(initial, started_at)],
        }
    }

    pub fn status(&self) -> ScenarioStatus {
        self.status
    }

    pub fn current_stage(&self) -> S {
        self.current
    }

    /// Every stage entered so far, oldest first, with the timestamp it was entered.
    pub fn history(&self) -> &[(S, i64)] {
        &self.history
    }

    pub fn bars_in_stage(&self) -> u32 {
        self.bars_in_stage
    }

    /// Advances one bar without a stage change; expires the scenario if the current stage's
    /// `max_bars` is exceeded. No-op once the machine is no longer `Active`.
    pub fn tick(&mut self) {
        if self.status != ScenarioStatus::Active {
            return;
        }
        self.bars_in_stage += 1;
        if let Some(cfg) = self.stages.iter().find(|c| c.stage == self.current) {
            if let Some(max) = cfg.max_bars {
                if self.bars_in_stage > max {
                    self.status = ScenarioStatus::Expired;
                }
            }
        }
    }

    /// Transitions to `next_stage`, resetting the per-stage bar counter. Returns `false` (no-op)
    /// if the machine is not currently `Active`. Marks the machine `Completed` if `next_stage` is
    /// one of the configured terminal stages.
    pub fn advance(&mut self, next_stage: S, timestamp: i64) -> bool {
        if self.status != ScenarioStatus::Active {
            return false;
        }
        self.current = next_stage;
        self.bars_in_stage = 0;
        self.history.push((next_stage, timestamp));
        if self.terminal_stages.contains(&next_stage) {
            self.status = ScenarioStatus::Completed;
        }
        true
    }

    /// Explicitly invalidates the scenario (e.g. a structural break of its setup premise).
    /// No-op once no longer `Active`.
    pub fn invalidate(&mut self) {
        if self.status == ScenarioStatus::Active {
            self.status = ScenarioStatus::Invalidated;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Named presets
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSetupStage {
    Edge,
    Setup,
    Watch,
    Trigger,
}

/// Edge -> Setup -> Watch -> Trigger, each stage sharing the same expiry window.
pub fn edge_setup_watch_trigger_machine(
    max_bars_per_stage: u32,
    started_at: i64,
) -> ScenarioStateMachine<EdgeSetupStage> {
    let stages = [
        EdgeSetupStage::Edge,
        EdgeSetupStage::Setup,
        EdgeSetupStage::Watch,
        EdgeSetupStage::Trigger,
    ]
    .into_iter()
    .map(|stage| StageConfig {
        stage,
        max_bars: Some(max_bars_per_stage),
    })
    .collect();
    ScenarioStateMachine::new(
        stages,
        vec![EdgeSetupStage::Trigger],
        EdgeSetupStage::Edge,
        started_at,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceBreakoutStage {
    ArmedBalance,
    Breakout,
    Aftermath,
}

/// Armed Balance -> Breakout -> Aftermath.
pub fn armed_balance_breakout_aftermath_machine(
    armed_max_bars: u32,
    breakout_max_bars: u32,
    started_at: i64,
) -> ScenarioStateMachine<BalanceBreakoutStage> {
    let stages = vec![
        StageConfig {
            stage: BalanceBreakoutStage::ArmedBalance,
            max_bars: Some(armed_max_bars),
        },
        StageConfig {
            stage: BalanceBreakoutStage::Breakout,
            max_bars: Some(breakout_max_bars),
        },
        StageConfig {
            stage: BalanceBreakoutStage::Aftermath,
            max_bars: None,
        },
    ];
    ScenarioStateMachine::new(
        stages,
        vec![BalanceBreakoutStage::Aftermath],
        BalanceBreakoutStage::ArmedBalance,
        started_at,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullbackOutcomeStage {
    Direct,
    Pullback,
    Failure,
}

/// Direct/Pullback/Failure: three mutually-exclusive resolutions of one setup, all terminal (no
/// further stage follows any of them).
pub fn direct_pullback_failure_machine(
    max_bars: u32,
    started_at: i64,
) -> ScenarioStateMachine<PullbackOutcomeStage> {
    let stages = [
        PullbackOutcomeStage::Direct,
        PullbackOutcomeStage::Pullback,
        PullbackOutcomeStage::Failure,
    ]
    .into_iter()
    .map(|stage| StageConfig {
        stage,
        max_bars: Some(max_bars),
    })
    .collect();
    ScenarioStateMachine::new(
        stages,
        vec![
            PullbackOutcomeStage::Direct,
            PullbackOutcomeStage::Pullback,
            PullbackOutcomeStage::Failure,
        ],
        PullbackOutcomeStage::Direct,
        started_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advances_through_stages_and_completes() {
        let mut machine = edge_setup_watch_trigger_machine(10, 0);
        assert_eq!(machine.current_stage(), EdgeSetupStage::Edge);
        assert_eq!(machine.status(), ScenarioStatus::Active);

        assert!(machine.advance(EdgeSetupStage::Setup, 60));
        assert!(machine.advance(EdgeSetupStage::Watch, 120));
        assert!(machine.advance(EdgeSetupStage::Trigger, 180));

        assert_eq!(machine.status(), ScenarioStatus::Completed);
        assert_eq!(machine.history().len(), 4);
        // No further advance is accepted once completed.
        assert!(!machine.advance(EdgeSetupStage::Edge, 240));
    }

    #[test]
    fn test_expires_after_max_bars_in_stage() {
        let mut machine = edge_setup_watch_trigger_machine(3, 0);
        for _ in 0..3 {
            machine.tick();
            assert_eq!(machine.status(), ScenarioStatus::Active);
        }
        machine.tick();
        assert_eq!(machine.status(), ScenarioStatus::Expired);
    }

    #[test]
    fn test_tick_resets_on_advance() {
        let mut machine = edge_setup_watch_trigger_machine(2, 0);
        machine.tick();
        machine.tick();
        assert!(machine.advance(EdgeSetupStage::Setup, 60));
        // Bar counter reset by the transition, so two more ticks must not expire it yet.
        machine.tick();
        machine.tick();
        assert_eq!(machine.status(), ScenarioStatus::Active);
    }

    #[test]
    fn test_invalidate_is_terminal_and_blocks_further_advance() {
        let mut machine = edge_setup_watch_trigger_machine(10, 0);
        machine.advance(EdgeSetupStage::Setup, 60);
        machine.invalidate();
        assert_eq!(machine.status(), ScenarioStatus::Invalidated);
        assert!(!machine.advance(EdgeSetupStage::Watch, 120));
        // Invalidating twice is a harmless no-op.
        machine.invalidate();
        assert_eq!(machine.status(), ScenarioStatus::Invalidated);
    }

    #[test]
    fn test_armed_balance_breakout_aftermath_preset() {
        let mut machine = armed_balance_breakout_aftermath_machine(5, 3, 0);
        assert!(machine.advance(BalanceBreakoutStage::Breakout, 60));
        assert!(machine.advance(BalanceBreakoutStage::Aftermath, 120));
        assert_eq!(machine.status(), ScenarioStatus::Completed);
    }

    #[test]
    fn test_direct_pullback_failure_preset_each_branch_is_terminal() {
        let mut machine = direct_pullback_failure_machine(5, 0);
        assert!(machine.advance(PullbackOutcomeStage::Failure, 60));
        assert_eq!(machine.status(), ScenarioStatus::Completed);
    }
}
