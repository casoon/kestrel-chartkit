//! Plausibility check between what an indicator's calculation needs from a bar series
//! ([`crate::applicability::DataRequirements`]) and what a series actually provides
//! ([`crate::model::SeriesCapabilities`]).
//!
//! `build_checked` (see [`crate::indicator::registry`]) validates *parameters* — periods in
//! range, threshold ordering. It says nothing about whether the *data* fits the indicator: a
//! volume-profile indicator run on tick-volume CFD data will compute a POC/VAH/VAL that looks
//! plausible and measures nothing meaningful. Nothing in the type system prevents that. This
//! module is the check that catches it: [`crate::applicability::check_applicability`] compares an
//! indicator's declared [`crate::applicability::DataRequirements`] against a series'
//! [`crate::model::SeriesCapabilities`] and returns a three-valued verdict
//! ([`crate::applicability::Applicability`]) with factual, publication-ready explanations
//! attached.
//!
//! (Note: this module-level doc comment uses fully-qualified `crate::` paths for its own items
//! rather than bare names — rustdoc resolves intra-doc links here against the *crate-root* scope,
//! not this module's, because `pub mod applicability;` in `lib.rs` also carries an outer `///`
//! doc comment that gets merged with this file's `//!` comment into one doc block for the module.)

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::{
    ContinuityKind, LiquidityTier, PriceAdjustment, Provenance, SeriesCapabilities, VolumeKind,
};

/// What an indicator's calculation needs from the bar series it runs over, beyond plain OHLC
/// prices. All-`false` (the [`Default`]) is the right value for pure price indicators — the
/// effort of declaring requirements is only spent on the indicators where it matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DataRequirements {
    /// Needs real traded turnover, not tick/update count (e.g. volume profile, VWAP).
    pub needs_real_volume: bool,
    /// Needs classified individual trades (buy/sell direction), not just aggregate volume.
    pub needs_trade_direction: bool,
    /// Result depends on where the session is cut (anchors, session extremes).
    pub session_sensitive: bool,
    /// Result depends on contract continuity (rolls mix liquidity pools / shift historical
    /// levels).
    pub roll_sensitive: bool,
    /// Result depends on historical price levels matching what was actually traded (broken by
    /// split/dividend adjustment).
    pub adjustment_sensitive: bool,
    /// Needs market-depth information beyond what a thin series can provide.
    pub needs_liquidity_depth: bool,
}

/// Which rule in [`check_applicability`] produced a given [`ApplicabilityNote`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ApplicabilityReason {
    /// `needs_real_volume` against [`VolumeKind::Tick`]: the field holds update frequency, not
    /// turnover.
    VolumeIsTickNotTurnover,
    /// `needs_real_volume` against [`VolumeKind::None`]: the series has no volume at all.
    NoVolumeAvailable,
    /// `needs_trade_direction` against a series without classified trades.
    NoTradeDirection,
    /// `roll_sensitive` against [`ContinuityKind::StitchedBackAdjusted`]: historical levels are
    /// not the traded ones.
    BackAdjustedLevelsNotTraded,
    /// `roll_sensitive` against [`ContinuityKind::StitchedUnadjusted`]: roll jumps in the series.
    UnadjustedRollJumps,
    /// `adjustment_sensitive` against a non-[`PriceAdjustment::Raw`] series: historical marks sit
    /// at different numbers in the adjusted series.
    AdjustedLevelsShifted,
    /// `needs_liquidity_depth` against [`LiquidityTier::Thin`].
    ThinLiquidity,
    /// An extreme-checking indicator (`session_sensitive || roll_sensitive`) run on
    /// [`Provenance::Broker`] data: the extreme is provider-specific and does not transfer.
    ///
    /// The plan's original rule is stated as "`provenance == Broker` for anything that checks
    /// extremes", but `DataRequirements` has no dedicated "checks extremes" field. This
    /// substitutes `session_sensitive || roll_sensitive` as that signal: those are exactly the
    /// two fields set on the plan's own list of extreme-checking indicators (pivots, structure,
    /// zigzag, Elliott), so no new field is introduced for a case the two existing fields already
    /// cover.
    ProviderSpecificExtreme,
}

/// A single applicability finding: which rule fired, and a publication-ready explanation.
///
/// `explanation` is meant to end up verbatim on a public docs page (see the plan's "Export für
/// die Doku" section): factual, no superlatives, no performance claims.
///
/// Derives `Serialize` only, not `Deserialize`: the `&'static str` explanation field can only be
/// deserialized into a `'static` lifetime, which conflicts with `serde_derive`'s generic `'de`
/// once this type sits behind a `Vec` inside [`Applicability`] (a struct field of a concrete
/// `&'static str` alone can derive `Deserialize` by adding a `'de: 'static` bound to its own impl,
/// but that bound cannot be discharged once another type calls into it generically). These are
/// computed diagnostic outputs, never something reconstructed from JSON, so this is not a loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ApplicabilityNote {
    pub reason: ApplicabilityReason,
    pub explanation: &'static str,
}

/// Which of the two non-`Applicable` buckets a rule belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ApplicabilityTier {
    Degraded,
    Unsuitable,
}

/// Static description of one rule in [`check_applicability`]'s table: which reason it produces,
/// which tier it falls into, and its publication-ready explanation. [`rule_catalog`] exposes all
/// of them for the doc export (see the plan's "Export für die Doku" section) — this is the single
/// source both `check_applicability` and the export pull from, so the exported table can never
/// drift from the rules actually enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct RuleDescription {
    pub reason: ApplicabilityReason,
    pub tier: ApplicabilityTier,
    pub explanation: &'static str,
}

/// Three-valued result of matching an indicator's [`DataRequirements`] against a series'
/// [`SeriesCapabilities`].
///
/// Derives `Serialize` only, not `Deserialize` — see [`ApplicabilityNote`]'s doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum Applicability {
    Applicable,
    /// The indicator still computes, but the result's meaning shifts (e.g. a relative-volume
    /// measure computed on tick data is an activity measure, not a turnover measure).
    Degraded {
        reasons: Vec<ApplicabilityNote>,
    },
    /// The result would be misleading; computing it anyway should not happen silently.
    Unsuitable {
        reasons: Vec<ApplicabilityNote>,
    },
}

// The rule table: one `RuleDescription` constant per rule, each naming the reason it produces,
// its tier, and its publication-ready explanation. `check_applicability` below turns each into an
// `ApplicabilityNote` when its condition holds; `rule_catalog` exposes the same constants for the
// doc export — one source, so the exported table can't drift from the rules actually enforced.

/// Rule 1: `needs_real_volume` && `volume == Tick` -> Unsuitable.
const RULE_VOLUME_IS_TICK: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::VolumeIsTickNotTurnover,
    tier: ApplicabilityTier::Unsuitable,
    explanation: "zeigt Update-Häufigkeit, nicht Umsatz",
};

/// Rule 2: `needs_real_volume` && `volume == None` -> Unsuitable.
const RULE_NO_VOLUME: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::NoVolumeAvailable,
    tier: ApplicabilityTier::Unsuitable,
    explanation: "die Serie hat kein Volumen; ein Index ist ein berechneter Wert — \
        Volumen-Referenz wäre Future oder ETF",
};

/// Rule 3: `needs_trade_direction` && `!trade_direction` -> Unsuitable.
const RULE_NO_TRADE_DIRECTION: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::NoTradeDirection,
    tier: ApplicabilityTier::Unsuitable,
    explanation: "die Serie enthält keine klassifizierten Einzeltrades, nur aggregierte Bars",
};

/// Rule 4: `roll_sensitive` && `continuity == StitchedBackAdjusted` -> Degraded.
const RULE_BACK_ADJUSTED: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::BackAdjustedLevelsNotTraded,
    tier: ApplicabilityTier::Degraded,
    explanation: "historische Niveaus sind nicht die gehandelten",
};

/// Rule 5: `roll_sensitive` && `continuity == StitchedUnadjusted` -> Degraded.
const RULE_UNADJUSTED_ROLL: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::UnadjustedRollJumps,
    tier: ApplicabilityTier::Degraded,
    explanation: "Roll-Sprünge in der Reihe",
};

/// Rule 6: `adjustment_sensitive` && `price_adjustment != Raw` -> Degraded.
const RULE_ADJUSTED_SHIFTED: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::AdjustedLevelsShifted,
    tier: ApplicabilityTier::Degraded,
    explanation: "historische Marken liegen in der bereinigten Reihe an anderen Zahlen",
};

/// Rule 7: `needs_liquidity_depth` && `liquidity_tier == Thin` -> Degraded.
const RULE_THIN_LIQUIDITY: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::ThinLiquidity,
    tier: ApplicabilityTier::Degraded,
    explanation: "geringe Liquidität; Markttiefe für diese Kennzahl nicht ausreichend abgebildet",
};

/// Rule 8: `(session_sensitive || roll_sensitive)` && `provenance == Broker` -> Degraded.
/// See [`ApplicabilityReason::ProviderSpecificExtreme`] for why this signal substitutes for the
/// plan's unimplemented "checks extremes" field.
const RULE_PROVIDER_SPECIFIC_EXTREME: RuleDescription = RuleDescription {
    reason: ApplicabilityReason::ProviderSpecificExtreme,
    tier: ApplicabilityTier::Degraded,
    explanation: "Extreme sind anbieterspezifisch, eine Auszeichnung ist nicht übertragbar",
};

/// All rules in [`check_applicability`]'s table, in the same order they are evaluated — the
/// source the doc export (see the plan's "Export für die Doku" section) reads from.
pub fn rule_catalog() -> [RuleDescription; 8] {
    [
        RULE_VOLUME_IS_TICK,
        RULE_NO_VOLUME,
        RULE_NO_TRADE_DIRECTION,
        RULE_BACK_ADJUSTED,
        RULE_UNADJUSTED_ROLL,
        RULE_ADJUSTED_SHIFTED,
        RULE_THIN_LIQUIDITY,
        RULE_PROVIDER_SPECIFIC_EXTREME,
    ]
}

fn note_from(rule: RuleDescription) -> ApplicabilityNote {
    ApplicabilityNote {
        reason: rule.reason,
        explanation: rule.explanation,
    }
}

/// Matches `requirements` against `capabilities` and classifies the result.
///
/// Collects *all* matching notes across all rules (does not stop at the first match), then
/// classifies: any note produced by an "Unsuitable" rule (1-3 above) makes the whole result
/// [`Applicability::Unsuitable`], carrying every note (Unsuitable and Degraded) found; otherwise
/// any note from a "Degraded" rule (4-8 above) makes it [`Applicability::Degraded`]; otherwise
/// [`Applicability::Applicable`].
pub fn check_applicability(
    requirements: &DataRequirements,
    capabilities: &SeriesCapabilities,
) -> Applicability {
    let mut unsuitable_notes = Vec::new();
    let mut degraded_notes = Vec::new();

    if requirements.needs_real_volume && capabilities.volume == VolumeKind::Tick {
        unsuitable_notes.push(note_from(RULE_VOLUME_IS_TICK));
    }

    if requirements.needs_real_volume && capabilities.volume == VolumeKind::None {
        unsuitable_notes.push(note_from(RULE_NO_VOLUME));
    }

    if requirements.needs_trade_direction && !capabilities.trade_direction {
        unsuitable_notes.push(note_from(RULE_NO_TRADE_DIRECTION));
    }

    if requirements.roll_sensitive
        && capabilities.continuity == ContinuityKind::StitchedBackAdjusted
    {
        degraded_notes.push(note_from(RULE_BACK_ADJUSTED));
    }

    if requirements.roll_sensitive && capabilities.continuity == ContinuityKind::StitchedUnadjusted
    {
        degraded_notes.push(note_from(RULE_UNADJUSTED_ROLL));
    }

    if requirements.adjustment_sensitive && capabilities.price_adjustment != PriceAdjustment::Raw {
        degraded_notes.push(note_from(RULE_ADJUSTED_SHIFTED));
    }

    if requirements.needs_liquidity_depth && capabilities.liquidity_tier == LiquidityTier::Thin {
        degraded_notes.push(note_from(RULE_THIN_LIQUIDITY));
    }

    if (requirements.session_sensitive || requirements.roll_sensitive)
        && capabilities.provenance == Provenance::Broker
    {
        degraded_notes.push(note_from(RULE_PROVIDER_SPECIFIC_EXTREME));
    }

    if !unsuitable_notes.is_empty() {
        unsuitable_notes.extend(degraded_notes);
        Applicability::Unsuitable {
            reasons: unsuitable_notes,
        }
    } else if !degraded_notes.is_empty() {
        Applicability::Degraded {
            reasons: degraded_notes,
        }
    } else {
        Applicability::Applicable
    }
}

/// Looks up the [`DataRequirements`] for a registry/catalog indicator name (case-insensitive,
/// matching the `.to_lowercase()` convention used by
/// [`crate::indicator::registry::build_checked`]/`build_typed`).
///
/// This is a separate lookup rather than a field on
/// [`crate::indicator::registry::IndicatorCatalogEntry`] because two indicators that need
/// requirements here (`elliott`, `swing_structure`) are not catalog/registry entries at all —
/// they are excluded from the single-bar registry due to their call signature (see the note above
/// `CANONICAL_INDICATOR_NAMES` in `src/indicator/registry.rs`). A standalone function is the only
/// way to give catalog and non-catalog indicators one shared source of truth without forcing a
/// signature change on those two engines.
///
/// Names not listed below (including all pure price indicators) return
/// [`DataRequirements::default()`] — deliberately conservative: `needs_trade_direction` and
/// `needs_liquidity_depth` are left `false` everywhere for now, since no indicator covered by the
/// plan is described as needing actual classified-trade data (bar-derived CVD is a documented
/// heuristic, not a hard requirement) or is named as needing liquidity-depth data.
pub fn data_requirements(name: &str) -> DataRequirements {
    match name.to_lowercase().as_str() {
        "volume_profile"
        | "extended_volume_profile"
        | "persistent_volume_profile"
        | "money_flow_profile" => DataRequirements {
            needs_real_volume: true,
            roll_sensitive: true,
            ..DataRequirements::default()
        },
        "vwap" | "anchored_vwap" => DataRequirements {
            needs_real_volume: true,
            session_sensitive: true,
            ..DataRequirements::default()
        },
        "cvd" | "buy_sell_pressure" | "mfi" | "eom" | "nvi" | "pvi" | "klinger"
        | "chaikin_oscillator" | "elder_ray" | "midas" => DataRequirements {
            needs_real_volume: true,
            ..DataRequirements::default()
        },
        "pivot_sets" | "pivots_structure" | "zigzag" | "zigzag_advanced" | "elliott"
        | "swing_structure" => DataRequirements {
            session_sensitive: true,
            roll_sensitive: true,
            adjustment_sensitive: true,
            ..DataRequirements::default()
        },
        _ => DataRequirements::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionKind;

    /// A neutral baseline: deep, exchange-provenance, regular-session, single-contract, raw,
    /// real-turnover series. Individual tests override only the field(s) relevant to the rule
    /// under test.
    fn baseline_capabilities() -> SeriesCapabilities {
        SeriesCapabilities {
            volume: VolumeKind::RealTurnover,
            trade_direction: false,
            session: SessionKind::Regular,
            continuity: ContinuityKind::SingleContract,
            price_adjustment: PriceAdjustment::Raw,
            provenance: Provenance::Exchange,
            liquidity_tier: LiquidityTier::Deep,
        }
    }

    fn only_unsuitable(applicability: &Applicability) -> &[ApplicabilityNote] {
        match applicability {
            Applicability::Unsuitable { reasons } => reasons,
            other => panic!("expected Unsuitable, got {other:?}"),
        }
    }

    fn only_degraded(applicability: &Applicability) -> &[ApplicabilityNote] {
        match applicability {
            Applicability::Degraded { reasons } => reasons,
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[test]
    fn rule1_real_volume_on_tick_is_unsuitable() {
        let requirements = DataRequirements {
            needs_real_volume: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            volume: VolumeKind::Tick,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_unsuitable(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::VolumeIsTickNotTurnover));
    }

    #[test]
    fn rule2_real_volume_on_none_is_unsuitable() {
        let requirements = DataRequirements {
            needs_real_volume: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            volume: VolumeKind::None,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_unsuitable(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::NoVolumeAvailable));
    }

    #[test]
    fn rule3_trade_direction_missing_is_unsuitable() {
        let requirements = DataRequirements {
            needs_trade_direction: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            trade_direction: false,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_unsuitable(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::NoTradeDirection));
    }

    #[test]
    fn rule4_roll_sensitive_on_back_adjusted_is_degraded() {
        let requirements = DataRequirements {
            roll_sensitive: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            continuity: ContinuityKind::StitchedBackAdjusted,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_degraded(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::BackAdjustedLevelsNotTraded));
    }

    #[test]
    fn rule5_roll_sensitive_on_unadjusted_stitched_is_degraded() {
        let requirements = DataRequirements {
            roll_sensitive: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            continuity: ContinuityKind::StitchedUnadjusted,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_degraded(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::UnadjustedRollJumps));
    }

    #[test]
    fn rule6_adjustment_sensitive_on_adjusted_series_is_degraded() {
        let requirements = DataRequirements {
            adjustment_sensitive: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            price_adjustment: PriceAdjustment::SplitAndDividend,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_degraded(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::AdjustedLevelsShifted));
    }

    #[test]
    fn rule7_liquidity_depth_on_thin_series_is_degraded() {
        let requirements = DataRequirements {
            needs_liquidity_depth: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            liquidity_tier: LiquidityTier::Thin,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_degraded(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::ThinLiquidity));
    }

    #[test]
    fn rule8_extreme_checking_on_broker_data_is_degraded() {
        let requirements = DataRequirements {
            session_sensitive: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            provenance: Provenance::Broker,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_degraded(&result);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::ProviderSpecificExtreme));

        // roll_sensitive alone is also a valid trigger for the same rule.
        let requirements2 = DataRequirements {
            roll_sensitive: true,
            ..DataRequirements::default()
        };
        let result2 = check_applicability(&requirements2, &capabilities);
        let reasons2 = only_degraded(&result2);
        assert!(reasons2
            .iter()
            .any(|n| n.reason == ApplicabilityReason::ProviderSpecificExtreme));
    }

    #[test]
    fn no_requirements_is_always_applicable() {
        let requirements = DataRequirements::default();
        let capabilities_variants = [
            baseline_capabilities(),
            SeriesCapabilities {
                volume: VolumeKind::None,
                continuity: ContinuityKind::StitchedBackAdjusted,
                price_adjustment: PriceAdjustment::SplitAndDividend,
                provenance: Provenance::Broker,
                liquidity_tier: LiquidityTier::Thin,
                ..baseline_capabilities()
            },
        ];
        for capabilities in capabilities_variants {
            assert_eq!(
                check_applicability(&requirements, &capabilities),
                Applicability::Applicable
            );
        }
    }

    #[test]
    fn multiple_violations_collect_into_one_result_with_all_notes() {
        // needs_real_volume (Unsuitable) + roll_sensitive (Degraded, twice-eligible collapsed to
        // whichever continuity is set) + adjustment_sensitive (Degraded).
        let requirements = DataRequirements {
            needs_real_volume: true,
            roll_sensitive: true,
            adjustment_sensitive: true,
            ..DataRequirements::default()
        };
        let capabilities = SeriesCapabilities {
            volume: VolumeKind::Tick,
            continuity: ContinuityKind::StitchedBackAdjusted,
            price_adjustment: PriceAdjustment::Split,
            ..baseline_capabilities()
        };
        let result = check_applicability(&requirements, &capabilities);
        let reasons = only_unsuitable(&result);
        assert_eq!(reasons.len(), 3);
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::VolumeIsTickNotTurnover));
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::BackAdjustedLevelsNotTraded));
        assert!(reasons
            .iter()
            .any(|n| n.reason == ApplicabilityReason::AdjustedLevelsShifted));
    }

    #[test]
    fn data_requirements_maps_known_names() {
        assert_eq!(
            data_requirements("volume_profile"),
            DataRequirements {
                needs_real_volume: true,
                roll_sensitive: true,
                ..DataRequirements::default()
            }
        );
        assert_eq!(
            data_requirements("VWAP"),
            DataRequirements {
                needs_real_volume: true,
                session_sensitive: true,
                ..DataRequirements::default()
            }
        );
        assert_eq!(
            data_requirements("elliott"),
            DataRequirements {
                session_sensitive: true,
                roll_sensitive: true,
                adjustment_sensitive: true,
                ..DataRequirements::default()
            }
        );
    }

    #[test]
    fn data_requirements_unmapped_name_is_default() {
        assert_eq!(data_requirements("rsi"), DataRequirements::default());
    }

    #[test]
    fn rule_catalog_has_one_entry_per_reason_with_matching_tier() {
        let rules = rule_catalog();
        assert_eq!(rules.len(), 8);

        // Every `ApplicabilityReason` variant appears exactly once (catches a rule added to
        // `check_applicability` without a matching `rule_catalog` entry, or vice versa).
        let unsuitable_reasons = [
            ApplicabilityReason::VolumeIsTickNotTurnover,
            ApplicabilityReason::NoVolumeAvailable,
            ApplicabilityReason::NoTradeDirection,
        ];
        for reason in unsuitable_reasons {
            let rule = rules.iter().find(|r| r.reason == reason).unwrap();
            assert_eq!(rule.tier, ApplicabilityTier::Unsuitable);
        }
        let degraded_reasons = [
            ApplicabilityReason::BackAdjustedLevelsNotTraded,
            ApplicabilityReason::UnadjustedRollJumps,
            ApplicabilityReason::AdjustedLevelsShifted,
            ApplicabilityReason::ThinLiquidity,
            ApplicabilityReason::ProviderSpecificExtreme,
        ];
        for reason in degraded_reasons {
            let rule = rules.iter().find(|r| r.reason == reason).unwrap();
            assert_eq!(rule.tier, ApplicabilityTier::Degraded);
        }
    }
}
