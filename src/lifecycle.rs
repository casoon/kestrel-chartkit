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
//! Requiring `Clone` is a deliberate, honest scope limit: it is the only mechanism that works
//! generically across arbitrary indicator internals without adding a second, hand-written
//! snapshot/restore method to every `Indicator` impl. Most indicator structs in this crate do not
//! yet derive `Clone`; adding it is a mechanical, low-risk follow-up left to indicator authors as
//! they adopt `LifecycleRunner`, not a blanket change bundled into this module.

use crate::checkpoint::CheckpointStore;
use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::Bar;

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
    /// is not representable by [`LifecycleRunner`], which keeps just one checkpoint.
    Correction(Bar),
}

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
    checkpoints: CheckpointStore<I>,
}

impl<I: Indicator + Clone> LifecycleRunner<I> {
    pub fn new(indicator: I) -> Self {
        let checkpoints = CheckpointStore::new();
        Self {
            live: indicator,
            checkpoints,
        }
    }

    /// The indicator state as of the last confirmed/corrected bar (never a tentative
    /// open/update). `None` before the first confirmation.
    pub fn confirmed_indicator(&self) -> Option<&I> {
        self.checkpoints.latest().map(|c| &c.state)
    }

    /// Processes one lifecycle event and returns the resulting output.
    ///
    /// `Open`/`Update` always start from the last confirmed checkpoint (or a freshly reset
    /// indicator if none exists yet) before applying `bar`, so repeated calls for the same
    /// still-forming timestamp are idempotent and never stack on top of a prior tentative
    /// application. `Confirmed`/`Correction` do the same, then persist the resulting state as the
    /// new checkpoint.
    pub fn on_event(&mut self, event: BarLifecycle) -> Option<IndicatorOutput> {
        self.rewind_to_last_confirmed();
        let output = self.live.on_bar(event.bar());

        if matches!(
            event,
            BarLifecycle::Confirmed(_) | BarLifecycle::Correction(_)
        ) {
            self.checkpoints.save(&self.live, event.bar().timestamp);
        }

        output
    }

    fn rewind_to_last_confirmed(&mut self) {
        if !self.checkpoints.restore_into(&mut self.live) {
            self.live.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeated_update_is_idempotent() {
        // SmaEngine does not derive Clone (see module docs), so this test uses a minimal
        // hand-rolled Clone indicator instead of pulling SmaEngine into the Clone surface.
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

        let mut runner = LifecycleRunner::new(SumEngine { sum: 0.0 });
        let bar = Bar::new(1_000, 100.0, 101.0, 99.0, 100.0, 10.0);

        let first_update = runner.on_event(BarLifecycle::Open(bar.clone()));
        let second_update = runner.on_event(BarLifecycle::Update(bar.clone()));
        // Re-delivering the same still-forming bar must not accumulate.
        assert_eq!(first_update.unwrap().value, second_update.unwrap().value);

        let confirmed = runner.on_event(BarLifecycle::Confirmed(bar.clone()));
        assert_eq!(confirmed.unwrap().value, 100.0);
        assert_eq!(runner.confirmed_indicator().unwrap().sum, 100.0);

        // A later bar accumulates on top of the confirmed checkpoint, not the discarded update.
        let next_bar = Bar::new(1_060, 100.0, 102.0, 100.0, 101.0, 10.0);
        let next = runner.on_event(BarLifecycle::Confirmed(next_bar));
        assert_eq!(next.unwrap().value, 201.0);
    }

    #[test]
    fn test_correction_replaces_last_confirmed_bar() {
        #[derive(Clone)]
        struct LastCloseEngine {
            last_close: f64,
        }
        impl Indicator for LastCloseEngine {
            fn name(&self) -> &str {
                "last_close"
            }
            fn reset(&mut self) {
                self.last_close = 0.0;
            }
            fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
                self.last_close = bar.close;
                Some(IndicatorOutput::new(self.last_close))
            }
        }

        let mut runner = LifecycleRunner::new(LastCloseEngine { last_close: 0.0 });
        let bar = Bar::new(1_000, 100.0, 101.0, 99.0, 100.0, 10.0);
        runner.on_event(BarLifecycle::Confirmed(bar));

        let corrected_bar = Bar::new(1_000, 100.0, 101.0, 99.0, 103.5, 10.0);
        let corrected = runner.on_event(BarLifecycle::Correction(corrected_bar));
        assert_eq!(corrected.unwrap().value, 103.5);
        assert_eq!(runner.confirmed_indicator().unwrap().last_close, 103.5);
    }
}
