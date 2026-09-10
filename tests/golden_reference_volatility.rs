mod common;

use kestrel_chartkit::indicator::adx::Adx;
use kestrel_chartkit::indicator::atr::{Atr, TrueRangeSmoothing};
use kestrel_chartkit::indicator::chande_kroll::ChandeKrollStop;
use kestrel_chartkit::indicator::chandelier_flip_radar::ChandelierFlipRadarEngine;
use kestrel_chartkit::indicator::relative_volatility::{
    RelativeVolatilityIndex, RelativeVolatilityVariant,
};
use kestrel_chartkit::indicator::ulcer::{ulcer_index, UlcerIndexCore, UlcerIndexEngine};
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;

const GOLDEN: &str = include_str!("fixtures/golden_volatility.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn test_golden_atr_reference_values() {
    let mut atr = Atr::new(3, 2);
    let bars: Vec<Bar> = (0..5)
        .map(|timestamp| Bar::new(timestamp, 100.0, 101.0, 99.0, 100.0, 100.0))
        .collect();

    let mut last_atr = 0.0;
    for b in &bars {
        if let Some(out) = atr.on_bar(b) {
            last_atr = out.value;
        }
    }

    common::assert_close(
        last_atr,
        expected("atr3_signal2_pct"),
        expected("atr_tolerance"),
        "ATR(3) normalized percentage",
    );
}

/// Rohwert und Prozentwert stammen aus derselben ATR-Rechnung: Der Test prueft beide Einheiten
/// an denselben Kerzen gegen unabhaengig hergeleitete Referenzwerte, damit ein Konsument den
/// Rohwert nicht aus `value * close / 100` zurueckrechnen muss.
#[test]
fn test_golden_atr_raw_and_percent_with_gaps() {
    // Aufwaertsgap auf Kerze 2, Abwaertsgap auf Kerze 4: ohne Gaps waere die True Range mit der
    // High-Low-Spanne identisch und der Unterschied zur naiven Spanne nicht sichtbar.
    let bars = [
        Bar::new(0, 100.0, 102.0, 99.0, 101.0, 1000.0),
        Bar::new(60, 105.0, 107.0, 104.0, 106.0, 1000.0),
        Bar::new(120, 103.0, 104.0, 100.0, 101.0, 1000.0),
        Bar::new(180, 95.0, 96.0, 93.0, 94.0, 1000.0),
        Bar::new(240, 94.0, 99.0, 93.0, 98.0, 1000.0),
    ];

    let mut atr = Atr::new(3, 2);
    let outputs: Vec<_> = bars.iter().filter_map(|bar| atr.on_bar(bar)).collect();

    assert_eq!(
        outputs.len(),
        2,
        "ATR(3)/Signal(2) darf erst mit der zweiten Prozentbeobachtung ausgeben"
    );

    let tolerance = expected("atr_tolerance");
    for (output, suffix) in outputs.iter().zip(["first", "second"]) {
        common::assert_close(
            output.extra["raw"],
            expected(&format!("atr_gap3_signal2_raw_{suffix}")),
            tolerance,
            &format!("ATR(3) raw ({suffix} output)"),
        );
        common::assert_close(
            output.value,
            expected(&format!("atr_gap3_signal2_pct_{suffix}")),
            tolerance,
            &format!("ATR(3) percent ({suffix} output)"),
        );
        common::assert_close(
            output.extra["signal"],
            expected(&format!("atr_gap3_signal2_signal_{suffix}")),
            tolerance,
            &format!("ATR(3) signal ({suffix} output)"),
        );
    }
}

/// Der Rohwert ist kein zweiter Rechenweg: Er muss zum Prozentwert derselben Kerze passen.
#[test]
fn test_atr_raw_and_percent_stay_consistent() {
    let mut atr = Atr::new(14, 20);
    let bars: Vec<Bar> = (0..60)
        .map(|i| {
            let center = 100.0 + (i as f64 * 0.7).sin() * 5.0;
            Bar::new(
                i as i64 * 60,
                center,
                center + 1.5,
                center - 1.2,
                center,
                1000.0,
            )
        })
        .collect();

    let mut seen = 0;
    for bar in &bars {
        if let Some(out) = atr.on_bar(bar) {
            seen += 1;
            common::assert_close(
                out.value,
                100.0 * out.extra["raw"] / bar.close,
                1e-9,
                "ATR percent equals raw relative to close",
            );
        }
    }
    assert!(seen > 0, "ATR produced no outputs");
}

#[test]
fn test_golden_adx_reference_values() {
    let mut adx = Adx::new(3, 3, 2, 20.0);
    let bars: Vec<Bar> = (0..10)
        .map(|i| {
            let center = 100.0 + i as f64;
            Bar::new(i, center, center + 1.0, center - 1.0, center, 100.0)
        })
        .collect();

    let output = bars
        .iter()
        .filter_map(|bar| adx.on_bar(bar))
        .last()
        .expect("ADX produced no output");
    common::assert_close(
        output.value,
        expected("adx3_smooth3"),
        expected("adx_tolerance"),
        "ADX(3,3)",
    );
}

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const VOL_PRICES: [f64; 20] = [
    44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61,
    46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64,
];

fn run_vol(
    name: &str,
    params: &HashMap<String, f64>,
) -> Option<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut ind = build_checked(name, params).unwrap();
    let mut last = None;
    for (i, &p) in VOL_PRICES.iter().enumerate() {
        let bar = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&bar) {
            last = Some(out);
        }
    }
    last
}

#[test]
fn test_golden_aroon_reference_values() {
    let out = run_vol("aroon", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("Aroon produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(out.value, expected("aroon5_osc"), tol, "Aroon Osc");
    common::assert_close(
        out.extra["aroon_up"],
        expected("aroon5_up"),
        tol,
        "Aroon Up",
    );
    common::assert_close(
        out.extra["aroon_down"],
        expected("aroon5_down"),
        tol,
        "Aroon Down",
    );
}

#[test]
fn test_golden_chandelier_exit_reference_values() {
    let out = run_vol(
        "chandelier_exit",
        &HashMap::from([("length".to_string(), 5.0), ("atr_mult".to_string(), 3.0)]),
    )
    .expect("Chandelier Exit produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(
        out.extra["long_stop"],
        expected("chandelier5_long"),
        tol,
        "Chandelier Long Stop",
    );
    common::assert_close(
        out.extra["short_stop"],
        expected("chandelier5_short"),
        tol,
        "Chandelier Short Stop",
    );
}

#[test]
fn test_golden_choppiness_reference_values() {
    let out = run_vol("choppiness", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("Choppiness produced no output");
    common::assert_close(
        out.value,
        expected("choppiness5_last"),
        expected("vol_tolerance"),
        "Choppiness(5)",
    );
}

#[test]
fn test_golden_dmi_reference_values() {
    let out = run_vol("dmi", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("DMI produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(out.value, expected("dmi5_dx"), tol, "DMI DX");
    common::assert_close(out.extra["plus_di"], expected("dmi5_plus"), tol, "DMI +DI");
    common::assert_close(
        out.extra["minus_di"],
        expected("dmi5_minus"),
        tol,
        "DMI -DI",
    );
}

#[test]
fn test_golden_donchian_reference_values() {
    let out = run_vol("donchian", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("Donchian produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(
        out.value,
        expected("donchian5_basis"),
        tol,
        "Donchian basis",
    );
    common::assert_close(
        out.extra["upper"],
        expected("donchian5_upper"),
        tol,
        "Donchian upper",
    );
    common::assert_close(
        out.extra["lower"],
        expected("donchian5_lower"),
        tol,
        "Donchian lower",
    );
}

#[test]
fn test_golden_envelope_reference_values() {
    let out = run_vol(
        "envelope",
        &HashMap::from([("period".to_string(), 5.0), ("percent".to_string(), 2.0)]),
    )
    .expect("Envelope produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(
        out.value,
        expected("envelope5_basis"),
        tol,
        "Envelope basis",
    );
    common::assert_close(
        out.extra["upper"],
        expected("envelope5_upper"),
        tol,
        "Envelope upper",
    );
    common::assert_close(
        out.extra["lower"],
        expected("envelope5_lower"),
        tol,
        "Envelope lower",
    );
}

#[test]
fn test_golden_garman_klass_reference_values() {
    let out = run_vol(
        "garman_klass",
        &HashMap::from([("period".to_string(), 5.0)]),
    )
    .expect("Garman-Klass produced no output");
    common::assert_close(
        out.value,
        expected("garman_klass5_last"),
        expected("vol_tolerance"),
        "Garman-Klass(5)",
    );
}

#[test]
fn test_golden_historical_volatility_reference_values() {
    let out = run_vol(
        "historical_volatility",
        &HashMap::from([("period".to_string(), 5.0)]),
    )
    .expect("Historical Volatility produced no output");
    common::assert_close(
        out.value,
        expected("hv5_last"),
        expected("vol_tolerance"),
        "Historical Volatility(5)",
    );
}

#[test]
fn test_golden_keltner_reference_values() {
    // "ema_period" is the published catalog contract (finding 05); this must be the key that
    // actually drives the EMA base, not the "ma_period" the builder used to read instead.
    let out = run_vol(
        "keltner",
        &HashMap::from([
            ("ema_period".to_string(), 5.0),
            ("atr_period".to_string(), 5.0),
            ("multiplier".to_string(), 2.0),
        ]),
    )
    .expect("Keltner produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(out.value, expected("keltner5_basis"), tol, "Keltner basis");
    common::assert_close(
        out.extra["upper"],
        expected("keltner5_upper"),
        tol,
        "Keltner upper",
    );
    common::assert_close(
        out.extra["lower"],
        expected("keltner5_lower"),
        tol,
        "Keltner lower",
    );
}

/// Finding 05: "ma_period" is accepted as a legacy alias for "ema_period" and must drive the same
/// EMA base, producing the same reference values as the canonical key.
#[test]
fn test_golden_keltner_legacy_ma_period_alias_matches_canonical() {
    let canonical = run_vol(
        "keltner",
        &HashMap::from([
            ("ema_period".to_string(), 5.0),
            ("atr_period".to_string(), 5.0),
            ("multiplier".to_string(), 2.0),
        ]),
    )
    .expect("Keltner produced no output");
    let via_alias = run_vol(
        "keltner",
        &HashMap::from([
            ("ma_period".to_string(), 5.0),
            ("atr_period".to_string(), 5.0),
            ("multiplier".to_string(), 2.0),
        ]),
    )
    .expect("Keltner produced no output");

    assert_eq!(canonical.value, via_alias.value);
    assert_eq!(canonical.extra["upper"], via_alias.extra["upper"]);
    assert_eq!(canonical.extra["lower"], via_alias.extra["lower"]);
}

/// A registry-built Keltner with a non-default `ema_period` must diverge from the catalog default
/// (period 20): this is the direct reproduction from finding 05 of the parameter being silently
/// ignored (previously `ema_period` had no effect because the builder read `ma_period`).
#[test]
fn test_golden_keltner_ema_period_actually_changes_output() {
    let short = run_vol(
        "keltner",
        &HashMap::from([
            ("ema_period".to_string(), 5.0),
            ("atr_period".to_string(), 5.0),
        ]),
    )
    .expect("Keltner produced no output");
    let default_period = run_vol("keltner", &HashMap::from([("atr_period".to_string(), 5.0)]))
        .expect("Keltner produced no output");

    assert_ne!(
        short.value, default_period.value,
        "ema_period=5 must not silently fall back to the default period-20 basis"
    );
}

#[test]
fn test_golden_mass_index_reference_values() {
    let out = run_vol("mass_index", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("Mass Index produced no output");
    common::assert_close(
        out.value,
        expected("mass_index5_last"),
        expected("vol_tolerance"),
        "Mass Index(5)",
    );
}

#[test]
fn test_golden_parabolic_sar_reference_values() {
    let out = run_vol(
        "parabolic_sar",
        &HashMap::from([("step".to_string(), 0.02), ("max_step".to_string(), 0.20)]),
    )
    .expect("Parabolic SAR produced no output");
    common::assert_close(
        out.value,
        expected("psar_last"),
        expected("vol_tolerance"),
        "Parabolic SAR",
    );
}

#[test]
fn test_golden_supertrend_reference_values() {
    let out = run_vol(
        "supertrend",
        &HashMap::from([("period".to_string(), 5.0), ("multiplier".to_string(), 3.0)]),
    )
    .expect("Supertrend produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(
        out.value,
        expected("supertrend5_line"),
        tol,
        "Supertrend line",
    );
    common::assert_close(
        out.extra["trend"],
        expected("supertrend5_dir"),
        tol,
        "Supertrend direction",
    );
}

#[test]
fn test_golden_true_range_reference_values() {
    let out = run_vol("true_range", &HashMap::new()).expect("True Range produced no output");
    common::assert_close(
        out.value,
        expected("true_range_last"),
        expected("vol_tolerance"),
        "True Range",
    );
}

#[test]
fn test_golden_vix_fix_reference_values() {
    let out = run_vol(
        "vix_fix",
        &HashMap::from([
            ("pd".to_string(), 5.0),
            ("bband_len".to_string(), 5.0),
            ("mult".to_string(), 2.0),
        ]),
    )
    .expect("VIX Fix produced no output");
    common::assert_close(
        out.value,
        expected("vix_fix5_last"),
        expected("vol_tolerance"),
        "VIX Fix",
    );
}

#[test]
fn test_golden_vortex_reference_values() {
    let out = run_vol("vortex", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("Vortex produced no output");
    let tol = expected("vol_tolerance");
    common::assert_close(
        out.value,
        expected("vortex5_plus"),
        tol,
        "Vortex +VI (value)",
    );
    common::assert_close(
        out.extra["vi_plus"],
        expected("vortex5_plus"),
        tol,
        "Vortex +VI",
    );
    common::assert_close(
        out.extra["vi_minus"],
        expected("vortex5_minus"),
        tol,
        "Vortex -VI",
    );
}

/// Auf ihren Eingaben halten ADX (streng steigend, DX überall 100) und Mass Index (konstante
/// Spanne, Quotient 1) nur einen Grenzwert fest. Diese Reihe bewegt sich in beide Richtungen mit
/// wechselnder Spanne, damit beide Formeln tatsächlich greifen.
fn shaped_bars() -> Vec<Bar> {
    [
        100.0, 102.0, 101.0, 104.0, 103.0, 106.0, 105.0, 103.0, 104.0, 101.0, 102.0, 99.0, 100.0,
        98.0,
    ]
    .iter()
    .enumerate()
    .map(|(i, &c)| {
        Bar::new(
            i as i64 * 60,
            c,
            c + 1.0 + 0.3 * (i % 3) as f64,
            c - 1.0 - 0.2 * (i % 4) as f64,
            c,
            1000.0,
        )
    })
    .collect()
}

#[test]
fn test_golden_adx_and_mass_index_on_shaped_bars() {
    let bars = shaped_bars();

    let mut adx = Adx::new(3, 3, 2, 20.0);
    let adx_value = bars
        .iter()
        .filter_map(|bar| adx.on_bar(bar))
        .last()
        .expect("ADX gab nichts aus")
        .value;
    common::assert_close(
        adx_value,
        expected("adx3_shaped_last"),
        expected("adx_tolerance"),
        "ADX(3,3), geformt",
    );
    assert!(
        adx_value < 99.0,
        "die Reihe muss die DX-Werte bewegen: {adx_value}"
    );

    let mut mass =
        build_checked("mass_index", &HashMap::from([("period".to_string(), 5.0)])).unwrap();
    let mass_value = bars
        .iter()
        .filter_map(|bar| mass.on_bar(bar))
        .last()
        .expect("Mass Index gab nichts aus")
        .value;
    common::assert_close(
        mass_value,
        expected("mass_index5_shaped_last"),
        expected("vol_tolerance"),
        "Mass Index(5), geformt",
    );
    assert!(
        (mass_value - 5.0).abs() > 1e-3,
        "die Reihe muss die Spanne bewegen: {mass_value}"
    );
}

// --- Paket 29: Chande Kroll Stop ------------------------------------------------------------

/// (open, high, low, close) — Aufwärtsbewegung mit Rücksetzern und einer Abwärtsphase am Ende,
/// damit beide Linien in Bewegung geraten.
const CKS_BARS: [(f64, f64, f64, f64); 20] = [
    (99.5, 101.5, 98.5, 100.0),
    (100.5, 102.0, 100.0, 101.0),
    (102.5, 104.0, 102.0, 103.0),
    (101.5, 103.5, 101.0, 102.0),
    (103.5, 105.0, 102.5, 104.0),
    (105.5, 107.0, 105.0, 106.0),
    (104.5, 106.5, 104.0, 105.0),
    (106.5, 108.0, 106.0, 107.0),
    (108.5, 110.0, 107.5, 109.0),
    (107.5, 109.5, 107.0, 108.0),
    (109.5, 111.0, 109.0, 110.0),
    (111.5, 113.0, 111.0, 112.0),
    (110.5, 112.5, 109.5, 111.0),
    (112.5, 114.0, 112.0, 113.0),
    (114.5, 116.0, 114.0, 115.0),
    (113.5, 115.5, 113.0, 114.0),
    (111.5, 113.0, 110.5, 112.0),
    (109.5, 111.0, 109.0, 110.0),
    (110.5, 112.5, 110.0, 111.0),
    (112.5, 114.0, 112.0, 113.0),
];

fn cks_outputs(
    atr_len: usize,
    stop_len: usize,
    mult: f64,
) -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut cks = ChandeKrollStop::new(atr_len, stop_len, mult);
    CKS_BARS
        .iter()
        .enumerate()
        .filter_map(|(i, &(o, h, l, c))| cks.on_bar(&Bar::new(i as i64 * 60, o, h, l, c, 1000.0)))
        .collect()
}

#[test]
fn test_golden_chande_kroll_reference_values() {
    let tolerance = expected("cks_tolerance");
    let outputs = cks_outputs(4, 3, 2.0);

    assert_eq!(
        outputs.len() as f64,
        expected("cks4_3_mult2_output_count"),
        "erste Ausgabe nach atr_len + stop_len Kerzen"
    );
    common::assert_close(
        outputs[0].extra["stop_long"],
        expected("cks4_3_mult2_long_first"),
        tolerance,
        "Chande Kroll Long-Stop, erste Ausgabe",
    );
    common::assert_close(
        outputs[0].extra["stop_short"],
        expected("cks4_3_mult2_short_first"),
        tolerance,
        "Chande Kroll Short-Stop, erste Ausgabe",
    );

    let last = outputs.last().unwrap();
    common::assert_close(
        last.extra["stop_long"],
        expected("cks4_3_mult2_long_last"),
        tolerance,
        "Chande Kroll Long-Stop, letzte Ausgabe",
    );
    common::assert_close(
        last.extra["stop_short"],
        expected("cks4_3_mult2_short_last"),
        tolerance,
        "Chande Kroll Short-Stop, letzte Ausgabe",
    );
    common::assert_close(
        last.value,
        last.extra["stop_long"],
        0.0,
        "value ist der Long-Stop",
    );
}

/// stop_len = 1 macht die zweite Stufe zur Identität — die Ausgabe muss dann genau die
/// Vorstufenwerte sein.
#[test]
fn test_chande_kroll_with_stop_len_one_returns_the_first_stage() {
    let tolerance = expected("cks_tolerance");
    let last = cks_outputs(4, 1, 2.0)
        .last()
        .cloned()
        .expect("Chande Kroll gab nichts aus");
    common::assert_close(
        last.extra["stop_long"],
        expected("cks4_1_mult2_long_last"),
        tolerance,
        "Vorstufe Long",
    );
    common::assert_close(
        last.extra["stop_short"],
        expected("cks4_1_mult2_short_last"),
        tolerance,
        "Vorstufe Short",
    );
}

/// Konstante Reihe ohne Spanne: True Range 0, also ATR 0 — beide Stops fallen auf den Preis.
#[test]
fn test_chande_kroll_on_constant_series_collapses_to_the_price() {
    let mut cks = ChandeKrollStop::new(4, 3, 3.0);
    let outputs: Vec<_> = (0..15)
        .filter_map(|i| cks.on_bar(&Bar::new(i * 60, 42.0, 42.0, 42.0, 42.0, 1000.0)))
        .collect();
    assert!(!outputs.is_empty(), "Chande Kroll gab nichts aus");
    for out in outputs {
        common::assert_close(out.extra["stop_long"], 42.0, 1e-12, "Long-Stop konstant");
        common::assert_close(out.extra["stop_short"], 42.0, 1e-12, "Short-Stop konstant");
    }
}

/// Die Linien dürfen sich kreuzen: Bei kleinem Multiplikator bleibt der Long-Stop nahe am
/// Fensterhoch und der Short-Stop nahe am Fenstertief, sodass der Long-Stop über dem Short-Stop
/// liegt. Das bleibt so stehen, statt sortiert zu werden.
#[test]
fn test_chande_kroll_lines_may_cross_without_being_sorted() {
    let mut cks = ChandeKrollStop::new(4, 3, 0.1);
    let mut crossed = false;
    for (i, &(o, h, l, c)) in CKS_BARS.iter().enumerate() {
        if let Some(out) = cks.on_bar(&Bar::new(i as i64 * 60, o, h, l, c, 1000.0)) {
            if out.extra["stop_long"] > out.extra["stop_short"] {
                crossed = true;
            }
        }
    }
    assert!(
        crossed,
        "bei kleinem Multiplikator müssen sich die Linien kreuzen können"
    );
}

#[test]
fn test_chande_kroll_reset_restarts_deterministically() {
    let mut cks = ChandeKrollStop::new(4, 3, 2.0);
    let run = |cks: &mut ChandeKrollStop| -> Vec<(f64, f64)> {
        CKS_BARS
            .iter()
            .enumerate()
            .filter_map(|(i, &(o, h, l, c))| {
                cks.on_bar(&Bar::new(i as i64 * 60, o, h, l, c, 1000.0))
            })
            .map(|o| (o.extra["stop_long"], o.extra["stop_short"]))
            .collect()
    };
    let first = run(&mut cks);
    cks.reset();
    assert_eq!(first, run(&mut cks));
}

// --- Paket 42: Wahl der True-Range-Glättung -------------------------------------------------

/// Sechs Kerzen mit Auf- und Abwärtsgap; dieselben True Ranges für alle vier Methoden.
const ATR_METHOD_BARS: [(f64, f64, f64, f64); 6] = [
    (100.0, 102.0, 99.0, 101.0),
    (105.0, 107.0, 104.0, 106.0),
    (103.0, 104.0, 100.0, 101.0),
    (95.0, 96.0, 93.0, 94.0),
    (94.0, 99.0, 93.0, 98.0),
    (98.0, 101.0, 97.0, 100.0),
];

fn atr_method_outputs(
    method: TrueRangeSmoothing,
) -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    // sig_len = 1, damit die Signalglättung die Ausgabe nicht zusätzlich verzögert und der
    // Vergleich allein die True-Range-Glättung trifft.
    let mut atr = Atr::new(3, 1).with_smoothing(method);
    ATR_METHOD_BARS
        .iter()
        .enumerate()
        .filter_map(|(i, &(o, h, l, c))| atr.on_bar(&Bar::new(i as i64 * 60, o, h, l, c, 1000.0)))
        .collect()
}

#[test]
fn test_golden_atr_smoothing_methods_reference_values() {
    let tolerance = expected("atr_tolerance");

    for (method, key) in [
        (TrueRangeSmoothing::Rma, "rma"),
        (TrueRangeSmoothing::Sma, "sma"),
        (TrueRangeSmoothing::Ema, "ema"),
        (TrueRangeSmoothing::Wma, "wma"),
    ] {
        let outputs = atr_method_outputs(method);
        assert_eq!(
            outputs.len() as f64,
            expected("atr3_method_output_count"),
            "{key}: jede Methode gibt ab der dritten True Range aus"
        );
        common::assert_close(
            outputs[0].extra["raw"],
            expected(&format!("atr3_{key}_raw_third")),
            tolerance,
            &format!("ATR(3) {key}, erste Ausgabe"),
        );
        common::assert_close(
            outputs.last().unwrap().extra["raw"],
            expected(&format!("atr3_{key}_raw_last")),
            tolerance,
            &format!("ATR(3) {key}, letzte Ausgabe"),
        );
    }
}

/// Der Prozentwert folgt in jeder Methode demselben Rohwert derselben Kerze.
#[test]
fn test_atr_percent_follows_the_selected_raw_value_in_every_method() {
    for method in [
        TrueRangeSmoothing::Rma,
        TrueRangeSmoothing::Sma,
        TrueRangeSmoothing::Ema,
        TrueRangeSmoothing::Wma,
    ] {
        let outputs = atr_method_outputs(method);
        for (output, bar) in outputs.iter().zip(&ATR_METHOD_BARS[2..]) {
            common::assert_close(
                output.value,
                100.0 * output.extra["raw"] / bar.3,
                1e-12,
                &format!("{method:?}: Prozentwert aus dem Rohwert"),
            );
        }
    }
}

/// Der Default bleibt Wilder, und die bestehenden Golden-Werte gelten unverändert weiter.
#[test]
fn test_atr_default_smoothing_is_unchanged() {
    let mut default = Atr::new(3, 2);
    let mut explicit = Atr::new(3, 2).with_smoothing(TrueRangeSmoothing::Rma);
    for (i, &(o, h, l, c)) in ATR_METHOD_BARS.iter().enumerate() {
        let bar = Bar::new(i as i64 * 60, o, h, l, c, 1000.0);
        assert_eq!(
            default.on_bar(&bar).map(|out| out.value),
            explicit.on_bar(&bar).map(|out| out.value)
        );
    }
}

/// Die Methodenwahl trifft nur die True-Range-Glättung. Die Signalreihe bleibt in jeder Methode
/// Wilder-geglättet, erkennbar an ihrer Rekursion über die veröffentlichten Prozentwerte:
/// `signal_t = signal_{t-1} + (value_t - signal_{t-1}) / sig_len`.
#[test]
fn test_signal_line_stays_wilder_smoothed_in_every_method() {
    for method in [
        TrueRangeSmoothing::Rma,
        TrueRangeSmoothing::Sma,
        TrueRangeSmoothing::Ema,
        TrueRangeSmoothing::Wma,
    ] {
        let mut atr = Atr::new(3, 2).with_smoothing(method);
        let outputs: Vec<_> = ATR_METHOD_BARS
            .iter()
            .enumerate()
            .filter_map(|(i, &(o, h, l, c))| {
                atr.on_bar(&Bar::new(i as i64 * 60, o, h, l, c, 1000.0))
            })
            .collect();

        assert!(outputs.len() >= 2, "{method:?}: zu wenige Ausgaben");
        for pair in outputs.windows(2) {
            let previous_signal = pair[0].extra["signal"];
            let expected = previous_signal + (pair[1].value - previous_signal) / 2.0;
            common::assert_close(
                pair[1].extra["signal"],
                expected,
                1e-12,
                &format!("{method:?}: Wilder-Rekursion der Signallinie"),
            );
        }
    }
}

#[test]
fn test_atr_smoothing_enum_is_validated_and_defaults_to_rma() {
    use kestrel_chartkit::indicator::params::{ParamValue, TypedParams};
    use kestrel_chartkit::indicator::registry::{build_typed, RegistryError};

    let bars: Vec<Bar> = ATR_METHOD_BARS
        .iter()
        .enumerate()
        .map(|(i, &(o, h, l, c))| Bar::new(i as i64 * 60, o, h, l, c, 1000.0))
        .collect();
    let run = |mut ind: Box<dyn Indicator>| -> Vec<f64> {
        bars.iter()
            .filter_map(|b| ind.on_bar(b))
            .map(|o| o.value)
            .collect()
    };

    let default_typed = build_typed("atr", &TypedParams::new()).unwrap();
    let explicit = build_typed(
        "atr",
        &TypedParams::from([("smoothing".to_string(), ParamValue::Enum("rma".to_string()))]),
    )
    .unwrap();
    assert_eq!(run(default_typed), run(explicit));

    let err = match build_typed(
        "atr",
        &TypedParams::from([(
            "smoothing".to_string(),
            ParamValue::Enum("wilder".to_string()),
        )]),
    ) {
        Ok(_) => panic!("unbekannte Methode muss abgelehnt werden"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::InvalidEnumValue { .. }),
        "{err:?}"
    );
}

// --- Paket 34: Ulcer Index ------------------------------------------------------------------

const ULCER_PRICES: [f64; 12] = [
    100.0, 102.0, 104.0, 101.0, 96.0, 92.0, 95.0, 99.0, 103.0, 106.0, 104.0, 105.0,
];

#[test]
fn test_golden_ulcer_index_reference_values() {
    let mut core = UlcerIndexCore::new(3);
    let values: Vec<f64> = ULCER_PRICES
        .iter()
        .filter_map(|v| core.update(*v))
        .collect();
    let tolerance = expected("ulcer_tolerance");

    assert_eq!(
        values.len() as f64,
        expected("ulcer3_output_count"),
        "erste Ausgabe nach 2*len - 1 Beobachtungen"
    );
    common::assert_close(
        values[0],
        expected("ulcer3_first"),
        tolerance,
        "erster Wert",
    );
    common::assert_close(
        values[1],
        expected("ulcer3_second"),
        tolerance,
        "zweiter Wert",
    );
    common::assert_close(
        *values.last().unwrap(),
        expected("ulcer3_last"),
        tolerance,
        "letzter Wert",
    );
}

/// Eine monoton steigende und eine flache Reihe kennen keinen Abstand zum eigenen Hoch: exakt 0.
#[test]
fn test_ulcer_index_is_zero_without_any_drawdown() {
    let rising: Vec<f64> = (0..10).map(|i| 100.0 * 1.01_f64.powi(i)).collect();
    assert_eq!(ulcer_index(&rising, 3), Some(0.0));
    assert_eq!(ulcer_index(&[50.0; 10], 3), Some(0.0));
}

/// Ein alter Drawdown wird nicht gegen ein späteres Hoch umgerechnet: Hängt man an dieselbe
/// Reihe einen kräftigen Anstieg an, bleiben die zuvor ausgegebenen Werte unverändert.
#[test]
fn test_past_drawdowns_are_not_recomputed_against_a_later_high() {
    let mut short = UlcerIndexCore::new(3);
    let prefix: Vec<f64> = ULCER_PRICES
        .iter()
        .filter_map(|v| short.update(*v))
        .collect();

    let mut extended_input = ULCER_PRICES.to_vec();
    extended_input.extend_from_slice(&[130.0, 160.0, 200.0]);
    let mut long = UlcerIndexCore::new(3);
    let full: Vec<f64> = extended_input
        .iter()
        .filter_map(|v| long.update(*v))
        .collect();

    assert_eq!(prefix, full[..prefix.len()]);
}

/// Nicht positive oder nicht endliche Werte haben keinen prozentualen Abstand zu einem Hoch —
/// sie werden abgelehnt, statt in das Fenster zu wandern.
#[test]
fn test_non_positive_values_are_refused_and_leave_the_state_untouched() {
    let mut core = UlcerIndexCore::new(3);
    for value in ULCER_PRICES.iter().take(5) {
        core.update(*value);
    }
    let before = core.clone();

    assert_eq!(core.update(0.0), None);
    assert_eq!(core.update(-5.0), None);
    assert_eq!(core.update(f64::NAN), None);

    let mut untouched = before;
    assert_eq!(core.update(95.0), untouched.update(95.0));
}

#[test]
fn test_ulcer_index_reset_restarts_deterministically() {
    let mut core = UlcerIndexCore::new(3);
    let run = |core: &mut UlcerIndexCore| -> Vec<f64> {
        ULCER_PRICES
            .iter()
            .filter_map(|v| core.update(*v))
            .collect()
    };
    let first = run(&mut core);
    core.reset();
    assert_eq!(first, run(&mut core));
}

/// Der Registry-Indikator rechnet über den Schlusskurs denselben Kern.
#[test]
fn test_ulcer_index_engine_matches_the_scalar_core() {
    let mut engine = UlcerIndexEngine::new(3);
    let mut core = UlcerIndexCore::new(3);
    for (i, &price) in ULCER_PRICES.iter().enumerate() {
        let bar = Bar::new(
            i as i64 * 60,
            price,
            price + 1.0,
            price - 1.0,
            price,
            1000.0,
        );
        assert_eq!(engine.on_bar(&bar).map(|out| out.value), core.update(price));
    }
}

// --- Paket 35: Relative Volatility Index ----------------------------------------------------

fn rvi_vol_bars() -> Vec<Bar> {
    (0..40)
        .map(|i| {
            let close = 100.0 + 6.0 * (i as f64 * 0.4).sin() + 0.5 * (-1.0_f64).powi(i);
            Bar::new(
                i as i64 * 60,
                close,
                close + 1.0,
                close - 1.0,
                close,
                1000.0,
            )
        })
        .collect()
}

fn rvi_vol_values(variant: RelativeVolatilityVariant) -> Vec<f64> {
    let mut rvi = RelativeVolatilityIndex::new(5, 4, variant);
    rvi_vol_bars()
        .iter()
        .filter_map(|bar| rvi.on_bar(bar))
        .map(|out| out.value)
        .collect()
}

#[test]
fn test_golden_relative_volatility_reference_values() {
    let values = rvi_vol_values(RelativeVolatilityVariant::Close);
    let tolerance = expected("rvi_vol_tolerance");

    assert_eq!(
        values.len() as f64,
        expected("rvi_vol_close_output_count"),
        "erste Ausgabe erst, wenn Fenster und Wilder-Glättung bereit sind"
    );
    common::assert_close(
        values[0],
        expected("rvi_vol_close_first"),
        tolerance,
        "erste Ausgabe",
    );
    common::assert_close(
        *values.last().unwrap(),
        expected("rvi_vol_close_last"),
        tolerance,
        "letzte Ausgabe",
    );
    common::assert_close(
        *rvi_vol_values(RelativeVolatilityVariant::HighLow)
            .last()
            .unwrap(),
        expected("rvi_vol_high_low_last"),
        tolerance,
        "Hoch/Tief-Variante",
    );
}

/// Monotone Reihen sind die analytisch eindeutigen Fälle: Steigt der Preis in jedem Schritt,
/// landet jede Standardabweichung auf der Aufwärtsseite und der Wert ist exakt 100.
#[test]
fn test_relative_volatility_is_bounded_by_monotone_series() {
    let run = |prices: Vec<f64>| {
        let mut rvi = RelativeVolatilityIndex::new(5, 4, RelativeVolatilityVariant::Close);
        prices
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                rvi.on_bar(&Bar::new(i as i64 * 60, c, c + 1.0, c - 1.0, c, 1000.0))
            })
            .map(|o| o.value)
            .last()
            .expect("keine Ausgabe")
    };

    common::assert_close(
        run((0..30).map(|i| 100.0 + i as f64).collect()),
        100.0,
        0.0,
        "monoton steigend",
    );
    common::assert_close(
        run((0..30).map(|i| 100.0 - i as f64).collect()),
        0.0,
        0.0,
        "monoton fallend",
    );
}

/// Ohne jede Bewegung landet nichts auf einer der beiden Seiten. Die dokumentierte Konvention
/// ist dann 50 — dieselbe neutrale Lesart, die auch der RSI dieses Crates verwendet.
#[test]
fn test_relative_volatility_on_a_flat_series_is_the_neutral_fifty() {
    let mut rvi = RelativeVolatilityIndex::new(5, 4, RelativeVolatilityVariant::Close);
    let values: Vec<f64> = (0..30)
        .filter_map(|i| rvi.on_bar(&Bar::new(i * 60, 50.0, 50.0, 50.0, 50.0, 1000.0)))
        .map(|o| o.value)
        .collect();
    assert!(!values.is_empty());
    for value in values {
        assert_eq!(value, 50.0);
    }
}

/// Beide Glätter sehen jede Beobachtung: Ein Kurzschluss während des Warmups hätte die beiden
/// Zustände um die Glättungslänge auseinanderlaufen lassen. Der Test prüft, dass die erste
/// Ausgabe genau dort steht, wo der Vertrag sie verspricht.
#[test]
fn test_relative_volatility_first_output_matches_the_declared_warmup() {
    let mut rvi = RelativeVolatilityIndex::new(5, 4, RelativeVolatilityVariant::Close);
    let bars = rvi_vol_bars();
    let first_index = bars
        .iter()
        .position(|bar| rvi.on_bar(bar).is_some())
        .expect("keine Ausgabe");

    assert!(
        first_index <= rvi.warmup_period(),
        "erste Ausgabe bei {first_index}, deklarierter Warmup {}",
        rvi.warmup_period()
    );
}

#[test]
fn test_relative_volatility_reset_restarts_deterministically() {
    let bars = rvi_vol_bars();
    let mut rvi = RelativeVolatilityIndex::new(5, 4, RelativeVolatilityVariant::HighLow);
    let run = |rvi: &mut RelativeVolatilityIndex| -> Vec<f64> {
        bars.iter()
            .filter_map(|bar| rvi.on_bar(bar))
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut rvi);
    rvi.reset();
    assert_eq!(first, run(&mut rvi));
}

/// Die Variante ist typisiert wählbar und wird geprüft.
#[test]
fn test_relative_volatility_variant_is_validated() {
    use kestrel_chartkit::indicator::params::{ParamValue, TypedParams};
    use kestrel_chartkit::indicator::registry::{build_typed, RegistryError};

    assert!(build_typed(
        "relative_volatility",
        &TypedParams::from([(
            "variant".to_string(),
            ParamValue::Enum("high_low".to_string())
        )])
    )
    .is_ok());

    let err = match build_typed(
        "relative_volatility",
        &TypedParams::from([("variant".to_string(), ParamValue::Enum("hl2".to_string()))]),
    ) {
        Ok(_) => panic!("unbekannte Variante muss abgelehnt werden"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::InvalidEnumValue { .. }),
        "{err:?}"
    );
}

/// Chandelier Flip Radar über drei Folgen: ein ruhiger Aufwärtstrend, ein Volatilitätssprung mit
/// Bärenfalle im adaptiven Modus und ein Schluss unter dem Long-Stop mit zu kleinem Körper.
#[test]
fn test_golden_chandelier_flip_radar_reference_values() {
    let tol = expected("cfr_tolerance");
    let trend = |n: usize| -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100.0 + i as f64 * 2.0;
                Bar::new(
                    i as i64 * 60,
                    base,
                    base + 3.0,
                    base - 3.0,
                    base + 1.0,
                    100.0,
                )
            })
            .collect()
    };
    let run = |engine: &mut ChandelierFlipRadarEngine, bars: &[Bar]| {
        bars.iter()
            .filter_map(|bar| engine.on_bar(bar))
            .last()
            .expect("output")
    };

    let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, false, false, 0.0, 0.35, 0.75);
    let out = run(&mut engine, &trend(15));
    common::assert_close(
        out.value,
        expected("cfr_trend_active_stop"),
        tol,
        "aktiver Stop",
    );
    common::assert_close(
        out.extra["dist_atr"],
        expected("cfr_trend_dist_atr"),
        tol,
        "Abstand",
    );
    common::assert_close(
        out.extra["multiplier"],
        expected("cfr_trend_multiplier"),
        tol,
        "Multiplikator",
    );
    assert_eq!(out.extra["risk_state"], expected("cfr_trend_risk_state"));
    assert_eq!(out.state.as_deref(), Some("long_healthy"));

    let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, false, true, 0.0, 0.35, 0.75);
    let mut spike: Vec<Bar> = (0..105)
        .map(|i| Bar::new(i * 60, 100.0, 101.0, 99.0, 100.0, 100.0))
        .collect();
    spike.push(Bar::new(105 * 60, 100.0, 250.0, 50.0, 150.0, 100.0));
    let out = run(&mut engine, &spike);
    common::assert_close(
        out.extra["multiplier"],
        expected("cfr_spike_multiplier"),
        tol,
        "adaptiver Multiplikator",
    );
    common::assert_close(
        out.value,
        expected("cfr_spike_active_stop"),
        tol,
        "aktiver Stop",
    );
    assert_eq!(
        engine.alerts().iter().any(|a| a.kind == "bull_bear_trap"),
        expected("cfr_spike_bear_trap") == 1.0,
        "Bärenfalle"
    );

    let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, false, false, 0.8, 0.35, 0.75);
    let mut weak = trend(6);
    weak.push(Bar::new(6 * 60, 94.6, 96.0, 94.0, 94.5, 100.0));
    let out = run(&mut engine, &weak);
    common::assert_close(
        out.value,
        expected("cfr_weak_active_stop"),
        tol,
        "Stop bleibt long",
    );
    assert_eq!(out.extra["risk_state"], expected("cfr_weak_risk_state"));
    assert_eq!(out.state.as_deref(), Some("long_danger"));
    let alerts = engine.alerts();
    for (kind, key) in [
        ("bear_weak_flip", "cfr_weak_bear_weak_flip"),
        ("bear_flip", "cfr_weak_bear_flip"),
    ] {
        assert_eq!(
            alerts.iter().any(|a| a.kind == kind),
            expected(key) == 1.0,
            "{kind}: {alerts:?}"
        );
    }
}
