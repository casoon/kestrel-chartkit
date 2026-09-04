mod common;

use kestrel_chartkit::indicator::adx::Adx;
use kestrel_chartkit::indicator::atr::Atr;
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
    let out = run_vol(
        "keltner",
        &HashMap::from([
            ("ma_period".to_string(), 5.0),
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
