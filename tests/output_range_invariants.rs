//! Prüft `registry::output_range` gegen tatsächliche Läufe.
//!
//! Die Deklaration ist eine Zusage an Konsumenten: Wer eine Achse danach auslegt, verlässt sich
//! darauf, dass die Werte den Bereich einhalten. Ohne diesen Test wäre sie eine gepflegte Liste,
//! die beim ersten geänderten Indikator still falsch wird — deshalb läuft hier jeder
//! Katalogeintrag über mehrere gegensätzliche Serien, statt einer Handvoll ausgewählter.
//!
//! Der Test ist bewusst einseitig: Er kann eine Deklaration widerlegen, aber keine bestätigen.
//! `Unbounded` sagt entsprechend nichts zu und wird hier auch nicht geprüft.

mod common;

use common::*;
use kestrel_chartkit::indicator::registry::{
    build_checked, catalog, output_range, threshold_params, OutputRange,
};
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::runner::run_batch;

/// Gegensätzliche Serien: Was auf einem stetigen Aufwärtstrend im Bereich bleibt, muss es auch
/// im Seitwärtsmarkt und in der Gegenrichtung.
fn serien() -> Vec<(&'static str, Vec<Bar>)> {
    vec![
        ("sinus", generate_sine_bars(300, 100.0, 15.0, 30.0, 1000.0)),
        ("aufwärts", generate_trend_bars(300, 100.0, 0.35, 1000.0)),
        ("abwärts", generate_trend_bars(300, 200.0, -0.35, 1000.0)),
        ("flach", generate_flat_spread_bars(300, 100.0, 0.5, 1000.0)),
        (
            "zufallslauf",
            generate_random_walk_bars(42, 300, 100.0, 0.0, 1.5, 1000.0),
        ),
    ]
}

/// Alle endlichen Werte eines Laufs — Haupt- und Nebenreihen.
///
/// Die Nebenreihen werden mitgeprüft, weil ein Konsument sie auf dieselbe Achse zeichnet: Ein
/// `di_plus`, das den deklarierten Bereich verlässt, sprengt die Skala genauso wie der Hauptwert.
fn werte(name: &str, bars: &[Bar]) -> Vec<(String, f64)> {
    let defaults = catalog()
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| e.default_params)
        .unwrap_or_default();

    let Ok(mut gebaut) = build_checked(name, &defaults) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for eintrag in run_batch(gebaut.as_mut(), bars) {
        let Some(output) = eintrag.output else { continue };
        if output.value.is_finite() {
            out.push(("value".to_string(), output.value));
        }
        for (key, v) in &output.extra {
            if v.is_finite() {
                out.push((key.clone(), *v));
            }
        }
    }
    out
}

#[test]
fn deklarierter_wertebereich_wird_eingehalten() {
    // Fließkomma-Toleranz: Ein RSI, der rechnerisch bei 100.0000000001 landet, ist kein Befund.
    const EPS: f64 = 1e-6;

    for eintrag in catalog() {
        let bereich = output_range(eintrag.name);
        if bereich == OutputRange::Unbounded {
            continue;
        }

        for (serie, bars) in serien() {
            for (reihe, wert) in werte(eintrag.name, &bars) {
                match bereich {
                    OutputRange::Bounded { min, max } => assert!(
                        wert >= min - EPS && wert <= max + EPS,
                        "{}/{reihe} verlässt den deklarierten Bereich [{min}, {max}] auf \
                         Serie „{serie}“: {wert}",
                        eintrag.name
                    ),
                    OutputRange::NonNegative => assert!(
                        wert >= -EPS,
                        "{}/{reihe} ist als nicht-negativ deklariert, liefert auf Serie \
                         „{serie}“ aber {wert}",
                        eintrag.name
                    ),
                    // Über die Spanne sagt `Centered` nichts — nur, dass die Mitte bedeutsam ist.
                    // Prüfbar ist damit allein, dass die Reihe die Mitte überhaupt erreicht;
                    // das steht als eigener Test unten.
                    OutputRange::Centered { .. } | OutputRange::Unbounded => {}
                }
            }
        }
    }
}

#[test]
fn zentrierte_reihen_wechseln_das_vorzeichen() {
    // Eine Reihe, die als um eine Mitte schwankend deklariert ist, muss diese Mitte auch
    // erreichen — sonst ist die Mittellinie eine Behauptung ohne Deckung.
    //
    // Gefordert wird das über *eine* der Serien, nicht über jede: Ein volumengetriebener
    // Oszillator liegt auf einer Serie mit konstantem Volumen konstruktionsbedingt bei exakt
    // null, und das ist eine Aussage über die Serie, nicht über die Deklaration.
    for eintrag in catalog() {
        let OutputRange::Centered { center } = output_range(eintrag.name) else {
            continue;
        };

        let mut beidseitig = false;
        let mut irgendwo_bewegt = false;

        for (_, bars) in serien() {
            let haupt: Vec<f64> = werte(eintrag.name, &bars)
                .into_iter()
                .filter(|(reihe, _)| reihe == "value")
                .map(|(_, v)| v)
                .collect();

            if haupt.is_empty() {
                continue;
            }
            if haupt.iter().any(|v| *v != haupt[0]) {
                irgendwo_bewegt = true;
            }
            if haupt.iter().any(|v| *v < center) && haupt.iter().any(|v| *v > center) {
                beidseitig = true;
                break;
            }
        }

        // Eine Reihe, die auf keiner Serie überhaupt schwankt, reizt der Test nicht aus — er sagt
        // dann nichts über die Deklaration, statt sie fälschlich zu verwerfen.
        assert!(
            beidseitig || !irgendwo_bewegt,
            "{} ist um {center} zentriert deklariert, bleibt aber auf jeder geprüften Serie \
             einseitig",
            eintrag.name
        );
    }
}

#[test]
fn schwellen_sind_parameter_des_indikators_und_liegen_im_bereich() {
    for eintrag in catalog() {
        let namen = threshold_params(eintrag.name);
        if namen.is_empty() {
            continue;
        }

        for parameter in namen {
            assert!(
                eintrag.default_params.contains_key(*parameter),
                "{} deklariert „{parameter}“ als Schwelle, kennt den Parameter aber nicht",
                eintrag.name
            );
        }

        // Eine Schwelle außerhalb des deklarierten Bereichs wäre nie erreichbar.
        if let OutputRange::Bounded { min, max } = output_range(eintrag.name) {
            for wert in eintrag.thresholds() {
                assert!(
                    wert >= min && wert <= max,
                    "Schwelle {wert} von {} liegt außerhalb des deklarierten Bereichs \
                     [{min}, {max}]",
                    eintrag.name
                );
            }
        }
    }
}

#[test]
fn schwellen_kommen_aufsteigend() {
    for eintrag in catalog() {
        let werte = eintrag.thresholds();
        assert!(
            werte.windows(2).all(|w| w[0] <= w[1]),
            "{} liefert Schwellen unsortiert: {werte:?}",
            eintrag.name
        );
    }
}
