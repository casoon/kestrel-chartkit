mod common;

use common::*;
use kestrel_chartkit::indicator::bollinger::BollingerBands;

#[test]
fn test_bollinger_bands_ordering_invariant() {
    let mut bb = BollingerBands::with_defaults();
    let bars = generate_sine_bars(250, 100.0, 10.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut bb, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let basis = out.extra.get("basis").copied().expect("basis extra");
            let upper = out.extra.get("upper").copied().expect("upper extra");
            let lower = out.extra.get("lower").copied().expect("lower extra");

            assert!(
                upper >= basis - 1e-6,
                "Bollinger upper ({}) < basis ({}) at bar {}",
                upper,
                basis,
                i
            );
            assert!(
                basis >= lower - 1e-6,
                "Bollinger basis ({}) < lower ({}) at bar {}",
                basis,
                lower,
                i
            );
        }
    }
}

#[test]
fn test_bollinger_zero_bandwidth_on_flat_price() {
    let mut bb = BollingerBands::with_defaults();
    let bars = generate_flat_spread_bars(50, 100.0, 0.0, 1000.0);

    let outputs = run_indicator(&mut bb, &bars);
    let valid: Vec<_> = outputs.into_iter().flatten().collect();

    assert!(!valid.is_empty());
    let last = valid.last().unwrap();
    let bandwidth = last.extra.get("bandwidth").copied().unwrap();
    assert!(
        bandwidth.abs() < 1e-6,
        "Zero variance flat prices should yield zero bandwidth, got {}",
        bandwidth
    );
}
