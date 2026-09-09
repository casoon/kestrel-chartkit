//! Abnahme des Bewertungskontexts: Kurven, Datenstand und die Anbindung von Anleihen und
//! Optionen.
//!
//! Die Referenzwerte sind unabhängig hergeleitet — analytisch eindeutige Fälle (flache Kurve,
//! Handinterpolation, Forward-Konsistenz) und der Abgleich gegen die bereits bestätigten
//! Konstantzins-Ergebnisse dieses Crates. Der Flat-Fall ist dabei der wichtigste: Er belegt, dass
//! der neue Kontext die bisherigen Zahlen reproduziert statt sie zu verschieben.

mod common;

use kestrel_chartkit::contract::{Currency, FxRate};
use kestrel_chartkit::finance::{
    discount_factor, BondSpec, BusinessCalendar, Compounding, Date, DayCountConvention,
};
use kestrel_chartkit::option::{black_scholes_merton, BlackScholesInputs, OptionType};
use kestrel_chartkit::valuation::{
    DiscountCurve, ForwardCurve, ValuationContext, ValuationContextError, YieldCurve,
};

fn reference_date() -> Date {
    Date::new(2026, 6, 15).unwrap()
}

fn flat_curve(rate: f64) -> YieldCurve {
    YieldCurve::flat(reference_date(), rate, DayCountConvention::Actual365Fixed)
}

/// Eine flache Kurve muss exakt das liefern, was die bestehende Konstantzins-Diskontierung
/// liefert — sonst hätte der Kontext die bisherigen Ergebnisse verschoben.
#[test]
fn test_flat_curve_reproduces_the_constant_rate_discount_factor() {
    for rate in [-0.005, 0.0, 0.03, 0.12] {
        let curve = flat_curve(rate);
        for years in [0.25, 1.0, 7.5] {
            let expected = discount_factor(rate, years, Compounding::Continuous);
            common::assert_close(
                curve.discount_factor_at(years).unwrap(),
                expected,
                0.0,
                &format!("flache Kurve r={rate}, t={years}"),
            );
        }
    }
}

/// Lineare Interpolation zwischen zwei Stützstellen, von Hand nachgerechnet: Auf halbem Weg
/// zwischen 1 Jahr (2%) und 3 Jahren (4%) liegt der Zerosatz bei 3%.
#[test]
fn test_zero_rates_interpolate_linearly_between_nodes() {
    let curve = YieldCurve::from_zero_rates(
        reference_date(),
        vec![(1.0, 0.02), (3.0, 0.04)],
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    common::assert_close(curve.zero_rate(2.0).unwrap(), 0.03, 1e-15, "Mitte");
    common::assert_close(
        curve.zero_rate(1.5).unwrap(),
        0.025,
        1e-15,
        "erstes Viertel",
    );
    common::assert_close(
        curve.zero_rate(1.0).unwrap(),
        0.02,
        0.0,
        "erste Stützstelle",
    );
    common::assert_close(
        curve.zero_rate(3.0).unwrap(),
        0.04,
        0.0,
        "letzte Stützstelle",
    );
}

/// Außerhalb der Stützstellen wird flach gehalten — eine Konvention, die am Typ steht und hier
/// festgenagelt wird, damit sie nicht unbemerkt zu einer Fortsetzung der Steigung wird.
#[test]
fn test_extrapolation_holds_the_edge_rates_flat() {
    let curve = YieldCurve::from_zero_rates(
        reference_date(),
        vec![(1.0, 0.02), (3.0, 0.04)],
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    common::assert_close(curve.zero_rate(0.0).unwrap(), 0.02, 0.0, "vor der ersten");
    common::assert_close(curve.zero_rate(0.5).unwrap(), 0.02, 0.0, "vor der ersten");
    common::assert_close(
        curve.zero_rate(10.0).unwrap(),
        0.04,
        0.0,
        "nach der letzten",
    );
    common::assert_close(curve.zero_rate(50.0).unwrap(), 0.04, 0.0, "weit danach");
}

/// Der Forward muss die Kurve reproduzieren: Diskontiert man über `t1` und dann mit dem Forward
/// weiter bis `t2`, muss derselbe Diskontfaktor herauskommen wie direkt auf `t2`.
#[test]
fn test_forward_rate_is_consistent_with_the_discount_factors() {
    let curve = YieldCurve::from_zero_rates(
        reference_date(),
        vec![(0.5, 0.01), (2.0, 0.025), (5.0, 0.03)],
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    for (t1, t2) in [(0.5, 2.0), (1.0, 3.0), (2.0, 5.0), (0.25, 0.75)] {
        let forward = curve.forward_rate(t1, t2).unwrap();
        let stepwise = curve.discount_factor_at(t1).unwrap() * (-forward * (t2 - t1)).exp();
        common::assert_close(
            stepwise,
            curve.discount_factor_at(t2).unwrap(),
            1e-15,
            &format!("Forward {t1}..{t2}"),
        );
    }
}

/// Negative Zinsen bleiben rechenbar: Der Diskontfaktor ist dann größer als eins, nicht ungültig.
#[test]
fn test_negative_rates_produce_discount_factors_above_one() {
    let curve = flat_curve(-0.01);
    let df = curve.discount_factor_at(5.0).unwrap();
    assert!(df > 1.0, "Diskontfaktor bei negativem Zins: {df}");
    common::assert_close(df, (0.05_f64).exp(), 1e-15, "exakt exp(0.05)");
}

/// Eine Abfrage vor dem Referenzdatum ist keine Extrapolation, sondern ein Eingabefehler.
#[test]
fn test_dates_before_the_reference_date_are_rejected() {
    let curve = flat_curve(0.03);
    assert_eq!(
        curve.discount_factor(Date::new(2026, 6, 14).unwrap()),
        Err(ValuationContextError::TimeOutsideCurve)
    );
    assert!(curve.discount_factor(reference_date()).is_ok());
}

#[test]
fn test_invalid_curves_are_rejected() {
    let reference = reference_date();
    let day_count = DayCountConvention::Actual365Fixed;
    assert!(YieldCurve::from_zero_rates(reference, vec![], day_count).is_err());
    assert!(
        YieldCurve::from_zero_rates(reference, vec![(2.0, 0.01), (1.0, 0.02)], day_count).is_err(),
        "Stützstellen müssen aufsteigen"
    );
    assert!(
        YieldCurve::from_zero_rates(reference, vec![(1.0, 0.01), (1.0, 0.02)], day_count).is_err(),
        "doppelte Laufzeit"
    );
    assert!(
        YieldCurve::from_zero_rates(reference, vec![(-1.0, 0.01)], day_count).is_err(),
        "negative Laufzeit"
    );
    assert!(
        YieldCurve::from_zero_rates(reference, vec![(1.0, f64::NAN)], day_count).is_err(),
        "nicht endlicher Satz"
    );
}

/// Fehlende Marktdaten werden nicht durch erfundene ersetzt.
#[test]
fn test_missing_curves_and_rates_are_errors_not_defaults() {
    let context = ValuationContext::new(reference_date(), 1_781_000_000, "snapshot-1");
    let eur = Currency::eur();

    assert_eq!(
        context.discount_curve(&eur),
        Err(ValuationContextError::MissingDiscountCurve(eur.clone()))
    );
    assert_eq!(
        context.forward_curve(&eur),
        Err(ValuationContextError::MissingForwardCurve(eur.clone()))
    );
    assert_eq!(
        context.convert(100.0, &eur, &Currency::usd()),
        Err(ValuationContextError::MissingFxRate {
            from: eur,
            to: Currency::usd(),
        })
    );
}

/// Der Datenstand wandert mit dem Ergebnis mit — sonst ließe sich eine Zahl später nicht mehr
/// ihrem Eingabestand zuordnen.
#[test]
fn test_results_carry_the_valuation_stamp() {
    let context = ValuationContext::new(reference_date(), 1_781_000_000, "snapshot-1")
        .with_discount_curve(Currency::eur(), DiscountCurve::new(flat_curve(0.03)));

    let priced = context
        .price_european_option(
            &Currency::eur(),
            OptionType::Call,
            100.0,
            100.0,
            Date::new(2027, 6, 15).unwrap(),
            0.2,
            0.0,
        )
        .unwrap();

    assert_eq!(priced.stamp.valuation_date, reference_date());
    assert_eq!(priced.stamp.as_of, 1_781_000_000);
    assert_eq!(priced.stamp.data_version, "snapshot-1");
}

/// Über eine flache Kurve muss der Kontext exakt den Preis liefern, den die direkte
/// Konstantzins-Bewertung liefert.
#[test]
fn test_option_on_a_flat_curve_matches_direct_constant_rate_pricing() {
    let expiry = Date::new(2027, 6, 15).unwrap();
    let context = ValuationContext::new(reference_date(), 0, "flat")
        .with_discount_curve(Currency::eur(), DiscountCurve::new(flat_curve(0.05)));

    let via_context = context
        .price_european_option(
            &Currency::eur(),
            OptionType::Call,
            100.0,
            95.0,
            expiry,
            0.25,
            0.02,
        )
        .unwrap();

    let direct = black_scholes_merton(
        OptionType::Call,
        &BlackScholesInputs {
            spot: 100.0,
            strike: 95.0,
            time_to_expiry_years: 365.0 / 365.0,
            risk_free_rate: 0.05,
            dividend_yield: 0.02,
            volatility: 0.25,
        },
    )
    .unwrap();

    assert_eq!(via_context.value, direct);
}

/// Die Zinskurve wirkt: Ein höherer Zerosatz zur Fälligkeit ergibt einen anderen Preis, und
/// zwischen zwei Stützstellen den interpolierten.
#[test]
fn test_option_uses_the_zero_rate_to_its_own_expiry() {
    let context = ValuationContext::new(reference_date(), 0, "term-structure").with_discount_curve(
        Currency::eur(),
        DiscountCurve::new(
            YieldCurve::from_zero_rates(
                reference_date(),
                vec![(1.0, 0.02), (3.0, 0.06)],
                DayCountConvention::Actual365Fixed,
            )
            .unwrap(),
        ),
    );

    // 2027-06-15 liegt 365 Tage entfernt, also exakt auf der ersten Stützstelle.
    let one_year = context
        .price_european_option(
            &Currency::eur(),
            OptionType::Call,
            100.0,
            100.0,
            Date::new(2027, 6, 15).unwrap(),
            0.2,
            0.0,
        )
        .unwrap();
    let expected_one_year = black_scholes_merton(
        OptionType::Call,
        &BlackScholesInputs {
            spot: 100.0,
            strike: 100.0,
            time_to_expiry_years: 1.0,
            risk_free_rate: 0.02,
            dividend_yield: 0.0,
            volatility: 0.2,
        },
    )
    .unwrap();
    assert_eq!(one_year.value, expected_one_year);

    // 2029-06-15 liegt 1096 Tage entfernt, also 3.0027 Jahre — hinter der letzten Stützstelle,
    // dort gilt flach deren Satz.
    let three_years = context
        .price_european_option(
            &Currency::eur(),
            OptionType::Call,
            100.0,
            100.0,
            Date::new(2029, 6, 15).unwrap(),
            0.2,
            0.0,
        )
        .unwrap();
    let expected_three_years = black_scholes_merton(
        OptionType::Call,
        &BlackScholesInputs {
            spot: 100.0,
            strike: 100.0,
            time_to_expiry_years: 1096.0 / 365.0,
            risk_free_rate: 0.06,
            dividend_yield: 0.0,
            volatility: 0.2,
        },
    )
    .unwrap();
    assert_eq!(three_years.value, expected_three_years);
}

/// Anleihe auf flacher Kurve gegen Anleihe auf konstanter Rendite: Beide müssen denselben Preis
/// ergeben, wenn die Verzinsungskonventionen ineinander umgerechnet werden —
/// `r_stetig = f * ln(1 + y/f)`. Das prüft die Anbindung an den Kontext gegen den bereits
/// bestätigten Bewertungsweg.
#[test]
fn test_bond_on_a_flat_curve_matches_the_equivalent_constant_yield() {
    let settlement = reference_date();
    let bond = BondSpec::new(
        1_000.0,
        0.05,
        2,
        settlement,
        Date::new(2031, 6, 15).unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .build(&BusinessCalendar::weekends_only())
    .unwrap();

    let ytm = 0.04_f64;
    let frequency = 2.0_f64;
    let continuous = frequency * (1.0 + ytm / frequency).ln();

    let context = ValuationContext::new(settlement, 0, "flat")
        .with_discount_curve(Currency::eur(), DiscountCurve::new(flat_curve(continuous)));

    let on_curve = context.price_bond(&bond, &Currency::eur()).unwrap();
    let at_yield = bond.price(settlement, ytm).unwrap();

    common::assert_close(
        on_curve.value.dirty_price,
        at_yield.dirty_price,
        1e-9,
        "Dirty Price",
    );
    common::assert_close(
        on_curve.value.accrued_interest,
        at_yield.accrued_interest,
        0.0,
        "Stückzinsen kommen aus dem Plan, nicht aus der Diskontierung",
    );
}

/// Eine steigende Kurve muss eine Anleihe niedriger bewerten als eine flache Kurve auf dem
/// kurzen Ende — die Zinsstruktur darf nicht wirkungslos durchgereicht werden.
#[test]
fn test_bond_valuation_reacts_to_the_shape_of_the_curve() {
    let settlement = reference_date();
    let bond = BondSpec::new(
        1_000.0,
        0.05,
        2,
        settlement,
        Date::new(2031, 6, 15).unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .build(&BusinessCalendar::weekends_only())
    .unwrap();

    let flat = ValuationContext::new(settlement, 0, "flat")
        .with_discount_curve(Currency::eur(), DiscountCurve::new(flat_curve(0.02)))
        .price_bond(&bond, &Currency::eur())
        .unwrap();

    let rising = ValuationContext::new(settlement, 0, "rising")
        .with_discount_curve(
            Currency::eur(),
            DiscountCurve::new(
                YieldCurve::from_zero_rates(
                    settlement,
                    vec![(0.5, 0.02), (5.0, 0.05)],
                    DayCountConvention::Actual365Fixed,
                )
                .unwrap(),
            ),
        )
        .price_bond(&bond, &Currency::eur())
        .unwrap();

    assert!(
        rising.value.dirty_price < flat.value.dirty_price,
        "steigende Kurve: {} muss unter {} liegen",
        rising.value.dirty_price,
        flat.value.dirty_price
    );
}

/// Diskontierungs- und Forward-Kurve sind getrennte Rollen; beide lesen dieselbe Struktur, aber
/// der Kontext hält sie auseinander.
#[test]
fn test_discount_and_forward_curves_are_kept_apart() {
    let curve = YieldCurve::from_zero_rates(
        reference_date(),
        vec![(1.0, 0.02), (5.0, 0.04)],
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    let context = ValuationContext::new(reference_date(), 0, "roles")
        .with_discount_curve(Currency::eur(), DiscountCurve::new(curve.clone()))
        .with_forward_curve(Currency::usd(), ForwardCurve::new(curve));

    assert!(context.discount_curve(&Currency::eur()).is_ok());
    assert!(context.forward_curve(&Currency::eur()).is_err());
    assert!(context.forward_curve(&Currency::usd()).is_ok());
    assert!(context.discount_curve(&Currency::usd()).is_err());
}

/// Umrechnung nutzt die hinterlegten Kurse in beiden Richtungen, erfindet aber keine Kreuzkurse.
#[test]
fn test_fx_conversion_uses_given_rates_and_refuses_to_build_cross_rates() {
    let context = ValuationContext::new(reference_date(), 0, "fx")
        .with_fx_rate(FxRate::new("EUR", "USD", 1.10));

    common::assert_close(
        context
            .convert(100.0, &Currency::eur(), &Currency::usd())
            .unwrap(),
        110.0,
        1e-12,
        "direkt",
    );
    common::assert_close(
        context
            .convert(110.0, &Currency::usd(), &Currency::eur())
            .unwrap(),
        100.0,
        1e-12,
        "invers",
    );
    common::assert_close(
        context
            .convert(100.0, &Currency::eur(), &Currency::eur())
            .unwrap(),
        100.0,
        0.0,
        "identisch",
    );
    assert!(
        context
            .convert(100.0, &Currency::gbp(), &Currency::usd())
            .is_err(),
        "kein Kreuzkurs über EUR"
    );
}
