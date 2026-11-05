//! `data_requirements(..).needs_real_volume` gegen das tatsächliche Verhalten jedes
//! Katalogeintrags.
//!
//! Jeder Eintrag läuft zweimal über dieselben Kurse, einmal mit einer zweiten, nicht
//! proportionalen Volumenreihe. Weicht ein Wert, ein Nebenwert oder ein Alarm ab, liest der
//! Indikator das Volumen. Das muss mit der Deklaration übereinstimmen — bis auf die Composites,
//! die Volumen nur als abschaltbaren Zusatzfaktor nutzen (siehe Doc von `data_requirements`).

mod common;

use common::*;
use kestrel_chartkit::applicability::data_requirements;
use kestrel_chartkit::indicator::registry::{build_checked, catalog};
use kestrel_chartkit::indicator::IndicatorOutput;
use kestrel_chartkit::model::Bar;

/// Volumen als optionaler Faktor, vom Indikator selbst abgeschaltet, wenn die Bars keins tragen.
const OPTIONAL_VOLUME: &[&str] = &[
    "overextension_risk",
    "compression_zones",
    "tradability_filter",
    "leg_conviction",
    "equal_level_liquidity",
    "reversal_setup_score",
    "structure_zones",
    "structural_flow",
];

fn same(a: &Option<IndicatorOutput>, b: &Option<IndicatorOutput>) -> bool {
    let eq = |x: f64, y: f64| (x - y).abs() < 1e-9 || (x.is_nan() && y.is_nan());
    match (a, b) {
        (Some(p), Some(q)) => {
            eq(p.value, q.value)
                && p.extra.len() == q.extra.len()
                && p.extra
                    .iter()
                    .all(|(k, v)| q.extra.get(k).is_some_and(|w| eq(*v, *w)))
        }
        (None, None) => true,
        _ => false,
    }
}

/// Zwei Reihen mit denselben Kursen, die sich nur im Umsatz einer Kerze unterscheiden: der
/// Klimax-Kerze, mit der die Wyckoff-Spanne einrastet. Auf Zufallsreihen rastet keine Spanne
/// ein, dort würde ein Volumenverbrauch von `wyckoff` unentdeckt bleiben.
///
/// Die 20. Kerze schließt unter ihrer Eröffnung und über dem Vorschluss. Mit Klimax-Umsatz ist
/// das ein Abverkauf (Akkumulation), ohne ihn entscheidet der Vorschluss (Distribution). Die
/// folgende Kerze durchsticht das Spannentief und schließt darüber: nur bei Akkumulation ein
/// Spring, sonst kein Ereignis.
fn climax_pair() -> (Vec<Bar>, Vec<Bar>) {
    let build = |climax_volume: f64| {
        let mut bars: Vec<Bar> = (0..19)
            .map(|i| {
                let price = 100.0 + ((i % 4) as f64 - 1.5) * 0.6;
                Bar::new(i as i64 * 60, price, price + 0.6, price - 0.6, price, 100.0)
            })
            .collect();
        bars.push(Bar::new(19 * 60, 100.8, 101.0, 99.9, 100.5, climax_volume));
        bars.push(Bar::new(20 * 60, 99.5, 100.0, 97.5, 99.5, 100.0));
        bars
    };
    (build(400.0), build(100.0))
}

#[test]
fn volume_requirement_matches_volume_use() {
    // Stündliche Bars über gut zwei Wochen, damit auch `rvat` (Vergleich je Tageszeit) rechnet.
    let bars: Vec<Bar> = generate_random_walk_bars(7, 400, 100.0, 0.0, 1.5, 1000.0)
        .into_iter()
        .enumerate()
        .map(|(i, b)| Bar::new(i as i64 * 3600, b.open, b.high, b.low, b.close, b.volume))
        .collect();
    let mut seed: u64 = 12345;
    let other_volume: Vec<Bar> = bars
        .iter()
        .map(|b| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let factor = 0.3 + ((seed >> 33) as f64 / (1u64 << 31) as f64) * 3.0;
            Bar::new(
                b.timestamp,
                b.open,
                b.high,
                b.low,
                b.close,
                b.volume * factor,
            )
        })
        .collect();

    let mut mismatches = Vec::new();
    for entry in catalog() {
        let mut a = build_checked(entry.name, &entry.default_params).unwrap();
        let mut b = build_checked(entry.name, &entry.default_params).unwrap();
        let mut uses_volume = false;
        for (x, y) in bars.iter().zip(&other_volume) {
            let (oa, ob) = (a.on_bar(x), b.on_bar(y));
            let kinds_a: Vec<String> = a.alerts().into_iter().map(|al| al.kind).collect();
            let kinds_b: Vec<String> = b.alerts().into_iter().map(|al| al.kind).collect();
            if !same(&oa, &ob) || kinds_a != kinds_b {
                uses_volume = true;
                break;
            }
        }
        // Zweiter Durchlauf mit der Klimax-Reihe, für Indikatoren, die nur auf einer
        // eingerasteten Spanne Umsatz lesen.
        if !uses_volume {
            let (with_climax, without) = climax_pair();
            let mut c = build_checked(entry.name, &entry.default_params).unwrap();
            let mut d = build_checked(entry.name, &entry.default_params).unwrap();
            for (x, y) in with_climax.iter().zip(&without) {
                let (oc, od) = (c.on_bar(x), d.on_bar(y));
                let kinds_c: Vec<String> = c.alerts().into_iter().map(|al| al.kind).collect();
                let kinds_d: Vec<String> = d.alerts().into_iter().map(|al| al.kind).collect();
                if !same(&oc, &od) || kinds_c != kinds_d {
                    uses_volume = true;
                    break;
                }
            }
        }
        let declared = data_requirements(entry.name).needs_real_volume;
        let expected = uses_volume && !OPTIONAL_VOLUME.contains(&entry.name);
        if declared != expected {
            mismatches.push(format!(
                "{}: liest Volumen = {uses_volume}, deklariert = {declared}",
                entry.name
            ));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
