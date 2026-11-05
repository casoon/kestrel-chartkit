//! Applicability verdicts against `reference/kestrel_reference/fixtures/applicability.py`.
//!
//! The reference derives each verdict from the documented rules and the declared properties of
//! ten series classes (the classes a learning site publishes a matrix for). Two fixtures:
//!
//! - `golden_applicability.txt` is the complete matrix, 19 indicators x 10 classes, including the
//!   named requirements of `elliott` and `wyckoff` and the single-contract class.
//! - `golden_applicability_v0_35.txt` is what 0.35.0 produced for the 17 other indicators on the 9
//!   classes that existed then. Those rows must not change when rules for other indicators or
//!   classes are added.

use kestrel_chartkit::applicability::{check_applicability, data_requirements, Applicability};
use kestrel_chartkit::model::{
    ContinuityKind, LiquidityTier, PriceAdjustment, Provenance, SeriesCapabilities, SessionKind,
    VolumeKind,
};

const FULL: &str = include_str!("fixtures/golden_applicability.txt");
const FROZEN: &str = include_str!("fixtures/golden_applicability_v0_35.txt");

const INDICATORS: [&str; 19] = [
    "rsi",
    "macd",
    "atr",
    "adx",
    "bollinger",
    "volume_profile",
    "vwap",
    "anchored_vwap",
    "mfi",
    "cvd",
    "buy_sell_pressure",
    "klinger",
    "elder_ray",
    "pivots_structure",
    "swing_structure",
    "zigzag",
    "elliott",
    "wyckoff",
    "pivot_sets",
];

const fn caps(
    volume: VolumeKind,
    session: SessionKind,
    continuity: ContinuityKind,
    price_adjustment: PriceAdjustment,
    provenance: Provenance,
    liquidity_tier: LiquidityTier,
) -> SeriesCapabilities {
    SeriesCapabilities {
        volume,
        trade_direction: false,
        session,
        continuity,
        price_adjustment,
        provenance,
        liquidity_tier,
    }
}

fn classes() -> [(&'static str, SeriesCapabilities); 10] {
    use ContinuityKind::*;
    use LiquidityTier::*;
    use PriceAdjustment::{Raw, SplitAndDividend};
    use Provenance::*;
    use SessionKind::*;
    use VolumeKind::*;
    [
        ("index-cash", caps(None, Regular, Spot, Raw, Exchange, Deep)),
        (
            "index-future",
            caps(RealTurnover, Extended, SingleContract, Raw, Exchange, Deep),
        ),
        (
            "future-single",
            caps(
                RealTurnover,
                Extended,
                SingleContract,
                Raw,
                Exchange,
                Normal,
            ),
        ),
        (
            "future-continuous",
            caps(
                RealTurnover,
                Extended,
                StitchedBackAdjusted,
                Raw,
                Exchange,
                Deep,
            ),
        ),
        (
            "cfd-broker",
            caps(Tick, Continuous, Spot, Raw, Broker, Unknown),
        ),
        ("fx-spot", caps(Tick, Continuous, Spot, Raw, Broker, Deep)),
        (
            "stock-adjusted",
            caps(
                RealTurnover,
                Regular,
                Spot,
                SplitAndDividend,
                Exchange,
                Deep,
            ),
        ),
        (
            "stock-thin",
            caps(RealTurnover, Regular, Spot, Raw, Exchange, Thin),
        ),
        (
            "commodity-chained",
            caps(
                RealTurnover,
                Extended,
                StitchedUnadjusted,
                Raw,
                Exchange,
                Normal,
            ),
        ),
        (
            "synthetic",
            caps(RealTurnover, Continuous, Spot, Raw, Synthetic, Normal),
        ),
    ]
}

/// `indicator<TAB>class<TAB>verdict<TAB>explanations` for the requested pairs, in the reference's
/// order (indicators outer, classes inner).
fn lines(indicators: &[&str], class_ids: &[&str]) -> Vec<String> {
    let all = classes();
    let mut out = Vec::new();
    for indicator in indicators {
        let requirements = data_requirements(indicator);
        for id in class_ids {
            let (_, capabilities) = all.iter().find(|(name, _)| name == id).unwrap();
            let (verdict, reasons) = match check_applicability(&requirements, capabilities) {
                Applicability::Applicable => ("applicable", Vec::new()),
                Applicability::Degraded { reasons } => ("limited", reasons),
                Applicability::Unsuitable { reasons } => ("misleading", reasons),
            };
            let explanations: Vec<&str> = reasons.iter().map(|n| n.explanation).collect();
            out.push(format!(
                "{indicator}\t{id}\t{verdict}\t{}",
                explanations.join(" || ")
            ));
        }
    }
    out
}

fn fixture_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .collect()
}

fn class_ids() -> Vec<&'static str> {
    classes().iter().map(|(id, _)| *id).collect()
}

#[test]
fn matrix_matches_reference_for_all_indicators_and_classes() {
    let actual = lines(&INDICATORS, &class_ids());
    let expected = fixture_lines(FULL);
    assert_eq!(actual.len(), 190);
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(&expected) {
        assert_eq!(a, e);
    }
}

#[test]
fn unchanged_indicators_keep_their_v0_35_rows() {
    let indicators: Vec<&str> = INDICATORS
        .iter()
        .copied()
        .filter(|name| !matches!(*name, "elliott" | "wyckoff"))
        .collect();
    let class_ids: Vec<&str> = class_ids()
        .into_iter()
        .filter(|id| *id != "future-single")
        .collect();
    let actual = lines(&indicators, &class_ids);
    let frozen = fixture_lines(FROZEN);
    assert_eq!(frozen.len(), 17 * 9);
    assert_eq!(actual.len(), frozen.len());
    for (a, e) in actual.iter().zip(&frozen) {
        assert_eq!(a, e);
    }
}

#[test]
fn new_single_contract_class_leaves_the_generic_indicators_as_for_index_future() {
    // A single contract has no roll in its course: for every indicator except the two with named
    // requirements the row equals the existing single-contract class.
    let generic: Vec<&str> = INDICATORS
        .iter()
        .copied()
        .filter(|name| !matches!(*name, "elliott" | "wyckoff"))
        .collect();
    let single = lines(&generic, &["future-single"]);
    let index = lines(&generic, &["index-future"]);
    for (s, i) in single.iter().zip(&index) {
        assert_eq!(s.replacen("future-single", "index-future", 1), *i, "{s}");
    }
}

#[test]
fn elliott_and_wyckoff_give_each_class_its_own_reason() {
    let rows = lines(&["elliott", "wyckoff"], &class_ids());
    let find = |indicator: &str, class: &str| {
        rows.iter()
            .find(|r| r.starts_with(&format!("{indicator}\t{class}\t")))
            .unwrap()
            .clone()
    };
    // Elliott: one finding per kind of series defect, none of the generic wordings.
    for class in ["index-cash", "synthetic", "stock-thin"] {
        assert!(find("elliott", class).contains("\tapplicable\t"), "{class}");
    }
    for (class, fragment) in [
        ("commodity-chained", "als Wellenbewegung gezählt"),
        ("future-continuous", "Differenzen"),
        ("stock-adjusted", "Welle 3 nie die kürzeste"),
        ("cfd-broker", "Quotes dieses Anbieters"),
        ("fx-spot", "Quotes dieses Anbieters"),
        ("future-single", "höhere Wellengrade"),
        ("index-future", "höhere Wellengrade"),
    ] {
        let row = find("elliott", class);
        assert!(row.contains("\tlimited\t"), "{class}: {row}");
        assert!(row.contains(fragment), "{class}: {row}");
        assert!(!row.contains("Extreme sind anbieterspezifisch"), "{class}");
        assert!(!row.contains("Roll-Sprünge in der Reihe"), "{class}");
    }
    // Wyckoff: no volume or tick volume is unsuitable; rolls limit; a single contract is fine.
    assert!(find("wyckoff", "index-cash").contains("\tmisleading\t"));
    for class in ["cfd-broker", "fx-spot"] {
        let row = find("wyckoff", class);
        assert!(row.contains("\tmisleading\t"), "{class}");
        assert!(row.contains("Update-Häufigkeit"), "{class}");
        assert!(row.contains("Spring und UTAD"), "{class}");
    }
    for class in ["commodity-chained", "future-continuous"] {
        let row = find("wyckoff", class);
        assert!(row.contains("\tlimited\t"), "{class}");
        assert!(row.contains("Umsatz verschiedener Kontrakte"), "{class}");
    }
    for class in [
        "index-future",
        "future-single",
        "stock-adjusted",
        "stock-thin",
        "synthetic",
    ] {
        assert!(find("wyckoff", class).contains("\tapplicable\t"), "{class}");
    }
}
