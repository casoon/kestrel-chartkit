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

#[test]
fn test_mfi_bounds_across_synthetic_seeds() {
    let mut mfi = Mfi::with_defaults();
    // Verify bounds [0.0, 100.0] and finiteness across 50 random walk seeds with volume
    for seed in 1..=50 {
        let bars = generate_random_walk_bars(seed, 100, 100.0, 0.05, 2.0, 1000.0);
        let outputs = run_indicator(&mut mfi, &bars);

        for (i, out) in outputs.iter().enumerate() {
            if let Some(out) = out {
                assert!(
                    (0.0..=100.0).contains(&out.value),
                    "MFI out of bounds at bar {} on seed {}: {}",
                    i,
                    seed,
                    out.value
                );
                assert!(
                    out.value.is_finite(),
                    "MFI produced non-finite value at bar {} on seed {}",
                    i,
                    seed
                );
            }
        }
    }
}
