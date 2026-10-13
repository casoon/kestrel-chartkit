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

/// 60 Bars abwärts bis zum Boden, dann 60 Bars aufwärts: der Oszillator liegt am unteren
/// Skalenrand und kreuzt danach seine Signallinie nach oben.
fn fall_then_rise() -> Vec<kestrel_chartkit::model::Bar> {
    (0..120)
        .map(|i: i64| {
            let p = if i < 60 {
                100.0 - i as f64
            } else {
                40.0 + (i - 60) as f64
            };
            kestrel_chartkit::model::Bar::new(i * 60, p, p + 0.5, p - 0.5, p, 1000.0)
        })
        .collect()
}

fn assert_bull_extreme_strength_finite(name: &str, params: &[(&str, f64)]) {
    let params: std::collections::HashMap<String, f64> =
        params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut indicator =
        kestrel_chartkit::indicator::registry::build_checked(name, &params).unwrap();
    let mut fired = 0;
    for bar in fall_then_rise() {
        indicator.on_bar(&bar);
        for alert in indicator.alerts() {
            assert!(
                (0.0..=1.0).contains(&alert.strength),
                "{name} {}: strength {}",
                alert.kind,
                alert.strength
            );
            if alert.kind == "bull_extreme" {
                fired += 1;
            }
        }
    }
    assert!(fired > 0, "{name}: Testserie muss bull_extreme auslösen");
}

#[test]
fn test_mfi_zero_oversold_gives_finite_alert_strength() {
    assert_bull_extreme_strength_finite("mfi", &[("oversold", 0.0), ("overbought", 100.0)]);
}
