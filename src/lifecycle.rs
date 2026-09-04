//! Bar lifecycle events and rollback-safe, idempotent recomputation.
//!
//! [`Indicator::on_bar`](crate::indicator::Indicator::on_bar) takes a single incoming bar and
//! permanently advances the indicator's state — it has no notion of a bar that is still forming
//! (Pine's `barstate.isconfirmed == false`) versus one that has closed, and no way to reprocess a
//! bar without double-counting it.
//!
//! [`BarLifecycle`] names the four events a feed can emit for a given timestamp. [`LifecycleRunner`]
//! wraps any `Indicator + Clone` and makes [`BarLifecycle::Update`]/[`BarLifecycle::Correction`]
//! safe to call repeatedly: it always recomputes from the last *confirmed* checkpoint rather than
//! mutating forward, so re-delivering the same revised bar is idempotent and never double-applies
//! a still-forming bar's data.
//!
//! [`LifecycleRunner`] keeps two checkpoints for the last confirmed bar: the state immediately
//! *before* it was applied and the state immediately *after*. A [`BarLifecycle::Correction`] (or a
//! repeated [`BarLifecycle::Confirmed`] for the same timestamp) restores the pre-bar checkpoint and
//! applies the revised bar exactly once, so the corrected bar *replaces* the original rather than
//! stacking on top of it. Only the most recently confirmed bar can be corrected this way — a
//! correction or an out-of-order confirmation targeting an older timestamp is rejected with
//! [`LifecycleError`] rather than silently mutating an unrelated state.
//!
//! Requiring `Clone` is a deliberate, honest scope limit: it is the only mechanism that works
//! generically across arbitrary indicator internals without adding a second, hand-written
//! snapshot/restore method to every `Indicator` impl. Most indicator structs in this crate do not
//! yet derive `Clone`; adding it is a mechanical, low-risk follow-up left to indicator authors as
//! they adopt `LifecycleRunner`, not a blanket change bundled into this module.

use crate::checkpoint::CheckpointStore;
use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::Bar;
use std::fmt;

/// A bar-lifecycle event for a single timestamp.
#[derive(Debug, Clone, PartialEq)]
pub enum BarLifecycle {
    /// The first tick of a new bar/timestamp.
    Open(Bar),
    /// A subsequent, still-unconfirmed tick for the same timestamp as the last `Open`/`Update`
    /// (Pine's `barstate.isconfirmed == false`).
    Update(Bar),
    /// The bar for this timestamp is final and will not be revised again.
    Confirmed(Bar),
    /// A feed correction to the most recently confirmed bar (e.g. an exchange restatement).
    /// Only single-level undo is supported: correcting a bar older than the last confirmed one
    /// is not representable by [`LifecycleRunner`], which keeps just one pre/post checkpoint pair
    /// and rejects such a correction with [`LifecycleError::CorrectionTimestampMismatch`].
    Correction(Bar),
}

/// An error produced by [`LifecycleRunner::on_event`] when an event cannot be applied without
/// losing or misrepresenting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// A [`BarLifecycle::Correction`] targeted a timestamp other than the last confirmed bar's.
    /// `LifecycleRunner` only supports single-level undo: it can replace the most recently
    /// confirmed bar, not an older one.
    CorrectionTimestampMismatch {
        /// The timestamp of the last confirmed bar, if any.
        last_confirmed_timestamp: Option<i64>,
        /// The timestamp the correction targeted.
        attempted_timestamp: i64,
    },
    /// A [`BarLifecycle::Confirmed`] event arrived for a timestamp strictly older than the last
    /// confirmed bar. `LifecycleRunner` cannot represent this: only the most recently confirmed
    /// bar can be replaced (via a matching `Confirmed` or `Correction`).
    NonMonotonicConfirmation {
        /// The timestamp of the last confirmed bar.
        last_confirmed_timestamp: i64,
        /// The timestamp the out-of-order confirmation targeted.
        attempted_timestamp: i64,
    },
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecycleError::CorrectionTimestampMismatch {
                last_confirmed_timestamp,
                attempted_timestamp,
            } => write!(
                f,
                "correction for timestamp {attempted_timestamp} does not match the last \
                 confirmed timestamp {last_confirmed_timestamp:?}; only the most recently \
                 confirmed bar can be corrected"
            ),
            LifecycleError::NonMonotonicConfirmation {
                last_confirmed_timestamp,
                attempted_timestamp,
            } => write!(
                f,
                "confirmed timestamp {attempted_timestamp} is older than the last confirmed \
                 timestamp {last_confirmed_timestamp}; only the most recently confirmed bar can \
                 be replaced"
            ),
        }
    }
}

impl std::error::Error for LifecycleError {}

impl BarLifecycle {
    pub fn bar(&self) -> &Bar {
        match self {
            BarLifecycle::Open(b)
            | BarLifecycle::Update(b)
            | BarLifecycle::Confirmed(b)
            | BarLifecycle::Correction(b) => b,
        }
    }
}

/// Wraps an `Indicator + Clone` so [`BarLifecycle::Open`]/[`BarLifecycle::Update`] events can be
/// replayed any number of times without permanently mutating state, and only
/// [`BarLifecycle::Confirmed`]/[`BarLifecycle::Correction`] commit a new checkpoint.
pub struct LifecycleRunner<I: Indicator + Clone> {
    live: I,
    /// State immediately before the last confirmed bar was applied (the rewind point for a
    /// correction/replacement of that bar).
    pre_confirmed: CheckpointStore<I>,
    /// State immediately after the last confirmed bar (the rewind point for `Open`/`Update`).
    post_confirmed: CheckpointStore<I>,
}

impl<I: Indicator + Clone> LifecycleRunner<I> {
    pub fn new(indicator: I) -> Self {
        Self {
            live: indicator,
            pre_confirmed: CheckpointStore::new(),
            post_confirmed: CheckpointStore::new(),
        }
    }

    /// The indicator state as of the last confirmed/corrected bar (never a tentative
    /// open/update). `None` before the first confirmation.
    pub fn confirmed_indicator(&self) -> Option<&I> {
        self.post_confirmed.latest().map(|c| &c.state)
    }

    /// Processes one lifecycle event and returns the resulting output.
    ///
    /// `Open`/`Update` always start from the last confirmed checkpoint (or a freshly reset
    /// indicator if none exists yet) before applying `bar`, so repeated calls for the same
    /// still-forming timestamp are idempotent and never stack on top of a prior tentative
    /// application.
    ///
    /// `Confirmed` for a new (strictly later, or first-ever) timestamp advances both checkpoints.
    /// `Confirmed` or `Correction` for the same timestamp as the last confirmed bar *replaces* it:
    /// the pre-bar checkpoint is restored and the revised bar is applied exactly once, instead of
    /// stacking on top of the original bar's effect. A `Correction`/out-of-order `Confirmed` for
    /// any other timestamp is rejected with [`LifecycleError`], since only single-level undo is
    /// representable.
    pub fn on_event(
        &mut self,
        event: BarLifecycle,
    ) -> Result<Option<IndicatorOutput>, LifecycleError> {
        match event {
            BarLifecycle::Open(bar) | BarLifecycle::Update(bar) => {
                self.rewind_to_last_confirmed();
                Ok(self.live.on_bar(&bar))
            }
            BarLifecycle::Confirmed(bar) => match self.last_confirmed_timestamp() {
                Some(ts) if bar.timestamp == ts => Ok(self.replace_last_confirmed(&bar)),
                Some(ts) if bar.timestamp < ts => Err(LifecycleError::NonMonotonicConfirmation {
                    last_confirmed_timestamp: ts,
                    attempted_timestamp: bar.timestamp,
                }),
                _ => {
                    self.rewind_to_last_confirmed();
                    let pre_state = self.live.clone();
                    let output = self.live.on_bar(&bar);
                    self.pre_confirmed.save(&pre_state, bar.timestamp);
                    self.post_confirmed.save(&self.live, bar.timestamp);
                    Ok(output)
                }
            },
            BarLifecycle::Correction(bar) => match self.last_confirmed_timestamp() {
                Some(ts) if bar.timestamp == ts => Ok(self.replace_last_confirmed(&bar)),
                last_ts => Err(LifecycleError::CorrectionTimestampMismatch {
                    last_confirmed_timestamp: last_ts,
                    attempted_timestamp: bar.timestamp,
                }),
            },
        }
    }

    /// Restores the checkpoint from immediately before the last confirmed bar and applies `bar`
    /// (which shares that bar's timestamp) exactly once, replacing it.
    fn replace_last_confirmed(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if !self.pre_confirmed.restore_into(&mut self.live) {
            self.live.reset();
        }
        let output = self.live.on_bar(bar);
        self.post_confirmed.save(&self.live, bar.timestamp);
        output
    }

    fn last_confirmed_timestamp(&self) -> Option<i64> {
        self.post_confirmed.latest().map(|c| c.timestamp)
    }

    fn rewind_to_last_confirmed(&mut self) {
        if !self.post_confirmed.restore_into(&mut self.live) {
            self.live.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::moving_averages::EmaEngine;

    // SmaEngine does not derive Clone (see module docs), so tests use a minimal hand-rolled Clone
    // indicator instead of pulling SmaEngine into the Clone surface. Being a cumulative sum, it
    // makes double-application (the bug in finding 01) visible: a `LastCloseEngine`-style
    // overwrite would hide it.
    #[derive(Clone)]
    struct SumEngine {
        sum: f64,
    }
    impl Indicator for SumEngine {
        fn name(&self) -> &str {
            "sum"
        }
        fn reset(&mut self) {
            self.sum = 0.0;
        }
        fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
            self.sum += bar.close;
            Some(IndicatorOutput::new(self.sum))
        }
    }

    #[test]
    fn test_repeated_update_is_idempotent() {
        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        let bar = Bar::new(1_000, 100.0, 101.0, 99.0, 100.0, 10.0);

        let first_update = runner.on_event(BarLifecycle::Open(bar.clone())).unwrap();
        let second_update = runner.on_event(BarLifecycle::Update(bar.clone())).unwrap();
        // Re-delivering the same still-forming bar must not accumulate.
        assert_eq!(first_update.unwrap().value, second_update.unwrap().value);

        let confirmed = runner
            .on_event(BarLifecycle::Confirmed(bar.clone()))
            .unwrap();
        assert_eq!(confirmed.unwrap().value, 100.0);
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 100.0);

        // A later bar accumulates on top of the confirmed checkpoint, not the discarded update.
        let next_bar = Bar::new(1_060, 100.0, 102.0, 100.0, 101.0, 10.0);
        let next = runner.on_event(BarLifecycle::Confirmed(next_bar)).unwrap();
        assert_eq!(next.unwrap().value, 201.0);
    }

    /// Minimal reproduction from finding 01: a correction must *replace* the last confirmed bar's
    /// contribution, not add to it. With the pre-fix implementation this asserted `203.5`.
    #[test]
    fn test_correction_replaces_rather_than_accumulates() {
        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        let bar = Bar::new(1_000, 100.0, 101.0, 99.0, 100.0, 10.0);
        runner.on_event(BarLifecycle::Confirmed(bar)).unwrap();

        let corrected_bar = Bar::new(1_000, 100.0, 101.0, 99.0, 103.5, 10.0);
        let corrected = runner
            .on_event(BarLifecycle::Correction(corrected_bar))
            .unwrap();
        assert_eq!(corrected.unwrap().value, 103.5);
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 103.5);
    }

    /// A correction to the second of two confirmed bars must replace only that bar; the first
    /// bar's contribution must survive untouched.
    #[test]
    fn test_correction_of_second_bar_preserves_first_bar() {
        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_000, 100.0, 101.0, 99.0, 100.0, 10.0,
            )))
            .unwrap();
        runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_060, 100.0, 102.0, 100.0, 101.0, 10.0,
            )))
            .unwrap();

        let corrected = runner
            .on_event(BarLifecycle::Correction(Bar::new(
                1_060, 100.0, 102.0, 100.0, 102.0, 10.0,
            )))
            .unwrap();
        assert_eq!(corrected.unwrap().value, 202.0);
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 202.0);
    }

    /// A correction whose timestamp does not match the last confirmed bar cannot be represented
    /// (only single-level undo is supported) and must be rejected explicitly rather than silently
    /// mutating an unrelated checkpoint.
    #[test]
    fn test_correction_with_mismatched_timestamp_is_rejected() {
        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_000, 100.0, 101.0, 99.0, 100.0, 10.0,
            )))
            .unwrap();
        runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_060, 100.0, 102.0, 100.0, 101.0, 10.0,
            )))
            .unwrap();

        // Targets the first bar's timestamp, not the last confirmed one.
        let err = runner
            .on_event(BarLifecycle::Correction(Bar::new(
                1_000, 100.0, 101.0, 99.0, 999.0, 10.0,
            )))
            .unwrap_err();
        assert_eq!(
            err,
            LifecycleError::CorrectionTimestampMismatch {
                last_confirmed_timestamp: Some(1_060),
                attempted_timestamp: 1_000,
            }
        );
        // State is unchanged after a rejected correction.
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 201.0);
    }

    /// A `Confirmed` event repeated for the same timestamp as the last confirmed bar behaves like
    /// a correction: it replaces rather than double-applies.
    #[test]
    fn test_repeated_confirmed_for_same_timestamp_replaces() {
        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        let bar = Bar::new(1_000, 100.0, 101.0, 99.0, 100.0, 10.0);
        runner.on_event(BarLifecycle::Confirmed(bar)).unwrap();

        let re_confirmed = Bar::new(1_000, 100.0, 101.0, 99.0, 105.0, 10.0);
        let output = runner
            .on_event(BarLifecycle::Confirmed(re_confirmed))
            .unwrap();
        assert_eq!(output.unwrap().value, 105.0);
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 105.0);
    }

    /// A `Confirmed` event for a timestamp older than the last confirmed bar is out-of-order and
    /// not representable; it must be rejected instead of silently corrupting state.
    #[test]
    fn test_confirmed_with_older_timestamp_is_rejected() {
        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_060, 100.0, 102.0, 100.0, 101.0, 10.0,
            )))
            .unwrap();

        let err = runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_000, 100.0, 101.0, 99.0, 100.0, 10.0,
            )))
            .unwrap_err();
        assert_eq!(
            err,
            LifecycleError::NonMonotonicConfirmation {
                last_confirmed_timestamp: 1_060,
                attempted_timestamp: 1_000,
            }
        );
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 101.0);
    }

    /// Same correction-replaces-rather-than-accumulates scenario, but against a real recursive
    /// indicator (`EmaEngine`) rather than the artificial `SumEngine`, so the fix is confirmed
    /// against production indicator state and not just a test double.
    #[test]
    fn test_correction_replaces_for_real_indicator() {
        let mut runner = LifecycleRunner::new(EmaEngine::new(2));
        let k = 2.0 / 3.0;

        runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_000, 100.0, 101.0, 99.0, 100.0, 10.0,
            )))
            .unwrap();
        let confirmed = runner
            .on_event(BarLifecycle::Confirmed(Bar::new(
                1_060, 100.0, 106.0, 100.0, 106.0, 10.0,
            )))
            .unwrap();
        let ema_after_second = 106.0 * k + 100.0 * (1.0 - k);
        assert!((confirmed.unwrap().value - ema_after_second).abs() < 1e-9);

        // Correcting the second bar's close must recompute EMA from the pre-second-bar state
        // (EMA = 100.0 after the first bar), not from the already-advanced post-second-bar EMA.
        let corrected = runner
            .on_event(BarLifecycle::Correction(Bar::new(
                1_060, 100.0, 109.0, 100.0, 109.0, 10.0,
            )))
            .unwrap();
        let expected_corrected_ema = 109.0 * k + 100.0 * (1.0 - k);
        assert!((corrected.unwrap().value - expected_corrected_ema).abs() < 1e-9);

        // A double-application bug would instead apply 109.0 on top of the post-second-bar EMA
        // (~104.0), producing a visibly different, larger value.
        let buggy_double_applied = 109.0 * k + ema_after_second * (1.0 - k);
        assert!((expected_corrected_ema - buggy_double_applied).abs() > 1e-6);
    }
}
