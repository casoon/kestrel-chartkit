//! Differenztest der integrierten Neubewertung gegen unabhängig erzeugte externe Referenzwerte.
//!
//! Paket 14 war bisher nur gegen sich selbst abgenommen: `golden_reference_portfolio_valuation`
//! prüft, dass jede Position genau das ergibt, was die entsprechende Einzelfunktion liefert. Das
//! ist eine notwendige, aber innere Prüfung — sie kann nicht zeigen, dass die Einzelfunktion für
//! *diesen* Fall richtig rechnet.
//!
//! Hier steht deshalb außen eine unabhängige Zweitimplementierung. Verglichen wird genau die
//! Lücke, die die bestehenden Fixtures offenlassen:
//!
//! * eine Anleihe, die auf einer Zinskurve diskontiert wird statt zu einer einzelnen Rendite —
//!   `golden_bond_diff` prüft ausschließlich die Renditerechnung,
//! * dieselbe Anleihe nach paralleler Verschiebung der Kurve,
//! * eine Option, deren Zinssatz aus derselben Kurve stammt, unter verschobenem Spot,
//!   verschobener Volatilität und verschobener Kurve,
//! * und die Sensitivitäten, die daraus folgen: einseitige Differenzen aus zwei vollständigen
//!   Bewertungen, so wie das Crate sie definiert.
//!
//! Bewusst nicht extern nachgerechnet: das Aufsummieren der Positionen und die
//! Währungsumrechnung. Eine Zweitimplementierung einer Addition und einer Multiplikation mit
//! einem gesetzten Kurs würde nichts belegen; dafür ist die Portfolio-Fixture zuständig.

mod common;

use kestrel_chartkit::contract::Currency;
use kestrel_chartkit::finance::{BondSpec, BusinessCalendar, Date, DayCountConvention};
use kestrel_chartkit::option::{OptionStyle, OptionType};
use kestrel_chartkit::portfolio::PositionSide;
use kestrel_chartkit::valuation::portfolio::{MarketScenario, ValuationPosition, ValuedInstrument};
use kestrel_chartkit::valuation::{DiscountCurve, ValuationContext, YieldCurve};

const GOLDEN: &str = include_str!("fixtures/golden_revaluation_diff.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn market(key: &str) -> f64 {
    value(&format!("market_{key}"))
}

fn scenario_value(index: usize, key: &str) -> f64 {
    value(&format!("scen{index}_{key}"))
}

fn date(prefix: &str) -> Date {
    Date::new(
        market(&format!("{prefix}_y")) as i32,
        market(&format!("{prefix}_m")) as u32,
        market(&format!("{prefix}_d")) as u32,
    )
    .expect("gültiges Referenzdatum")
}

fn reference_date() -> Date {
    Date::new(2026, 6, 15).unwrap()
}

/// Beide Seiten diskontieren dieselben Zahlungen mit `exp(-z*t)` über derselben linear
/// interpolierten Kurve, und beide werten dieselbe geschlossene Optionsformel aus. Was bleibt,
/// ist die Reihenfolge der Gleitkommaoperationen: Gemessen weichen die Barwerte um höchstens
/// 5e-13 bei einem Niveau von rund 1000 ab, also etwa ein Bit der Mantisse. Der absolute Boden
/// von 1e-11 plus ein relativer Anteil von 1e-13 lässt davon rund das Zweihundertfache zu und
/// bleibt damit weit unter jeder Abweichung, die ein Modellfehler erzeugen würde.
fn tolerance(expected: f64) -> f64 {
    1e-11 + 1e-13 * expected.abs()
}

fn context(currency: &Currency) -> ValuationContext {
    let node_count = market("node_count") as usize;
    let nodes: Vec<(f64, f64)> = (0..node_count)
        .map(|i| {
            (
                market(&format!("node{i}_t")),
                market(&format!("node{i}_rate")),
            )
        })
        .collect();
    let curve =
        YieldCurve::from_zero_rates(reference_date(), nodes, DayCountConvention::Actual365Fixed)
            .expect("gültige Kurve");

    ValuationContext::new(reference_date(), 1_781_000_000, "reference-14")
        .with_discount_curve(currency.clone(), DiscountCurve::new(curve))
}

fn bond() -> kestrel_chartkit::finance::FixedRateBond {
    BondSpec::new(
        market("face"),
        market("coupon"),
        market("frequency") as u32,
        date("issue"),
        date("maturity"),
        DayCountConvention::Actual365Fixed,
    )
    .build(&BusinessCalendar::weekends_only())
    .expect("gültige Anleihe")
}

fn option_position(currency: &Currency) -> ValuationPosition {
    ValuationPosition::new(
        "OPT",
        currency.clone(),
        PositionSide::Long,
        1.0,
        ValuedInstrument::EuropeanOption {
            option_type: if market("option_type") > 0.0 {
                OptionType::Call
            } else {
                OptionType::Put
            },
            style: OptionStyle::European,
            spot: market("spot"),
            strike: market("strike"),
            expiry: date("expiry"),
            volatility: market("volatility"),
            dividend_yield: market("dividend_yield"),
            contract_size: 1.0,
        },
    )
}

fn scenario(index: usize) -> MarketScenario {
    MarketScenario::neutral()
        .with_rate_shift(scenario_value(index, "rate_shift"))
        .with_underlying_shock(scenario_value(index, "underlying_shock"))
        .with_volatility_shift(scenario_value(index, "volatility_shift"))
}

fn scenario_count() -> usize {
    market("scenario_count") as usize
}

#[test]
fn test_curve_discounted_bond_matches_the_reference_in_every_scenario() {
    let currency = Currency::eur();
    let context = context(&currency);
    let bond = bond();

    for index in 0..scenario_count() {
        let shift = scenario_value(index, "rate_shift");
        let priced = context
            .price_bond_shifted(&bond, &currency, shift)
            .expect("bewertbare Anleihe")
            .into_inner();

        for (label, actual, expected) in [
            (
                "Dirty",
                priced.dirty_price,
                scenario_value(index, "bond_dirty"),
            ),
            (
                "Clean",
                priced.clean_price,
                scenario_value(index, "bond_clean"),
            ),
            (
                "Stückzins",
                priced.accrued_interest,
                scenario_value(index, "bond_accrued"),
            ),
        ] {
            common::assert_close(
                actual,
                expected,
                tolerance(expected),
                &format!("Szenario {index}, {label} der kurvendiskontierten Anleihe"),
            );
        }
    }
}

/// Der Zinssatz, mit dem die Referenz die Option bewertet hat, muss der Satz sein, den die
/// verschobene Kurve zur Fälligkeit liefert. Stimmt das nicht, verglichen die beiden Seiten in
/// [`test_option_revaluation_matches_the_reference_in_every_scenario`] zufällig gleiche Preise
/// aus unterschiedlichen Zinsen.
#[test]
fn test_the_shifted_curve_reproduces_the_reference_rate() {
    let curve_time = market("expiry_t");

    for index in 0..scenario_count() {
        let shift = scenario_value(index, "rate_shift");
        let expected = scenario_value(index, "option_rate");
        let curve = YieldCurve::from_zero_rates(
            reference_date(),
            (0..market("node_count") as usize)
                .map(|i| {
                    (
                        market(&format!("node{i}_t")),
                        market(&format!("node{i}_rate")),
                    )
                })
                .collect(),
            DayCountConvention::Actual365Fixed,
        )
        .unwrap()
        .shifted(shift);

        common::assert_close(
            curve.zero_rate(curve_time).unwrap(),
            expected,
            tolerance(expected),
            &format!("Szenario {index}, Zinssatz zur Fälligkeit"),
        );
    }
}

#[test]
fn test_option_revaluation_matches_the_reference_in_every_scenario() {
    let currency = Currency::eur();
    let context = context(&currency);
    let position = option_position(&currency);

    for index in 0..scenario_count() {
        let expected = scenario_value(index, "option_price");
        let valued = context
            .value_position(&position, &currency, &scenario(index))
            .expect("bewertbare Position");
        common::assert_close(
            valued.unit_value,
            expected,
            tolerance(expected),
            &format!("Szenario {index}, Optionspreis nach Neubewertung"),
        );
    }
}

#[test]
fn test_sensitivities_are_the_reference_differences() {
    let currency = Currency::eur();
    let context = context(&currency);
    let bond = bond();
    let position = option_position(&currency);

    let base_bond = context
        .price_bond_shifted(&bond, &currency, 0.0)
        .unwrap()
        .into_inner()
        .dirty_price;
    let base_option = context
        .value_position(&position, &currency, &MarketScenario::neutral())
        .unwrap()
        .unit_value;

    for index in 0..scenario_count() {
        let bond_delta = context
            .price_bond_shifted(&bond, &currency, scenario_value(index, "rate_shift"))
            .unwrap()
            .into_inner()
            .dirty_price
            - base_bond;
        let option_delta = context
            .value_position(&position, &currency, &scenario(index))
            .unwrap()
            .unit_value
            - base_option;

        for (label, actual, expected, scale) in [
            (
                "Anleihe",
                bond_delta,
                scenario_value(index, "bond_delta"),
                base_bond,
            ),
            (
                "Option",
                option_delta,
                scenario_value(index, "option_delta"),
                base_option,
            ),
        ] {
            // Die Toleranz richtet sich nach dem Bewertungsniveau, aus dem die Differenz
            // gebildet wurde, nicht nach der Differenz selbst: Bei einer Sensitivität von wenigen
            // Hundertsteln wäre ein relativer Anteil daran enger als die Rechengenauigkeit der
            // beiden Bewertungen, aus denen sie entsteht.
            common::assert_close(
                actual,
                expected,
                tolerance(scale),
                &format!("Szenario {index}, Sensitivität {label}"),
            );
        }
    }
}

/// Die Fixture muss die Aussage tragen, die der Test macht.
///
/// Ein Szenariosatz, in dem sich nichts bewegt, würde jede Neubewertung bestehen. Geprüft wird
/// deshalb, dass jede der drei Marktgrößen mindestens einmal allein verschoben wird, dass die
/// Anleihe auf Zinsen und nur auf Zinsen reagiert, und dass die Option auf alle drei reagiert.
#[test]
fn test_the_scenario_set_actually_moves_each_market_variable() {
    let mut isolated_rate = 0;
    let mut isolated_spot = 0;
    let mut isolated_vol = 0;

    for index in 0..scenario_count() {
        let rate = scenario_value(index, "rate_shift");
        let spot = scenario_value(index, "underlying_shock");
        let vol = scenario_value(index, "volatility_shift");

        if rate != 0.0 && spot == 0.0 && vol == 0.0 {
            isolated_rate += 1;
            assert!(
                scenario_value(index, "bond_delta") != 0.0,
                "Szenario {index}: eine Zinsverschiebung muss die Anleihe bewegen"
            );
        }
        if spot != 0.0 && rate == 0.0 && vol == 0.0 {
            isolated_spot += 1;
            assert_eq!(
                scenario_value(index, "bond_delta"),
                0.0,
                "Szenario {index}: ein Kursschock darf die Anleihe nicht bewegen"
            );
            assert!(
                scenario_value(index, "option_delta") != 0.0,
                "Szenario {index}: ein Kursschock muss die Option bewegen"
            );
        }
        if vol != 0.0 && rate == 0.0 && spot == 0.0 {
            isolated_vol += 1;
            assert_eq!(
                scenario_value(index, "bond_delta"),
                0.0,
                "Szenario {index}: eine Volatilitätsverschiebung darf die Anleihe nicht bewegen"
            );
            assert!(
                scenario_value(index, "option_delta") > 0.0,
                "Szenario {index}: mehr Volatilität muss eine gekaufte Option teurer machen"
            );
        }
    }

    assert!(
        isolated_rate >= 2 && isolated_spot >= 2 && isolated_vol >= 2,
        "jede Marktgröße muss isoliert und in zwei Größenordnungen vorkommen: \
         Zins {isolated_rate}, Kurs {isolated_spot}, Volatilität {isolated_vol}"
    );
}

/// Ein großer Schock ist nicht der kleine, hochskaliert — und die Referenz bestätigt das.
#[test]
fn test_a_large_shock_is_not_the_small_one_scaled_up() {
    let small = (0..scenario_count())
        .find(|i| scenario_value(*i, "rate_shift") == 0.0001)
        .expect("ein Basispunkt-Szenario");
    let large = (0..scenario_count())
        .find(|i| scenario_value(*i, "rate_shift") == 0.01)
        .expect("ein 100-Basispunkte-Szenario");

    let scaled = scenario_value(small, "bond_delta") * 100.0;
    let actual = scenario_value(large, "bond_delta");
    assert!(
        actual > scaled,
        "Konvexität: die tatsächliche Wertänderung {actual} müsste über der linearen \
         Hochrechnung {scaled} liegen"
    );
    assert!(
        (actual - scaled).abs() > 1e-3,
        "der Unterschied wäre mit {} nicht messbar",
        (actual - scaled).abs()
    );
}
