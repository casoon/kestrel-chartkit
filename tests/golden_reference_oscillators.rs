mod common;

use kestrel_chartkit::indicator::registry::build_checked;
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
