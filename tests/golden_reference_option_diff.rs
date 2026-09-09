//! Differenztests der Optionsbewertung gegen unabhängig erzeugte externe Referenzwerte.
//!
//! Die Fixtures entstehen offline aus einer unabhängigen Zweitimplementierung derselben Modelle
//! und tragen ihre Eingaben selbst; dieser Test rechnet nur gegen die Fixtures. Verglichen werden
//! nur fachlich identische Modelle: europäische Optionen mit flachen Zinskurven, Actual/365Fixed
//! und ganztägigen Laufzeiten, sodass der Jahresbruchteil der Referenz exakt die
//! `time_to_expiry_years` dieses Crates ist.
//!
//! **Toleranzen.** Verglichen wird `|ours - reference| <= abs_floor + rel * |reference|`. Der
//! absolute Boden ist nötig, weil tief aus dem Geld liegende Optionen mit fast null Volatilität
//! Preise um 1e-12 haben: dort ist jede relative Aussage nur noch Auslöschung. Der relative Anteil
//! trägt die Preisgröße. Beide sind so eng gewählt, dass sie die tatsächlich gemessenen
//! Abweichungen nur knapp überdecken — sie sind kein Puffer für unerklärte Unterschiede.

mod common;

use kestrel_chartkit::option::{
    black_76, black_scholes_merton, implied_volatility, normal_cdf, BlackScholesInputs, OptionType,
};

const GOLDEN: &str = include_str!("fixtures/golden_option_diff.txt");

/// Absoluter Boden und relativer Anteil der Vergleichstoleranz.
const ABS_FLOOR: f64 = 1e-10;
const REL: f64 = 1e-11;

fn close_enough(ours: f64, reference: f64, label: &str) {
    let tolerance = ABS_FLOOR + REL * reference.abs();
    assert!(
        (ours - reference).abs() <= tolerance,
        "{label}: {ours} weicht von {reference} um {} ab (erlaubt {tolerance})",
        (ours - reference).abs()
    );
}

fn option_type(code: f64) -> OptionType {
    if code > 0.0 {
        OptionType::Call
    } else {
        OptionType::Put
    }
}

#[test]
fn test_normal_cdf_matches_reference_across_body_and_tails() {
    let n = common::golden_value(GOLDEN, "meta_cdf_case_count") as usize;
    for i in 1..=n {
        let x = common::golden_value(GOLDEN, &format!("cdf{i}_x"));
        let reference = common::golden_value(GOLDEN, &format!("cdf{i}_value"));
        // Der Boden von 1e-16 trägt die Fernausläufer, wo der Referenzwert selbst bei 6e-16 liegt.
        let tolerance = 1e-16 + 1e-12 * reference.abs();
        assert!(
            (normal_cdf(x) - reference).abs() <= tolerance,
            "N({x}): {} weicht von {reference} ab (erlaubt {tolerance})",
            normal_cdf(x)
        );
    }
}

#[test]
fn test_black_scholes_prices_and_greeks_match_reference() {
    let n = common::golden_value(GOLDEN, "meta_option_case_count") as usize;
    assert!(n >= 70, "das Parameterfeld darf nicht schrumpfen");

    for i in 1..=n {
        let p = format!("opt{i}");
        let value = |key: &str| common::golden_value(GOLDEN, &format!("{p}_{key}"));

        let inputs = BlackScholesInputs {
            spot: value("spot"),
            strike: value("strike"),
            time_to_expiry_years: value("t"),
            risk_free_rate: value("r"),
            dividend_yield: value("q"),
            volatility: value("vol"),
        };
        let got = black_scholes_merton(option_type(value("type")), &inputs)
            .unwrap_or_else(|e| panic!("{p}: Bewertung schlug fehl: {e:?}"));

        close_enough(got.price, value("price"), &format!("{p} Preis"));
        close_enough(got.greeks.delta, value("delta"), &format!("{p} Delta"));
        close_enough(got.greeks.gamma, value("gamma"), &format!("{p} Gamma"));
        close_enough(got.greeks.vega, value("vega"), &format!("{p} Vega"));
        close_enough(
            got.greeks.theta_annual,
            value("theta_annual"),
            &format!("{p} Theta"),
        );
        close_enough(got.greeks.rho, value("rho"), &format!("{p} Rho"));
    }
}

/// Theta pro Tag ist definitionsgemäß das annualisierte Theta durch 365 — dieselbe Konvention,
/// unter der die Referenzwerte entstanden sind. Geprüft an der Referenzreihe, damit die
/// Umrechnung nicht unabhängig vom Referenzwert driftet.
#[test]
fn test_daily_theta_is_the_annual_reference_over_365() {
    let n = common::golden_value(GOLDEN, "meta_option_case_count") as usize;
    for i in 1..=n {
        let p = format!("opt{i}");
        let value = |key: &str| common::golden_value(GOLDEN, &format!("{p}_{key}"));
        let inputs = BlackScholesInputs {
            spot: value("spot"),
            strike: value("strike"),
            time_to_expiry_years: value("t"),
            risk_free_rate: value("r"),
            dividend_yield: value("q"),
            volatility: value("vol"),
        };
        let got = black_scholes_merton(option_type(value("type")), &inputs).unwrap();
        close_enough(
            got.greeks.theta_daily,
            value("theta_annual") / 365.0,
            &format!("{p} Theta pro Tag"),
        );
    }
}

#[test]
fn test_black_76_matches_reference_black_formula() {
    let n = common::golden_value(GOLDEN, "meta_black76_case_count") as usize;
    assert!(n >= 70, "das Parameterfeld darf nicht schrumpfen");

    for i in 1..=n {
        let p = format!("b76_{i}");
        let value = |key: &str| common::golden_value(GOLDEN, &format!("{p}_{key}"));
        let got = black_76(
            option_type(value("type")),
            value("forward"),
            value("strike"),
            value("t"),
            value("r"),
            value("vol"),
        )
        .unwrap_or_else(|e| panic!("{p}: Bewertung schlug fehl: {e:?}"));

        close_enough(got.price, value("price"), &format!("{p} Preis"));
    }
}

/// Die Umkehrung wird gegen die *Volatilität* geprüft, nicht gegen den Preis: Die Referenz
/// bepreist bei bekannter Volatilität, und der Solver dieses Crates muss genau diese
/// zurückliefern.
#[test]
fn test_implied_volatility_recovers_the_reference_input_volatility() {
    let n = common::golden_value(GOLDEN, "meta_option_case_count") as usize;
    let mut checked = 0;

    for i in 1..=n {
        let p = format!("opt{i}");
        let value = |key: &str| common::golden_value(GOLDEN, &format!("{p}_{key}"));
        let inputs = BlackScholesInputs {
            spot: value("spot"),
            strike: value("strike"),
            time_to_expiry_years: value("t"),
            risk_free_rate: value("r"),
            dividend_yield: value("q"),
            volatility: value("vol"),
        };
        let vega = black_scholes_merton(option_type(value("type")), &inputs)
            .unwrap()
            .greeks
            .vega;
        // Bei verschwindendem Vega ist der Preis von der Volatilität praktisch unabhängig; dort
        // ist keine Umkehrung definiert, und ein Solver, der trotzdem eine Zahl liefert, würde
        // Genauigkeit behaupten, die die Eingabe nicht hergibt.
        if vega < 1e-4 {
            continue;
        }

        let solved = implied_volatility(
            option_type(value("type")),
            value("price"),
            inputs.spot,
            inputs.strike,
            inputs.time_to_expiry_years,
            inputs.risk_free_rate,
            inputs.dividend_yield,
        )
        .unwrap_or_else(|e| panic!("{p}: Umkehrung schlug fehl: {e:?}"));

        assert!(
            (solved - value("vol")).abs() < 1e-6,
            "{p}: Umkehrung ergab {solved} statt {}",
            value("vol")
        );
        checked += 1;
    }

    assert!(
        checked >= 40,
        "zu wenige Fälle mit auswertbarem Vega geprüft: {checked}"
    );
}
