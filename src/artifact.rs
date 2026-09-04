//! Shared, typed result/artifact models for indicator outputs.
//!
//! Individual indicators historically attached auxiliary results (pivots, zones, profile bins,
//! scenario progress) to [`crate::indicator::IndicatorOutput`] as ad-hoc, indicator-local
//! `extra: HashMap<String, f64>` keys. [`Artifact`] gives those shapes a shared, typed
//! representation so consumers can pattern-match on them generically instead of parsing
//! indicator-specific string keys.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A single labeled swing/pivot point.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PivotArtifact {
    pub timestamp: i64,
    pub price: f64,
    pub is_high: bool,
    pub confirmed: bool,
}

/// A price zone (support/resistance, order block, FVG, liquidity pool, ...).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ZoneArtifact {
    pub kind: String,
    pub price_top: f64,
    pub price_bottom: f64,
    pub strength: f64,
    pub touches: u32,
}

/// A single bin/level of a distribution profile (price/volume profile, delta profile, ...).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProfileBin {
    pub price_low: f64,
    pub price_high: f64,
    pub value: f64,
}

/// A distribution profile made of ordered [`ProfileBin`]s plus its summary levels.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProfileArtifact {
    pub kind: String,
    pub bins: Vec<ProfileBin>,
    pub poc: f64,
    pub value_area_high: f64,
    pub value_area_low: f64,
}

/// Progress of a multi-stage composite scenario (e.g. Setup -> Watch -> Trigger).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ScenarioArtifact {
    pub name: String,
    pub stage: String,
    pub progress: f64,
    pub invalidated: bool,
}

/// A typed, indicator-emitted result artifact.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Artifact {
    Pivot(PivotArtifact),
    Zone(ZoneArtifact),
    Profile(ProfileArtifact),
    Scenario(ScenarioArtifact),
}

impl From<PivotArtifact> for Artifact {
    fn from(value: PivotArtifact) -> Self {
        Artifact::Pivot(value)
    }
}

impl From<ZoneArtifact> for Artifact {
    fn from(value: ZoneArtifact) -> Self {
        Artifact::Zone(value)
    }
}

impl From<ProfileArtifact> for Artifact {
    fn from(value: ProfileArtifact) -> Self {
        Artifact::Profile(value)
    }
}

impl From<ScenarioArtifact> for Artifact {
    fn from(value: ScenarioArtifact) -> Self {
        Artifact::Scenario(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_conversions() {
        let pivot = PivotArtifact {
            timestamp: 1_000,
            price: 100.0,
            is_high: true,
            confirmed: true,
        };
        let artifact: Artifact = pivot.into();
        assert!(matches!(artifact, Artifact::Pivot(p) if p.price == 100.0));

        let zone = ZoneArtifact {
            kind: "order_block".to_string(),
            price_top: 105.0,
            price_bottom: 100.0,
            strength: 0.8,
            touches: 2,
        };
        let artifact: Artifact = zone.into();
        assert!(matches!(artifact, Artifact::Zone(z) if z.touches == 2));
    }
}
