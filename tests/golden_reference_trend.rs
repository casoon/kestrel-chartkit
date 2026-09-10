mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_trend.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn run_trend_bars(
    name: &str,
    params: &HashMap<String, f64>,
    count: usize,
) -> Option<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut ind = build_checked(name, params).unwrap();
    let mut last = None;
    for i in 0..count {
        let p = 44.0 + i as f64 * 0.1;
        let bar = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&bar) {
            last = Some(out);
        }
    }
    last
}

#[test]
fn test_golden_alligator_reference_values() {
    let out =
        run_trend_bars("alligator", &HashMap::new(), 25).expect("Alligator produced no output");
    let tol = expected("trend_tolerance");
    common::assert_close(
        out.extra["jaw"],
        expected("alligator_jaw"),
        tol,
        "Alligator Jaw",
    );
    common::assert_close(
        out.extra["teeth"],
        expected("alligator_teeth"),
        tol,
        "Alligator Teeth",
    );
    common::assert_close(
        out.extra["lips"],
        expected("alligator_lips"),
        tol,
        "Alligator Lips",
    );
}

#[test]
fn test_golden_efficiency_reference_values() {
    let out = run_trend_bars("efficiency", &HashMap::from([("len".to_string(), 5.0)]), 25)
        .expect("Efficiency produced no output");
    common::assert_close(
        out.value,
        expected("efficiency5_last"),
        expected("trend_tolerance"),
        "Leg Efficiency",
    );
}

#[test]
fn test_golden_ichimoku_reference_values() {
    let out = run_trend_bars(
        "ichimoku",
        &HashMap::from([
            ("tenkan_p".to_string(), 3.0),
            ("kijun_p".to_string(), 5.0),
            ("senkou_b_p".to_string(), 10.0),
        ]),
        25,
    )
    .expect("Ichimoku produced no output");
    let tol = expected("trend_tolerance");
    common::assert_close(
        out.extra["tenkan"],
        expected("ichimoku_tenkan"),
        tol,
        "Ichimoku Tenkan",
    );
    common::assert_close(
        out.extra["kijun"],
        expected("ichimoku_kijun"),
        tol,
        "Ichimoku Kijun",
    );
    common::assert_close(
        out.extra["senkou_a"],
        expected("ichimoku_senkou_a"),
        tol,
        "Ichimoku Senkou A",
    );
    common::assert_close(
        out.extra["senkou_b"],
        expected("ichimoku_senkou_b"),
        tol,
        "Ichimoku Senkou B",
    );
}

#[test]
fn test_golden_midas_reference_values() {
    let out = run_trend_bars(
        "midas",
        &HashMap::from([("maturity_bars".to_string(), 5.0)]),
        25,
    )
    .expect("MIDAS produced no output");
    let tol = expected("trend_tolerance");
    common::assert_close(out.value, expected("midas_curve"), tol, "MIDAS Curve");
    if let Some(proj) = out.secondary {
        common::assert_close(proj, expected("midas_proj"), tol, "MIDAS Projection");
    }
}

#[test]
fn test_golden_trend_relationship_reference_values() {
    let out = run_trend_bars(
        "trend_relationship",
        &HashMap::from([("fast_len".to_string(), 3.0), ("slow_len".to_string(), 5.0)]),
        25,
    )
    .expect("Trend Relationship produced no output");
    let tol = expected("trend_tolerance");
    common::assert_close(
        out.extra["fast"],
        expected("trend_rel_fast"),
        tol,
        "Trend Rel Fast",
    );
    common::assert_close(
        out.extra["slow"],
        expected("trend_rel_slow"),
        tol,
        "Trend Rel Slow",
    );
    common::assert_close(out.value, expected("trend_rel_diff"), tol, "Trend Rel Diff");
}

#[test]
fn test_golden_zscore_reference_values() {
    let out = run_trend_bars("zscore", &HashMap::from([("period".to_string(), 5.0)]), 25)
        .expect("Z-Score produced no output");
    common::assert_close(
        out.value,
        expected("zscore5_last"),
        expected("trend_tolerance"),
        "Z-Score(5)",
    );
}

/// Auf einer Geraden ist die Effizienz für jedes Fenster 1, und die MIDAS-Projektion greift nie,
/// weil jede Kerze ein neues Hoch setzt. Diese beiden Reihen prüfen, was die Gerade nicht prüfen
/// kann.
const ZIGZAG: [f64; 8] = [10.0, 11.0, 10.5, 11.5, 11.0, 12.0, 11.2, 12.4];
const PULLBACK: [(f64, f64); 10] = [
    (100.0, 1000.0),
    (101.0, 1200.0),
    (102.0, 900.0),
    (103.0, 1500.0),
    (104.0, 1100.0),
    (105.0, 1300.0),
    (104.0, 800.0),
    (103.5, 1000.0),
    (103.0, 1400.0),
    (102.5, 900.0),
];

#[test]
fn test_golden_efficiency_and_midas_projection_off_the_straight_line() {
    let tol = expected("trend_tolerance");

    let mut er = build_checked("efficiency", &HashMap::from([("len".to_string(), 5.0)])).unwrap();
    let mut last = None;
    for (i, &c) in ZIGZAG.iter().enumerate() {
        last = er
            .on_bar(&Bar::new(i as i64 * 60, c, c + 0.3, c - 0.3, c, 1000.0))
            .or(last);
    }
    let efficiency = last.expect("Efficiency gab nichts aus").value;
    common::assert_close(
        efficiency,
        expected("efficiency5_shaped_last"),
        tol,
        "Efficiency(5), Zickzack",
    );
    assert!(
        efficiency > 0.0 && efficiency < 1.0,
        "die Reihe muss die Kennzahl bewegen: {efficiency}"
    );

    let mut midas = build_checked(
        "midas",
        &HashMap::from([("maturity_bars".to_string(), 5.0)]),
    )
    .unwrap();
    let mut last = None;
    for (i, &(c, v)) in PULLBACK.iter().enumerate() {
        last = midas
            .on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, v))
            .or(last);
    }
    let out = last.expect("MIDAS gab nichts aus");
    common::assert_close(
        out.value,
        expected("midas5_shaped_curve"),
        tol,
        "MIDAS-Kurve",
    );
    let projection = *out
        .extra
        .get("projection")
        .expect("nach dem Rücksetzer muss die Projektion greifen");
    common::assert_close(
        projection,
        expected("midas5_shaped_projection"),
        tol,
        "MIDAS-Projektion",
    );
    assert!(
        (projection - out.value).abs() > 0.1,
        "die Projektion muss sich von der Kurve unterscheiden"
    );
}
