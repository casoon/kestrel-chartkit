mod common;

use common::*;
use kestrel_chartkit::indicator::registry::{build, catalog};

#[test]
fn test_all_indicators_short_series_no_panic() {
    let entries = catalog();
    for entry in &entries {
        let mut indicator = build(entry.name, &entry.default_params)
            .unwrap_or_else(|| panic!("Failed to build indicator {}", entry.name));

        let short_bars = generate_flat_spread_bars(5, 100.0, 1.0, 1000.0);
        assert_no_panic(indicator.as_mut(), &short_bars);
    }
}

#[test]
fn test_all_indicators_flatline_data_no_panic() {
    let entries = catalog();
    for entry in &entries {
        let mut indicator = build(entry.name, &entry.default_params)
            .unwrap_or_else(|| panic!("Failed to build indicator {}", entry.name));

        let flat_bars = generate_flat_spread_bars(200, 100.0, 0.0, 1000.0);
        assert_no_panic(indicator.as_mut(), &flat_bars);
    }
}

#[test]
fn test_all_indicators_zero_volume_no_panic() {
    let entries = catalog();
    for entry in &entries {
        let mut indicator = build(entry.name, &entry.default_params)
            .unwrap_or_else(|| panic!("Failed to build indicator {}", entry.name));

        let zero_vol_bars = generate_zero_volume_bars(200, 100.0);
        assert_no_panic(indicator.as_mut(), &zero_vol_bars);
    }
}

#[test]
fn test_all_indicators_nan_robustness_no_panic() {
    let entries = catalog();
    for entry in &entries {
        let mut indicator = build(entry.name, &entry.default_params)
            .unwrap_or_else(|| panic!("Failed to build indicator {}", entry.name));

        let nan_bars = generate_nan_bars(100);
        assert_no_panic(indicator.as_mut(), &nan_bars);
    }
}

#[test]
fn test_all_indicators_warmup_contract() {
    let entries = catalog();
    for entry in &entries {
        let mut indicator = build(entry.name, &entry.default_params)
            .unwrap_or_else(|| panic!("Failed to build indicator {}", entry.name));

        let warmup = indicator.warmup_period();
        let bars = generate_sine_bars(warmup + 100, 100.0, 10.0, 20.0, 1000.0);

        let outputs = run_indicator(indicator.as_mut(), &bars);

        // After warmup period, output should be Some(IndicatorOutput)
        if warmup < outputs.len() {
            let post_warmup_outputs = &outputs[warmup..];
            for (idx, out) in post_warmup_outputs.iter().enumerate() {
                assert!(
                    out.is_some(),
                    "Indicator {} produced None at bar {} after warmup period {}",
                    entry.name,
                    warmup + idx,
                    warmup
                );
            }
        }
    }
}
