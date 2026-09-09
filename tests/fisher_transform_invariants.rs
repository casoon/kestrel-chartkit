mod common;

use common::*;
use kestrel_chartkit::indicator::fisher_transform::FisherTransform;

#[test]
fn test_fisher_transform_finite_invariant() {
    let mut fisher = FisherTransform::with_defaults();
    let bars = generate_sine_bars(300, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut fisher, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let val = out.value;
            assert!(!val.is_nan(), "Fisher transform value is NaN at bar {}", i);
            assert!(
                !val.is_infinite(),
                "Fisher transform value is Infinite at bar {}",
                i
            );
        }
    }
}
