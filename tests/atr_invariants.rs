mod common;

use common::*;
use kestrel_chartkit::indicator::atr::Atr;

#[test]
fn test_atr_positivity_invariant() {
    let mut atr = Atr::with_defaults();
    let bars = generate_sine_bars(200, 100.0, 10.0, 20.0, 1000.0);

    let outputs = run_indicator(&mut atr, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            assert!(
                out.value >= 0.0,
                "ATR value must be >= 0.0 at bar {}: {}",
                i,
                out.value
            );
            if let Some(&sig) = out.extra.get("signal") {
                assert!(
                    sig >= 0.0,
                    "ATR signal must be >= 0.0 at bar {}: {}",
                    i,
                    sig
                );
            }
        }
    }
}
