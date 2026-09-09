//! Position-to-signal alignment and distance, without broker fields or presentation text.
use crate::{Agreement, SignalDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum PositionAlignment {
    Aligned,
    Opposed,
    Neutral,
    Conflict,
    Unconfigured,
}

pub fn position_alignment(
    position: SignalDirection,
    recommendation: Option<&Agreement>,
) -> PositionAlignment {
    let Some(recommendation) = recommendation else {
        return PositionAlignment::Unconfigured;
    };
    if recommendation.conflict {
        PositionAlignment::Conflict
    } else if recommendation.direction == SignalDirection::Neutral
        || position == SignalDirection::Neutral
    {
        PositionAlignment::Neutral
    } else if recommendation.direction == position {
        PositionAlignment::Aligned
    } else {
        PositionAlignment::Opposed
    }
}

/// Unsigned price distance in ATR units; unavailable for a non-positive ATR.
pub fn atr_distance(entry: f64, current: f64, atr: f64) -> Option<f64> {
    (atr > 0.0).then_some((current - entry).abs() / atr)
}
