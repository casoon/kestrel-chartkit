mod common;

use common::*;
use kestrel_chartkit::indicator::tsi::Tsi;

#[test]
fn test_tsi_bounds_invariant() {
    let mut tsi = Tsi::with_defaults();
    let bars = generate_sine_bars(300, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut tsi, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let val = out.value;
            assert!(
                (-100.0..=100.0).contains(&val),
                "TSI out of bounds [-100, 100] at bar {}: {}",
                i,
                val
            );
        }
    }
}
