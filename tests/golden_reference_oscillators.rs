mod common;

use kestrel_chartkit::indicator::bbtrend::BbTrend;
use kestrel_chartkit::indicator::bollinger::{BollingerBands, VarianceConvention};
use kestrel_chartkit::indicator::params::{ParamValue, TypedParams};
use kestrel_chartkit::indicator::pmo::PriceMomentumOscillator;
use kestrel_chartkit::indicator::rci::RciEngine;
use kestrel_chartkit::indicator::registry::{build_checked, build_typed, RegistryError};
use kestrel_chartkit::indicator::rsi::{Rsi, RsiSmoothing};
use kestrel_chartkit::indicator::smi::StochasticMomentumIndex;
use kestrel_chartkit::indicator::trix::Trix;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_oscillators.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn test_golden_rsi_reference_values() {
    // Standard Wilder's RSI(14) reference dataset (Close prices)
    let prices = vec![
        44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61,
        46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64,
    ];

    let mut rsi = build_checked("rsi", &HashMap::from([("rsi_len".to_string(), 14.0)])).unwrap();
    let mut outputs = Vec::new();

    for (i, p) in prices.into_iter().enumerate() {
        let bar = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = rsi.on_bar(&bar) {
            outputs.push(out.value);
        }
    }

    assert!(!outputs.is_empty(), "RSI produced no outputs");
    common::assert_close(
        *outputs.last().unwrap(),
        expected("rsi14_last"),
        expected("rsi14_tolerance"),
        "RSI(14)",
    );
}

#[test]
fn test_golden_macd_reference_values() {
    // MACD(12, 26, 9) reference test
    let prices = vec![
        10.0, 10.5, 11.0, 11.5, 12.0, 12.5, 13.0, 13.5, 14.0, 14.5, 15.0, 15.5, 16.0, 16.5, 17.0,
        17.5, 18.0, 18.5, 19.0, 19.5, 20.0, 20.5, 21.0, 21.5, 22.0, 22.5, 23.0, 23.5, 24.0, 24.5,
    ];

    let mut macd = build_checked(
        "macd",
        &HashMap::from([
            ("fast_len".to_string(), 12.0),
            ("slow_len".to_string(), 26.0),
            ("signal_len".to_string(), 9.0),
        ]),
    )
    .unwrap();

    let mut final_out = None;
    for (i, p) in prices.into_iter().enumerate() {
        let bar = Bar::new(i as i64 * 60, p, p + 0.2, p - 0.2, p, 1000.0);
        if let Some(out) = macd.on_bar(&bar) {
            final_out = Some(out);
        }
    }

    let out = final_out.expect("MACD produced no outputs");
    let tolerance = expected("macd_tolerance");
    common::assert_close(
        out.value,
        expected("macd_line_last"),
        tolerance,
        "MACD line",
    );
    common::assert_close(
        out.extra["signal"],
        expected("macd_signal_last"),
        tolerance,
        "MACD signal",
    );
    common::assert_close(
        out.extra["hist"],
        expected("macd_hist_last"),
        tolerance,
        "MACD histogram",
    );
}

const OSC_PRICES: [f64; 20] = [
    44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61,
    46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64,
];

fn run_osc(
    name: &str,
    params: &HashMap<String, f64>,
) -> Option<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut ind = build_checked(name, params).unwrap();
    let mut last = None;
    for (i, &p) in OSC_PRICES.iter().enumerate() {
        let bar = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&bar) {
            last = Some(out);
        }
    }
    last
}

#[test]
fn test_golden_bollinger_reference_values() {
    let out = run_osc(
        "bollinger",
        &HashMap::from([("len".to_string(), 5.0), ("mult".to_string(), 2.0)]),
    )
    .expect("Bollinger produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(
        out.value,
        expected("bollinger5_basis"),
        tol,
        "Bollinger basis",
    );
    common::assert_close(
        out.extra["upper"],
        expected("bollinger5_upper"),
        tol,
        "Bollinger upper",
    );
    common::assert_close(
        out.extra["lower"],
        expected("bollinger5_lower"),
        tol,
        "Bollinger lower",
    );
    common::assert_close(
        out.extra["bandwidth"],
        expected("bollinger5_bandwidth"),
        tol,
        "Bollinger bandwidth",
    );
    common::assert_close(
        out.extra["percent_b"],
        expected("bollinger5_percent_b"),
        tol,
        "Bollinger %B",
    );
}

#[test]
fn test_golden_cci_reference_values() {
    let out = run_osc("cci", &HashMap::from([("cci_len".to_string(), 5.0)]))
        .expect("CCI produced no output");
    common::assert_close(
        out.value,
        expected("cci5_last"),
        expected("osc_tolerance"),
        "CCI(5)",
    );
}

#[test]
fn test_golden_stochastic_reference_values() {
    let out = run_osc(
        "stochastic",
        &HashMap::from([("k_period".to_string(), 5.0), ("d_period".to_string(), 3.0)]),
    )
    .expect("Stochastic produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(
        out.extra["percent_k"],
        expected("stoch5_k"),
        tol,
        "Stochastic %K",
    );
    common::assert_close(
        out.extra["percent_d"],
        expected("stoch5_d"),
        tol,
        "Stochastic %D",
    );
}

#[test]
fn test_golden_stoch_rsi_reference_values() {
    let out = run_osc(
        "stoch_rsi",
        &HashMap::from([
            ("rsi_len".to_string(), 5.0),
            ("stoch_len".to_string(), 5.0),
            ("k_len".to_string(), 3.0),
            ("d_len".to_string(), 3.0),
        ]),
    )
    .expect("StochRSI produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("stoch_rsi5_k"), tol, "StochRSI %K");
    common::assert_close(
        out.extra["signal"],
        expected("stoch_rsi5_d"),
        tol,
        "StochRSI %D",
    );
}

#[test]
fn test_golden_mfi_reference_values() {
    let out = run_osc("mfi", &HashMap::from([("mfi_len".to_string(), 5.0)]))
        .expect("MFI produced no output");
    common::assert_close(
        out.value,
        expected("mfi5_last"),
        expected("osc_tolerance"),
        "MFI(5)",
    );
}

#[test]
fn test_golden_williams_r_reference_values() {
    let out = run_osc("williams_r", &HashMap::from([("wpr_len".to_string(), 5.0)]))
        .expect("Williams %R produced no output");
    common::assert_close(
        out.value,
        expected("williams_r5_last"),
        expected("osc_tolerance"),
        "Williams %R(5)",
    );
}

#[test]
fn test_golden_tsi_reference_values() {
    let out = run_osc(
        "tsi",
        &HashMap::from([
            ("long_len".to_string(), 5.0),
            ("short_len".to_string(), 3.0),
            ("sig_len".to_string(), 3.0),
        ]),
    )
    .expect("TSI produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("tsi5_line"), tol, "TSI line");
    common::assert_close(
        out.extra["signal"],
        expected("tsi5_signal"),
        tol,
        "TSI signal",
    );
}

#[test]
fn test_golden_fisher_transform_reference_values() {
    let out = run_osc(
        "fisher_transform",
        &HashMap::from([
            ("fish_len".to_string(), 5.0),
            ("avg_len".to_string(), 2.0),
            ("sig_len".to_string(), 3.0),
        ]),
    )
    .expect("Fisher Transform produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("fisher5_line"), tol, "Fisher line");
    common::assert_close(
        out.extra["signal"],
        expected("fisher5_signal"),
        tol,
        "Fisher signal",
    );
}

#[test]
fn test_golden_awesome_oscillator_reference_values() {
    let out = run_osc(
        "awesome_oscillator",
        &HashMap::from([
            ("fast_period".to_string(), 3.0),
            ("slow_period".to_string(), 5.0),
        ]),
    )
    .expect("Awesome Oscillator produced no output");
    common::assert_close(
        out.value,
        expected("ao3_5_last"),
        expected("osc_tolerance"),
        "AO(3,5)",
    );
}

#[test]
fn test_golden_bop_reference_values() {
    let out = run_osc("bop", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("BOP produced no output");
    common::assert_close(
        out.value,
        expected("bop5_last"),
        expected("osc_tolerance"),
        "BOP(5)",
    );
}

#[test]
fn test_golden_chaikin_oscillator_reference_values() {
    let out = run_osc(
        "chaikin_oscillator",
        &HashMap::from([("fast_len".to_string(), 3.0), ("slow_len".to_string(), 5.0)]),
    )
    .expect("Chaikin Oscillator produced no output");
    common::assert_close(
        out.value,
        expected("chaikin_osc_last"),
        expected("osc_tolerance"),
        "Chaikin Osc",
    );
}

#[test]
fn test_golden_cmo_reference_values() {
    let out = run_osc("cmo", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("CMO produced no output");
    common::assert_close(
        out.value,
        expected("cmo5_last"),
        expected("osc_tolerance"),
        "CMO(5)",
    );
}

#[test]
fn test_golden_connors_rsi_reference_values() {
    let out = run_osc(
        "connors_rsi",
        &HashMap::from([
            ("rsi_len".to_string(), 3.0),
            ("streak_len".to_string(), 2.0),
            ("rank_len".to_string(), 5.0),
        ]),
    )
    .expect("Connors RSI produced no output");
    common::assert_close(
        out.value,
        expected("connors_rsi_last"),
        expected("osc_tolerance"),
        "Connors RSI",
    );
}

fn run_osc_60(
    name: &str,
    params: &HashMap<String, f64>,
) -> Option<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut ind = build_checked(name, params).unwrap();
    let mut last = None;
    for i in 0..60 {
        let p = 44.0 + i as f64 * 0.1;
        let bar = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&bar) {
            last = Some(out);
        }
    }
    last
}

#[test]
fn test_golden_coppock_reference_values() {
    let out = run_osc_60("coppock", &HashMap::new()).expect("Coppock produced no output");
    common::assert_close(
        out.value,
        expected("coppock_60bar_last"),
        expected("osc_tolerance"),
        "Coppock",
    );
}

#[test]
fn test_golden_dpo_reference_values() {
    let out = run_osc("dpo", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("DPO produced no output");
    common::assert_close(
        out.value,
        expected("dpo5_last"),
        expected("osc_tolerance"),
        "DPO(5)",
    );
}

#[test]
fn test_golden_elder_ray_reference_values() {
    let out = run_osc("elder_ray", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("Elder Ray produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(
        out.value,
        expected("elder_bull_last"),
        tol,
        "Elder Bull Power",
    );
    common::assert_close(
        out.extra["bear_power"],
        expected("elder_bear_last"),
        tol,
        "Elder Bear Power",
    );
}

#[test]
fn test_golden_kst_reference_values() {
    let out = run_osc_60("kst", &HashMap::new()).expect("KST produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("kst60_line"), tol, "KST line");
    common::assert_close(
        out.extra["signal"],
        expected("kst60_signal"),
        tol,
        "KST signal",
    );
}

#[test]
fn test_golden_ppo_reference_values() {
    let out = run_osc(
        "ppo",
        &HashMap::from([
            ("fast_period".to_string(), 3.0),
            ("slow_period".to_string(), 5.0),
            ("signal_period".to_string(), 3.0),
        ]),
    )
    .expect("PPO produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("ppo_line"), tol, "PPO line");
    common::assert_close(
        out.extra["signal"],
        expected("ppo_signal"),
        tol,
        "PPO signal",
    );
}

#[test]
fn test_golden_roc_reference_values() {
    let out = run_osc("roc", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("ROC produced no output");
    common::assert_close(
        out.value,
        expected("roc5_last"),
        expected("osc_tolerance"),
        "ROC(5)",
    );
}

#[test]
fn test_golden_rvi_reference_values() {
    let out = run_osc("rvi", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("RVI produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("rvi_line"), tol, "RVI line");
    common::assert_close(
        out.extra["signal"],
        expected("rvi_signal"),
        tol,
        "RVI signal",
    );
}

#[test]
fn test_golden_ultimate_oscillator_reference_values() {
    let out = run_osc(
        "ultimate_oscillator",
        &HashMap::from([
            ("period1".to_string(), 3.0),
            ("period2".to_string(), 5.0),
            ("period3".to_string(), 10.0),
        ]),
    )
    .expect("Ultimate Oscillator produced no output");
    common::assert_close(
        out.value,
        expected("uo_last"),
        expected("osc_tolerance"),
        "UO",
    );
}

#[test]
fn test_golden_wavetrend_reference_values() {
    let out = run_osc(
        "wavetrend",
        &HashMap::from([
            ("n1".to_string(), 3.0),
            ("n2".to_string(), 5.0),
            ("ob_level".to_string(), 60.0),
            ("os_level".to_string(), -60.0),
        ]),
    )
    .expect("WaveTrend produced no output");
    let tol = expected("osc_tolerance");
    common::assert_close(out.value, expected("wt1_last"), tol, "WaveTrend WT1");
    common::assert_close(out.extra["wt2"], expected("wt2_last"), tol, "WaveTrend WT2");
}

// --- Paket 17: RSI-Glättungsmethode ---------------------------------------------------------

/// Preisreihe mit steigendem, flachem, fallendem und wechselndem Abschnitt: In einer reinen
/// Trendreihe wären beide Glättungsmodi kaum unterscheidbar, weil `avg_gain`/`avg_loss` dort
/// gegen dieselben Extremwerte laufen.
const RSI_MIXED_CLOSES: [f64; 20] = [
    100.0, 101.0, 102.0, 103.0, 104.0, 104.0, 104.0, 104.0, 103.0, 101.0, 98.0, 96.0, 95.0, 96.0,
    95.0, 97.0, 96.0, 98.0, 97.0, 99.0,
];

fn rsi_mixed(smoothing: RsiSmoothing) -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    // ctx_len = 8 statt der Voreinstellung 100, damit die Kontextlinie innerhalb der Reihe
    // überhaupt ausgibt und mitgeprüft werden kann.
    let mut rsi =
        Rsi::new(5, 3, 3, 50.0, 70.0, 30.0, 5, true, 8, 4, 10.0).with_smoothing(smoothing);
    RSI_MIXED_CLOSES
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| rsi.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)))
        .collect()
}

#[test]
fn test_golden_rsi_smoothing_modes_reference_values() {
    let tolerance = expected("rsi5_mixed_tolerance");

    for (smoothing, key) in [(RsiSmoothing::Wilder, "wilder"), (RsiSmoothing::Ema, "ema")] {
        let outputs = rsi_mixed(smoothing);
        assert_eq!(
            outputs.len(),
            15,
            "{key}: beide Modi geben ab der fünften Änderung aus"
        );

        let last = outputs.last().unwrap();
        common::assert_close(
            last.value,
            expected(&format!("rsi5_mixed_{key}_line")),
            tolerance,
            &format!("RSI(5) line, {key}"),
        );
        common::assert_close(
            last.extra["signal"],
            expected(&format!("rsi5_mixed_{key}_signal")),
            tolerance,
            &format!("RSI(5) signal, {key}"),
        );
        common::assert_close(
            last.extra["ctx"],
            expected(&format!("rsi5_mixed_{key}_ctx")),
            tolerance,
            &format!("RSI(5) context, {key}"),
        );
    }
}

/// Der Modus wirkt auch auf die Kontextlinie: Sonst verglichen die Divergenzen zwei
/// unterschiedlich geglättete Reihen miteinander.
#[test]
fn test_rsi_smoothing_mode_reaches_context_line() {
    let wilder = rsi_mixed(RsiSmoothing::Wilder);
    let ema = rsi_mixed(RsiSmoothing::Ema);

    let wilder_ctx = wilder.last().unwrap().extra["ctx"];
    let ema_ctx = ema.last().unwrap().extra["ctx"];
    assert!(
        (wilder_ctx - ema_ctx).abs() > 1e-6,
        "Kontextlinie unverändert: {wilder_ctx} vs {ema_ctx}"
    );
}

/// Flachmarkt bleibt in beiden Modi bei 50 — dort ist weder ein Aufwärts- noch ein
/// Abwärtsdurchschnitt definiert, und die dokumentierte Konvention ist die Mitte.
#[test]
fn test_rsi_flat_series_stays_at_fifty_in_both_modes() {
    for smoothing in [RsiSmoothing::Wilder, RsiSmoothing::Ema] {
        let mut rsi =
            Rsi::new(5, 3, 3, 50.0, 70.0, 30.0, 5, true, 8, 4, 10.0).with_smoothing(smoothing);
        let outputs: Vec<_> = (0..20)
            .filter_map(|i| rsi.on_bar(&Bar::new(i * 60, 50.0, 50.5, 49.5, 50.0, 1000.0)))
            .collect();
        assert!(!outputs.is_empty(), "{smoothing:?}: keine Ausgabe");
        for out in outputs {
            common::assert_close(out.value, 50.0, 1e-12, "RSI flat series");
        }
    }
}

/// Nach `reset` beginnt der Zustand deterministisch neu — in beiden Modi, auch im EMA-Modus,
/// dessen Seed die erste tatsächliche Änderung ist.
#[test]
fn test_rsi_reset_restarts_both_modes_deterministically() {
    for smoothing in [RsiSmoothing::Wilder, RsiSmoothing::Ema] {
        let mut rsi =
            Rsi::new(5, 3, 3, 50.0, 70.0, 30.0, 5, true, 8, 4, 10.0).with_smoothing(smoothing);
        let first: Vec<f64> = RSI_MIXED_CLOSES
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                rsi.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0))
            })
            .map(|o| o.value)
            .collect();
        rsi.reset();
        let second: Vec<f64> = RSI_MIXED_CLOSES
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                rsi.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0))
            })
            .map(|o| o.value)
            .collect();
        assert_eq!(first, second, "{smoothing:?}: Reset nicht deterministisch");
    }
}

/// Der Default bleibt Wilder — auch über die typisierte Registry ohne `smoothing`-Angabe.
#[test]
fn test_rsi_registry_default_is_wilder_and_enum_is_validated() {
    let default_typed = build_typed("rsi", &TypedParams::new()).expect("rsi ohne smoothing");
    let explicit = build_typed(
        "rsi",
        &TypedParams::from([(
            "smoothing".to_string(),
            ParamValue::Enum("wilder".to_string()),
        )]),
    )
    .expect("rsi mit smoothing=wilder");

    let bars: Vec<Bar> = RSI_MIXED_CLOSES
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
        "rsi",
        &TypedParams::from([(
            "smoothing".to_string(),
            ParamValue::Enum("wilders".to_string()),
        )]),
    ) {
        Ok(_) => panic!("unbekannter Modus muss abgelehnt werden"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::InvalidEnumValue { .. }),
        "{err:?}"
    );
}

// --- Paket 18: Bollinger-Varianzkonvention --------------------------------------------------

/// Schlusskurse nahe 10000 mit Schwankungen im Cent-Bereich.
const BOLLINGER_OFFSET_CLOSES: [f64; 5] = [10000.05, 10000.02, 10000.08, 10000.01, 10000.09];

fn bollinger_offset_output(
    variance: VarianceConvention,
) -> kestrel_chartkit::indicator::IndicatorOutput {
    let mut bb = BollingerBands::new(5, 2.0).with_variance(variance);
    BOLLINGER_OFFSET_CLOSES
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| bb.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0)))
        .last()
        .expect("Bollinger(5) gab nichts aus")
}

#[test]
fn test_golden_bollinger_variance_conventions_reference_values() {
    let tolerance = expected("bollinger5_band_tolerance");

    for (variance, key) in [
        (VarianceConvention::Population, "population"),
        (VarianceConvention::Sample, "sample"),
    ] {
        let out = bollinger_offset_output(variance);
        for (field, value) in [
            ("basis", out.value),
            ("upper", out.extra["upper"]),
            ("lower", out.extra["lower"]),
            ("bandwidth", out.extra["bandwidth"]),
            ("percent_b", out.extra["percent_b"]),
        ] {
            common::assert_close(
                value,
                expected(&format!("bollinger5_{key}_{field}")),
                tolerance,
                &format!("Bollinger(5) {field}, {key}"),
            );
        }
        common::assert_close(
            out.extra["basis"],
            out.value,
            0.0,
            "Bollinger basis equals main value",
        );
    }
}

/// Bandbreite und Prozent-B folgen den gewählten Bändern, statt aus einer zweiten,
/// populationsbasierten Rechnung zu stammen.
#[test]
fn test_bollinger_derived_outputs_follow_selected_bands() {
    for variance in [VarianceConvention::Population, VarianceConvention::Sample] {
        let out = bollinger_offset_output(variance);
        let close = BOLLINGER_OFFSET_CLOSES[BOLLINGER_OFFSET_CLOSES.len() - 1];
        let (upper, lower, basis) = (out.extra["upper"], out.extra["lower"], out.value);
        common::assert_close(
            out.extra["bandwidth"],
            (upper - lower) / basis,
            1e-15,
            "bandwidth from selected bands",
        );
        common::assert_close(
            out.extra["percent_b"],
            (close - lower) / (upper - lower),
            1e-12,
            "percent_b from selected bands",
        );
    }
}

/// Bei N = 20 unterscheiden sich die Bandabstände um sqrt(20/19) — die Bandwerte selbst nicht,
/// weil die Basis von der Divisorwahl unberührt bleibt.
#[test]
fn test_bollinger_sample_widens_band_distance_by_bessel_factor() {
    let closes: Vec<f64> = (0..20).map(|i| 100.0 + (i % 7) as f64 * 0.5).collect();
    let run = |variance: VarianceConvention| {
        let mut bb = BollingerBands::new(20, 2.0).with_variance(variance);
        closes
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                bb.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0))
            })
            .last()
            .expect("Bollinger(20) gab nichts aus")
    };

    let population = run(VarianceConvention::Population);
    let sample = run(VarianceConvention::Sample);

    common::assert_close(
        sample.value,
        population.value,
        0.0,
        "Basis ist von der Divisorwahl unabhängig",
    );
    common::assert_close(
        (sample.extra["upper"] - sample.value) / (population.extra["upper"] - population.value),
        expected("bollinger20_sample_over_population_distance"),
        expected("bollinger5_ratio_tolerance"),
        "Bandabstand sample/population",
    );
}

/// Konstante Reihe: Standardabweichung 0 in beiden Modi, also entartete Bänder mit der
/// dokumentierten Prozent-B-Konvention 0.5.
#[test]
fn test_bollinger_constant_series_has_degenerate_bands_in_both_modes() {
    for variance in [VarianceConvention::Population, VarianceConvention::Sample] {
        let mut bb = BollingerBands::new(5, 2.0).with_variance(variance);
        let out = (0..10)
            .filter_map(|i| bb.on_bar(&Bar::new(i * 60, 42.0, 42.0, 42.0, 42.0, 1000.0)))
            .last()
            .expect("Bollinger gab nichts aus");
        common::assert_close(out.extra["upper"], 42.0, 1e-12, "upper on constant series");
        common::assert_close(out.extra["lower"], 42.0, 1e-12, "lower on constant series");
        common::assert_close(out.extra["bandwidth"], 0.0, 1e-12, "bandwidth constant");
        common::assert_close(out.extra["percent_b"], 0.5, 0.0, "percent_b constant");
    }
}

/// `sample` braucht N >= 2; die Registry lehnt die Kombination ab, statt durch null zu teilen.
#[test]
fn test_bollinger_sample_variance_requires_at_least_two_periods() {
    let params = TypedParams::from([
        ("len".to_string(), ParamValue::Int(1)),
        (
            "variance".to_string(),
            ParamValue::Enum("sample".to_string()),
        ),
    ]);
    let err = match build_typed("bollinger", &params) {
        Ok(_) => panic!("len=1 mit sample muss scheitern"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::IncompatibleParameter { .. }),
        "{err:?}"
    );

    // Populationsmodus bleibt bei N = 1 erlaubt.
    let population = TypedParams::from([("len".to_string(), ParamValue::Int(1))]);
    assert!(build_typed("bollinger", &population).is_ok());

    let unknown = TypedParams::from([(
        "variance".to_string(),
        ParamValue::Enum("populational".to_string()),
    )]);
    let err = match build_typed("bollinger", &unknown) {
        Ok(_) => panic!("unbekannte Konvention muss abgelehnt werden"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::InvalidEnumValue { .. }),
        "{err:?}"
    );
}

/// Der Default über die Registry bleibt die Populationsvarianz.
#[test]
fn test_bollinger_registry_default_is_population() {
    let default_typed = build_typed("bollinger", &TypedParams::new()).unwrap();
    let explicit = build_typed(
        "bollinger",
        &TypedParams::from([(
            "variance".to_string(),
            ParamValue::Enum("population".to_string()),
        )]),
    )
    .unwrap();

    let bars: Vec<Bar> = (0..30)
        .map(|i| {
            let c = 100.0 + (i as f64 * 0.4).sin() * 3.0;
            Bar::new(i as i64 * 60, c, c + 0.3, c - 0.3, c, 1000.0)
        })
        .collect();
    let run = |mut ind: Box<dyn Indicator>| -> Vec<f64> {
        bars.iter()
            .filter_map(|b| ind.on_bar(b))
            .map(|o| o.extra["upper"])
            .collect()
    };
    assert_eq!(run(default_typed), run(explicit));
}

// --- Paket 22: TRIX -------------------------------------------------------------------------

/// Steigende Reihe mit Rücksetzern; eine reine Gerade würde den Unterschied zwischen den drei
/// Glättungsstufen kaum zeigen.
const TRIX_CLOSES: [f64; 20] = [
    100.0, 101.0, 102.5, 101.5, 103.0, 104.5, 104.0, 105.5, 107.0, 106.0, 107.5, 109.0, 108.5,
    110.0, 111.5, 111.0, 112.5, 114.0, 113.5, 115.0,
];

fn trix_outputs(
    len: usize,
    signal_len: usize,
) -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut trix = Trix::new(len, signal_len);
    TRIX_CLOSES
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| trix.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)))
        .collect()
}

#[test]
fn test_golden_trix_reference_values() {
    let outputs = trix_outputs(4, 3);
    let tolerance = expected("trix4_signal3_tolerance");

    assert_eq!(
        outputs.len() as f64,
        expected("trix4_signal3_output_count"),
        "TRIX(4) gibt ab der fünften Kerze aus"
    );
    common::assert_close(
        outputs[0].value,
        expected("trix4_signal3_line_first"),
        tolerance,
        "TRIX(4) erste Linie",
    );
    let last = outputs.last().unwrap();
    common::assert_close(
        last.value,
        expected("trix4_signal3_line_last"),
        tolerance,
        "TRIX(4) letzte Linie",
    );
    common::assert_close(
        last.extra["signal"],
        expected("trix4_signal3_signal_last"),
        tolerance,
        "TRIX(4) Signal",
    );
    common::assert_close(
        last.extra["hist"],
        last.value - last.extra["signal"],
        1e-15,
        "TRIX-Histogramm ist Linie minus Signal",
    );
}

/// Signal und Histogramm weisen eigene Bereitschaft aus: Vor der dritten Linie fehlen die
/// Schlüssel, statt eine Null vorzutäuschen.
#[test]
fn test_trix_signal_and_histogram_report_their_own_readiness() {
    let outputs = trix_outputs(4, 3);
    for (i, out) in outputs.iter().enumerate() {
        let ready = i + 1 >= 3;
        assert_eq!(
            out.extra.contains_key("signal"),
            ready,
            "Ausgabe {i}: Signalbereitschaft"
        );
        assert_eq!(
            out.extra.contains_key("hist"),
            ready,
            "Ausgabe {i}: Histogrammbereitschaft"
        );
    }
}

/// Konstante Reihe: Die dreifach geglättete Reihe ändert sich nicht, die Rate ist exakt 0.
#[test]
fn test_trix_constant_series_is_zero() {
    let mut trix = Trix::new(4, 3);
    let outputs: Vec<f64> = (0..20)
        .filter_map(|i| trix.on_bar(&Bar::new(i * 60, 50.0, 50.5, 49.5, 50.0, 1000.0)))
        .map(|o| o.value)
        .collect();
    assert!(!outputs.is_empty(), "TRIX gab nichts aus");
    for value in outputs {
        common::assert_close(value, 0.0, 0.0, "TRIX auf konstanter Reihe");
    }
}

/// Geometrisches Wachstum ist der analytisch eindeutige Fall: Wächst der Preis je Kerze um
/// denselben Faktor, wächst nach dem Einschwingen jede Stufe um denselben Faktor, und TRIX
/// nähert sich der prozentualen Wachstumsrate.
#[test]
fn test_trix_on_geometric_series_approaches_the_growth_rate() {
    let mut trix = Trix::new(5, 3);
    let mut last = None;
    for i in 0..400 {
        let c = 100.0 * 1.01_f64.powi(i);
        if let Some(out) = trix.on_bar(&Bar::new(i as i64 * 60, c, c, c, c, 1000.0)) {
            last = Some(out.value);
        }
    }
    common::assert_close(
        last.expect("TRIX gab nichts aus"),
        expected("trix_geometric_growth_percent"),
        expected("trix_geometric_tolerance"),
        "TRIX auf geometrischer Reihe",
    );
}

#[test]
fn test_trix_reset_restarts_deterministically() {
    let mut trix = Trix::new(4, 3);
    let run = |trix: &mut Trix| -> Vec<f64> {
        TRIX_CLOSES
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                trix.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0))
            })
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut trix);
    trix.reset();
    assert_eq!(first, run(&mut trix));
}

/// Die Registry baut denselben Indikator wie der Konstruktor mit den Katalog-Voreinstellungen.
#[test]
fn test_trix_registry_defaults_match_direct_construction() {
    let mut via_registry = build_checked("trix", &HashMap::new()).unwrap();
    let mut direct = Trix::with_defaults();
    for (i, &c) in TRIX_CLOSES.iter().enumerate() {
        let bar = Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0);
        assert_eq!(
            via_registry.on_bar(&bar).map(|o| o.value),
            direct.on_bar(&bar).map(|o| o.value)
        );
    }
}

// --- Paket 33: Rank Correlation Index -------------------------------------------------------

fn rci_last(prices: &[f64], len: usize) -> f64 {
    let mut rci = RciEngine::new(len);
    prices
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| rci.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)))
        .map(|out| out.value)
        .last()
        .expect("RCI gab nichts aus")
}

#[test]
fn test_golden_rci_reference_values() {
    let tolerance = expected("rci_tolerance");

    common::assert_close(
        rci_last(&[10.0, 11.0, 12.0, 13.0, 14.0], 5),
        expected("rci5_rising"),
        tolerance,
        "monoton steigend",
    );
    common::assert_close(
        rci_last(&[14.0, 13.0, 12.0, 11.0, 10.0], 5),
        expected("rci5_falling"),
        tolerance,
        "monoton fallend",
    );
    common::assert_close(
        rci_last(&[10.0, 12.0, 11.0, 14.0, 13.0], 5),
        expected("rci5_mixed"),
        tolerance,
        "gemischte Ränge",
    );
}

/// Gleichstände teilen sich den Durchschnittsrang. Genau hier liefert die verbreitete
/// Kurzformel über die Rangdifferenzen einen anderen Wert, weshalb der Fall eigens festgehalten
/// wird.
#[test]
fn test_rci_shares_average_ranks_among_ties() {
    common::assert_close(
        rci_last(&[10.0, 11.0, 11.0, 11.0, 13.0], 5),
        expected("rci5_with_ties"),
        expected("rci_tolerance"),
        "drei gleiche Preise in der Mitte",
    );
}

#[test]
fn test_golden_rci_series_reference_values() {
    let series = [
        100.0, 101.0, 99.0, 102.0, 103.0, 101.0, 104.0, 103.0, 105.0, 104.0,
    ];
    let mut rci = RciEngine::new(5);
    let values: Vec<f64> = series
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| rci.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0)))
        .map(|out| out.value)
        .collect();

    let tolerance = expected("rci_tolerance");
    assert_eq!(values.len() as f64, expected("rci5_series_output_count"));
    common::assert_close(
        values[0],
        expected("rci5_series_first"),
        tolerance,
        "erste Ausgabe",
    );
    common::assert_close(
        *values.last().unwrap(),
        expected("rci5_series_last"),
        tolerance,
        "letzte Ausgabe",
    );
}

/// Ohne Preisvarianz gibt es keine Rangordnung. Die dokumentierte Konvention ist 0 — keine
/// Richtung, statt einer aus dem Nichts erzeugten Trendstärke.
#[test]
fn test_rci_on_a_constant_series_is_zero_by_convention() {
    common::assert_close(rci_last(&[7.0; 8], 5), 0.0, 0.0, "konstante Reihe");
}

/// Ränge kennen nur die Reihenfolge: Eine beliebige streng monoton steigende Transformation der
/// Preise lässt den Wert unverändert.
#[test]
fn test_rci_depends_only_on_the_order_not_on_the_scale() {
    let prices = [100.0, 101.0, 99.0, 102.0, 103.0, 101.0, 104.0];
    let scaled: Vec<f64> = prices.iter().map(|p| (p - 90.0) * 1_000.0).collect();
    common::assert_close(
        rci_last(&prices, 5),
        rci_last(&scaled, 5),
        1e-12,
        "skalenunabhängig",
    );
}

#[test]
fn test_rci_reset_restarts_deterministically() {
    let series = [100.0, 101.0, 99.0, 102.0, 103.0, 101.0, 104.0];
    let mut rci = RciEngine::new(5);
    let run = |rci: &mut RciEngine| -> Vec<f64> {
        series
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                rci.on_bar(&Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, 1000.0))
            })
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut rci);
    rci.reset();
    assert_eq!(first, run(&mut rci));
}

// --- Paket 32: Stochastic Momentum Index ----------------------------------------------------

fn smi_outputs(
    bars: &[(f64, f64, f64)],
    len: usize,
) -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut smi = StochasticMomentumIndex::new(len, 3, 3, 3);
    bars.iter()
        .enumerate()
        .filter_map(|(i, &(high, low, close))| {
            smi.on_bar(&Bar::new(i as i64 * 60, close, high, low, close, 1000.0))
        })
        .collect()
}

fn smi_sine_bars() -> Vec<(f64, f64, f64)> {
    (0..24)
        .map(|i| {
            let close = 100.0 + 6.0 * (i as f64 * 0.5).sin();
            (close + 1.0, close - 1.0, close)
        })
        .collect()
}

#[test]
fn test_golden_smi_reference_values() {
    let outputs = smi_outputs(&smi_sine_bars(), 5);
    let tolerance = expected("smi_tolerance");

    assert_eq!(outputs.len() as f64, expected("smi5_output_count"));
    common::assert_close(
        outputs[0].value,
        expected("smi5_first"),
        tolerance,
        "SMI erste Ausgabe",
    );
    let last = outputs.last().unwrap();
    common::assert_close(last.value, expected("smi5_last"), tolerance, "SMI letzte");
    common::assert_close(
        last.extra["signal"],
        expected("smi5_signal_last"),
        tolerance,
        "SMI Signal",
    );
}

/// Der analytisch eindeutige Fall: Auf einer Geraden mit fester Kerzenform stehen Spanne und
/// Abstand zur Fenstermitte ab dem vollen Fenster fest, der Wert ist also exakt 200 * 2/6.
#[test]
fn test_smi_on_a_linear_ramp_is_exactly_two_hundred_thirds() {
    let bars: Vec<(f64, f64, f64)> = (0..20)
        .map(|i| {
            let close = 100.0 + i as f64;
            (close + 1.0, close - 1.0, close)
        })
        .collect();
    common::assert_close(
        smi_outputs(&bars, 5).last().unwrap().value,
        expected("smi5_linear_ramp"),
        expected("smi_tolerance"),
        "Gerade",
    );
}

/// Flacher Markt: Die Spanne ist null, es gibt keine Position darin — 0 statt einer Division
/// durch null.
#[test]
fn test_smi_on_a_flat_market_is_zero_without_dividing_by_zero() {
    let flat: Vec<(f64, f64, f64)> = vec![(50.0, 50.0, 50.0); 20];
    for output in smi_outputs(&flat, 5) {
        assert_eq!(output.value, 0.0);
    }
}

/// Symmetrie: Spiegelt man die Kerzen an einer Achse, kehrt sich der Wert exakt um.
#[test]
fn test_smi_is_antisymmetric_under_mirrored_prices() {
    let bars = smi_sine_bars();
    let mirrored: Vec<(f64, f64, f64)> = bars
        .iter()
        .map(|&(high, low, close)| (200.0 - low, 200.0 - high, 200.0 - close))
        .collect();

    let straight = smi_outputs(&bars, 5);
    let flipped = smi_outputs(&mirrored, 5);
    assert_eq!(straight.len(), flipped.len());
    for (a, b) in straight.iter().zip(&flipped) {
        common::assert_close(a.value, -b.value, 1e-9, "gespiegelte Reihe");
    }
}

/// Die Signallinie weist ihre Bereitschaft getrennt aus.
#[test]
fn test_smi_signal_reports_its_own_readiness() {
    let outputs = smi_outputs(&smi_sine_bars(), 5);
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(
            output.extra.contains_key("signal"),
            i + 1 >= 3,
            "Ausgabe {i}"
        );
    }
}

#[test]
fn test_smi_reset_restarts_deterministically() {
    let bars = smi_sine_bars();
    let mut smi = StochasticMomentumIndex::new(5, 3, 3, 3);
    let run = |smi: &mut StochasticMomentumIndex| -> Vec<f64> {
        bars.iter()
            .enumerate()
            .filter_map(|(i, &(high, low, close))| {
                smi.on_bar(&Bar::new(i as i64 * 60, close, high, low, close, 1000.0))
            })
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut smi);
    smi.reset();
    assert_eq!(first, run(&mut smi));
}

// --- Paket 36: BBTrend ----------------------------------------------------------------------

const BBTREND_CLOSES: [f64; 12] = [
    100.0, 101.0, 102.5, 101.5, 103.0, 104.5, 104.0, 105.5, 107.0, 106.0, 107.5, 109.0,
];

fn bbtrend_outputs() -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut bbtrend = BbTrend::new(3, 6, 2.0, VarianceConvention::Population);
    BBTREND_CLOSES
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            bbtrend.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0))
        })
        .collect()
}

#[test]
fn test_golden_bbtrend_reference_values() {
    let outputs = bbtrend_outputs();
    let tolerance = expected("bbtrend_tolerance");

    assert_eq!(outputs.len() as f64, expected("bbtrend_3_6_output_count"));
    common::assert_close(
        outputs[0].value,
        expected("bbtrend_3_6_first"),
        tolerance,
        "BBTrend erste Ausgabe",
    );
    common::assert_close(
        outputs[0].extra["upper_gap"],
        expected("bbtrend_3_6_first_upper_gap"),
        tolerance,
        "oberer Abstand",
    );
    common::assert_close(
        outputs[0].extra["lower_gap"],
        expected("bbtrend_3_6_first_lower_gap"),
        tolerance,
        "unterer Abstand",
    );
    common::assert_close(
        outputs.last().unwrap().value,
        expected("bbtrend_3_6_last"),
        tolerance,
        "BBTrend letzte Ausgabe",
    );
}

/// Gleiche Perioden bedeuten identische Bandsätze: Beide Abstände sind null, der Wert ist es auch.
#[test]
fn test_bbtrend_with_equal_periods_is_zero() {
    let mut bbtrend = BbTrend::new(4, 4, 2.0, VarianceConvention::Population);
    let outputs: Vec<_> = BBTREND_CLOSES
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            bbtrend.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0))
        })
        .collect();

    assert!(!outputs.is_empty());
    for out in outputs {
        common::assert_close(out.value, 0.0, 1e-12, "gleiche Perioden");
        common::assert_close(out.extra["upper_gap"], 0.0, 1e-12, "oberer Abstand");
    }
}

/// Flacher Markt: Beide Sätze fallen auf denselben Preis zusammen, es gibt keinen Abstand.
#[test]
fn test_bbtrend_on_a_flat_market_is_zero() {
    let mut bbtrend = BbTrend::new(3, 6, 2.0, VarianceConvention::Population);
    let outputs: Vec<_> = (0..12)
        .filter_map(|i| bbtrend.on_bar(&Bar::new(i * 60, 50.0, 50.0, 50.0, 50.0, 1000.0)))
        .collect();
    assert!(!outputs.is_empty());
    for out in outputs {
        common::assert_close(out.value, 0.0, 0.0, "flacher Markt");
    }
}

/// Die Varianzkonvention gilt für beide Sätze — ein Vergleich zweier Konventionen misst die
/// Konvention, nicht den Markt.
#[test]
fn test_bbtrend_variance_convention_reaches_both_sets_and_is_validated() {
    use kestrel_chartkit::indicator::params::{ParamValue, TypedParams};
    use kestrel_chartkit::indicator::registry::{build_typed, RegistryError};

    let sample = BbTrend::new(3, 6, 2.0, VarianceConvention::Sample);
    let population = BbTrend::new(3, 6, 2.0, VarianceConvention::Population);
    let run = |mut engine: BbTrend| -> Vec<f64> {
        BBTREND_CLOSES
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                engine.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0))
            })
            .map(|o| o.value)
            .collect()
    };
    assert_ne!(run(sample), run(population));

    let err = match build_typed(
        "bbtrend",
        &TypedParams::from([(
            "variance".to_string(),
            ParamValue::Enum("populational".to_string()),
        )]),
    ) {
        Ok(_) => panic!("unbekannte Konvention muss abgelehnt werden"),
        Err(err) => err,
    };
    assert!(
        matches!(err, RegistryError::InvalidEnumValue { .. }),
        "{err:?}"
    );
}

// --- Paket 38: Price Momentum Oscillator ----------------------------------------------------

fn pmo_sawtooth() -> Vec<f64> {
    (0..30)
        .map(|i| 100.0 * (1.0 + 0.01 * ((i % 7) as f64 - 3.0)))
        .collect()
}

fn pmo_outputs(closes: &[f64]) -> Vec<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut pmo = PriceMomentumOscillator::new(5, 3, 3);
    closes
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| pmo.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0)))
        .collect()
}

#[test]
fn test_golden_pmo_reference_values() {
    let outputs = pmo_outputs(&pmo_sawtooth());
    let tolerance = expected("pmo_tolerance");

    assert_eq!(outputs.len() as f64, expected("pmo_5_3_output_count"));
    common::assert_close(
        outputs[0].value,
        expected("pmo_5_3_first"),
        tolerance,
        "PMO erste Ausgabe",
    );
    let last = outputs.last().unwrap();
    common::assert_close(
        last.value,
        expected("pmo_5_3_last"),
        tolerance,
        "PMO letzte",
    );
    common::assert_close(
        last.extra["signal"],
        expected("pmo_5_3_signal_last"),
        tolerance,
        "PMO Signal",
    );
}

/// Analytisch eindeutig: Bei konstantem Wachstum ist die Rendite konstant, beide Stufen laufen
/// dagegen, und der Wert nähert sich zehn mal der Rendite.
#[test]
fn test_pmo_on_constant_growth_approaches_ten_times_the_return() {
    let growing: Vec<f64> = (0..200).map(|i| 100.0 * 1.01_f64.powi(i)).collect();
    common::assert_close(
        pmo_outputs(&growing).last().unwrap().value,
        expected("pmo_constant_growth"),
        1e-9,
        "konstantes Wachstum",
    );
}

/// Die Glättungskonstante ist `2/length`, nicht `2/(length+1)`. Der Nachweis nutzt zwei
/// Sonderfälle: `length_2 = 2` ergibt alpha = 1, die zweite Stufe reicht also unverändert durch,
/// und `length_1 = 4` ergibt alpha = 0.5 — dasselbe alpha wie eine gewöhnliche EMA der Periode 3,
/// nicht der Periode 4. Genau diese Verschiebung um eins wäre der stille Fehler.
#[test]
fn test_pmo_uses_two_over_length_not_the_ordinary_ema_convention() {
    use kestrel_chartkit::indicator::smoothing::Ema;

    let closes = pmo_sawtooth();
    let mut same_alpha = Ema::new(3); // alpha = 2/4 = 0.5, wie length_1 = 4
    let mut shifted_alpha = Ema::new(4); // alpha = 2/5 = 0.4
    let mut same_series = Vec::new();
    let mut shifted_series = Vec::new();
    let mut previous = closes[0];
    for close in &closes[1..] {
        let roc = 100.0 * (close / previous - 1.0);
        previous = *close;
        same_series.push(same_alpha.update(roc).unwrap());
        shifted_series.push(shifted_alpha.update(roc).unwrap());
    }

    let mut pmo = PriceMomentumOscillator::new(4, 2, 1);
    let published: Vec<f64> = closes
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| pmo.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0)))
        .map(|o| o.value)
        .collect();

    // Ausgabe ab der (length_1 + length_2 - 1)-ten Rendite, also ab Index 4 der Renditereihe.
    let offset = same_series.len() - published.len();
    for (index, value) in published.iter().enumerate() {
        common::assert_close(
            *value,
            10.0 * same_series[offset + index],
            1e-12,
            "alpha = 2/length",
        );
        assert!(
            (value - 10.0 * shifted_series[offset + index]).abs() > 1e-9,
            "die verschobene Konvention 2/(length+1) muss unterscheidbar sein"
        );
    }
}

#[test]
fn test_pmo_signal_reports_its_own_readiness() {
    let outputs = pmo_outputs(&pmo_sawtooth());
    for (i, out) in outputs.iter().enumerate() {
        assert_eq!(out.extra.contains_key("signal"), i + 1 >= 3, "Ausgabe {i}");
    }
}

#[test]
fn test_pmo_reset_restarts_deterministically() {
    let closes = pmo_sawtooth();
    let mut pmo = PriceMomentumOscillator::new(5, 3, 3);
    let run = |pmo: &mut PriceMomentumOscillator| -> Vec<f64> {
        closes
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                pmo.on_bar(&Bar::new(i as i64 * 60, c, c + 0.2, c - 0.2, c, 1000.0))
            })
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut pmo);
    pmo.reset();
    assert_eq!(first, run(&mut pmo));
}
