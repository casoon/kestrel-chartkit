mod common;

use kestrel_chartkit::indicator::lsma::LsmaEngine;
use kestrel_chartkit::indicator::moving_averages::EmaEngine;
use kestrel_chartkit::indicator::params::{ParamValue, TypedParams};
use kestrel_chartkit::indicator::registry::{build_checked, build_typed, RegistryError};
use kestrel_chartkit::indicator::smoothing::{Ema, EmaInit};
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_moving_averages.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

// Shared 10-bar close series for the moving-average family, see
// tests/fixtures/golden_moving_averages.txt for the independently derived reference values.
const CLOSES: [f64; 10] = [10.0, 11.0, 12.0, 11.0, 13.0, 14.0, 13.0, 15.0, 16.0, 15.0];

fn run_last(name: &str) -> f64 {
    let mut indicator = build_checked(name, &HashMap::from([("period".to_string(), 5.0)])).unwrap();
    let mut last = None;
    for (i, &c) in CLOSES.iter().enumerate() {
        let bar = Bar::new(i as i64, c, c + 0.5, c - 0.5, c, 1000.0);
        if let Some(out) = indicator.on_bar(&bar) {
            last = Some(out.value);
        }
    }
    last.expect("indicator produced no output")
}

#[test]
fn test_golden_sma_reference_values() {
    common::assert_close(
        run_last("sma"),
        expected("sma5_last"),
        expected("ma_tolerance"),
        "SMA(5)",
    );
}

#[test]
fn test_golden_ema_reference_values() {
    common::assert_close(
        run_last("ema"),
        expected("ema5_last"),
        expected("ma_tolerance"),
        "EMA(5)",
    );
}

#[test]
fn test_golden_wma_reference_values() {
    common::assert_close(
        run_last("wma"),
        expected("wma5_last"),
        expected("ma_tolerance"),
        "WMA(5)",
    );
}

#[test]
fn test_golden_vwma_reference_values() {
    common::assert_close(
        run_last("vwma"),
        expected("vwma5_last"),
        expected("ma_tolerance"),
        "VWMA(5)",
    );
}

#[test]
fn test_golden_hma_reference_values() {
    common::assert_close(
        run_last("hma"),
        expected("hma5_last"),
        expected("ma_tolerance"),
        "HMA(5)",
    );
}

#[test]
fn test_golden_dema_reference_values() {
    common::assert_close(
        run_last("dema"),
        expected("dema5_last"),
        expected("ma_tolerance"),
        "DEMA(5)",
    );
}

#[test]
fn test_golden_kama_reference_values() {
    common::assert_close(
        run_last("kama"),
        expected("kama5_last"),
        expected("ma_tolerance"),
        "KAMA(5)",
    );
}

#[test]
fn test_golden_tema_reference_values() {
    common::assert_close(
        run_last("tema"),
        expected("tema5_last"),
        expected("ma_tolerance"),
        "TEMA(5)",
    );
}

#[test]
fn test_golden_lsma_reference_values() {
    common::assert_close(
        run_last("lsma"),
        expected("lsma5_last"),
        expected("ma_tolerance"),
        "LSMA(5)",
    );
}

#[test]
fn test_golden_mcginley_reference_values() {
    common::assert_close(
        run_last("mcginley"),
        expected("mcginley5_last"),
        expected("ma_tolerance"),
        "McGinley(5)",
    );
}

// --- Paket 19: EMA-Initialisierung ----------------------------------------------------------

/// Zwölf Schlusskurse; siehe Fixture für die unabhängig hergeleiteten Referenzwerte.
const EMA_SEED_CLOSES: [f64; 12] = [
    22.0, 22.5, 23.25, 22.75, 23.5, 24.0, 23.0, 22.25, 23.75, 24.5, 25.0, 24.25,
];

fn ema_seed_outputs(period: usize, init: EmaInit) -> Vec<f64> {
    let mut ema = EmaEngine::new(period).with_init(init);
    EMA_SEED_CLOSES
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| ema.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)))
        .map(|out| out.value)
        .collect()
}

#[test]
fn test_golden_ema_seed_modes_reference_values() {
    let tolerance = expected("ma_tolerance");

    let sma_seeded = ema_seed_outputs(5, EmaInit::Sma);
    let first_sample = ema_seed_outputs(5, EmaInit::FirstSample);

    assert_eq!(
        sma_seeded.len(),
        first_sample.len(),
        "beide Modi geben ab derselben Kerze aus"
    );
    common::assert_close(
        sma_seeded[0],
        expected("ema5_seed_sma_first_output"),
        tolerance,
        "EMA(5) SMA-Seed, erste Ausgabe",
    );
    common::assert_close(
        *sma_seeded.last().unwrap(),
        expected("ema5_seed_sma_last"),
        tolerance,
        "EMA(5) SMA-Seed, letzter Wert",
    );
    common::assert_close(
        *first_sample.last().unwrap(),
        expected("ema5_seed_first_sample_last"),
        tolerance,
        "EMA(5) First-Sample-Seed, letzter Wert",
    );
}

/// 19-01: Bei N = 1 ist alpha = 1, der Seed also in beiden Modi der erste Wert selbst.
#[test]
fn test_ema_period_one_is_identical_in_both_seed_modes() {
    assert_eq!(
        ema_seed_outputs(1, EmaInit::FirstSample),
        ema_seed_outputs(1, EmaInit::Sma)
    );
}

/// Konstante Reihe: Der SMA-Seed ist der konstante Wert, die Rekursion verlässt ihn nicht.
#[test]
fn test_ema_sma_seed_on_constant_series_stays_constant() {
    let mut ema = EmaEngine::new(5).with_init(EmaInit::Sma);
    let outputs: Vec<f64> = (0..10)
        .filter_map(|i| ema.on_bar(&Bar::new(i * 60, 42.0, 42.0, 42.0, 42.0, 1000.0)))
        .map(|o| o.value)
        .collect();
    assert!(!outputs.is_empty());
    for value in outputs {
        common::assert_close(value, 42.0, 1e-12, "EMA auf konstanter Reihe");
    }
}

/// Der Glätter selbst gibt im SMA-Modus vor der Seed-Bereitschaft nichts aus, statt einen
/// angesammelten Teilwert auszugeben.
#[test]
fn test_ema_smoother_withholds_values_until_sma_seed_is_ready() {
    let mut ema = Ema::new(4).with_init(EmaInit::Sma);
    assert_eq!(ema.warmup_period(), 4);
    assert_eq!(ema.update(10.0), None);
    assert_eq!(ema.update(11.0), None);
    assert_eq!(ema.update(12.0), None);
    assert_eq!(ema.update(13.0), Some(11.5));

    let mut first_sample = Ema::new(4);
    assert_eq!(first_sample.warmup_period(), 0);
    assert_eq!(first_sample.update(10.0), Some(10.0));
}

/// Nach `reset` beginnt auch der angesammelte Seed neu.
#[test]
fn test_ema_reset_clears_the_partial_sma_seed() {
    let mut ema = Ema::new(3).with_init(EmaInit::Sma);
    assert_eq!(ema.update(1.0), None);
    assert_eq!(ema.update(2.0), None);
    ema.reset();
    assert_eq!(ema.update(10.0), None);
    assert_eq!(ema.update(20.0), None);
    assert_eq!(ema.update(30.0), Some(20.0));
}

/// Der Default über die Registry bleibt der First-Sample-Seed; verschachtelte Verwender
/// (DEMA/TEMA) werden nicht mitgezogen.
#[test]
fn test_ema_registry_default_is_first_sample_and_enum_is_validated() {
    let default_typed = build_typed("ema", &TypedParams::new()).unwrap();
    let explicit = build_typed(
        "ema",
        &TypedParams::from([(
            "init".to_string(),
            ParamValue::Enum("first_sample".to_string()),
        )]),
    )
    .unwrap();

    let bars: Vec<Bar> = EMA_SEED_CLOSES
        .iter()
        .enumerate()
        .map(|(i, &c)| Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0))
        .collect();
    let run = |mut ind: Box<dyn Indicator>| -> Vec<f64> {
        bars.iter()
            .filter_map(|b| ind.on_bar(b))
            .map(|o| o.value)
            .collect()
    };
    assert_eq!(run(default_typed), run(explicit));

    let err = match build_typed(
        "ema",
        &TypedParams::from([("init".to_string(), ParamValue::Enum("sma_seed".to_string()))]),
    ) {
        Ok(_) => panic!("unbekannter Modus muss abgelehnt werden"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::InvalidEnumValue { .. }),
        "{err:?}"
    );
}

// --- Paket 23: Regressionsausgaben ----------------------------------------------------------

fn lsma_last_output(closes: &[f64]) -> kestrel_chartkit::indicator::IndicatorOutput {
    let mut lsma = LsmaEngine::new(closes.len());
    closes
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| lsma.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)))
        .last()
        .expect("LSMA gab nichts aus")
}

#[test]
fn test_golden_lsma_regression_outputs_reference_values() {
    let tolerance = expected("ma_tolerance");

    for (closes, key) in [
        (vec![100.0, 102.5, 105.0, 107.5, 110.0], "linear"),
        (vec![10.0, 12.0, 11.0, 14.0, 13.0], "noisy"),
    ] {
        let out = lsma_last_output(&closes);
        for (field, value) in [
            ("slope", out.extra["slope"]),
            ("intercept", out.extra["intercept"]),
            ("r2", out.extra["r2"]),
            ("value", out.value),
        ] {
            common::assert_close(
                value,
                expected(&format!("lsma5_{key}_{field}")),
                tolerance,
                &format!("LSMA(5) {field}, {key}"),
            );
        }
    }
}

/// Der Endpunkt ist der Fit an x = N-1 und muss aus Slope und Intercept derselben Ausgabe
/// folgen — sonst stammten die Zusatzfelder aus einer zweiten Rechnung.
#[test]
fn test_lsma_endpoint_follows_from_published_slope_and_intercept() {
    let mut lsma = LsmaEngine::new(5);
    let mut seen = 0;
    for (i, c) in [10.0, 12.0, 11.0, 14.0, 13.0, 15.0, 14.5, 16.0]
        .into_iter()
        .enumerate()
    {
        if let Some(out) = lsma.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)) {
            seen += 1;
            common::assert_close(
                out.value,
                out.extra["intercept"] + out.extra["slope"] * 4.0,
                1e-9,
                "LSMA-Endpunkt aus Slope und Intercept",
            );
        }
    }
    assert!(seen > 0, "LSMA gab nichts aus");
}

/// Flache Reihe: Steigung 0. Es gibt keine Preisvarianz, die die Gerade erklären könnte —
/// die dokumentierte Konvention dafür ist R² = 1.
#[test]
fn test_lsma_flat_window_has_zero_slope_and_documented_r2() {
    let out = lsma_last_output(&[7.0; 5]);
    common::assert_close(out.extra["slope"], 0.0, 1e-12, "Steigung flach");
    common::assert_close(out.extra["intercept"], 7.0, 1e-12, "Intercept flach");
    common::assert_close(out.extra["r2"], 1.0, 0.0, "R² flach");
    common::assert_close(out.value, 7.0, 1e-12, "Endpunkt flach");
}

/// Kausalität: Bereits ausgegebene Regressionswerte ändern sich durch spätere Kerzen nicht.
#[test]
fn test_lsma_regression_outputs_are_causal() {
    let closes = [10.0, 12.0, 11.0, 14.0, 13.0, 15.0, 14.5, 16.0];
    let collect = |upto: usize| -> Vec<(f64, f64)> {
        let mut lsma = LsmaEngine::new(5);
        closes[..upto]
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                lsma.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0))
            })
            .map(|o| (o.extra["slope"], o.extra["r2"]))
            .collect()
    };
    let prefix = collect(6);
    let full = collect(closes.len());
    assert_eq!(prefix, full[..prefix.len()]);
}
