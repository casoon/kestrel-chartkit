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
    ContinuityKind, LiquidityTier, PriceAdjustment, Provenance, SeriesCapabilities, SessionKind,
    VolumeKind,
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
    /// Set for the structure-counting methods whose requirements are named individually (see
    /// [`StructureMethod`]). Such an indicator does not use the generic `session_sensitive` /
    /// `roll_sensitive` / `adjustment_sensitive` flags: its own rules in
    /// [`method_rule_catalog`] state, per series property, what the method reads from the series
    /// and why a series without that property is only partly suitable.
    pub structure_method: Option<StructureMethod>,
}

/// A structure-counting method with requirements of its own.
///
/// The generic flags of [`DataRequirements`] give every extreme-based indicator (pivots, ZigZag,
/// Elliott, ...) the same finding for the same series property. For these methods the finding
/// depends on what the calculation does with the series, so each has its own rules in
/// [`method_rule_catalog`], with explanations written for that calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StructureMethod {
    /// Elliott wave rule checks (`src/indicator/elliott.rs`): compare leg *lengths* (differences
    /// of swing prices) and retracement ratios of such differences, and compare swing *levels*
    /// (wave 2 against the start of wave 1, wave 4 against the end of wave 1). The swings come
    /// from bar highs and lows; a percent swing threshold is relative to the price level.
    ElliottWaves,
    /// Wyckoff range analysis (`src/indicator/wyckoff.rs`): every distance is measured in ATR
    /// (true range, so gaps and jumps count); a climax is a wide bar whose volume is an outlier
    /// against the median of the last `range_lookback` bars, and the range runs from its extreme
    /// to the extreme of the counter-move (without a climax: the high-low spread of those bars);
    /// Spring and UTAD are a single bar's high or low piercing a range boundary while the close
    /// returns.
    WyckoffRange,
    /// Price gaps (`src/indicator/gap.rs`): the open of a bar against the high and low (or the
    /// close) of the bar before it, the distance measured in the ATR of that bar; a gap is closed
    /// by a later low or high that reaches its far edge.
    PriceGaps,
    /// Point-and-figure charts (`src/point_and_figure.rs`): prices are sorted into boxes on a
    /// fixed grid of absolute price levels, and a column turns after a move of a set number of
    /// boxes; the horizontal count multiplies a number of columns by box size and reversal.
    PointAndFigure,
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
    /// [`StructureMethod`] rule: `continuity == StitchedUnadjusted`, a contract change stands in
    /// the series as a price jump.
    StructureRollJumps,
    /// [`StructureMethod`] rule: `continuity == StitchedBackAdjusted`, earlier prices are shifted
    /// by the roll gaps (additively), volume is not.
    StructureBackAdjustedLevels,
    /// [`StructureMethod`] rule: `price_adjustment != Raw`, earlier prices are scaled
    /// proportionally by the adjustment factors.
    StructureProportionalAdjustment,
    /// [`StructureMethod`] rule: `provenance == Broker`, the extremes the method reads are the
    /// provider's quotes.
    StructureProviderExtreme,
    /// [`StructureMethod`] rule: `continuity == SingleContract`, the series ends with the
    /// contract's expiry and holds only that contract's life.
    StructureFiniteHistory,
    /// [`StructureMethod`] rule: `session == Continuous`, the series has no session boundary
    /// between two bars.
    StructureContinuousSession,
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
///
/// **Kein Registry-Indikator loest diese Regel derzeit aus** (Stand
/// 2026-09-08): keiner deklariert `needs_trade_direction`, das einzige
/// `true` im Quelltext steht in den Tests weiter unten. Das ist kein
/// Versehen — auch `cvd` braucht sie nicht, weil es die Aggressorseite aus
/// der Bar-Form schaetzt statt Abschlüsse zu klassifizieren.
///
/// Der Hinweis steht hier, damit ein Leser eine ungenutzte Regel nicht fuer
/// eine seltene haelt. Eine echte Aggressorseiten-Rechnung braeuchte
/// Tickdaten mit Geld- und Briefkurs zum Abschlusszeitpunkt; keine der
/// angebundenen Quellen liefert das.
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

/// Static description of one [`StructureMethod`] rule: for which method, which series property
/// triggers it ([`ApplicabilityReason`]), its tier and its publication-ready explanation.
/// [`method_rule_catalog`] exposes all of them; [`check_applicability`] evaluates exactly this
/// table, so an exported table cannot drift from the rules enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct MethodRuleDescription {
    pub method: StructureMethod,
    pub reason: ApplicabilityReason,
    pub tier: ApplicabilityTier,
    pub explanation: &'static str,
}

const METHOD_RULES: [MethodRuleDescription; 17] = [
    // Elliott: leg lengths are differences of swing prices, retracements are ratios of such
    // differences, swing points are bar highs and lows.
    MethodRuleDescription {
        method: StructureMethod::ElliottWaves,
        reason: ApplicabilityReason::StructureRollJumps,
        tier: ApplicabilityTier::Degraded,
        explanation: "Roll-Sprünge zwischen den Kontrakten stehen als Kursbewegung in der Reihe \
            und würden als Wellenbewegung gezählt",
    },
    MethodRuleDescription {
        method: StructureMethod::ElliottWaves,
        reason: ApplicabilityReason::StructureBackAdjustedLevels,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Rückwärtsbereinigung verschiebt frühere Kurse additiv: Längenvergleiche \
            der Regelprüfung (Differenzen) bleiben erhalten, prozentuale Schwellen und \
            Verhältnisse zum Kursniveau nicht; Wellenpunkte liegen auf nie gehandelten Preisen, \
            und Niveaus können negativ werden",
    },
    MethodRuleDescription {
        method: StructureMethod::ElliottWaves,
        reason: ApplicabilityReason::StructureProportionalAdjustment,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Bereinigung skaliert frühere Kurse proportional: prozentuale \
            Verhältnisse bleiben erhalten, absolute Differenzen vor und nach dem Stichtag nicht — \
            Längenvergleiche wie „Welle 3 nie die kürzeste“ vergleichen über den Stichtag \
            hinweg verschiedene Maßstäbe",
    },
    MethodRuleDescription {
        method: StructureMethod::ElliottWaves,
        reason: ApplicabilityReason::StructureProviderExtreme,
        tier: ApplicabilityTier::Degraded,
        explanation:
            "Hoch und Tief der Wellenpunkte sind Quotes dieses Anbieters und gelten nicht \
            am gehandelten Markt; bei einem anderen Anbieter ergeben sich andere Extreme und \
            damit andere Überlappungen",
    },
    MethodRuleDescription {
        method: StructureMethod::ElliottWaves,
        reason: ApplicabilityReason::StructureFiniteHistory,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Reihe umfasst nur die Laufzeit eines Kontrakts und endet mit dem \
            Verfall; die Zahl der Swings, aus denen höhere Wellengrade gebildet werden, ist \
            dadurch begrenzt",
    },
    // Wyckoff: distances in ATR (true range), climax as a wide bar with a volume outlier against
    // the median of the window, range boundaries from single bars' extremes, Spring/UTAD from a
    // single bar's extreme.
    MethodRuleDescription {
        method: StructureMethod::WyckoffRange,
        reason: ApplicabilityReason::StructureRollJumps,
        tier: ApplicabilityTier::Degraded,
        explanation: "Ein Roll-Sprung verschiebt die Kurse gegen die bereits gesetzten \
            Spannengrenzen und weitet die ATR, in der jede Distanz gemessen wird, und der Umsatz \
            wechselt mit dem Kontrakt: Climax-Test und Sign of Strength messen an einer \
            Umsatzschwelle aus Bars verschiedener Kontrakte",
    },
    MethodRuleDescription {
        method: StructureMethod::WyckoffRange,
        reason: ApplicabilityReason::StructureBackAdjustedLevels,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Preise sind über den Kontraktwechsel angeglichen, der Umsatz nicht: \
            Climax-Test, Secondary Test und Sign of Strength vergleichen an der Roll-Grenze Umsatz \
            verschiedener Kontrakte, und die Spannengrenzen liegen vor dem Wechsel auf nie \
            gehandelten Preisen",
    },
    MethodRuleDescription {
        method: StructureMethod::WyckoffRange,
        reason: ApplicabilityReason::StructureProviderExtreme,
        tier: ApplicabilityTier::Degraded,
        explanation: "Spannengrenzen, Spring und UTAD hängen an einzelnen Hochs und Tiefs (Extrem \
            der Climax-Bar und der Gegenbewegung, Durchstoß einer einzelnen Bar); diese Extreme \
            sind Quotes des Anbieters und gelten nicht am gehandelten Markt",
    },
    // Price gaps: the open against the previous bar's high and low (or close), the distance in
    // ATR of the previous bar, closing by a later bar's low or high.
    MethodRuleDescription {
        method: StructureMethod::PriceGaps,
        reason: ApplicabilityReason::StructureRollJumps,
        tier: ApplicabilityTier::Degraded,
        explanation: "Ein Kontraktwechsel steht als Kurssprung zwischen zwei Bars in der Reihe \
            und wird als Kurslücke gemeldet, ohne dass sich der Markt bewegt hat",
    },
    MethodRuleDescription {
        method: StructureMethod::PriceGaps,
        reason: ApplicabilityReason::StructureBackAdjustedLevels,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Rückwärtsbereinigung verschiebt frühere Kurse additiv: Abstände und \
            damit die Größe früherer Lücken bleiben erhalten, ihre Ränder liegen auf nie \
            gehandelten Preisen; ob an der Roll-Grenze eine Lücke steht, hängt daran, welche \
            Kurse der beiden Kontrakte die Bereinigung angleicht",
    },
    MethodRuleDescription {
        method: StructureMethod::PriceGaps,
        reason: ApplicabilityReason::StructureProportionalAdjustment,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Bereinigung skaliert frühere Kurse proportional: Ränder und absolute \
            Größe früherer Lücken stehen an anderen Zahlen als gehandelt, und der Kurssprung am \
            Stichtag, den die Bereinigung herausnimmt, steht nicht als Lücke in der Reihe",
    },
    MethodRuleDescription {
        method: StructureMethod::PriceGaps,
        reason: ApplicabilityReason::StructureProviderExtreme,
        tier: ApplicabilityTier::Degraded,
        explanation: "Eröffnung, Hoch und Tief sind Quotes des Anbieters: ob zwischen zwei Bars \
            eine Lücke steht und wie groß sie ist, hängt an dessen Kursstellung und \
            Handelszeiten und gilt nicht am gehandelten Markt",
    },
    MethodRuleDescription {
        method: StructureMethod::PriceGaps,
        reason: ApplicabilityReason::StructureContinuousSession,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Reihe hat keine Sitzungsgrenze: zwischen Schluss und nächster \
            Eröffnung liegt keine Handelspause, und eine Lücke steht nur dort, wo die Reihe \
            unterbrochen ist (Wochenende, Ausfall der Kursstellung)",
    },
    // Point and figure: boxes on a fixed grid of absolute prices, a new column after a move of
    // `reversal` boxes.
    MethodRuleDescription {
        method: StructureMethod::PointAndFigure,
        reason: ApplicabilityReason::StructureRollJumps,
        tier: ApplicabilityTier::Degraded,
        explanation: "Ein Roll-Sprung wird wie eine Kursbewegung in Kästchen gezählt: er \
            verlängert die laufende Spalte oder löst eine Umkehr aus, ohne dass sich der Markt \
            bewegt hat",
    },
    MethodRuleDescription {
        method: StructureMethod::PointAndFigure,
        reason: ApplicabilityReason::StructureBackAdjustedLevels,
        tier: ApplicabilityTier::Degraded,
        explanation: "Das Kästchenraster hängt an absoluten Preisen: die Rückwärtsbereinigung \
            verschiebt frühere Kurse additiv gegen das Raster, mit jedem Kontraktwechsel erneut, \
            sodass sich frühere Spalten ändern können und auf nie gehandelten Preisen liegen",
    },
    MethodRuleDescription {
        method: StructureMethod::PointAndFigure,
        reason: ApplicabilityReason::StructureProportionalAdjustment,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Bereinigung skaliert frühere Kurse proportional: eine feste \
            Kästchengröße entspricht vor dem Stichtag einem anderen gehandelten Betrag als \
            danach, und die Spalten liegen auf nie gehandelten Preisen",
    },
    MethodRuleDescription {
        method: StructureMethod::PointAndFigure,
        reason: ApplicabilityReason::StructureProviderExtreme,
        tier: ApplicabilityTier::Degraded,
        explanation: "Die Kurse, aus denen die Spalten entstehen, sind Quotes des Anbieters: bei \
            einem anderen Anbieter liegen sie anders zum Kästchenraster und können andere \
            Spalten und eine andere Zählung ergeben",
    },
];

/// All [`StructureMethod`] rules in the order [`check_applicability`] evaluates them.
pub fn method_rule_catalog() -> [MethodRuleDescription; 17] {
    METHOD_RULES
}

/// Whether the series property a [`StructureMethod`] rule is about is present.
fn method_rule_applies(reason: ApplicabilityReason, capabilities: &SeriesCapabilities) -> bool {
    match reason {
        ApplicabilityReason::StructureRollJumps => {
            capabilities.continuity == ContinuityKind::StitchedUnadjusted
        }
        ApplicabilityReason::StructureBackAdjustedLevels => {
            capabilities.continuity == ContinuityKind::StitchedBackAdjusted
        }
        ApplicabilityReason::StructureProportionalAdjustment => {
            capabilities.price_adjustment != PriceAdjustment::Raw
        }
        ApplicabilityReason::StructureProviderExtreme => {
            capabilities.provenance == Provenance::Broker
        }
        ApplicabilityReason::StructureFiniteHistory => {
            capabilities.continuity == ContinuityKind::SingleContract
        }
        ApplicabilityReason::StructureContinuousSession => {
            capabilities.session == SessionKind::Continuous
        }
        _ => false,
    }
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
/// [`Applicability::Applicable`]. An indicator with a [`DataRequirements::structure_method`]
/// is additionally checked against that method's rules ([`method_rule_catalog`]), whose notes
/// join the same two buckets by their tier.
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

    if let Some(method) = requirements.structure_method {
        for rule in METHOD_RULES.iter().filter(|rule| rule.method == method) {
            if method_rule_applies(rule.reason, capabilities) {
                let note = ApplicabilityNote {
                    reason: rule.reason,
                    explanation: rule.explanation,
                };
                match rule.tier {
                    ApplicabilityTier::Unsuitable => unsuitable_notes.push(note),
                    ApplicabilityTier::Degraded => degraded_notes.push(note),
                }
            }
        }
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

/// Looks up the [`DataRequirements`] for a registry/catalog indicator name. The name is
/// canonicalized first ([`crate::indicator::registry::canonical_name`]: case-insensitive, registry
/// aliases such as `avwap` or `vp` resolved), so an alias carries exactly the requirements of the
/// indicator it builds.
///
/// This is a separate lookup rather than a field on
/// [`crate::indicator::registry::IndicatorCatalogEntry`] because two indicators that need
/// requirements here (`elliott`, `swing_structure`) are not catalog/registry entries at all —
/// they are excluded from the single-bar registry due to their call signature (see the note above
/// `CANONICAL_INDICATOR_NAMES` in `src/indicator/registry.rs`). A standalone function is the only
/// way to give catalog and non-catalog indicators one shared source of truth without forcing a
/// signature change on those two engines.
///
/// `needs_real_volume` follows the actual inputs: it is set for every indicator whose formula
/// always consumes the bar volume (volume and money-flow measures, volume-weighted averages and
/// profiles, and the composites `trend_quality`/`multi_factor`, whose participation factor is a
/// relative volume). It is **not** set for formulas that read OHLC only (`buy_sell_pressure`,
/// `elder_ray`), nor for the composites that use volume as an optional factor and switch it off
/// themselves when the bars carry none (`overextension_risk`, `compression_zones`,
/// `tradability_filter`, `leg_conviction`, `equal_level_liquidity`,
/// `reversal_setup_score`, `structure_zones`, `structural_flow`) — a caller with tick
/// volume passes zero volume to them. `tests/applicability_catalog.rs` checks this split against
/// a run of every catalog entry.
///
/// `elliott` and `wyckoff` do not use the generic session/roll/adjustment flags: they carry a
/// [`StructureMethod`] whose rules ([`method_rule_catalog`]) name what each method reads from the
/// series. `wyckoff` additionally sets `needs_real_volume`: its climax test, its Secondary Test
/// and (unless `breakout_volume` is off) its Sign of Strength/Weakness read the volume. Without
/// volume it finds no climax; only its width fallback then sets a range.
///
/// `gap` carries a [`StructureMethod`] as well: whether a series shows gaps at all depends on its
/// session cut, and a contract change or a price adjustment puts gaps into the series or takes
/// them out of it. `point_and_figure` carries one because its boxes sit on a grid of absolute
/// price levels, which a roll jump crosses and an adjustment shifts.
///
/// Names not listed below (including all pure price indicators) return
/// [`DataRequirements::default()`] — deliberately conservative: `needs_trade_direction` and
/// `needs_liquidity_depth` are left `false` everywhere for now, since no indicator covered by the
/// plan is described as needing actual classified-trade data (bar-derived CVD is a documented
/// heuristic, not a hard requirement) or is named as needing liquidity-depth data.
pub fn data_requirements(name: &str) -> DataRequirements {
    match crate::indicator::registry::canonical_name(name).as_str() {
        "volume_profile"
        | "extended_volume_profile"
        | "persistent_volume_profile"
        | "money_flow_profile" => DataRequirements {
            needs_real_volume: true,
            roll_sensitive: true,
            ..DataRequirements::default()
        },
        "vwap" | "anchored_vwap" | "rvat" => DataRequirements {
            needs_real_volume: true,
            session_sensitive: true,
            ..DataRequirements::default()
        },
        "volume" | "rvol" | "obv" | "acc_dist" | "cmf" | "vwma" | "cvd" | "hires_volume_flow"
        | "efi" | "mfi" | "eom" | "nvi" | "pvi" | "pvt" | "klinger" | "chaikin_oscillator"
        | "midas" | "flow_exhaustion" | "trend_quality" | "multi_factor" => DataRequirements {
            needs_real_volume: true,
            ..DataRequirements::default()
        },
        "elliott" => DataRequirements {
            structure_method: Some(StructureMethod::ElliottWaves),
            ..DataRequirements::default()
        },
        "wyckoff" => DataRequirements {
            needs_real_volume: true,
            structure_method: Some(StructureMethod::WyckoffRange),
            ..DataRequirements::default()
        },
        "gap" => DataRequirements {
            structure_method: Some(StructureMethod::PriceGaps),
            ..DataRequirements::default()
        },
        "point_and_figure" => DataRequirements {
            structure_method: Some(StructureMethod::PointAndFigure),
            ..DataRequirements::default()
        },
        "pivot_sets" | "pivots_structure" | "zigzag" | "zigzag_advanced" | "swing_structure" => {
            DataRequirements {
                session_sensitive: true,
                roll_sensitive: true,
                adjustment_sensitive: true,
                ..DataRequirements::default()
            }
        }
        _ => DataRequirements::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                structure_method: Some(StructureMethod::ElliottWaves),
                ..DataRequirements::default()
            }
        );
        assert_eq!(
            data_requirements("wyckoff"),
            DataRequirements {
                needs_real_volume: true,
                structure_method: Some(StructureMethod::WyckoffRange),
                ..DataRequirements::default()
            }
        );
        // The other extreme-based indicators keep the generic flags.
        for name in [
            "zigzag",
            "zigzag_advanced",
            "pivots_structure",
            "pivot_sets",
            "swing_structure",
        ] {
            assert_eq!(
                data_requirements(name),
                DataRequirements {
                    session_sensitive: true,
                    roll_sensitive: true,
                    adjustment_sensitive: true,
                    ..DataRequirements::default()
                },
                "{name}"
            );
        }
    }

    fn reasons_of(applicability: &Applicability) -> Vec<ApplicabilityReason> {
        match applicability {
            Applicability::Applicable => Vec::new(),
            Applicability::Degraded { reasons } | Applicability::Unsuitable { reasons } => {
                reasons.iter().map(|n| n.reason).collect()
            }
        }
    }

    #[test]
    fn structure_method_rules_follow_the_series_property() {
        use ApplicabilityReason::*;
        // `baseline_capabilities` is a single contract, which has a rule of its own; start from spot.
        let spot_capabilities = || SeriesCapabilities {
            continuity: ContinuityKind::Spot,
            ..baseline_capabilities()
        };
        let elliott = data_requirements("elliott");
        let wyckoff = data_requirements("wyckoff");
        let cases = [
            (ContinuityKind::StitchedUnadjusted, StructureRollJumps),
            (
                ContinuityKind::StitchedBackAdjusted,
                StructureBackAdjustedLevels,
            ),
            (ContinuityKind::SingleContract, StructureFiniteHistory),
        ];
        for (continuity, reason) in cases {
            let caps = SeriesCapabilities {
                continuity,
                ..spot_capabilities()
            };
            assert_eq!(
                reasons_of(&check_applicability(&elliott, &caps)),
                vec![reason]
            );
        }
        let adjusted = SeriesCapabilities {
            price_adjustment: PriceAdjustment::Split,
            ..spot_capabilities()
        };
        assert_eq!(
            reasons_of(&check_applicability(&elliott, &adjusted)),
            vec![StructureProportionalAdjustment]
        );
        // Wyckoff has no rule for adjusted prices or for a finite contract life.
        assert_eq!(
            check_applicability(&wyckoff, &adjusted),
            Applicability::Applicable
        );
        let single = SeriesCapabilities {
            continuity: ContinuityKind::SingleContract,
            ..spot_capabilities()
        };
        assert_eq!(
            check_applicability(&wyckoff, &single),
            Applicability::Applicable
        );
        let broker = SeriesCapabilities {
            provenance: Provenance::Broker,
            ..spot_capabilities()
        };
        assert_eq!(
            reasons_of(&check_applicability(&elliott, &broker)),
            vec![StructureProviderExtreme]
        );
        assert_eq!(
            reasons_of(&check_applicability(&wyckoff, &broker)),
            vec![StructureProviderExtreme]
        );
    }

    #[test]
    fn wyckoff_without_real_volume_is_unsuitable_and_keeps_its_path_notes() {
        let caps = SeriesCapabilities {
            volume: VolumeKind::Tick,
            continuity: ContinuityKind::StitchedUnadjusted,
            ..baseline_capabilities()
        };
        let result = check_applicability(&data_requirements("wyckoff"), &caps);
        assert_eq!(
            reasons_of(&result),
            vec![
                ApplicabilityReason::VolumeIsTickNotTurnover,
                ApplicabilityReason::StructureRollJumps
            ]
        );
        assert!(matches!(result, Applicability::Unsuitable { .. }));
    }

    #[test]
    fn method_rule_catalog_is_unique_per_method_and_reason_and_all_reachable() {
        let rules = method_rule_catalog();
        for (i, a) in rules.iter().enumerate() {
            for b in &rules[i + 1..] {
                assert!(
                    (a.method, a.reason) != (b.method, b.reason),
                    "duplicate rule {:?}/{:?}",
                    a.method,
                    a.reason
                );
            }
            assert_eq!(a.tier, ApplicabilityTier::Degraded);
            assert!(!a.explanation.is_empty());
            // Every rule in the table is triggered by some capability combination.
            let hit = [
                ContinuityKind::Spot,
                ContinuityKind::SingleContract,
                ContinuityKind::StitchedUnadjusted,
                ContinuityKind::StitchedBackAdjusted,
            ]
            .into_iter()
            .flat_map(|continuity| {
                [Provenance::Exchange, Provenance::Broker]
                    .into_iter()
                    .flat_map(move |provenance| {
                        [PriceAdjustment::Raw, PriceAdjustment::SplitAndDividend]
                            .into_iter()
                            .map(move |price_adjustment| SeriesCapabilities {
                                continuity,
                                provenance,
                                price_adjustment,
                                ..baseline_capabilities()
                            })
                    })
            })
            .chain([SeriesCapabilities {
                session: SessionKind::Continuous,
                ..baseline_capabilities()
            }])
            .any(|caps| method_rule_applies(a.reason, &caps));
            assert!(hit, "{:?} is never triggered", a.reason);
        }
    }

    #[test]
    fn gap_rules_follow_session_roll_adjustment_and_provenance() {
        use ApplicabilityReason::*;
        let gap = data_requirements("gap");
        // A single contract on an exchange, regular session, raw prices: gaps mean what they say.
        assert_eq!(
            check_applicability(&gap, &baseline_capabilities()),
            Applicability::Applicable
        );
        let cases = [
            (
                SeriesCapabilities {
                    session: SessionKind::Continuous,
                    ..baseline_capabilities()
                },
                StructureContinuousSession,
            ),
            (
                SeriesCapabilities {
                    continuity: ContinuityKind::StitchedUnadjusted,
                    ..baseline_capabilities()
                },
                StructureRollJumps,
            ),
            (
                SeriesCapabilities {
                    continuity: ContinuityKind::StitchedBackAdjusted,
                    ..baseline_capabilities()
                },
                StructureBackAdjustedLevels,
            ),
            (
                SeriesCapabilities {
                    price_adjustment: PriceAdjustment::Split,
                    ..baseline_capabilities()
                },
                StructureProportionalAdjustment,
            ),
            (
                SeriesCapabilities {
                    provenance: Provenance::Broker,
                    ..baseline_capabilities()
                },
                StructureProviderExtreme,
            ),
        ];
        for (caps, reason) in cases {
            assert_eq!(reasons_of(&check_applicability(&gap, &caps)), vec![reason]);
        }
        // An extended session still has a break between two sessions.
        let extended = SeriesCapabilities {
            session: SessionKind::Extended,
            ..baseline_capabilities()
        };
        assert_eq!(
            check_applicability(&gap, &extended),
            Applicability::Applicable
        );
    }

    #[test]
    fn point_and_figure_rules_follow_roll_adjustment_and_provenance() {
        use ApplicabilityReason::*;
        let pnf = data_requirements("point_and_figure");
        assert_eq!(pnf, data_requirements("pnf"));
        let cases = [
            (
                SeriesCapabilities {
                    continuity: ContinuityKind::StitchedUnadjusted,
                    ..baseline_capabilities()
                },
                StructureRollJumps,
            ),
            (
                SeriesCapabilities {
                    continuity: ContinuityKind::StitchedBackAdjusted,
                    ..baseline_capabilities()
                },
                StructureBackAdjustedLevels,
            ),
            (
                SeriesCapabilities {
                    price_adjustment: PriceAdjustment::SplitAndDividend,
                    ..baseline_capabilities()
                },
                StructureProportionalAdjustment,
            ),
            (
                SeriesCapabilities {
                    provenance: Provenance::Broker,
                    ..baseline_capabilities()
                },
                StructureProviderExtreme,
            ),
        ];
        for (caps, reason) in cases {
            assert_eq!(reasons_of(&check_applicability(&pnf, &caps)), vec![reason]);
        }
        // The chart has no time axis: where the session is cut makes no rule of its own.
        let continuous = SeriesCapabilities {
            session: SessionKind::Continuous,
            ..baseline_capabilities()
        };
        assert_eq!(
            check_applicability(&pnf, &continuous),
            Applicability::Applicable
        );
    }

    /// N28: an alias carries the requirements of the indicator it builds.
    #[test]
    fn data_requirements_resolve_registry_aliases() {
        for (alias, canonical) in [
            ("avwap", "anchored_vwap"),
            ("vp", "volume_profile"),
            ("VP", "volume_profile"),
            ("mfp", "money_flow_profile"),
            ("vp_extended", "extended_volume_profile"),
            ("vp_persistent", "persistent_volume_profile"),
            ("kvo", "klinger"),
            ("cho", "chaikin_oscillator"),
            ("pivots", "pivots_structure"),
            ("multi_pivots", "pivot_sets"),
        ] {
            assert_ne!(data_requirements(canonical), DataRequirements::default());
            assert_eq!(
                data_requirements(alias),
                data_requirements(canonical),
                "alias '{alias}'"
            );
        }
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
