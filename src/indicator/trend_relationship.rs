//! Adaptive trend-relationship engine: classifies trend state from the relative position and
//! slope of two configurable [`super::smoothing::Smoother`] stages (any [`SmootherKind`], any
//! length), instead of duplicating the same "fast MA vs. slow MA, is it rising" logic per script.
//! Combine with [`super::smoothing::SmootherChain`] to compare cascades of more than one stage per
//! side (e.g. an EMA-of-SMA fast leg against a plain slow EMA).

use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::{Smoother, SmootherKind};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Relative trend classification of the fast smoother against the slow one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendRelation {
    /// Fast above slow and still rising: a confirmed, still-strengthening uptrend.
    Bullish,
    /// Fast below slow and still falling: a confirmed, still-strengthening downtrend.
    Bearish,
    /// Fast/slow relationship disagrees with the fast leg's own slope (e.g. fast above slow but
    /// now falling) — likely losing momentum or about to cross.
    Transition,
}

impl TrendRelation {
    fn as_str(self) -> &'static str {
        match self {
            TrendRelation::Bullish => "bullish",
            TrendRelation::Bearish => "bearish",
            TrendRelation::Transition => "transition",
        }
    }
}

/// Compares a fast and a slow [`Smoother`] fed the same source series, classifying the
/// relationship as [`TrendRelation`] and alerting on fast/slow crossovers.
pub struct AdaptiveTrendRelationship {
    fast: Box<dyn Smoother>,
    slow: Box<dyn Smoother>,
    prev_fast: Option<f64>,
    prev_relation_above: Option<bool>,
    alerts: Vec<IndicatorAlert>,
}

impl AdaptiveTrendRelationship {
    pub fn new(
        fast_kind: SmootherKind,
        fast_len: usize,
        slow_kind: SmootherKind,
        slow_len: usize,
    ) -> Self {
        Self {
            fast: fast_kind.build(fast_len),
            slow: slow_kind.build(slow_len),
            prev_fast: None,
            prev_relation_above: None,
            alerts: Vec::new(),
        }
    }

    /// Builds the relationship from two arbitrary, already-constructed smoothers (e.g. a
    /// multi-stage [`super::smoothing::SmootherChain`] wrapped to implement [`Smoother`]).
    pub fn from_smoothers(fast: Box<dyn Smoother>, slow: Box<dyn Smoother>) -> Self {
        Self {
            fast,
            slow,
            prev_fast: None,
            prev_relation_above: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for AdaptiveTrendRelationship {
    fn name(&self) -> &str {
        "trend_relationship"
    }

    fn reset(&mut self) {
        self.fast.reset();
        self.slow.reset();
        self.prev_fast = None;
        self.prev_relation_above = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let fast = self.fast.update(bar.close);
        let slow = self.slow.update(bar.close);
        let (fast, slow) = match (fast, slow) {
            (Some(f), Some(s)) => (f, s),
            _ => {
                self.prev_fast = fast.or(self.prev_fast);
                return None;
            }
        };

        let above = fast > slow;
        if let Some(prev_above) = self.prev_relation_above {
            if prev_above != above {
                let (kind, note) = if above {
                    (
                        "trend_relationship_cross_up",
                        "Fast smoother crossed above slow",
                    )
                } else {
                    (
                        "trend_relationship_cross_down",
                        "Fast smoother crossed below slow",
                    )
                };
                self.alerts.push(IndicatorAlert::new(kind, note, 1.0));
            }
        }
        self.prev_relation_above = Some(above);

        let rising = self.prev_fast.map(|p| fast > p);
        let relation = match (above, rising) {
            (true, Some(true)) | (true, None) => TrendRelation::Bullish,
            (false, Some(false)) | (false, None) => TrendRelation::Bearish,
            _ => TrendRelation::Transition,
        };
        self.prev_fast = Some(fast);

        let mut extra = HashMap::new();
        extra.insert("fast".to_string(), fast);
        extra.insert("slow".to_string(), slow);

        Some(
            IndicatorOutput::with_extra(fast - slow, extra)
                .with_secondary(slow)
                .with_state(relation.as_str()),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar::new(i as i64 * 60, c, c + 1.0, c - 1.0, c, 100.0))
            .collect()
    }

    #[test]
    fn test_classifies_bullish_when_fast_above_and_rising() {
        let mut engine = AdaptiveTrendRelationship::new(SmootherKind::Sma, 2, SmootherKind::Sma, 4);
        let closes: Vec<f64> = (0..15).map(|i| 100.0 + i as f64).collect();
        let mut last_state = None;
        for bar in bars_from_closes(&closes) {
            if let Some(out) = engine.on_bar(&bar) {
                last_state = out.state;
            }
        }
        assert_eq!(last_state.as_deref(), Some("bullish"));
    }

    #[test]
    fn test_classifies_bearish_when_fast_below_and_falling() {
        let mut engine = AdaptiveTrendRelationship::new(SmootherKind::Sma, 2, SmootherKind::Sma, 4);
        let closes: Vec<f64> = (0..15).map(|i| 200.0 - i as f64).collect();
        let mut last_state = None;
        for bar in bars_from_closes(&closes) {
            if let Some(out) = engine.on_bar(&bar) {
                last_state = out.state;
            }
        }
        assert_eq!(last_state.as_deref(), Some("bearish"));
    }

    #[test]
    fn test_crossover_alert_fires_once_per_cross() {
        let mut engine = AdaptiveTrendRelationship::new(SmootherKind::Sma, 2, SmootherKind::Sma, 3);
        // Falling then rising, forcing at least one crossover of fast vs. slow.
        let closes = [100.0, 99.0, 98.0, 97.0, 100.0, 104.0, 108.0, 112.0];
        let mut cross_count = 0;
        for bar in bars_from_closes(&closes) {
            engine.on_bar(&bar);
            cross_count += engine.alerts().len();
        }
        assert!(cross_count >= 1);
    }

    #[test]
    fn test_from_smoothers_accepts_chain() {
        use super::super::smoothing::SmootherChain;

        let fast = Box::new(SmootherChain::new(vec![
            SmootherKind::Sma.build(2),
            SmootherKind::Ema.build(2),
        ]));
        let slow = SmootherKind::Sma.build(5);
        let mut engine = AdaptiveTrendRelationship::from_smoothers(fast, slow);

        let closes: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
        let mut saw_output = false;
        for bar in bars_from_closes(&closes) {
            if engine.on_bar(&bar).is_some() {
                saw_output = true;
            }
        }
        assert!(saw_output);
    }
}
