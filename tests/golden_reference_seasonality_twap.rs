//! Abnahme der Pakete 39 (Seasonality) und 40 (verankerter TWAP).
//!
//! Beide sind Fälle mit synthetischen Reihen bekannter Zusammensetzung: Die Monatsrenditen werden
//! aus konstruierten Monatsschlusskursen von Hand nachgerechnet, die TWAP-Gewichtungen an
//! ungleichen Zeitabständen, an denen sich die beiden Varianten unterscheiden müssen.

use kestrel_chartkit::analytics::monthly_seasonality;
use kestrel_chartkit::indicator::twap::{AnchoredTwap, TwapAnchor, TwapWeighting};
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::{Bar, Source};

mod common;

/// Unix-Zeitstempel für einen Tag in UTC.
fn day(year: i32, month: u32, day_of_month: u32) -> i64 {
    let epoch = kestrel_chartkit::finance::Date::new(1970, 1, 1).unwrap();
    let date = kestrel_chartkit::finance::Date::new(year, month, day_of_month).unwrap();
    epoch.days_until(&date) * 86_400
}

fn bar_at(timestamp: i64, close: f64) -> Bar {
    Bar::new(timestamp, close, close + 0.5, close - 0.5, close, 1000.0)
}

// --- Paket 39: Seasonality ------------------------------------------------------------------

/// Drei Monate mit bekannten Schlusskursen: 100 → 110 → 99. Die Renditen sind damit exakt
/// +10 % und −10 %, der Januar hat keine, weil vor ihm nichts liegt.
#[test]
fn test_monthly_returns_run_from_previous_month_close_to_month_close() {
    let bars = vec![
        bar_at(day(2024, 1, 15), 95.0),
        bar_at(day(2024, 1, 31), 100.0),
        bar_at(day(2024, 2, 15), 105.0),
        bar_at(day(2024, 2, 29), 110.0),
        bar_at(day(2024, 3, 15), 99.0),
    ];
    let report = monthly_seasonality(&bars);

    assert_eq!(
        report.monthly_returns.len(),
        2,
        "der erste Monat hat keine Rendite"
    );
    common::assert_close(report.monthly_returns[0].return_pct, 10.0, 1e-12, "Februar");
    common::assert_close(report.monthly_returns[1].return_pct, -10.0, 1e-12, "März");
    assert_eq!(report.as_of, day(2024, 3, 15));
}

/// Der letzte Monat einer Reihe ist unvollständig — ob seine letzte Kerze die letzte des Monats
/// war, sagt die Reihe nicht. Er erscheint in den Renditen, aber nicht in der Statistik.
#[test]
fn test_final_month_is_marked_incomplete_and_left_out_of_the_statistics() {
    let bars = vec![
        bar_at(day(2024, 1, 31), 100.0),
        bar_at(day(2024, 2, 29), 110.0),
        bar_at(day(2024, 3, 10), 121.0),
    ];
    let report = monthly_seasonality(&bars);

    assert!(
        report.monthly_returns[0].complete,
        "Februar ist abgeschlossen"
    );
    assert!(
        !report.monthly_returns[1].complete,
        "der März ist der letzte Monat der Reihe"
    );
    assert_eq!(report.completed_months, 1);
    assert_eq!(
        report.months.len(),
        1,
        "nur der Februar zählt in die Statistik"
    );
    assert_eq!(report.months[0].month, 2);
}

/// Über mehrere Jahre: Mittel, Streuung und Anteil positiver Monate von Hand nachgerechnet.
/// Der Februar liefert +10 %, −5 % und +15 % — Mittel 6.666…, Anteil positiver Monate 2/3.
#[test]
fn test_month_statistics_are_hand_checkable() {
    let mut bars = Vec::new();
    let januaries = [100.0, 200.0, 400.0];
    let februaries = [110.0, 190.0, 460.0];
    for (index, year) in [2022, 2023, 2024].iter().enumerate() {
        bars.push(bar_at(day(*year, 1, 31), januaries[index]));
        bars.push(bar_at(day(*year, 2, 28), februaries[index]));
        // Ein Märzwert, damit der Februar jeweils abgeschlossen ist.
        bars.push(bar_at(day(*year, 3, 31), februaries[index]));
    }
    let report = monthly_seasonality(&bars);

    let february = report
        .months
        .iter()
        .find(|entry| entry.month == 2)
        .expect("Februar fehlt");
    assert_eq!(february.samples, 3);
    common::assert_close(
        february.mean_return_pct,
        (10.0 - 5.0 + 15.0) / 3.0,
        1e-12,
        "Mittel",
    );
    common::assert_close(
        february.positive_share,
        2.0 / 3.0,
        1e-12,
        "Anteil positiver Monate",
    );
    assert!(february.stdev_return_pct.is_some());
}

/// Mit nur einer Beobachtung gibt es keine Streuung — `None` statt einer Null, die Sicherheit
/// suggerieren würde.
#[test]
fn test_a_single_observation_has_no_spread() {
    let bars = vec![
        bar_at(day(2024, 1, 31), 100.0),
        bar_at(day(2024, 2, 29), 110.0),
        bar_at(day(2024, 3, 31), 120.0),
    ];
    let report = monthly_seasonality(&bars);
    let february = report.months.iter().find(|m| m.month == 2).unwrap();
    assert_eq!(february.samples, 1);
    assert_eq!(february.stdev_return_pct, None);
}

#[test]
fn test_empty_and_single_bar_series_produce_no_returns() {
    assert!(monthly_seasonality(&[]).monthly_returns.is_empty());
    assert!(monthly_seasonality(&[bar_at(day(2024, 1, 31), 100.0)])
        .monthly_returns
        .is_empty());
}

// --- Paket 40: verankerter TWAP -------------------------------------------------------------

/// Bei gleichen Abständen fallen beide Gewichtungen zusammen — der Unterschied entsteht erst
/// durch ungleiche Abstände.
#[test]
fn test_both_weightings_agree_on_evenly_spaced_bars() {
    let bars: Vec<Bar> = (0..5)
        .map(|i| bar_at(i as i64 * 60, 100.0 + i as f64))
        .collect();

    let run = |weighting| {
        let mut twap = AnchoredTwap::new(TwapAnchor::Continuous, Source::Close, weighting);
        bars.iter()
            .filter_map(|bar| twap.on_bar(bar))
            .map(|out| out.value)
            .last()
            .unwrap()
    };

    // Zeitgewichtet zählt der letzte Preis noch nicht, weil er noch nicht gestanden hat.
    common::assert_close(run(TwapWeighting::PerBar), 102.0, 1e-12, "je Bar");
    common::assert_close(
        run(TwapWeighting::ByDuration),
        (100.0 + 101.0 + 102.0 + 103.0) / 4.0,
        1e-12,
        "nach Dauer",
    );
}

/// Ungleiche Abstände: Ein Preis, der eine Stunde stand, wiegt zeitgewichtet schwerer als einer,
/// der eine Minute stand — je Bar dagegen gleich viel. Handrechnung:
/// 100 steht 3600 s, 200 steht 60 s → (100*3600 + 200*60) / 3660.
#[test]
fn test_duration_weighting_counts_how_long_a_price_stood() {
    let bars = [bar_at(0, 100.0), bar_at(3_600, 200.0), bar_at(3_660, 300.0)];

    let mut per_bar =
        AnchoredTwap::new(TwapAnchor::Continuous, Source::Close, TwapWeighting::PerBar);
    let mut by_duration = AnchoredTwap::new(
        TwapAnchor::Continuous,
        Source::Close,
        TwapWeighting::ByDuration,
    );

    let per_bar_value = bars
        .iter()
        .filter_map(|bar| per_bar.on_bar(bar))
        .map(|o| o.value)
        .last()
        .unwrap();
    let duration_value = bars
        .iter()
        .filter_map(|bar| by_duration.on_bar(bar))
        .map(|o| o.value)
        .last()
        .unwrap();

    common::assert_close(per_bar_value, 200.0, 1e-12, "je Bar");
    common::assert_close(
        duration_value,
        (100.0 * 3600.0 + 200.0 * 60.0) / 3660.0,
        1e-9,
        "nach Dauer",
    );
    assert!(
        (per_bar_value - duration_value).abs() > 50.0,
        "bei ungleichen Abständen müssen sich die Gewichtungen deutlich unterscheiden"
    );
}

/// Mit einer einzigen Beobachtung gibt es nichts zu gewichten: Der Wert ist dieser Preis.
#[test]
fn test_a_single_observation_is_its_own_average() {
    let mut twap = AnchoredTwap::new(
        TwapAnchor::Continuous,
        Source::Close,
        TwapWeighting::ByDuration,
    );
    let value = twap.on_bar(&bar_at(0, 123.0)).unwrap().value;
    common::assert_close(value, 123.0, 0.0, "eine Beobachtung");
}

/// Der Tages-Anchor beginnt die Mittelung neu.
#[test]
fn test_daily_anchor_restarts_the_average() {
    let mut twap = AnchoredTwap::new(
        TwapAnchor::Daily {
            start_offset_seconds: 0,
        },
        Source::Close,
        TwapWeighting::PerBar,
    );
    twap.on_bar(&bar_at(0, 100.0));
    twap.on_bar(&bar_at(3_600, 200.0));
    let next_day = twap.on_bar(&bar_at(86_400, 50.0)).unwrap().value;
    common::assert_close(next_day, 50.0, 0.0, "neuer Tag, neue Mittelung");
}

/// Vor einem manuellen Anchor gibt es keine Ausgabe.
#[test]
fn test_manual_anchor_produces_nothing_before_its_timestamp() {
    let mut twap = AnchoredTwap::new(
        TwapAnchor::ManualTimestamp(600),
        Source::Close,
        TwapWeighting::PerBar,
    );
    assert!(twap.on_bar(&bar_at(0, 100.0)).is_none());
    assert!(twap.on_bar(&bar_at(540, 100.0)).is_none());
    common::assert_close(
        twap.on_bar(&bar_at(600, 42.0)).unwrap().value,
        42.0,
        0.0,
        "ab dem Anchor",
    );
}

/// Die Preisquelle ist wählbar und wirkt.
#[test]
fn test_source_selection_changes_what_is_averaged() {
    let bars = [
        Bar::new(0, 100.0, 110.0, 90.0, 105.0, 1000.0),
        Bar::new(60, 105.0, 115.0, 95.0, 110.0, 1000.0),
    ];
    let run = |source| {
        let mut twap = AnchoredTwap::new(TwapAnchor::Continuous, source, TwapWeighting::PerBar);
        bars.iter()
            .filter_map(|bar| twap.on_bar(bar))
            .map(|o| o.value)
            .last()
            .unwrap()
    };
    common::assert_close(run(Source::Close), 107.5, 1e-12, "Schlusskurs");
    common::assert_close(run(Source::High), 112.5, 1e-12, "Hoch");
    assert_ne!(run(Source::Close), run(Source::Hlc3));
}
