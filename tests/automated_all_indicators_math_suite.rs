mod common;

use common::*;
use kestrel_chartkit::indicator::registry::{build, catalog};

/// AUTOMATED VERIFICATION SUITE: Runs across 100% of registered indicators in catalog()
/// without any manual test writing needed for individual indicators.

#[test]
fn automated_test_1_all_indicators_instantiation_and_catalog() {
    let cat = catalog();
    assert!(
        cat.len() >= 35,
        "Expected at least 35 indicators in catalog, found {}",
        cat.len()
    );

    for entry in &cat {
        let ind = build(entry.name, &entry.default_params);
        assert!(
            ind.is_some(),
            "AUTOMATED CHECK FAILED: Indicator '{}' in catalog could not be built with default params",
            entry.name
        );
        let ind = ind.unwrap();
        assert_eq!(
            ind.name(),
            entry.name,
            "Indicator name mismatch: expected {}, got {}",
            entry.name,
            ind.name()
        );
    }
}

#[test]
fn automated_test_2_all_indicators_determinism_parity() {
    let cat = catalog();
    let bars = generate_sine_bars(150, 100.0, 10.0, 20.0, 5000.0);

    for entry in &cat {
        let mut ind_a = build(entry.name, &entry.default_params).unwrap();
        let mut ind_b = build(entry.name, &entry.default_params).unwrap();

        let mut outputs_a = Vec::new();
        let mut outputs_b = Vec::new();

        for bar in &bars {
            outputs_a.push(ind_a.on_bar(bar));
            outputs_b.push(ind_b.on_bar(bar));
        }

        assert_eq!(
            outputs_a.len(),
            outputs_b.len(),
            "Determinism failure: output lengths differ for {}",
            entry.name
        );

        for (i, (out_a, out_b)) in outputs_a.into_iter().zip(outputs_b).enumerate() {
            match (out_a, out_b) {
                (Some(a), Some(b)) => {
                    assert_eq!(
                        a.value, b.value,
                        "Determinism failure for indicator '{}' at bar {}: {} vs {}",
                        entry.name, i, a.value, b.value
                    );
                    for (k, v_a) in &a.extra {
                        let v_b = b.extra.get(k).expect("Missing extra key in instance b");
                        assert_eq!(
                            v_a, v_b,
                            "Determinism extra key '{}' mismatch for '{}' at bar {}",
                            k, entry.name, i
                        );
                    }
                }
                (None, None) => {}
                _ => panic!(
                    "Determinism failure for indicator '{}' at bar {}: Some/None mismatch",
                    entry.name, i
                ),
            }
        }
    }
}

#[test]
fn automated_test_3_all_indicators_reset_equivalency() {
    let cat = catalog();
    let bars = generate_sine_bars(100, 100.0, 5.0, 10.0, 1000.0);

    for entry in &cat {
        let mut ind = build(entry.name, &entry.default_params).unwrap();

        // Pass 1
        let mut run_1 = Vec::new();
        for bar in &bars {
            run_1.push(ind.on_bar(bar));
        }

        // Reset
        ind.reset();

        // Pass 2
        let mut run_2 = Vec::new();
        for bar in &bars {
            run_2.push(ind.on_bar(bar));
        }

        assert_eq!(
            run_1.len(),
            run_2.len(),
            "Reset failure: run lengths differ for {}",
            entry.name
        );

        for (i, (out_1, out_2)) in run_1.into_iter().zip(run_2).enumerate() {
            match (out_1, out_2) {
                (Some(a), Some(b)) => {
                    assert_eq!(
                        a.value, b.value,
                        "Reset contract failure for '{}' at bar {}: {} vs {}",
                        entry.name, i, a.value, b.value
                    );
                }
                (None, None) => {}
                _ => panic!(
                    "Reset contract failure for '{}' at bar {}: Some/None mismatch after reset()",
                    entry.name, i
                ),
            }
        }
    }
}

#[test]
fn automated_test_4_all_indicators_finite_math_guarantee() {
    let cat = catalog();
    let series_list = [
        generate_sine_bars(200, 100.0, 15.0, 30.0, 10000.0),
        generate_trend_bars(200, 50.0, 2.0, 500.0),
        generate_step_bars(200, 100.0, 100, 20.0, 1000.0),
        generate_flat_spread_bars(200, 100.0, 0.0, 0.0), // Extreme flatline
    ];

    for entry in &cat {
        for (s_idx, bars) in series_list.iter().enumerate() {
            let mut ind = build(entry.name, &entry.default_params).unwrap();

            for (b_idx, bar) in bars.iter().enumerate() {
                if let Some(out) = ind.on_bar(bar) {
                    assert!(
                        out.value.is_finite(),
                        "AUTOMATED MATH FAILURE: Indicator '{}' produced non-finite value {} in series {} at bar {}",
                        entry.name,
                        out.value,
                        s_idx,
                        b_idx
                    );

                    for (k, v) in &out.extra {
                        assert!(
                            v.is_finite(),
                            "AUTOMATED MATH FAILURE: Indicator '{}' produced non-finite extra['{}'] = {} in series {} at bar {}",
                            entry.name,
                            k,
                            v,
                            s_idx,
                            b_idx
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn automated_test_5_all_bounded_indicators_domain_constraints() {
    let cat = catalog();
    let bars = generate_sine_bars(300, 100.0, 25.0, 40.0, 10000.0);

    for entry in &cat {
        let mut ind = build(entry.name, &entry.default_params).unwrap();

        for bar in &bars {
            if let Some(out) = ind.on_bar(bar) {
                match entry.name {
                    "rsi" | "stoch_rsi" | "stochastic" | "mfi" | "williams_r" => {
                        assert!(
                            (0.0..=100.0).contains(&out.value),
                            "AUTOMATED DOMAIN FAILURE: Bounded indicator '{}' value {} outside [0, 100]",
                            entry.name,
                            out.value
                        );
                    }
                    "aroon" => {
                        assert!(
                            (-100.0..=100.0).contains(&out.value),
                            "AUTOMATED DOMAIN FAILURE: Aroon oscillator value {} outside [-100, 100]",
                            out.value
                        );
                    }
                    "cmf" => {
                        assert!(
                            (-1.0..=1.0).contains(&out.value),
                            "AUTOMATED DOMAIN FAILURE: CMF value {} outside [-1, 1]",
                            out.value
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn automated_test_6_all_indicators_high_volume_stream_stress() {
    let cat = catalog();
    let bars = generate_sine_bars(2000, 100.0, 10.0, 50.0, 1000.0);

    for entry in &cat {
        let mut ind = build(entry.name, &entry.default_params).unwrap();
        let mut generated_count = 0;

        for bar in &bars {
            if let Some(out) = ind.on_bar(bar) {
                generated_count += 1;
                assert!(out.value.is_finite());
            }
        }

        assert!(
            generated_count > 1500,
            "Indicator '{}' failed to generate outputs over 2000 bars stream (only produced {})",
            entry.name,
            generated_count
        );
    }
}
