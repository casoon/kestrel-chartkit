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
///
/// `from_ts`/`to_ts` bound the zone in time: the first and last bar it was derived from.
/// A zone without them can only be drawn as a band across the full width, which loses
/// where it formed — so emitters should fill them whenever they know. `None` means
/// genuinely unknown, not "now": consumers must not substitute a guess.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ZoneArtifact {
    pub kind: String,
    pub price_top: f64,
    pub price_bottom: f64,
    pub strength: f64,
    pub touches: u32,
    /// First bar the zone was derived from, if known.
    pub from_ts: Option<i64>,
    /// Last bar the zone was derived from, if known.
    pub to_ts: Option<i64>,
}

impl ZoneArtifact {
    /// Builds a zone without time bounds — for emitters that genuinely have none.
    pub fn new(kind: impl Into<String>, price_top: f64, price_bottom: f64) -> Self {
        Self {
            kind: kind.into(),
            price_top,
            price_bottom,
            strength: 0.0,
            touches: 0,
            from_ts: None,
            to_ts: None,
        }
    }

    /// Attaches the bar range the zone was derived from.
    pub fn spanning(mut self, from_ts: i64, to_ts: i64) -> Self {
        self.from_ts = Some(from_ts);
        self.to_ts = Some(to_ts);
        self
    }

    /// The zone's time bounds, if both are known.
    pub fn span(&self) -> Option<(i64, i64)> {
        self.from_ts.zip(self.to_ts)
    }
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
///
/// `from_ts`/`to_ts` bound the accumulation window, the same contract as
/// [`ZoneArtifact`]: `None` means unknown, never "now".
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProfileArtifact {
    pub kind: String,
    pub bins: Vec<ProfileBin>,
    pub poc: f64,
    pub value_area_high: f64,
    pub value_area_low: f64,
    /// First bar the profile was accumulated from, if known.
    pub from_ts: Option<i64>,
    /// Last bar the profile was accumulated from, if known.
    pub to_ts: Option<i64>,
}

impl ProfileArtifact {
    /// Attaches the bar range the profile was accumulated over.
    pub fn spanning(mut self, from_ts: i64, to_ts: i64) -> Self {
        self.from_ts = Some(from_ts);
        self.to_ts = Some(to_ts);
        self
    }

    /// The profile's time bounds, if both are known.
    pub fn span(&self) -> Option<(i64, i64)> {
        self.from_ts.zip(self.to_ts)
    }
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

/// Typed container pairing an indicator artifact with optional series provenance.
///
/// Ensures price-level artifacts (e.g. pivots, zones, profile POCs) can be persisted
/// or transferred alongside the exact [`crate::model::SeriesIdentity`] on which they were formed.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TaggedArtifact {
    pub artifact: Artifact,
    pub series_identity: Option<crate::model::SeriesIdentity>,
}

impl TaggedArtifact {
    pub fn new(artifact: Artifact) -> Self {
        Self {
            artifact,
            series_identity: None,
        }
    }

    pub fn with_series(mut self, identity: crate::model::SeriesIdentity) -> Self {
        self.series_identity = Some(identity);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SeriesIdentity;

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
            from_ts: None,
            to_ts: None,
        };
        let artifact: Artifact = zone.into();
        assert!(matches!(&artifact, Artifact::Zone(z) if z.touches == 2));

        let series_id = SeriesIdentity::new("ES", "5m");
        let tagged = TaggedArtifact::new(artifact).with_series(series_id.clone());
        assert_eq!(tagged.series_identity, Some(series_id));
    }

    #[test]
    fn test_zone_span_is_absent_until_set() {
        let zone = ZoneArtifact::new("order_block", 105.0, 100.0);
        assert_eq!(zone.span(), None, "unknown bounds must not be guessed");

        let spanned = zone.spanning(1_000, 2_000);
        assert_eq!(spanned.span(), Some((1_000, 2_000)));
    }

    #[test]
    fn test_profile_span_is_absent_until_set() {
        let profile = ProfileArtifact {
            kind: "volume_profile".to_string(),
            bins: Vec::new(),
            poc: 100.0,
            value_area_high: 101.0,
            value_area_low: 99.0,
            from_ts: None,
            to_ts: None,
        };
        assert_eq!(profile.span(), None);
        assert_eq!(profile.spanning(10, 20).span(), Some((10, 20)));
    }

    /// A half-known span stays unknown: a zone with only a start would otherwise be
    /// drawn as if it ended now.
    #[test]
    fn test_a_half_known_span_reads_as_unknown() {
        let mut zone = ZoneArtifact::new("fvg", 105.0, 100.0);
        zone.from_ts = Some(1_000);
        assert_eq!(zone.span(), None);
    }
}
