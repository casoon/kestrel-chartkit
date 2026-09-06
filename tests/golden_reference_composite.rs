mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_composite.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn run_composite_bars(
    name: &str,
    params: &HashMap<String, f64>,
    count: usize,
) -> Option<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut ind = build_checked(name, params).unwrap();
    let mut last = None;
    for i in 0..count {
        let p = 44.0 + i as f64 * 0.1;
        let bar = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&bar) {
            last = Some(out);
        }
    }
    last
}

#[test]
fn test_golden_trend_quality_sub_components_and_composition() {
    let out = run_composite_bars(
        "trend_quality",
        &HashMap::from([("period".to_string(), 5.0)]),
        30,
    )
    .expect("Trend Quality produced no output");
    let tol = expected("composite_tolerance");

    // 1. Verify individual sub-indicator factors extracted into extra
    let direction = out.extra["direction"];
    let efficiency = out.extra["efficiency"];
    let strength = out.extra["strength"];
    let participation = out.extra["participation"];

    common::assert_close(
        direction,
        expected("trend_quality5_direction"),
        tol,
        "Trend Quality Direction Factor",
    );
    common::assert_close(
        efficiency,
        expected("trend_quality5_efficiency"),
        tol,
        "Trend Quality Efficiency Factor",
    );
    common::assert_close(
        strength,
        expected("trend_quality5_strength"),
        tol,
        "Trend Quality Strength Factor",
    );
    common::assert_close(
        participation,
        expected("trend_quality5_participation"),
        tol,
        "Trend Quality Participation Factor",
    );

    // 2. Anti-Zirkularität: Verify composite output against documented formula, applied to the
    // already-confirmed golden sub-values (not to the values just extracted from this run).
    let expected_composite = expected("trend_quality5_direction")
        * expected("trend_quality5_efficiency")
        * expected("trend_quality5_strength")
        * expected("trend_quality5_participation")
        * 100.0;
    common::assert_close(
        out.value,
        expected_composite,
        tol,
        "Trend Quality Formula Composition",
    );
    common::assert_close(
        out.value,
        expected("trend_quality5_score"),
        tol,
        "Trend Quality Score",
    );
}

#[test]
fn test_golden_buy_sell_pressure_neutral_and_extreme_scenarios() {
    let tol = expected("composite_tolerance");

    // 1. Neutral baseline (symmetric wicks and centered close)
    let out_neutral = run_composite_bars(
        "buy_sell_pressure",
        &HashMap::from([("period".to_string(), 5.0)]),
        30,
    )
    .expect("Buy/Sell Pressure produced no output");

    common::assert_close(
        out_neutral.extra["location"],
        0.0,
        tol,
        "Neutral Bar Location",
    );
    common::assert_close(
        out_neutral.extra["wick_balance"],
        0.0,
        tol,
        "Neutral Bar Wick Balance",
    );
    common::assert_close(
        out_neutral.value,
        expected("buy_sell_pressure5_neutral"),
        tol,
        "Buy/Sell Pressure Neutral",
    );

    // 2. Pure Bullish Hammer Extreme Scenario:
    // Open = Close = High = 100.0, Low = 90.0
    // Location = +1.0, Wick Balance = +1.0 -> Raw Pressure = 100.0
    let mut ind_hammer = build_checked(
        "buy_sell_pressure",
        &HashMap::from([("period".to_string(), 5.0)]),
    )
    .unwrap();
    let hammer_bar = Bar::new(0, 100.0, 100.0, 90.0, 100.0, 1000.0);
    let mut hammer_out = None;
    for _ in 0..10 {
        hammer_out = ind_hammer.on_bar(&hammer_bar);
    }
    let h_out = hammer_out.expect("Hammer sequence should produce output");
    common::assert_close(h_out.extra["location"], 1.0, tol, "Hammer Bar Location");
    common::assert_close(
        h_out.extra["wick_balance"],
        1.0,
        tol,
        "Hammer Bar Wick Balance",
    );
    common::assert_close(
        h_out.value,
        expected("buy_sell_pressure_bullish_hammer"),
        tol,
        "Buy/Sell Pressure Hammer Extreme",
    );
}

#[test]
fn test_golden_volatility_regime_squeeze_and_expansion() {
    let tol = expected("composite_tolerance");

    // 1. Squeeze scenario: steady linear prices with tight BB within KC
    let out = run_composite_bars(
        "volatility_regime",
        &HashMap::from([("period".to_string(), 5.0)]),
        30,
    )
    .expect("Volatility Regime produced no output");

    common::assert_close(
        out.value,
        expected("volatility_regime5_squeeze_state"),
        tol,
        "Volatility Regime Squeeze State",
    );
    assert_eq!(
        out.extra["squeeze"], 1.0,
        "Squeeze flag should be active when BB is inside KC"
    );
    assert!(
        out.extra["bb_width"] < out.extra["kc_width"],
        "BB width must be smaller than KC width during squeeze"
    );

    // 2. Expansion scenario: strong trending price moves where BB width expands beyond 1.3x KC width
    let mut ind = build_checked(
        "volatility_regime",
        &HashMap::from([("period".to_string(), 5.0)]),
    )
    .unwrap();
    // Warm up with 10 bars
    for i in 0..10 {
        let b = Bar::new(i as i64 * 60, 100.0, 101.0, 99.0, 100.0, 1000.0);
        ind.on_bar(&b);
    }
    // High-trend bars with large close variance and tight ranges
    let trend_prices = [110.0, 125.0, 145.0, 170.0, 200.0];
    let mut exp_out = None;
    for (i, &p) in trend_prices.iter().enumerate() {
        let b = Bar::new((10 + i) as i64 * 60, p - 1.0, p + 1.0, p - 1.0, p, 10000.0);
        exp_out = ind.on_bar(&b);
    }
    let spike_out = exp_out.expect("Trend bars should yield output");
    assert_eq!(
        spike_out.value, 1.0,
        "Volatility regime should flip to Expansion (1.0) on trend-driven BB expansion"
    );
}

#[test]
fn test_golden_multi_factor_sub_components_and_composition() {
    let out = run_composite_bars(
        "multi_factor",
        &HashMap::from([("period".to_string(), 5.0)]),
        30,
    )
    .expect("Multi-Factor Market Score produced no output");
    let tol = expected("composite_tolerance");

    // 1. Verify sub-factor values from out.extra
    let trend_f = out.extra["trend_factor"];
    let rsi_f = out.extra["rsi_factor"];
    let pressure_f = out.extra["pressure_factor"];
    let vol_f = out.extra["volatility_factor"];

    common::assert_close(
        trend_f,
        expected("multi_factor5_trend_factor"),
        tol,
        "Multi-Factor Trend Component",
    );
    common::assert_close(
        rsi_f,
        expected("multi_factor5_rsi_factor"),
        tol,
        "Multi-Factor RSI Component",
    );
    common::assert_close(
        pressure_f,
        expected("multi_factor5_pressure_factor"),
        tol,
        "Multi-Factor Pressure Component",
    );
    common::assert_close(
        vol_f,
        expected("multi_factor5_vol_state"),
        tol,
        "Multi-Factor Volatility Regime Component",
    );

    // 2. Anti-Zirkularität: Verify composite output via documented weighting & squeeze dampening
    // formula, applied to the already-confirmed golden sub-values (not to the values just
    // extracted from this run).
    let golden_trend_f = expected("multi_factor5_trend_factor");
    let golden_rsi_f = expected("multi_factor5_rsi_factor");
    let golden_pressure_f = expected("multi_factor5_pressure_factor");
    let golden_vol_f = expected("multi_factor5_vol_state");
    let raw_composite = golden_trend_f * 0.35 + golden_rsi_f * 0.25 + golden_pressure_f * 0.40;
    let expected_score = if golden_vol_f < 0.0 {
        raw_composite * 0.5
    } else {
        raw_composite
    };

    common::assert_close(
        out.value,
        expected_score,
        tol,
        "Multi-Factor Composition Formula Match",
    );
    common::assert_close(
        out.value,
        expected("multi_factor5_final_score"),
        tol,
        "Multi-Factor Golden Score",
    );
}
