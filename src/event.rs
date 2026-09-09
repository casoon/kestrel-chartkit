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
///
/// `instrument` and `timeframe` are private (with [`AlertEvent::instrument`]/
/// [`AlertEvent::timeframe`] accessors) so that [`AlertEvent::with_instrument`]/
/// [`AlertEvent::with_timeframe`] are the only way to change them, guaranteeing `event_id` is
/// always recomputed from the complete, current identity context and can never go stale relative
/// to it — regardless of the order those builders are called in.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AlertEvent {
    pub alert: IndicatorAlert,
    pub timestamp: i64,
    pub phase: EventPhase,
    instrument: Option<String>,
    timeframe: Option<Timeframe>,
    pub event_id: String,
}

/// Encodes `s` with an explicit byte-length prefix so that a delimiter character occurring inside
/// `s` (e.g. a `:` in an instrument or alert-kind name) cannot be mistaken for a component
/// boundary when concatenated with other encoded components.
fn encode_component(s: &str) -> String {
    format!("{}#{}", s.len(), s)
}

/// Computes the full identity-based event ID from every field that distinguishes one logical
/// event from another: instrument, timeframe, alert kind, lifecycle phase, and bar timestamp. A
/// missing instrument/timeframe is encoded as an explicit empty component (distinct from any
/// non-empty value), so "no instrument" is a stable, singular identity rather than being
/// indistinguishable from some in-band placeholder string.
fn compute_event_id(
    kind: &str,
    phase: EventPhase,
    timestamp: i64,
    instrument: Option<&str>,
    timeframe: Option<Timeframe>,
) -> String {
    let instrument_component = encode_component(instrument.unwrap_or(""));
    let timeframe_component =
        encode_component(&timeframe.map(|tf| tf.to_string()).unwrap_or_default());
    let kind_component = encode_component(kind);
    let phase_component = encode_component(&phase.to_string());
    format!(
        "{instrument_component}:{timeframe_component}:{kind_component}:{phase_component}:{timestamp}"
    )
}

impl AlertEvent {
    /// Builds an event with a deterministic ID derived from the alert kind, phase, and timestamp
    /// (instrument/timeframe default to absent; attach them via [`AlertEvent::with_instrument`]/
    /// [`AlertEvent::with_timeframe`]), so re-emitting the same logical event on the same bar
    /// always yields the same ID.
    pub fn new(alert: IndicatorAlert, timestamp: i64, phase: EventPhase) -> Self {
        let event_id = compute_event_id(&alert.kind, phase, timestamp, None, None);
        Self {
            alert,
            timestamp,
            phase,
            instrument: None,
            timeframe: None,
            event_id,
        }
    }

    pub fn instrument(&self) -> Option<&str> {
        self.instrument.as_deref()
    }

    pub fn timeframe(&self) -> Option<Timeframe> {
        self.timeframe
    }

    /// Attaches an instrument and recomputes `event_id` from the complete identity context, so
    /// two events that only differ by instrument never collide.
    pub fn with_instrument(mut self, instrument: impl Into<String>) -> Self {
        self.instrument = Some(instrument.into());
        self.event_id = compute_event_id(
            &self.alert.kind,
            self.phase,
            self.timestamp,
            self.instrument.as_deref(),
            self.timeframe,
        );
        self
    }

    /// Attaches a timeframe and recomputes `event_id` from the complete identity context, so two
    /// events that only differ by timeframe never collide.
    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.timeframe = Some(timeframe);
        self.event_id = compute_event_id(
            &self.alert.kind,
            self.phase,
            self.timestamp,
            self.instrument.as_deref(),
            self.timeframe,
        );
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
        assert_eq!(event.instrument(), Some("GENERIC"));
        assert_eq!(event.timeframe(), Some(Timeframe::Minute(5)));
    }

    /// Finding 03: events that differ only by instrument must both be admitted by a shared
    /// deduplicator, not collide.
    #[test]
    fn test_event_id_distinguishes_instrument() {
        let alert = IndicatorAlert::new("cross_up", "crossed above trigger", 0.8);
        let dax = AlertEvent::new(alert.clone(), 1_700_000_000, EventPhase::Trigger)
            .with_instrument("DAX")
            .with_timeframe(Timeframe::Minute(5));
        let es = AlertEvent::new(alert, 1_700_000_000, EventPhase::Trigger)
            .with_instrument("ES")
            .with_timeframe(Timeframe::Minute(5));

        assert_ne!(dax.event_id, es.event_id);

        let mut dedup = AlertDeduplicator::new();
        assert!(dedup.admit(&dax));
        assert!(
            dedup.admit(&es),
            "same kind/phase/timestamp on a different instrument must not be treated as a duplicate"
        );
    }

    /// Finding 03: events that differ only by timeframe must both be admitted.
    #[test]
    fn test_event_id_distinguishes_timeframe() {
        let alert = IndicatorAlert::new("cross_up", "crossed above trigger", 0.8);
        let m5 = AlertEvent::new(alert.clone(), 1_700_000_000, EventPhase::Trigger)
            .with_instrument("DAX")
            .with_timeframe(Timeframe::Minute(5));
        let m15 = AlertEvent::new(alert, 1_700_000_000, EventPhase::Trigger)
            .with_instrument("DAX")
            .with_timeframe(Timeframe::Minute(15));

        assert_ne!(m5.event_id, m15.event_id);

        let mut dedup = AlertDeduplicator::new();
        assert!(dedup.admit(&m5));
        assert!(
            dedup.admit(&m15),
            "same instrument on a different timeframe must not be treated as a duplicate"
        );
    }

    /// A fully identical context (including instrument and timeframe) must still be deduplicated.
    #[test]
    fn test_event_id_same_full_context_is_duplicate() {
        let alert = IndicatorAlert::new("cross_up", "crossed above trigger", 0.8);
        let first = AlertEvent::new(alert.clone(), 1_700_000_000, EventPhase::Trigger)
            .with_instrument("DAX")
            .with_timeframe(Timeframe::Minute(5));
        let second = AlertEvent::new(alert, 1_700_000_000, EventPhase::Trigger)
            .with_instrument("DAX")
            .with_timeframe(Timeframe::Minute(5));

        assert_eq!(first.event_id, second.event_id);

        let mut dedup = AlertDeduplicator::new();
        assert!(dedup.admit(&first));
        assert!(!dedup.admit(&second));
    }

    /// The final ID must not depend on the order `with_instrument`/`with_timeframe` were called
    /// in.
    #[test]
    fn test_event_id_independent_of_builder_call_order() {
        let alert = IndicatorAlert::new("cross_up", "crossed above trigger", 0.8);
        let instrument_then_timeframe =
            AlertEvent::new(alert.clone(), 1_700_000_000, EventPhase::Trigger)
                .with_instrument("DAX")
                .with_timeframe(Timeframe::Minute(5));
        let timeframe_then_instrument = AlertEvent::new(alert, 1_700_000_000, EventPhase::Trigger)
            .with_timeframe(Timeframe::Minute(5))
            .with_instrument("DAX");

        assert_eq!(
            instrument_then_timeframe.event_id,
            timeframe_then_instrument.event_id
        );
    }

    /// A delimiter character occurring inside an instrument or alert-kind name must not create an
    /// ID collision with an otherwise-different event, since components are length-prefixed
    /// rather than joined with a bare separator.
    #[test]
    fn test_event_id_special_characters_do_not_collide() {
        // Without length-prefixing, instrument "A:B" + kind "C" could collide with instrument "A"
        // + kind "B:C" once joined with ':'.
        let alert_ab_c = IndicatorAlert::new("C", "note", 0.5);
        let event_1 =
            AlertEvent::new(alert_ab_c, 1_000, EventPhase::Trigger).with_instrument("A:B");

        let alert_b_c = IndicatorAlert::new("B:C", "note", 0.5);
        let event_2 = AlertEvent::new(alert_b_c, 1_000, EventPhase::Trigger).with_instrument("A");

        assert_ne!(event_1.event_id, event_2.event_id);
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
