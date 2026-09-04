//! Versioned state snapshots for long-running engines.
//!
//! Any `Clone` state (an `Indicator`, an `engine::*` tracker, a composition graph) can be captured
//! into a [`Checkpoint`] and restored later, in-process, to deterministically save/rewind
//! execution. When `S` also implements `serde::Serialize`/`Deserialize` (behind the `serde`
//! feature), the checkpoint itself becomes serializable for cross-process persistence — no
//! separate persistence API is required.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A versioned, timestamped snapshot of some `Clone`-able engine state `S`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Checkpoint<S: Clone> {
    /// Monotonically increasing checkpoint version, incremented by the caller on each capture.
    /// Lets consumers detect stale/out-of-order restores.
    pub version: u32,
    /// The bar timestamp the state reflects (i.e. state after processing this bar).
    pub timestamp: i64,
    pub state: S,
}

impl<S: Clone> Checkpoint<S> {
    /// Captures a snapshot of `state` at `timestamp` tagged with `version`.
    pub fn capture(state: &S, timestamp: i64, version: u32) -> Self {
        Self {
            version,
            timestamp,
            state: state.clone(),
        }
    }

    /// Overwrites `target` with this checkpoint's state.
    pub fn restore_into(&self, target: &mut S) {
        *target = self.state.clone();
    }
}

/// Keeps the single most recent [`Checkpoint`] for `S`, auto-incrementing the version on every
/// [`CheckpointStore::save`]. Suited for a "last confirmed state" rewind point on a long-running
/// engine.
#[derive(Debug, Clone, Default)]
pub struct CheckpointStore<S: Clone> {
    latest: Option<Checkpoint<S>>,
    next_version: u32,
}

impl<S: Clone> CheckpointStore<S> {
    pub fn new() -> Self {
        Self {
            latest: None,
            next_version: 0,
        }
    }

    /// Captures `state` as the new latest checkpoint, replacing any prior one.
    pub fn save(&mut self, state: &S, timestamp: i64) -> u32 {
        let version = self.next_version;
        self.next_version += 1;
        self.latest = Some(Checkpoint::capture(state, timestamp, version));
        version
    }

    pub fn latest(&self) -> Option<&Checkpoint<S>> {
        self.latest.as_ref()
    }

    /// Restores `target` from the latest checkpoint, if one exists.
    pub fn restore_into(&self, target: &mut S) -> bool {
        match &self.latest {
            Some(checkpoint) => {
                checkpoint.restore_into(target);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_capture_and_restore() {
        let state = vec![1, 2, 3];
        let checkpoint = Checkpoint::capture(&state, 1_000, 0);
        assert_eq!(checkpoint.version, 0);
        assert_eq!(checkpoint.timestamp, 1_000);

        let mut target = vec![9, 9];
        checkpoint.restore_into(&mut target);
        assert_eq!(target, state);
    }

    #[test]
    fn test_checkpoint_store_versions_increment() {
        let mut store: CheckpointStore<i32> = CheckpointStore::new();
        assert!(store.latest().is_none());

        let v0 = store.save(&10, 1_000);
        let v1 = store.save(&20, 1_060);
        assert_eq!(v0, 0);
        assert_eq!(v1, 1);
        assert_eq!(store.latest().unwrap().state, 20);

        let mut target = 0;
        assert!(store.restore_into(&mut target));
        assert_eq!(target, 20);
    }
}
