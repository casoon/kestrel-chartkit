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
