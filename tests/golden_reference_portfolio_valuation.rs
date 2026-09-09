//! Abnahme von Paket 14: ein gemischtes Portfolio aus linearem Instrument, europäischer Option
//! und Anleihe, in zwei Währungen, unter Szenarien neu bewertet.
//!
//! Die Referenz ist hier nicht eine externe Bibliothek, sondern die bereits abgenommene
//! Einzelbewertung: Jede Portfolioposition muss genau das ergeben, was die entsprechende
//! Einzelfunktion für dieselben Eingaben liefert. Ein Aggregat, das von seinen Bestandteilen
//! abweicht, wäre ein zweiter Rechenweg.

mod common;

use kestrel_chartkit::contract::{Currency, FxRate};
use kestrel_chartkit::finance::{BondSpec, BusinessCalendar, Date, DayCountConvention};
use kestrel_chartkit::option::{OptionStyle, OptionType};
use kestrel_chartkit::portfolio::PositionSide;
use kestrel_chartkit::valuation::portfolio::{
    MarketScenario, ValuationModel, ValuationPosition, ValuedInstrument,
};
use kestrel_chartkit::valuation::{
    DiscountCurve, ValuationContext, ValuationContextError, YieldCurve,
};

fn today() -> Date {
    Date::new(2026, 6, 15).unwrap()
}

fn context() -> ValuationContext {
    let eur_curve = YieldCurve::from_zero_rates(
        today(),
        vec![(0.5, 0.02), (5.0, 0.035)],
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();
    let usd_curve = YieldCurve::flat(today(), 0.045, DayCountConvention::Actual365Fixed);

    ValuationContext::new(today(), 1_781_000_000, "snapshot-14")
        .with_discount_curve(Currency::eur(), DiscountCurve::new(eur_curve))
        .with_discount_curve(Currency::usd(), DiscountCurve::new(usd_curve))
        .with_fx_rate(FxRate::new("USD", "EUR", 0.9))
}

fn mixed_portfolio() -> Vec<ValuationPosition> {
    let bond = BondSpec::new(
        1_000.0,
        0.05,
        2,
        today(),
        Date::new(2031, 6, 15).unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .build(&BusinessCalendar::weekends_only())
    .unwrap();

    vec![
        ValuationPosition::new(
            "EQ",
            Currency::eur(),
            PositionSide::Long,
            100.0,
            ValuedInstrument::Linear {
                price: 42.5,
                multiplier: 1.0,
            },
        ),
        ValuationPosition::new(
            "CALL",
            Currency::eur(),
            PositionSide::Long,
            5.0,
            ValuedInstrument::EuropeanOption {
                option_type: OptionType::Call,
                style: OptionStyle::European,
                spot: 100.0,
                strike: 105.0,
                expiry: Date::new(2027, 6, 15).unwrap(),
                volatility: 0.22,
                dividend_yield: 0.01,
                contract_size: 100.0,
            },
        ),
        ValuationPosition::new(
            "BOND",
            Currency::usd(),
            PositionSide::Long,
            3.0,
            ValuedInstrument::Bond {
                bond: Box::new(bond),
            },
        ),
    ]
}

/// Das neutrale Szenario muss die Basiswerte reproduzieren — sonst wäre der Szenarioweg ein
/// anderer Rechenweg als der Basisweg.
#[test]
fn test_neutral_scenario_reproduces_the_base_valuation() {
    let context = context();
    let positions = mixed_portfolio();
    let account = Currency::eur();

    let base = context
        .value_portfolio(&positions, &account, &MarketScenario::neutral())
        .unwrap();
    let stressed = context
        .stress_portfolio(&positions, &account, &MarketScenario::neutral())
        .unwrap();

    assert_eq!(stressed.value.base, base.value);
    assert_eq!(stressed.value.stressed, base.value);
    assert_eq!(stressed.value.pnl_account, 0.0);
}

/// Jede Position muss genau das ergeben, was ihre Einzelbewertung liefert.
#[test]
fn test_positions_match_their_independent_single_instrument_valuations() {
    let context = context();
    let positions = mixed_portfolio();
    let account = Currency::eur();

    let valued = context
        .value_portfolio(&positions, &account, &MarketScenario::neutral())
        .unwrap();

    // Linear: Preis mal Multiplikator mal Stück.
    let equity = &valued.value.positions[0];
    assert_eq!(equity.model, ValuationModel::Linear);
    common::assert_close(equity.unit_value, 42.5, 0.0, "linearer Stückwert");
    common::assert_close(equity.value, 4_250.0, 0.0, "linearer Positionswert");
    common::assert_close(equity.value_account, 4_250.0, 0.0, "bereits Kontowährung");

    // Option: gegen die Einzelbewertung über denselben Kontext.
    let option = &valued.value.positions[1];
    assert_eq!(option.model, ValuationModel::BlackScholesMerton);
    let single = context
        .price_european_option(
            &Currency::eur(),
            OptionType::Call,
            100.0,
            105.0,
            Date::new(2027, 6, 15).unwrap(),
            0.22,
            0.01,
        )
        .unwrap();
    common::assert_close(
        option.unit_value,
        single.value.price * 100.0,
        0.0,
        "Optionsstückwert ist Preis mal Kontraktgröße",
    );

    // Anleihe: gegen die kurvenbasierte Einzelbewertung, danach in Kontowährung.
    let bond_position = &valued.value.positions[2];
    assert_eq!(bond_position.model, ValuationModel::DiscountedCashflows);
    let ValuedInstrument::Bond { bond } = &positions[2].instrument else {
        panic!("dritte Position ist die Anleihe");
    };
    let single_bond = context.price_bond(bond, &Currency::usd()).unwrap();
    common::assert_close(
        bond_position.unit_value,
        single_bond.value.dirty_price,
        0.0,
        "Anleihen-Stückwert ist der Dirty Price",
    );
    common::assert_close(
        bond_position.value_account,
        single_bond.value.dirty_price * 3.0 * 0.9,
        1e-12,
        "in Kontowährung umgerechnet",
    );

    // Summe der Positionen ist der Portfoliowert.
    let sum: f64 = valued.value.positions.iter().map(|p| p.value_account).sum();
    common::assert_close(
        valued.value.total_value_account,
        sum,
        1e-9,
        "Aggregat ist die Summe seiner Teile",
    );
}

/// Short-Positionen kehren das Vorzeichen um, nicht den Stückwert.
#[test]
fn test_short_positions_invert_the_sign_not_the_unit_value() {
    let context = context();
    let account = Currency::eur();
    let long = ValuationPosition::new(
        "EQ",
        Currency::eur(),
        PositionSide::Long,
        10.0,
        ValuedInstrument::Linear {
            price: 42.5,
            multiplier: 1.0,
        },
    );
    let mut short = long.clone();
    short.side = PositionSide::Short;

    let long_value = context
        .value_position(&long, &account, &MarketScenario::neutral())
        .unwrap();
    let short_value = context
        .value_position(&short, &account, &MarketScenario::neutral())
        .unwrap();

    assert_eq!(long_value.unit_value, short_value.unit_value);
    assert_eq!(long_value.value, -short_value.value);
    assert_eq!(short_value.signed_quantity, -10.0);
}

/// Gerichtete Schocks: Ein Kurssturz trifft die lineare Position proportional und die Option
/// stärker als proportional — jeweils gegen die unabhängige Einzelbewertung geprüft.
#[test]
fn test_directional_shocks_match_independent_single_valuations() {
    let context = context();
    let positions = mixed_portfolio();
    let account = Currency::eur();
    let crash = MarketScenario::neutral().with_underlying_shock(-0.10);

    let stressed = context
        .value_portfolio(&positions, &account, &crash)
        .unwrap();

    // Linear: exakt minus zehn Prozent.
    common::assert_close(
        stressed.value.positions[0].value,
        4_250.0 * 0.9,
        1e-12,
        "lineare Position folgt dem Schock proportional",
    );

    // Option: gegen dieselbe Formel mit geschocktem Spot.
    let single = context
        .price_european_option(
            &Currency::eur(),
            OptionType::Call,
            90.0,
            105.0,
            Date::new(2027, 6, 15).unwrap(),
            0.22,
            0.01,
        )
        .unwrap();
    common::assert_close(
        stressed.value.positions[1].unit_value,
        single.value.price * 100.0,
        0.0,
        "Option unter Kursschock",
    );
}

/// Ein Zinsanstieg senkt den Anleihenwert und lässt die lineare Position unberührt.
#[test]
fn test_rate_shift_hits_the_bond_and_leaves_the_linear_position_alone() {
    let context = context();
    let positions = mixed_portfolio();
    let account = Currency::eur();

    let base = context
        .value_portfolio(&positions, &account, &MarketScenario::neutral())
        .unwrap();
    let higher_rates = context
        .value_portfolio(
            &positions,
            &account,
            &MarketScenario::neutral().with_rate_shift(0.01),
        )
        .unwrap();

    assert_eq!(
        base.value.positions[0].value, higher_rates.value.positions[0].value,
        "die lineare Position kennt keinen Zins"
    );
    assert!(
        higher_rates.value.positions[2].value < base.value.positions[2].value,
        "höhere Zinsen senken den Anleihenwert"
    );
}

/// Sensitivitäten sind Neubewertungen: Jede muss der Differenz entsprechen, die dasselbe
/// Szenario einzeln erzeugt.
#[test]
fn test_sensitivities_equal_the_revaluation_they_are_defined_as() {
    let context = context();
    let positions = mixed_portfolio();
    let account = Currency::eur();

    let result = context
        .stress_portfolio(&positions, &account, &MarketScenario::neutral())
        .unwrap();
    let base = result.value.base.total_value_account;

    for (label, scenario, reported) in [
        (
            "Underlying +1%",
            MarketScenario::neutral().with_underlying_shock(0.01),
            result.value.sensitivities.underlying_up_1pct,
        ),
        (
            "Volatilität +1 Punkt",
            MarketScenario::neutral().with_volatility_shift(0.01),
            result.value.sensitivities.volatility_up_1pt,
        ),
        (
            "Zinsen +1 bp",
            MarketScenario::neutral().with_rate_shift(0.0001),
            result.value.sensitivities.rate_up_1bp,
        ),
        (
            "FX +1%",
            MarketScenario::neutral().with_fx_shock(0.01),
            result.value.sensitivities.fx_up_1pct,
        ),
    ] {
        let recomputed = context
            .value_portfolio(&positions, &account, &scenario)
            .unwrap()
            .value
            .total_value_account
            - base;
        common::assert_close(reported, recomputed, 0.0, label);
    }

    // Richtungen, die aus der Zusammensetzung eindeutig folgen.
    assert!(
        result.value.sensitivities.underlying_up_1pct > 0.0,
        "long in Aktie und Call"
    );
    assert!(
        result.value.sensitivities.volatility_up_1pt > 0.0,
        "long in einer Option"
    );
    assert!(
        result.value.sensitivities.fx_up_1pct > 0.0,
        "eine Position notiert in Fremdwährung"
    );
}

/// Die Zinsrichtung ist im gemischten Portfolio *nicht* eindeutig: Der Long Call gewinnt bei
/// steigenden Zinsen, die Anleihe verliert. Das Aggregat kann deshalb positiv sein, obwohl eine
/// Anleihe darin steckt — genau der Fall, in dem eine je Instrumenttyp getrennt gerechnete
/// Kennzahl in die Irre führen würde. Der Test hält beide Seiten einzeln fest.
#[test]
fn test_rate_sensitivity_nets_opposing_positions_instead_of_hiding_them() {
    let context = context();
    let account = Currency::eur();
    let positions = mixed_portfolio();

    let sensitivity_of = |subset: Vec<ValuationPosition>| {
        let base = context
            .value_portfolio(&subset, &account, &MarketScenario::neutral())
            .unwrap()
            .value
            .total_value_account;
        context
            .value_portfolio(
                &subset,
                &account,
                &MarketScenario::neutral().with_rate_shift(0.0001),
            )
            .unwrap()
            .value
            .total_value_account
            - base
    };

    let bond_only = sensitivity_of(vec![positions[2].clone()]);
    let option_only = sensitivity_of(vec![positions[1].clone()]);
    let together = sensitivity_of(positions.clone());

    assert!(
        bond_only < 0.0,
        "die Anleihe verliert bei steigenden Zinsen"
    );
    assert!(option_only > 0.0, "der Long Call gewinnt");
    common::assert_close(
        together,
        bond_only + option_only,
        1e-9,
        "das Aggregat ist die Summe der Einzelwirkungen",
    );
}

/// Konvexität wird nicht wegskaliert: Ein großer Schock ist beim Optionsanteil nicht das
/// Hundertfache des kleinen.
#[test]
fn test_large_shocks_are_not_the_small_one_scaled_up() {
    let context = context();
    let positions = mixed_portfolio();
    let account = Currency::eur();

    let base = context
        .value_portfolio(&positions, &account, &MarketScenario::neutral())
        .unwrap()
        .value
        .total_value_account;
    let small = context
        .value_portfolio(
            &positions,
            &account,
            &MarketScenario::neutral().with_underlying_shock(0.01),
        )
        .unwrap()
        .value
        .total_value_account
        - base;
    let large = context
        .value_portfolio(
            &positions,
            &account,
            &MarketScenario::neutral().with_underlying_shock(0.20),
        )
        .unwrap()
        .value
        .total_value_account
        - base;

    assert!(
        large > small * 20.0,
        "die Long-Option macht den großen Anstieg überproportional: {large} vs {}",
        small * 20.0
    );
}

/// Amerikanische Ausübung hat keine Engine — und wird abgelehnt statt europäisch bewertet.
#[test]
fn test_american_exercise_is_refused_rather_than_priced_as_european() {
    let context = context();
    let account = Currency::eur();
    let american = ValuationPosition::new(
        "PUT",
        Currency::eur(),
        PositionSide::Long,
        1.0,
        ValuedInstrument::EuropeanOption {
            option_type: OptionType::Put,
            style: OptionStyle::American,
            spot: 100.0,
            strike: 110.0,
            expiry: Date::new(2027, 6, 15).unwrap(),
            volatility: 0.3,
            dividend_yield: 0.0,
            contract_size: 1.0,
        },
    );

    assert_eq!(
        context.value_position(&american, &account, &MarketScenario::neutral()),
        Err(ValuationContextError::UnsupportedExercise(
            OptionStyle::American
        ))
    );
}

/// Eine fehlende Kurve oder ein fehlender Wechselkurs bleibt auch im Portfolio ein Fehler.
#[test]
fn test_missing_market_data_fails_the_whole_valuation() {
    let context = ValuationContext::new(today(), 0, "leer");
    let account = Currency::eur();
    assert!(context
        .value_portfolio(&mixed_portfolio(), &account, &MarketScenario::neutral())
        .is_err());

    let without_fx = ValuationContext::new(today(), 0, "ohne fx").with_discount_curve(
        Currency::usd(),
        DiscountCurve::new(YieldCurve::flat(
            today(),
            0.03,
            DayCountConvention::Actual365Fixed,
        )),
    );
    let usd_only = vec![ValuationPosition::new(
        "EQ",
        Currency::usd(),
        PositionSide::Long,
        1.0,
        ValuedInstrument::Linear {
            price: 10.0,
            multiplier: 1.0,
        },
    )];
    assert!(matches!(
        without_fx.value_portfolio(&usd_only, &account, &MarketScenario::neutral()),
        Err(ValuationContextError::MissingFxRate { .. })
    ));
}

/// Unbrauchbare Szenarien werden abgelehnt.
#[test]
fn test_invalid_scenarios_are_rejected() {
    let context = context();
    let account = Currency::eur();
    let positions = mixed_portfolio();

    for scenario in [
        MarketScenario::neutral().with_underlying_shock(-1.5),
        MarketScenario::neutral().with_fx_shock(-2.0),
        MarketScenario::neutral().with_rate_shift(f64::NAN),
    ] {
        assert!(matches!(
            context.value_portfolio(&positions, &account, &scenario),
            Err(ValuationContextError::InvalidScenario(_))
        ));
    }
}

/// Das Ergebnis trägt den Datenstand mit, damit ein Report ihn weiterreichen kann.
#[test]
fn test_portfolio_results_carry_the_stamp() {
    let context = context();
    let result = context
        .stress_portfolio(
            &mixed_portfolio(),
            &Currency::eur(),
            &MarketScenario::neutral().with_underlying_shock(-0.05),
        )
        .unwrap();

    assert_eq!(result.stamp.valuation_date, today());
    assert_eq!(result.stamp.as_of, 1_781_000_000);
    assert_eq!(result.stamp.data_version, "snapshot-14");
}
