mod common;

use common::*;
use kestrel_chartkit::indicator::stoch_rsi::StochRsi;

#[test]
fn test_stoch_rsi_bounds_invariant() {
    let mut stoch_rsi = StochRsi::with_defaults();
    let bars = generate_sine_bars(350, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut stoch_rsi, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let k = out.value;
            assert!(
                (0.0..=100.0).contains(&k),
                "StochRSI %K out of bounds at bar {}: {}",
                i,
                k
            );
            if let Some(d) = out.secondary {
                assert!(
                    (0.0..=100.0).contains(&d),
                    "StochRSI %D out of bounds at bar {}: {}",
                    i,
                    d
                );
            }
        }
    }
}
