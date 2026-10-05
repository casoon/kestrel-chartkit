//! Unbekanntes Volumen (`NaN`) darf OBV, CMF und RVOL nicht vergiften.
//!
//! Eine Volumenreferenz hat Lücken: außerhalb der Börsenzeit einer fremden Reihe, an
//! Feiertagen. Dort steht `NaN` — „unbekannt", nicht „null". OBV ist eine laufende Summe: ein
//! einziges `NaN` ließe sie für immer `NaN` bleiben. Die Fenster von CMF und RVOL erholten sich
//! erst nach `period` Bars.

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

fn bars(n: usize, unknown: &[usize]) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            let close = 100.0 + (i as f64 * 0.7).sin() * 3.0 + i as f64 * 0.1;
            let volume = if unknown.contains(&i) {
                f64::NAN
            } else {
                1_000.0 + (i % 7) as f64 * 100.0
            };
            Bar::new(
                i as i64 * 3600,
                close - 0.2,
                close + 1.0,
                close - 1.0,
                close,
                volume,
            )
        })
        .collect()
}

/// Werte je Bar (`None` = kein Wert) und die Alert-Arten insgesamt.
fn run(name: &str, params: &[(&str, f64)], series: &[Bar]) -> (Vec<Option<f64>>, Vec<String>) {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut ind = build_checked(name, &params).unwrap();
    let mut values = Vec::new();
    let mut kinds = Vec::new();
    for b in series {
        values.push(ind.on_bar(b).map(|o| o.value));
        kinds.extend(ind.alerts().into_iter().map(|a| a.kind));
    }
    (values, kinds)
}

#[test]
fn an_unknown_bar_produces_no_value_and_never_a_nan() {
    let gaps = [10, 11, 25];
    let series = bars(60, &gaps);
    for (name, params) in [
        ("obv", vec![]),
        ("cmf", vec![("period", 5.0)]),
        ("rvol", vec![("period", 5.0)]),
    ] {
        let (values, _) = run(name, &params, &series);
        for (i, v) in values.iter().enumerate() {
            if gaps.contains(&i) {
                assert!(
                    v.is_none(),
                    "{name}: Bar {i} mit unbekanntem Volumen ergibt {v:?}"
                );
            }
            if let Some(v) = v {
                assert!(v.is_finite(), "{name}: Bar {i} ergibt {v}");
            }
        }
        assert!(
            values.last().unwrap().is_some(),
            "{name}: erholt sich nach den Lücken nicht"
        );
    }
}

/// Die Summe läuft ohne die unbekannten Bars weiter: ihr Schluss zählt für die Richtung der
/// nächsten, ihr Volumen trägt nichts bei.
#[test]
fn obv_skips_the_unknown_volume_but_keeps_the_close() {
    // Schlüsse 10, 11, 12, 11; Volumen 5, unbekannt, 7, 3.
    let mk = |i: i64, close: f64, volume: f64| {
        Bar::new(i, close, close + 0.5, close - 0.5, close, volume)
    };
    let series = [
        mk(0, 10.0, 5.0),
        mk(1, 11.0, f64::NAN),
        mk(2, 12.0, 7.0),
        mk(3, 11.0, 3.0),
    ];
    let (values, _) = run("obv", &[], &series);
    assert_eq!(values[0], Some(0.0), "die erste Bar trägt nichts bei");
    assert_eq!(values[1], None);
    // Bar 2: 12 > 11 (Schluss der unbekannten Bar) → +7.
    assert_eq!(values[2], Some(7.0));
    // Bar 3: 11 < 12 → −3.
    assert_eq!(values[3], Some(4.0));
}

/// Schlüsse am Hoch der Bar (Kaufdruck) und am Tief (Verkaufsdruck) lösen die Alerts mit
/// Richtungspräfix aus — nicht mehr `cmf_bullish`/`cmf_bearish`, aus denen kein Verbraucher die
/// Richtung lesen konnte.
#[test]
fn cmf_alerts_carry_the_direction_prefix() {
    let mk = |i: i64, close_at_high: bool| {
        let (lo, hi) = (99.0, 101.0);
        let close = if close_at_high { hi } else { lo };
        Bar::new(i, 100.0, hi, lo, close, 1_000.0)
    };
    let up: Vec<Bar> = (0..10).map(|i| mk(i, true)).collect();
    let down: Vec<Bar> = (0..10).map(|i| mk(i, false)).collect();
    let (_, kinds_up) = run("cmf", &[("period", 5.0)], &up);
    let (_, kinds_down) = run("cmf", &[("period", 5.0)], &down);
    assert!(
        !kinds_up.is_empty() && kinds_up.iter().all(|k| k == "bull_bias"),
        "{kinds_up:?}"
    );
    assert!(
        !kinds_down.is_empty() && kinds_down.iter().all(|k| k == "bear_bias"),
        "{kinds_down:?}"
    );
}
