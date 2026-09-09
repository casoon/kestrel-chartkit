use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::session::{SessionConfig, SessionConfigError, SessionTracker};
use crate::timeframe::Timeframe;
use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Anchor trigger condition for resetting cumulative VWAP calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum VwapAnchorKind {
    #[default]
    Session,
    Day,
    Week,
    Month,
    ManualTimestamp(i64),
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ZeroVolumePolicy {
    Skip,
    #[default]
    EqualWeight,
}

/// Anchored VWAP Engine with volume-weighted stddev bands.
#[derive(Debug, Clone)]
pub struct AnchoredVwapEngine {
    anchor_kind: VwapAnchorKind,
    cum_pv: f64,
    cum_vol: f64,
    cum_pv2: f64,
    prev_timestamp: Option<i64>,
    utc_offset_seconds: i32,
    active: bool,
    zero_volume_policy: ZeroVolumePolicy,
    session_tracker: Option<SessionTracker>,
    stddev_mult1: f64,
    stddev_mult2: f64,
}

impl AnchoredVwapEngine {
    pub fn new(anchor_kind: VwapAnchorKind, stddev_mult1: f64, stddev_mult2: f64) -> Self {
        Self {
            anchor_kind,
            cum_pv: 0.0,
            cum_vol: 0.0,
            cum_pv2: 0.0,
            prev_timestamp: None,
            utc_offset_seconds: 0,
            active: !matches!(anchor_kind, VwapAnchorKind::ManualTimestamp(_)),
            zero_volume_policy: ZeroVolumePolicy::EqualWeight,
            session_tracker: matches!(anchor_kind, VwapAnchorKind::Session).then(|| {
                SessionTracker::new(SessionConfig::default()).expect("default session is valid")
            }),
            stddev_mult1,
            stddev_mult2,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(VwapAnchorKind::Session, 1.0, 2.0)
    }

    pub fn with_utc_offset(mut self, utc_offset_seconds: i32) -> Self {
        self.utc_offset_seconds = utc_offset_seconds;
        self
    }

    pub fn with_zero_volume_policy(mut self, policy: ZeroVolumePolicy) -> Self {
        self.zero_volume_policy = policy;
        self
    }

    pub fn with_session_config(
        mut self,
        config: SessionConfig,
    ) -> Result<Self, SessionConfigError> {
        self.anchor_kind = VwapAnchorKind::Session;
        self.session_tracker = Some(SessionTracker::new(config)?);
        self.active = true;
        Ok(self)
    }

    fn check_anchor_reset(&self, current_ts: i64) -> bool {
        let prev_ts = match self.prev_timestamp {
            Some(ts) => ts,
            None => return false,
        };

        let timeframe = match self.anchor_kind {
            VwapAnchorKind::Session => return false,
            VwapAnchorKind::Day => Timeframe::Day(1),
            VwapAnchorKind::Week => Timeframe::Week(1),
            VwapAnchorKind::Month => Timeframe::Month(1),
            VwapAnchorKind::ManualTimestamp(_) | VwapAnchorKind::External => return false,
        };
        timeframe.bucket_start(prev_ts, self.utc_offset_seconds)
            != timeframe.bucket_start(current_ts, self.utc_offset_seconds)
    }

    /// Processes a bar and optionally resets an externally/pivot-anchored VWAP.
    pub fn on_bar_with_anchor(&mut self, bar: &Bar, anchor_event: bool) -> Option<IndicatorOutput> {
        let session_reset = if let Some(tracker) = &mut self.session_tracker {
            tracker.on_bar(bar);
            if !tracker.in_session() {
                self.prev_timestamp = Some(bar.timestamp);
                return None;
            }
            tracker.is_new_session()
        } else {
            false
        };
        let manual_activated = match self.anchor_kind {
            VwapAnchorKind::ManualTimestamp(timestamp) => {
                !self.active && bar.timestamp >= timestamp
            }
            VwapAnchorKind::External => anchor_event,
            _ => false,
        };
        if session_reset
            || manual_activated
            || anchor_event
            || self.check_anchor_reset(bar.timestamp)
        {
            self.cum_pv = 0.0;
            self.cum_vol = 0.0;
            self.cum_pv2 = 0.0;
            self.active = true;
        }
        self.prev_timestamp = Some(bar.timestamp);
        if !self.active {
            return None;
        }

        let volume = if bar.volume > 0.0 {
            bar.volume
        } else if self.zero_volume_policy == ZeroVolumePolicy::EqualWeight {
            1.0
        } else {
            return None;
        };
        let price = bar.typical_price();
        self.cum_pv += price * volume;
        self.cum_vol += volume;
        self.cum_pv2 += price * price * volume;

        let vwap = self.cum_pv / self.cum_vol;
        let variance = (self.cum_pv2 / self.cum_vol - vwap * vwap).max(0.0);
        let stddev = variance.sqrt();
        let mut extra = HashMap::new();
        extra.insert("vwap".to_string(), vwap);
        extra.insert("stddev".to_string(), stddev);
        extra.insert("band1_upper".to_string(), vwap + self.stddev_mult1 * stddev);
        extra.insert("band1_lower".to_string(), vwap - self.stddev_mult1 * stddev);
        extra.insert("band2_upper".to_string(), vwap + self.stddev_mult2 * stddev);
        extra.insert("band2_lower".to_string(), vwap - self.stddev_mult2 * stddev);
        Some(IndicatorOutput::with_extra(vwap, extra))
    }
}

impl Indicator for AnchoredVwapEngine {
    fn name(&self) -> &str {
        "anchored_vwap"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.cum_pv = 0.0;
        self.cum_vol = 0.0;
        self.cum_pv2 = 0.0;
        self.prev_timestamp = None;
        self.active = !matches!(self.anchor_kind, VwapAnchorKind::ManualTimestamp(_));
        if let Some(tracker) = &mut self.session_tracker {
            tracker.reset();
        }
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.on_bar_with_anchor(bar, false)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchored_vwap_reset() {
        let mut avwap = AnchoredVwapEngine::with_defaults();
        let bar1 = Bar::new(0, 100.0, 105.0, 95.0, 100.0, 1000.0);
        let bar2 = Bar::new(86400, 200.0, 205.0, 195.0, 200.0, 1000.0);

        let out1 = avwap.on_bar(&bar1).unwrap();
        assert_eq!(out1.value, 100.0);

        let out2 = avwap.on_bar(&bar2).unwrap();
        assert_eq!(out2.value, 200.0);
    }

    #[test]
    fn manual_anchor_emits_only_from_anchor_timestamp() {
        let mut avwap = AnchoredVwapEngine::new(VwapAnchorKind::ManualTimestamp(10), 1.0, 2.0);
        assert!(avwap
            .on_bar(&Bar::new(9, 100.0, 100.0, 100.0, 100.0, 1.0))
            .is_none());
        assert_eq!(
            avwap
                .on_bar(&Bar::new(10, 110.0, 110.0, 110.0, 110.0, 1.0))
                .unwrap()
                .value,
            110.0
        );
    }

    #[test]
    fn external_anchor_resets_accumulation() {
        let mut avwap = AnchoredVwapEngine::new(VwapAnchorKind::External, 1.0, 2.0);
        avwap.on_bar(&Bar::new(1, 100.0, 100.0, 100.0, 100.0, 1.0));
        let output = avwap
            .on_bar_with_anchor(&Bar::new(2, 200.0, 200.0, 200.0, 200.0, 1.0), true)
            .unwrap();
        assert_eq!(output.value, 200.0);
    }

    #[test]
    fn session_anchor_observes_configured_session() {
        let config = SessionConfig {
            start_hour: 9,
            start_minute: 0,
            end_hour: 10,
            end_minute: 0,
            orb_duration_mins: 30,
            utc_offset_seconds: 0,
        };
        let mut avwap = AnchoredVwapEngine::with_defaults()
            .with_session_config(config)
            .unwrap();
        assert!(avwap
            .on_bar(&Bar::new(8 * 3_600, 100.0, 100.0, 100.0, 100.0, 1.0))
            .is_none());
        assert_eq!(
            avwap
                .on_bar(&Bar::new(9 * 3_600, 110.0, 110.0, 110.0, 110.0, 1.0))
                .unwrap()
                .value,
            110.0
        );
        assert!(avwap
            .on_bar(&Bar::new(10 * 3_600, 120.0, 120.0, 120.0, 120.0, 1.0))
            .is_none());
    }
}
