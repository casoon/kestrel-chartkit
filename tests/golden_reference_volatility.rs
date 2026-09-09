mod common;

use kestrel_chartkit::indicator::adx::Adx;
use kestrel_chartkit::indicator::atr::Atr;
use kestrel_chartkit::indicator::chande_kroll::ChandeKrollStop;
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
