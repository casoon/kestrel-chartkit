mod common;

use common::*;
use kestrel_chartkit::indicator::williams_r::WilliamsR;

#[test]
fn test_williams_r_bounds_invariant() {
    let mut wpr = WilliamsR::with_defaults();
    let bars = generate_sine_bars(300, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut wpr, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let val = out.value;
            assert!(
                (0.0..=100.0).contains(&val),
                "Williams %R out of bounds [0, 100] at bar {}: {}",
                i,
                val
            );
        }
    }
}
