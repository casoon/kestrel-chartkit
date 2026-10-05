//! Szenario-Referenz für `PivotDivergence`: handgebaute Folgen, deren Ergebnis ohne Rechnung
//! feststeht.
//!
//! Aufbau (Schlusskurse, Hoch/Tief = Schluss ± 0,5, Eröffnung = Schluss — so sind Tiefs und Hochs
//! streng monoton wie die Schlüsse, und jedes Pivot ist eindeutig):
//!
//! - Bars 0..=30: gleichmäßiger Anstieg 100 → 106 — keine Pivot-Tiefs.
//! - Bars 31..=40: zehn Abschläge zu je 2 auf 86 (Tief A, Bar 40). Der RSI steht nach zehn
//!   Verlusten in Folge tief unten.
//! - Bars 41..=46: Erholung um je 1,5 auf 95.
//! - Bars 47..=58: zwölf Abschläge zu je 0,8 auf 85,4 (Tief B, Bar 58, unter A). Die Verluste
//!   sind kleiner als die Gewinne der Erholung davor, der RSI bleibt also über seinem Wert an A.
//! - Bars 59..=66: Anstieg um je 1,5.
//!
//! Kurs: tieferes Tief. RSI: höheres Tief. → reguläre bullische Divergenz, bestätigt genau
//! `right` Bars nach B (Bar 61 bei `right = 3`), mit B's Tief als Pivotkurs. Vorher meldet der
//! Indikator nichts Bullisches.

use kestrel_chartkit::indicator::pivot_divergence::{DivergenceOscillator, PivotDivergence};
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;

fn closes() -> Vec<f64> {
    let mut c: Vec<f64> = (0..=30).map(|i| 100.0 + 0.2 * i as f64).collect();
    for _ in 0..10 {
        c.push(c.last().unwrap() - 2.0);
    }
    for _ in 0..6 {
        c.push(c.last().unwrap() + 1.5);
    }
    for _ in 0..12 {
        c.push(c.last().unwrap() - 0.8);
    }
    for _ in 0..8 {
        c.push(c.last().unwrap() + 1.5);
    }
    c
}

fn bars() -> Vec<Bar> {
    closes()
        .iter()
        .enumerate()
        .map(|(i, &close)| Bar::new(i as i64 * 3600, close, close + 0.5, close - 0.5, close, 0.0))
        .collect()
}

/// (Bar, Alert-Art, Pivotkurs) für jede Meldung.
fn run(osc: DivergenceOscillator) -> Vec<(usize, String, f64)> {
    let mut ind = PivotDivergence::new(osc, 3, 3, 5, 60, 5);
    let mut events = Vec::new();
    for (i, bar) in bars().iter().enumerate() {
        let out = ind.on_bar(bar);
        for alert in ind.alerts() {
            let pivot = out.as_ref().unwrap().extra["pivot_price"];
            events.push((i, alert.kind, pivot));
        }
    }
    events
}

#[test]
fn lower_low_with_higher_rsi_low_is_a_regular_bullish_divergence_on_confirmation() {
    let c = closes();
    assert_eq!(c.len(), 67);
    // Tief B liegt unter Tief A.
    assert!((c[58] - 85.4).abs() < 1e-9 && (c[40] - 86.0).abs() < 1e-9);

    let events = run(DivergenceOscillator::Rsi);
    let bullish: Vec<_> = events
        .iter()
        .filter(|(_, kind, _)| kind == "bull_divergence")
        .collect();
    assert_eq!(bullish.len(), 1, "{events:?}");
    let (bar, _, pivot) = bullish[0];
    assert_eq!(*bar, 61, "gemeldet auf der Bestätigungsbar, nicht am Pivot");
    // Tief der Bar 58: Schluss 85,4 − 0,5.
    assert!((pivot - 84.9).abs() < 1e-9, "{pivot}");
}

#[test]
fn nothing_bullish_before_the_confirmation_bar() {
    for osc in [
        DivergenceOscillator::Rsi,
        DivergenceOscillator::WaveTrend,
        DivergenceOscillator::StochRsi,
        DivergenceOscillator::WilliamsR,
        DivergenceOscillator::Cci,
    ] {
        let events = run(osc);
        assert!(
            events
                .iter()
                .filter(|(_, kind, _)| kind.starts_with("bull"))
                .all(|(bar, _, _)| *bar == 61),
            "{osc:?}: {events:?}"
        );
    }
}

#[test]
fn the_series_never_looks_ahead() {
    // Dieselben Meldungen, ob die Reihe nach Bar k endet oder weiterläuft.
    let all = run(DivergenceOscillator::Rsi);
    assert!(!all.is_empty(), "ohne Meldung prüft der Test nichts");
    let bars = bars();
    for cut in 40..bars.len() {
        let mut ind = PivotDivergence::new(DivergenceOscillator::Rsi, 3, 3, 5, 60, 5);
        let mut events = Vec::new();
        for (i, bar) in bars[..cut].iter().enumerate() {
            ind.on_bar(bar);
            for alert in ind.alerts() {
                events.push((i, alert.kind));
            }
        }
        let prefix: Vec<_> = all
            .iter()
            .filter(|(i, _, _)| *i < cut)
            .map(|(i, k, _)| (*i, k.clone()))
            .collect();
        assert_eq!(events, prefix, "cut {cut}");
    }
}
