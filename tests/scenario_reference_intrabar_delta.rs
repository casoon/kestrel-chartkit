//! Szenario-Abnahme des Intrabar-Delta-Vertrags (Paket 41).
//!
//! Hier gibt es keine Einzelzahl aus einer Formel, sondern eine Buchungsregel über eine Sequenz.
//! Geprüft wird deshalb an kleinen, von Hand nachvollziehbaren Intrabar-Folgen: gleiche Closes,
//! Dojis, Richtungswechsel, fehlende Kinder und der Anchor-Reset.

use kestrel_chartkit::intrabar::IntrabarGroup;
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::{DeltaAnchor, DeltaProvenance, IntrabarCvd, UnchangedIntrabarPolicy};

/// (open, close, volume) je Intrabar; Hoch und Tief spielen für die Buchung keine Rolle.
fn group(timestamp: i64, children: &[(f64, f64, f64)]) -> IntrabarGroup {
    IntrabarGroup {
        parent_timestamp: timestamp,
        children: children
            .iter()
            .enumerate()
            .map(|(i, &(open, close, volume))| {
                Bar::new(
                    timestamp + i as i64 * 60,
                    open,
                    open.max(close) + 0.5,
                    open.min(close) - 0.5,
                    close,
                    volume,
                )
            })
            .collect(),
    }
}

/// Handrechnung: Erste Kerze gegen ihre eigene Eröffnung, danach jede gegen den Vorgängerschluss.
/// 100 hoch, 200 runter, 300 hoch ergibt Delta 100 + (-200) + 300 = 200.
#[test]
fn test_children_are_booked_against_the_previous_child_close() {
    let mut cvd = IntrabarCvd::with_defaults();
    let delta = cvd.on_group(&group(
        0,
        &[
            (100.0, 101.0, 100.0), // gegen die eigene Eröffnung: aufwärts
            (101.0, 100.5, 200.0), // gegen 101.0: abwärts
            (100.5, 102.0, 300.0), // gegen 100.5: aufwärts
        ],
    ));

    assert_eq!(delta.buy_volume, 400.0);
    assert_eq!(delta.sell_volume, 200.0);
    assert_eq!(delta.delta, 200.0);
    assert_eq!(delta.unattributed_volume, 0.0);
    assert_eq!(delta.children, 3);
    assert_eq!(delta.provenance, DeltaProvenance::IntrabarShape);
}

/// Die vier Ecken der kumulativen Reihe zeigen den Weg, nicht nur das Ergebnis: Derselbe
/// Schlusswert kann geradlinig oder über einen Ausschlag zustande kommen.
#[test]
fn test_cumulative_corners_show_the_path_not_only_the_result() {
    let straight = IntrabarCvd::with_defaults()
        .on_group(&group(0, &[(100.0, 101.0, 100.0), (101.0, 102.0, 100.0)]));
    let swinging = IntrabarCvd::with_defaults().on_group(&group(
        0,
        &[
            (100.0, 101.0, 300.0),
            (101.0, 100.0, 400.0),
            (100.0, 101.0, 300.0),
        ],
    ));

    assert_eq!(straight.cumulative_close, 200.0);
    assert_eq!(straight.cumulative_low, 0.0);
    assert_eq!(straight.cumulative_high, 200.0);

    assert_eq!(swinging.cumulative_close, 200.0);
    assert_eq!(swinging.cumulative_high, 300.0);
    assert_eq!(
        swinging.cumulative_low, -100.0,
        "der Ausschlag nach unten bleibt sichtbar"
    );
}

/// Ein unveränderter Schluss trägt standardmäßig keine Seite — sein Volumen wird ausgewiesen,
/// nicht verteilt.
#[test]
fn test_unchanged_children_are_reported_as_unattributed_by_default() {
    let delta = IntrabarCvd::with_defaults().on_group(&group(
        0,
        &[
            (100.0, 101.0, 100.0),
            (101.0, 101.0, 500.0), // Doji gegen den Vorgängerschluss
            (101.0, 101.0, 400.0),
        ],
    ));

    assert_eq!(delta.buy_volume, 100.0);
    assert_eq!(delta.sell_volume, 0.0);
    assert_eq!(delta.unattributed_volume, 900.0);
    assert_eq!(delta.delta, 100.0);
    assert_eq!(
        delta.cumulative_close, 100.0,
        "unattribuiertes Volumen bewegt die Summe nicht"
    );
}

/// Die Fortschreibung der letzten Richtung ist eine ausdrückliche Annahme und muss anders
/// buchen als die Voreinstellung.
#[test]
fn test_carry_previous_direction_is_an_explicit_alternative() {
    let mut carrying = IntrabarCvd::new(
        DeltaAnchor::Continuous,
        UnchangedIntrabarPolicy::CarryPreviousDirection,
    );
    let delta = carrying.on_group(&group(
        0,
        &[
            (100.0, 99.0, 100.0), // abwärts
            (99.0, 99.0, 500.0),  // unverändert: übernimmt abwärts
        ],
    ));

    assert_eq!(delta.sell_volume, 600.0);
    assert_eq!(delta.unattributed_volume, 0.0);

    // Ohne vorangehende Richtung bleibt auch diese Politik ohne Zuordnung.
    let mut fresh = IntrabarCvd::new(
        DeltaAnchor::Continuous,
        UnchangedIntrabarPolicy::CarryPreviousDirection,
    );
    let first = fresh.on_group(&group(0, &[(100.0, 100.0, 300.0)]));
    assert_eq!(first.unattributed_volume, 300.0);
}

/// Fehlende Kinder werden nicht als Trades ausgegeben: Die Elternkerze wird nicht als ein
/// einzelner Intrabar gebucht, ihr Volumen bleibt unattribuiert, die Summe steht still.
#[test]
fn test_missing_children_are_never_presented_as_trades() {
    let mut cvd = IntrabarCvd::with_defaults();
    cvd.on_group(&group(0, &[(100.0, 101.0, 100.0)]));
    let before = cvd.cumulative();

    let gap = cvd.on_missing_children(&Bar::new(600, 101.0, 103.0, 100.0, 103.0, 5_000.0));

    assert_eq!(gap.children, 0);
    assert_eq!(gap.buy_volume, 0.0);
    assert_eq!(gap.sell_volume, 0.0);
    assert_eq!(gap.unattributed_volume, 5_000.0);
    assert_eq!(gap.cumulative_close, before);
    assert_eq!(cvd.cumulative(), before);
}

/// Der Tages-Anchor setzt die kumulative Reihe zurück; die fortlaufende Variante nicht.
#[test]
fn test_daily_anchor_resets_the_running_total() {
    let day = 86_400;
    let first = group(0, &[(100.0, 101.0, 100.0), (101.0, 102.0, 100.0)]);
    let next_day = group(day, &[(102.0, 103.0, 50.0)]);

    let mut daily = IntrabarCvd::new(
        DeltaAnchor::Daily {
            start_offset_seconds: 0,
        },
        UnchangedIntrabarPolicy::default(),
    );
    daily.on_group(&first);
    let after_reset = daily.on_group(&next_day);
    assert_eq!(after_reset.cumulative_open, 0.0);
    assert_eq!(after_reset.cumulative_close, 50.0);

    let mut continuous = IntrabarCvd::with_defaults();
    continuous.on_group(&first);
    let carried = continuous.on_group(&next_day);
    assert_eq!(carried.cumulative_open, 200.0);
    assert_eq!(carried.cumulative_close, 250.0);
}

/// Der Reset beginnt deterministisch neu.
#[test]
fn test_reset_restarts_the_accumulation() {
    let groups = [
        group(0, &[(100.0, 101.0, 100.0), (101.0, 100.0, 200.0)]),
        group(600, &[(100.0, 102.0, 300.0)]),
    ];
    let mut cvd = IntrabarCvd::with_defaults();
    let run = |cvd: &mut IntrabarCvd| -> Vec<f64> {
        groups
            .iter()
            .map(|g| cvd.on_group(g).cumulative_close)
            .collect()
    };
    let first = run(&mut cvd);
    cvd.reset();
    assert_eq!(first, run(&mut cvd));
}

/// Die Herkunft ist Teil des Ergebnisses: Was hier entsteht, ist eine Schätzung aus der Form der
/// Intrabars, niemals eine Messung klassifizierter Abschlüsse.
#[test]
fn test_provenance_never_claims_classified_trades() {
    let delta = IntrabarCvd::with_defaults().on_group(&group(0, &[(100.0, 101.0, 100.0)]));
    assert_eq!(delta.provenance, DeltaProvenance::IntrabarShape);
    assert_ne!(delta.provenance, DeltaProvenance::ClassifiedTrades);
    assert_eq!(
        DeltaProvenance::IntrabarShape.to_string(),
        "intrabar shape",
        "die Herkunft ist auch in einer Anzeige benennbar"
    );
}
