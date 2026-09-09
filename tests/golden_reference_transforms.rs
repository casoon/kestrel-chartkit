mod common;

use kestrel_chartkit::model::{Bar, Provenance, SeriesIdentity};
use kestrel_chartkit::transform::{HeikinAshi, HeikinAshiBar};

const GOLDEN: &str = include_str!("fixtures/golden_transforms.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// Fünf Kerzen mit Aufwärtsgap auf Kerze 3: Ohne Gap bliebe offen, ob das transformierte High
/// dem Originalhoch oder dem synthetischen Open folgt.
const HA_BARS: [(f64, f64, f64, f64); 5] = [
    (100.0, 103.0, 99.0, 102.0),
    (102.0, 105.0, 101.0, 104.0),
    (110.0, 112.0, 108.0, 109.0),
    (109.0, 110.0, 104.0, 105.0),
    (105.0, 106.0, 103.0, 103.5),
];

fn ha_bars() -> Vec<Bar> {
    HA_BARS
        .iter()
        .enumerate()
        .map(|(i, &(o, h, l, c))| Bar::new(i as i64 * 60, o, h, l, c, 1000.0 + i as f64))
        .collect()
}

fn transform(bars: &[Bar]) -> Vec<HeikinAshiBar> {
    let mut ha = HeikinAshi::new();
    bars.iter().map(|bar| ha.update(bar)).collect()
}

#[test]
fn test_golden_heikin_ashi_reference_values() {
    let candles = transform(&ha_bars());
    let tolerance = expected("ha_tolerance");

    assert_eq!(
        candles.len(),
        HA_BARS.len(),
        "eine transformierte Kerze je Eingang, auch für die erste"
    );

    for (i, candle) in candles.iter().enumerate() {
        let n = i + 1;
        for (field, value) in [
            ("open", candle.open),
            ("high", candle.high),
            ("low", candle.low),
            ("close", candle.close),
        ] {
            common::assert_close(
                value,
                expected(&format!("ha_bar{n}_{field}")),
                tolerance,
                &format!("Heikin-Ashi Kerze {n} {field}"),
            );
        }
    }
}

/// Zeitstempel, Volumen und Originalkerze bleiben am Ergebnis: Das Volumen gehört der
/// beobachteten Kerze, nicht der gerechneten.
#[test]
fn test_heikin_ashi_keeps_timestamp_volume_and_source() {
    let bars = ha_bars();
    for (candle, bar) in transform(&bars).iter().zip(&bars) {
        assert_eq!(candle.timestamp, bar.timestamp);
        assert_eq!(candle.volume, bar.volume);
        assert_eq!(&candle.source, bar);
    }
}

/// Die transformierte Spanne enthält die synthetischen Kurse und das Originalhoch/-tief.
#[test]
fn test_heikin_ashi_range_contains_open_close_and_source_extremes() {
    let bars = ha_bars();
    for candle in transform(&bars) {
        assert!(candle.high >= candle.open && candle.high >= candle.close);
        assert!(candle.low <= candle.open && candle.low <= candle.close);
        assert!(candle.high >= candle.source.high);
        assert!(candle.low <= candle.source.low);
    }
}

/// Flache Reihe: Alle vier Werte fallen auf denselben Preis zusammen.
#[test]
fn test_heikin_ashi_flat_series_collapses_to_the_price() {
    let bars: Vec<Bar> = (0..5)
        .map(|i| Bar::new(i * 60, 42.0, 42.0, 42.0, 42.0, 1000.0))
        .collect();
    for candle in transform(&bars) {
        assert_eq!(
            (candle.open, candle.high, candle.low, candle.close),
            (42.0, 42.0, 42.0, 42.0)
        );
    }
}

/// Kausalität: Eine bestätigte Kerze ändert sich durch spätere Bars nicht.
#[test]
fn test_heikin_ashi_is_causal_over_prefixes() {
    let bars = ha_bars();
    let prefix = transform(&bars[..3]);
    let full = transform(&bars);
    assert_eq!(prefix, full[..prefix.len()]);
}

/// Nach `reset` beginnt die Rekursion neu — ein Serienwechsel führt sie nicht über die Grenze
/// fort.
#[test]
fn test_heikin_ashi_reset_restarts_the_recursion() {
    let bars = ha_bars();
    let mut ha = HeikinAshi::new();
    let first: Vec<HeikinAshiBar> = bars.iter().map(|b| ha.update(b)).collect();
    ha.reset();
    let second: Vec<HeikinAshiBar> = bars.iter().map(|b| ha.update(b)).collect();
    assert_eq!(first, second);
}

/// Wer die transformierte Reihe analysieren will, verliert die Kennzeichnung im Bar-Typ — die
/// Serienidentität trägt sie dann weiter.
#[test]
fn test_heikin_ashi_series_identity_marks_prices_as_synthetic() {
    let base = SeriesIdentity::new("TEST", "1h");
    assert_eq!(base.provenance, Provenance::Exchange);

    let synthetic = HeikinAshiBar::synthetic_identity(&base);
    assert_eq!(synthetic.provenance, Provenance::Synthetic);
    assert_eq!(synthetic.symbol, base.symbol);
    assert_eq!(synthetic.timeframe, base.timeframe);

    let bars = ha_bars();
    let candle = transform(&bars)[2].clone();
    let as_bar = candle.to_bar();
    assert_eq!(as_bar.timestamp, candle.timestamp);
    assert_eq!(as_bar.close, candle.close);
    assert_ne!(
        as_bar.close, candle.source.close,
        "die gerechnete Kerze ist nicht die beobachtete"
    );
}
