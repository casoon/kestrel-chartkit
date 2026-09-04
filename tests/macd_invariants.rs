mod common;

use common::*;
use kestrel_chartkit::indicator::macd::Macd;

#[test]
fn test_macd_formula_invariant() {
    let mut macd = Macd::with_defaults();
    let bars = generate_sine_bars(200, 100.0, 15.0, 40.0, 1000.0);

    let outputs = run_indicator(&mut macd, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let macd_val = out.value;
            let signal_val = out
                .extra
                .get("signal")
                .copied()
                .expect("MACD should have signal extra");
            let hist_val = out
                .extra
                .get("hist")
                .copied()
                .expect("MACD should have hist extra");

            // Histogram = MACD - Signal (with tolerance)
            assert!(
                (hist_val - (macd_val - signal_val)).abs() < 1e-6,
                "Histogram formula mismatch at bar {}: hist={}, macd={}, signal={}",
                i,
                hist_val,
                macd_val,
                signal_val
            );
        }
    }
}

#[test]
fn test_macd_uptrend_positive() {
    let mut macd = Macd::with_defaults();
    let bars = generate_trend_bars(100, 100.0, 2.0, 1000.0);

    let outputs = run_indicator(&mut macd, &bars);
    let valid: Vec<_> = outputs.into_iter().flatten().collect();

    assert!(!valid.is_empty());
    let last = valid.last().unwrap();
    assert!(
        last.value > 0.0,
        "MACD value should be positive in strong uptrend, got {}",
        last.value
    );
}

#[test]
fn test_macd_downtrend_negative() {
    let mut macd = Macd::with_defaults();
    let bars = generate_trend_bars(100, 500.0, -2.0, 1000.0);

    let outputs = run_indicator(&mut macd, &bars);
    let valid: Vec<_> = outputs.into_iter().flatten().collect();

    assert!(!valid.is_empty());
    let last = valid.last().unwrap();
    assert!(
        last.value < 0.0,
        "MACD value should be negative in strong downtrend, got {}",
        last.value
    );
}
