mod common;

use common::*;
use kestrel_chartkit::indicator::mfi::Mfi;

#[test]
fn test_mfi_bounds_invariant() {
    let mut mfi = Mfi::with_defaults();
    let bars = generate_sine_bars(300, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut mfi, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let val = out.value;
            assert!(
                (0.0..=100.0).contains(&val),
                "MFI out of bounds [0, 100] at bar {}: {}",
                i,
                val
            );
        }
    }
}
