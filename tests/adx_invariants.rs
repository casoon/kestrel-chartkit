mod common;

use common::*;
use kestrel_chartkit::indicator::adx::Adx;

#[test]
fn test_adx_bounds_invariant() {
    let mut adx = Adx::with_defaults();
    let bars = generate_sine_bars(300, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut adx, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let val = out.value; // ADX line
            assert!(
                (0.0..=100.0).contains(&val),
                "ADX out of bounds [0, 100] at bar {}: {}",
                i,
                val
            );
            if let Some(plus_di) = out.extra.get("plus_di").copied() {
                assert!(
                    (0.0..=100.0).contains(&plus_di),
                    "+DI out of bounds [0, 100] at bar {}: {}",
                    i,
                    plus_di
                );
            }
            if let Some(minus_di) = out.extra.get("minus_di").copied() {
                assert!(
                    (0.0..=100.0).contains(&minus_di),
                    "-DI out of bounds [0, 100] at bar {}: {}",
                    i,
                    minus_di
                );
            }
        }
    }
}
