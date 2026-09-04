mod common;

use common::*;
use kestrel_chartkit::indicator::rsi::Rsi;

#[test]
fn test_rsi_bounds_invariant() {
    let mut rsi = Rsi::with_defaults();
    let bars = generate_sine_bars(300, 100.0, 20.0, 50.0, 1000.0);

    let outputs = run_indicator(&mut rsi, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            assert!(
                (0.0..=100.0).contains(&out.value),
                "RSI out of bounds at bar {}: {}",
                i,
                out.value
            );
            assert!(!out.value.is_nan(), "RSI value is NaN at bar {}", i);
        }
    }
}

#[test]
fn test_rsi_uptrend_monotonicity() {
    let mut rsi = Rsi::with_defaults();
    let bars = generate_trend_bars(150, 100.0, 2.0, 1000.0); // Strong uptrend

    let outputs = run_indicator(&mut rsi, &bars);
    let valid_outputs: Vec<f64> = outputs.into_iter().flatten().map(|o| o.value).collect();

    assert!(!valid_outputs.is_empty());
    // In a strong uptrend, RSI should be high (> 60.0)
    let last_rsi = *valid_outputs.last().unwrap();
    assert!(
        last_rsi > 60.0,
        "Strong uptrend expected high RSI, got {}",
        last_rsi
    );
}

#[test]
fn test_rsi_downtrend_low_values() {
    let mut rsi = Rsi::with_defaults();
    let bars = generate_trend_bars(150, 500.0, -2.0, 1000.0); // Strong downtrend

    let outputs = run_indicator(&mut rsi, &bars);
    let valid_outputs: Vec<f64> = outputs.into_iter().flatten().map(|o| o.value).collect();

    assert!(!valid_outputs.is_empty());
    let last_rsi = *valid_outputs.last().unwrap();
    assert!(
        last_rsi < 40.0,
        "Strong downtrend expected low RSI, got {}",
        last_rsi
    );
}

#[test]
fn test_rsi_flat_prices() {
    let mut rsi = Rsi::with_defaults();
    let bars = generate_flat_spread_bars(200, 100.0, 1.0, 1000.0);

    let outputs = run_indicator(&mut rsi, &bars);
    let valid_outputs: Vec<f64> = outputs.into_iter().flatten().map(|o| o.value).collect();

    assert!(!valid_outputs.is_empty());
    // On flat alternating prices, RSI should settle near 50.0
    let last_rsi = *valid_outputs.last().unwrap();
    assert!(
        (last_rsi - 50.0).abs() < 15.0,
        "Flat prices RSI expected near 50, got {}",
        last_rsi
    );
}
