//! Event and alert enrichment model.
//!
//! [`crate::indicator::IndicatorAlert`] stays a lightweight per-bar signal (kind/note/strength)
//! so every existing indicator keeps constructing it unchanged. [`AlertEvent`] is the layer a
//! runner/composition graph wraps around it once timestamp, instrument, and timeframe context is
//! known, adding a stable event ID, an explicit state phase, and deduplication support.

use std::collections::HashSet;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::indicator::IndicatorAlert;
use crate::timeframe::Timeframe;

/// Lifecycle phase of a tracked setup/alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum EventPhase {
    Setup,
    Watch,
    Trigger,
    Invalidation,
    Expiry,
    TargetHit,
}

impl fmt::Display for EventPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EventPhase::Setup => "setup",
            EventPhase::Watch => "watch",
            EventPhase::Trigger => "trigger",
            EventPhase::Invalidation => "invalidation",
            EventPhase::Expiry => "expiry",
            EventPhase::TargetHit => "target_hit",
        };
        f.write_str(s)
    }
}

/// An [`IndicatorAlert`] enriched with timestamp, instrument, timeframe, a stable event ID, and
/// an explicit lifecycle phase, suitable for deduplication and cross-bar tracking.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AlertEvent {
    pub alert: IndicatorAlert,
    pub timestamp: i64,
    pub phase: EventPhase,
    pub instrument: Option<String>,
    pub timeframe: Option<Timeframe>,
    pub event_id: String,
}

impl AlertEvent {
    /// Builds an event with a deterministic ID derived from the alert kind, phase, and timestamp,
    /// so re-emitting the same logical event on the same bar always yields the same ID.
    pub fn new(alert: IndicatorAlert, timestamp: i64, phase: EventPhase) -> Self {
        let event_id = format!("{}:{}:{}", alert.kind, phase, timestamp);
        Self {
            alert,
            timestamp,
            phase,
            instrument: None,
            timeframe: None,
            event_id,
        }
    }

    pub fn with_instrument(mut self, instrument: impl Into<String>) -> Self {
        self.instrument = Some(instrument.into());
        self
    }

    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.timeframe = Some(timeframe);
        self
    }
}

/// Tracks event IDs already admitted so repeated recalculation (e.g. rollback/idempotent replay
/// of the same bar) does not re-emit duplicate alerts downstream.
#[derive(Debug, Clone, Default)]
pub struct AlertDeduplicator {
    seen: HashSet<String>,
}

impl AlertDeduplicator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` and records the event if its ID has not been seen before; returns `false`
    /// without side effects if it is a duplicate.
    pub fn admit(&mut self, event: &AlertEvent) -> bool {
        self.seen.insert(event.event_id.clone())
    }

    pub fn reset(&mut self) {
        self.seen.clear();
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_event_deterministic_id() {
        let alert = IndicatorAlert::new("cross_up", "RSI crossed above 70", 0.8);
        let event_a = AlertEvent::new(alert.clone(), 1_000, EventPhase::Trigger);
        let event_b = AlertEvent::new(alert, 1_000, EventPhase::Trigger);
        assert_eq!(event_a.event_id, event_b.event_id);
    }

    #[test]
    fn test_alert_event_builders() {
        let alert = IndicatorAlert::new("cross_up", "RSI crossed above 70", 0.8);
        let event = AlertEvent::new(alert, 1_000, EventPhase::Watch)
            .with_instrument("GENERIC")
            .with_timeframe(Timeframe::Minute(5));
        assert_eq!(event.instrument.as_deref(), Some("GENERIC"));
        assert_eq!(event.timeframe, Some(Timeframe::Minute(5)));
    }

    #[test]
    fn test_alert_deduplicator() {
        let alert = IndicatorAlert::new("cross_up", "RSI crossed above 70", 0.8);
        let event = AlertEvent::new(alert.clone(), 1_000, EventPhase::Trigger);
        let mut dedup = AlertDeduplicator::new();

        assert!(dedup.admit(&event));
        assert!(
            !dedup.admit(&event),
            "duplicate event must not be re-admitted"
        );
        assert_eq!(dedup.len(), 1);

        let other_bar = AlertEvent::new(alert, 1_060, EventPhase::Trigger);
        assert!(dedup.admit(&other_bar), "distinct timestamp is a new event");
        assert_eq!(dedup.len(), 2);

        dedup.reset();
        assert!(dedup.is_empty());
    }
}
