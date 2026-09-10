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
        expected("buy_sell_pressure5_neutral_location"),
        tol,
        "Neutral Bar Location",
    );
    common::assert_close(
        out_neutral.extra["wick_balance"],
        expected("buy_sell_pressure5_neutral_wick_balance"),
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
    common::assert_close(
        h_out.extra["location"],
        expected("buy_sell_pressure_hammer_location"),
        tol,
        "Hammer Bar Location",
    );
    common::assert_close(
        h_out.extra["wick_balance"],
        expected("buy_sell_pressure_hammer_wick_balance"),
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
    common::assert_close(
        out.extra["bb_width"],
        expected("volatility_regime5_squeeze_bb_width"),
        tol,
        "Squeeze BB Width",
    );
    common::assert_close(
        out.extra["kc_width"],
        expected("volatility_regime5_squeeze_kc_width"),
        tol,
        "Squeeze KC Width",
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
    common::assert_close(
        spike_out.value,
        expected("volatility_regime5_expansion_state"),
        tol,
        "Volatility Regime Expansion State",
    );
    common::assert_close(
        spike_out.extra["bb_width"],
        expected("volatility_regime5_expansion_bb_width"),
        tol,
        "Expansion BB Width",
    );
    common::assert_close(
        spike_out.extra["kc_width"],
        expected("volatility_regime5_expansion_kc_width"),
        tol,
        "Expansion KC Width",
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

/// Auf der Geraden sitzt jeder Trendfaktor an einer Grenze (ER 1, ADX 100, RSI 100), der Druck ist
/// null und das Regime ein Squeeze. Die fallende Reihe mit Gegenbewegungen löst jeden Faktor von
/// seiner Grenze und lässt das Regime normal, sodass der Multi-Faktor-Wert ungedämpft bleibt.
fn falling_bars() -> Vec<Bar> {
    const OFFSETS: [f64; 3] = [0.0, 0.6, -0.3];
    let mut previous = 60.0;
    (0..30usize)
        .map(|i| {
            let close = 60.0 - 0.3 * i as f64 + OFFSETS[i % 3];
            let high = f64::max(previous, close) + 0.05 + 0.05 * (i % 3) as f64;
            let low = f64::min(previous, close) - 0.05 - 0.05 * (i % 2) as f64;
            let volume = 1000.0 + 250.0 * (i % 5) as f64;
            let bar = Bar::new(i as i64 * 60, previous, high, low, close, volume);
            previous = close;
            bar
        })
        .collect()
}

fn run_on(name: &str, bars: &[Bar]) -> kestrel_chartkit::indicator::IndicatorOutput {
    let mut ind = build_checked(name, &HashMap::from([("period".to_string(), 5.0)])).unwrap();
    let mut last = None;
    for bar in bars {
        last = ind.on_bar(bar).or(last);
    }
    last.unwrap_or_else(|| panic!("{name} gab nichts aus"))
}

#[test]
fn test_golden_composites_off_the_straight_line() {
    let bars = falling_bars();
    let tol = expected("composite_tolerance");

    let trend = run_on("trend_quality", &bars);
    let mut product = 100.0;
    for factor in ["direction", "efficiency", "strength", "participation"] {
        let key = format!("trend_quality5_shaped_{factor}");
        common::assert_close(trend.extra[factor], expected(&key), tol, &key);
        product *= expected(&key);
    }
    // Zirkularitätsverbot: die Kombinationsformel auf die Teilwerte der Fixture, nicht auf die
    // gerade gemessenen.
    common::assert_close(
        trend.value,
        product,
        tol,
        "Trend Quality, Kombinationsformel",
    );
    common::assert_close(
        trend.value,
        expected("trend_quality5_shaped_score"),
        tol,
        "Trend Quality, fallende Reihe",
    );

    let pressure = run_on("buy_sell_pressure", &bars);
    common::assert_close(
        pressure.value,
        expected("buy_sell_pressure5_shaped"),
        tol,
        "Buy/Sell Pressure, fallende Reihe",
    );
    for field in ["location", "wick_balance"] {
        let key = format!("buy_sell_pressure5_shaped_{field}");
        common::assert_close(pressure.extra[field], expected(&key), tol, &key);
    }

    let regime = run_on("volatility_regime", &bars);
    common::assert_close(
        regime.value,
        expected("volatility_regime5_shaped_state"),
        tol,
        "Volatility Regime, fallende Reihe",
    );
    for field in ["bb_width", "kc_width"] {
        let key = format!("volatility_regime5_shaped_{field}");
        common::assert_close(regime.extra[field], expected(&key), tol, &key);
    }

    let multi = run_on("multi_factor", &bars);
    for (field, key) in [
        ("trend_factor", "trend_factor"),
        ("rsi_factor", "rsi_factor"),
        ("pressure_factor", "pressure_factor"),
        ("volatility_factor", "vol_state"),
    ] {
        let key = format!("multi_factor5_shaped_{key}");
        common::assert_close(multi.extra[field], expected(&key), tol, &key);
    }
    assert_eq!(
        expected("multi_factor5_shaped_vol_state"),
        0.0,
        "die Reihe muss den ungedämpften Pfad treffen"
    );
    let combined = expected("multi_factor5_shaped_trend_factor") * 0.35
        + expected("multi_factor5_shaped_rsi_factor") * 0.25
        + expected("multi_factor5_shaped_pressure_factor") * 0.40;
    common::assert_close(
        multi.value,
        combined,
        tol,
        "Multi-Factor, Kombinationsformel",
    );
    common::assert_close(
        multi.value,
        expected("multi_factor5_shaped_final_score"),
        tol,
        "Multi-Factor, fallende Reihe",
    );
}
