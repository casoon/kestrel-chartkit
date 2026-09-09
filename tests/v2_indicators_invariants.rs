mod common;

use common::*;
use kestrel_chartkit::indicator::registry::{build, catalog};
use kestrel_chartkit::indicator::relative_strength::RelativeStrengthEngine;

#[test]
fn test_all_catalog_v2_indicators_build_and_run() {
    let cat = catalog();
    assert!(
        cat.len() >= 25,
        "Expected at least 25 indicators in catalog"
    );

    let bars = generate_sine_bars(100, 100.0, 10.0, 20.0, 1000.0);

    for entry in &cat {
        let mut ind = build(entry.name, &entry.default_params)
            .unwrap_or_else(|| panic!("Failed to build indicator {}", entry.name));

        let mut output_count = 0;
        for bar in &bars {
            if let Some(out) = ind.on_bar(bar) {
                output_count += 1;
                assert!(
                    out.value.is_finite(),
                    "Indicator {} produced non-finite value {}",
                    entry.name,
                    out.value
                );
            }
        }
        assert!(
            output_count > 0,
            "Indicator {} produced zero outputs over 100 bars",
            entry.name
        );
    }
}

#[test]
fn test_relative_strength_dual_bar_update() {
    let mut rs = RelativeStrengthEngine::new(10);
    let own_bars = generate_trend_bars(30, 100.0, 2.0, 1000.0); // +2 per bar (+100% trend)
    let bench_bars = generate_trend_bars(30, 100.0, 0.5, 1000.0); // +0.5 per bar (+25% trend)

    let mut final_out = None;
    for i in 0..30 {
        final_out = rs.update(&own_bars[i], &bench_bars[i]);
    }

    let out = final_out.expect("Expected RelativeStrength output");
    assert!(
        out.value > 0.0,
        "Asset outperforming benchmark should have positive alpha"
    );
    assert!(out.extra.contains_key("alpha_pct"));
    assert!(out.extra.contains_key("ratio"));
}
