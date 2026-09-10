//! Differenztests der Volatilitätsfläche gegen eine unabhängige Zweitimplementierung.
//!
//! Die Fixture erzeugt `reference/generate.py` aus den dokumentierten Formeln (nur
//! Python-Standardbibliothek). Offen war an der Fläche nicht die Rechnung, sondern die
//! Festlegung, *wogegen* verglichen wird — denn zwischen den notierten Punkten gibt es mehr als
//! eine gebräuchliche Konvention.
//!
//! Dieses Crate interpoliert bilinear in der Volatilität selbst: linear in der Laufzeit, linear
//! im Strike. Der Vergleich läuft gegen genau diese Konvention und muss auf Rechengenauigkeit
//! stimmen.
//!
//! **Die Alternative steht mit im Test.** Die andere etablierte Konvention interpoliert entlang
//! der Laufzeitachse linear in der Gesamtvarianz `σ²·t`; sie ist es, die Kalender-Arbitrage
//! zwischen den notierten Laufzeiten ausschließt. Beide stimmen auf jedem notierten Gitterpunkt
//! überein und weichen dazwischen ab. Die Fixture trägt beide Werte, und
//! [`test_the_variance_convention_is_a_real_alternative_not_a_rounding_difference`] hält die
//! Größe der Abweichung fest — die Konventionswahl steht damit als Zahl im Test und nicht nur als
//! Satz in der Dokumentation.
//!
//! Außerhalb des notierten Gitters weichen beide Seiten ohnehin ab: Die Referenz setzt die
//! Steigung des Randsegments fort, dieses Crate hält den Randwert flach. Solche Abfragen sind als
//! `inside = 0` markiert und werden getrennt geprüft.

mod common;

use kestrel_chartkit::finance::{Date, DayCountConvention};
use kestrel_chartkit::valuation::volatility::{SurfaceValidity, VolatilitySurface};

const GOLDEN: &str = include_str!("fixtures/golden_surface_diff.txt");

/// Rechengenauigkeit, kein Näherungsspielraum: Beide Seiten werten dieselbe bilineare Form über
/// denselben Stützstellen aus.
const TOLERANCE: f64 = 1e-12;

fn value(surface: usize, key: &str) -> f64 {
    common::golden_value(GOLDEN, &format!("surface{surface}_{key}"))
}

fn reference_date() -> Date {
    Date::new(2026, 6, 15).unwrap()
}

/// Großzügig genug für die absichtlich knapp außerhalb liegenden Abfragen der Fixture, damit
/// deren Randverhalten überhaupt geprüft werden kann statt an der Gültigkeitsgrenze zu scheitern.
fn validity() -> SurfaceValidity {
    SurfaceValidity {
        strike_tolerance: 0.1,
        maturity_tolerance_years: 0.25,
    }
}

fn build(case: usize) -> VolatilitySurface {
    let maturity_count = value(case, "maturity_count") as usize;
    let strike_count = value(case, "strike_count") as usize;

    let maturities: Vec<f64> = (0..maturity_count)
        .map(|row| value(case, &format!("maturity{row}_t")))
        .collect();
    let strikes: Vec<f64> = (0..strike_count)
        .map(|column| value(case, &format!("strike{column}")))
        .collect();
    let volatilities: Vec<Vec<f64>> = (0..maturity_count)
        .map(|row| {
            (0..strike_count)
                .map(|column| value(case, &format!("vol{row}_{column}")))
                .collect()
        })
        .collect();

    VolatilitySurface::new(
        reference_date(),
        maturities,
        strikes,
        volatilities,
        DayCountConvention::Actual365Fixed,
        validity(),
    )
    .expect("gültige Fläche")
}

fn case_count() -> usize {
    common::golden_value(GOLDEN, "meta_surface_case_count") as usize
}

fn inside(case: usize, query: usize) -> bool {
    value(case, &format!("query{query}_inside")) > 0.5
}

#[test]
fn test_interpolated_volatilities_match_the_reference() {
    let cases = case_count();
    assert!(cases >= 3, "geneigte, geskewte und flache Fläche prüfen");
    let mut compared = 0;

    for case in 1..=cases {
        let surface = build(case);
        for query in 0..(value(case, "query_count") as usize) {
            if !inside(case, query) {
                continue;
            }
            let time = value(case, &format!("query{query}_t"));
            let strike = value(case, &format!("query{query}_k"));
            common::assert_close(
                surface.volatility_at(time, strike).unwrap(),
                value(case, &format!("query{query}_vol")),
                TOLERANCE,
                &format!("Fläche {case}, Vola bei t={time}, K={strike}"),
            );
            compared += 1;
        }
    }

    assert!(
        compared >= 60,
        "der Vergleich muss die Gitter wirklich abdecken; verglichen: {compared}"
    );
}

#[test]
fn test_quoted_grid_points_are_returned_unchanged() {
    for case in 1..=case_count() {
        let surface = build(case);
        let maturity_count = value(case, "maturity_count") as usize;
        let strike_count = value(case, "strike_count") as usize;

        for row in 0..maturity_count {
            let time = value(case, &format!("maturity{row}_t"));
            for column in 0..strike_count {
                let strike = value(case, &format!("strike{column}"));
                common::assert_close(
                    surface.volatility_at(time, strike).unwrap(),
                    value(case, &format!("vol{row}_{column}")),
                    TOLERANCE,
                    &format!("Fläche {case}, notierter Punkt t={time}, K={strike}"),
                );
            }
        }
    }
}

/// Auf den notierten Punkten müssen beide Konventionen dieselbe Zahl liefern — täten sie das
/// nicht, wäre eine von beiden falsch parametrisiert und der Kontrast wertlos.
#[test]
fn test_both_conventions_agree_on_the_quoted_points() {
    for case in 1..=case_count() {
        for query in 0..(value(case, "query_count") as usize) {
            let time = value(case, &format!("query{query}_t"));
            let strike = value(case, &format!("query{query}_k"));
            let on_node = (0..value(case, "maturity_count") as usize)
                .any(|row| value(case, &format!("maturity{row}_t")) == time)
                && (0..value(case, "strike_count") as usize)
                    .any(|column| value(case, &format!("strike{column}")) == strike);
            if !on_node {
                continue;
            }
            common::assert_close(
                value(case, &format!("query{query}_variance_vol")),
                value(case, &format!("query{query}_vol")),
                1e-12,
                &format!("Fläche {case}, notierter Punkt t={time}, K={strike}"),
            );
        }
    }
}

/// Hält die Konventionsentscheidung als Zahl fest.
///
/// Zwischen den Laufzeiten liefert die Varianz-Konvention andere Volatilitäten. Der Test prüft,
/// dass dieser Unterschied real und gerichtet ist: groß genug, um keine Rundung zu sein, und auf
/// der flachen Fläche exakt null, wo jede sinnvolle Konvention denselben Wert liefern muss.
#[test]
fn test_the_variance_convention_is_a_real_alternative_not_a_rounding_difference() {
    let mut largest_difference: f64 = 0.0;

    for case in 1..=case_count() {
        let surface = build(case);
        let flat_case = case == 3;
        let mut case_largest: f64 = 0.0;

        for query in 0..(value(case, "query_count") as usize) {
            if !inside(case, query) {
                continue;
            }
            let time = value(case, &format!("query{query}_t"));
            let strike = value(case, &format!("query{query}_k"));
            let ours = surface.volatility_at(time, strike).unwrap();
            let variance_convention = value(case, &format!("query{query}_variance_vol"));
            case_largest = case_largest.max((ours - variance_convention).abs());
        }

        if flat_case {
            common::assert_close(
                case_largest,
                0.0,
                1e-12,
                "auf einer flachen Fläche darf die Konvention keinen Unterschied machen",
            );
        } else {
            assert!(
                case_largest > 1e-4,
                "Fläche {case}: der Konventionsunterschied wäre mit {case_largest} nicht \
                 unterscheidbar von Rechenrauschen — dann wäre der Kontrast wertlos"
            );
        }
        largest_difference = largest_difference.max(case_largest);
    }

    assert!(
        largest_difference > 1e-3,
        "der größte Konventionsunterschied liegt bei {largest_difference}; die Fixture muss \
         Flächen enthalten, auf denen die Wahl tatsächlich sichtbar wird"
    );
}

/// Außerhalb des Gitters gilt die eigene Zusage, nicht die der Referenz.
///
/// Geprüft wird zweierlei: dass die Fläche dort wirklich den Randwert flach hält, und dass sich
/// das von der fortgesetzten Steigung der Referenz überhaupt unterscheidet. Auf der flachen
/// Fläche tut es das nicht — die Fortsetzung einer Konstanten ist dieselbe Konstante —, deshalb
/// zählt der Test die Abweichungen über alle Flächen, statt sie je Abfrage zu verlangen.
#[test]
fn test_outside_the_grid_the_edge_value_is_held_flat() {
    let mut checked = 0;
    let mut diverged = 0;

    for case in 1..=case_count() {
        let surface = build(case);
        let maturity_count = value(case, "maturity_count") as usize;
        let strike_count = value(case, "strike_count") as usize;
        let first_time = value(case, "maturity0_t");
        let last_time = value(case, &format!("maturity{}_t", maturity_count - 1));
        let first_strike = value(case, "strike0");
        let last_strike = value(case, &format!("strike{}", strike_count - 1));

        for query in 0..(value(case, "query_count") as usize) {
            if inside(case, query) {
                continue;
            }
            let time = value(case, &format!("query{query}_t"));
            let strike = value(case, &format!("query{query}_k"));
            let actual = surface.volatility_at(time, strike).unwrap();

            let clamped_time = time.clamp(first_time, last_time);
            let clamped_strike = strike.clamp(first_strike, last_strike);
            common::assert_close(
                actual,
                surface.volatility_at(clamped_time, clamped_strike).unwrap(),
                TOLERANCE,
                &format!("Fläche {case}, flache Fortsetzung bei t={time}, K={strike}"),
            );
            if (actual - value(case, &format!("query{query}_vol"))).abs() > 1e-6 {
                diverged += 1;
            }
            checked += 1;
        }
    }

    assert!(
        checked >= 4,
        "es müssen Abfragen außerhalb des Gitters geprüft werden; geprüft: {checked}"
    );
    assert!(
        diverged >= 2,
        "die Abweichung zur fortgesetzten Steigung muss sichtbar werden, sonst prüft der Test \
         nur flache Ränder; abweichende Abfragen: {diverged}"
    );
}
