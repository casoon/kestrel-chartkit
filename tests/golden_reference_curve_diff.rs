//! Differenztests der Zinskurve gegen unabhängig erzeugte externe Referenzwerte.
//!
//! Die Fixtures entstehen offline aus einer unabhängigen Zweitimplementierung: stetig verzinste
//! Zerosätze, linear in der Zeit interpoliert. Innerhalb des Stützstellenbereichs müssen beide
//! Seiten auf Rechengenauigkeit übereinstimmen.
//!
//! **Hinter der letzten Stützstelle tun sie das nicht, und das ist Absicht.** Die Referenz setzt
//! die Steigung des letzten Segments fort, dieses Crate hält den Randsatz flach — eine
//! ausdrücklich getroffene Entscheidung, weil eine fortgesetzte Steigung wenige Jahre später
//! unsinnige Diskontfaktoren erzeugt. Solche Abfragen sind in der Fixture als `inside = 0`
//! markiert; der Test prüft dort die eigene Zusage und hält die Abweichung fest, statt sie mit
//! einer weiten Toleranz zu verwischen.

mod common;

use kestrel_chartkit::finance::{Date, DayCountConvention};
use kestrel_chartkit::valuation::YieldCurve;

const GOLDEN: &str = include_str!("fixtures/golden_curve_diff.txt");

fn value(curve: usize, key: &str) -> f64 {
    common::golden_value(GOLDEN, &format!("curve{curve}_{key}"))
}

fn reference_date() -> Date {
    Date::new(2026, 6, 15).unwrap()
}

fn build(curve_index: usize) -> YieldCurve {
    let node_count = value(curve_index, "node_count") as usize;
    let nodes: Vec<(f64, f64)> = (0..node_count)
        .map(|i| {
            (
                value(curve_index, &format!("node{i}_t")),
                value(curve_index, &format!("node{i}_rate")),
            )
        })
        .collect();
    YieldCurve::from_zero_rates(reference_date(), nodes, DayCountConvention::Actual365Fixed)
        .expect("gültige Kurve")
}

/// Rechengenauigkeit, kein Näherungsspielraum: Beide Seiten interpolieren dieselben Stützstellen
/// linear und diskontieren mit `exp(-z*t)`.
const TOLERANCE: f64 = 1e-12;

fn inside(curve_index: usize, query: usize) -> bool {
    value(curve_index, &format!("query{query}_inside")) > 0.5
}

#[test]
fn test_interpolated_zero_rates_match_the_reference() {
    let cases = common::golden_value(GOLDEN, "meta_curve_case_count") as usize;
    assert!(cases >= 3, "steigende, invertierte und flache Kurve prüfen");
    let mut compared = 0;

    for curve_index in 1..=cases {
        let curve = build(curve_index);
        for query in 0..(value(curve_index, "query_count") as usize) {
            if !inside(curve_index, query) {
                continue;
            }
            let time = value(curve_index, &format!("query{query}_t"));
            let expected = value(curve_index, &format!("query{query}_zero"));
            common::assert_close(
                curve.zero_rate(time).unwrap(),
                expected,
                TOLERANCE,
                &format!("Kurve {curve_index}, Zerosatz bei t={time}"),
            );
            compared += 1;
        }
    }
    assert!(
        compared >= 15,
        "zu wenige vergleichbare Abfragen: {compared}"
    );
}

#[test]
fn test_discount_factors_match_the_reference() {
    let cases = common::golden_value(GOLDEN, "meta_curve_case_count") as usize;
    for curve_index in 1..=cases {
        let curve = build(curve_index);
        for query in 0..(value(curve_index, "query_count") as usize) {
            if !inside(curve_index, query) {
                continue;
            }
            let time = value(curve_index, &format!("query{query}_t"));
            let expected = value(curve_index, &format!("query{query}_discount"));
            common::assert_close(
                curve.discount_factor_at(time).unwrap(),
                expected,
                TOLERANCE,
                &format!("Kurve {curve_index}, Diskontfaktor bei t={time}"),
            );
        }
    }
}

/// Vor der ersten Stützstelle halten beide Seiten flach. Dahinter nicht: Der Test hält die
/// eigene Zusage fest *und* dass sie von der Referenz abweicht — verschwindet diese Abweichung
/// eines Tages, hat sich eine der beiden Konventionen geändert und das soll auffallen.
#[test]
fn test_flat_extrapolation_is_a_stated_divergence_beyond_the_last_node() {
    let curve = build(1);
    let last_rate = value(1, "node3_rate");

    common::assert_close(
        curve.zero_rate(0.1).unwrap(),
        value(1, "node0_rate"),
        TOLERANCE,
        "vor der ersten Stützstelle halten beide flach",
    );

    let outside: Vec<usize> = (0..(value(1, "query_count") as usize))
        .filter(|query| !inside(1, *query))
        .collect();
    assert!(
        !outside.is_empty(),
        "es muss Abfragen jenseits der Kurve geben"
    );

    for query in outside {
        let time = value(1, &format!("query{query}_t"));
        let reference = value(1, &format!("query{query}_zero"));
        common::assert_close(
            curve.zero_rate(time).unwrap(),
            last_rate,
            TOLERANCE,
            &format!("flach gehalten bei t={time}"),
        );
        assert!(
            (reference - last_rate).abs() > 1e-9,
            "bei t={time} setzt die Referenz die Steigung fort; wäre sie hier gleich, hätte sich \
             eine der Konventionen geändert"
        );
    }
}

/// Negative Sätze sind kein Sonderfall der Referenz und keiner hier.
#[test]
fn test_negative_rates_agree_with_the_reference() {
    let curve = build(2);
    for query in 0..(value(2, "query_count") as usize) {
        if !inside(2, query) {
            continue;
        }
        let time = value(2, &format!("query{query}_t"));
        let discount = curve.discount_factor_at(time).unwrap();
        let expected = value(2, &format!("query{query}_discount"));
        common::assert_close(discount, expected, TOLERANCE, "invertierte Kurve");
    }
    assert!(
        curve.discount_factor_at(1.0).unwrap() > 1.0,
        "bei negativem Satz liegt der Diskontfaktor über eins"
    );
}
