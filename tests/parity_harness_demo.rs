//! Demonstrates `kestrel_chartkit::parity` as an external consumer would use it: a standardized,
//! reusable fixture-comparison harness (timestamp alignment, warmup handling, tolerance, explicit
//! missing values, MTF-boundary mode), as opposed to hand-rolling per-indicator comparison code
//! like the `tests/golden_reference_*.rs` group does. Not a replacement for those tests — a
//! demonstration that the harness genuinely works end-to-end against a real indicator.

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::parity::{compare_series_at_timeframe_boundaries, ParityFixture};
use kestrel_chartkit::runner::run_batch;
use kestrel_chartkit::timeframe::Timeframe;
use kestrel_chartkit::Bar;
use std::collections::HashMap;

#[test]
fn test_parity_harness_validates_sma_against_a_reference_fixture() {
    let bars: Vec<Bar> = (0..10)
        .map(|i| {
            let price = 100.0 + i as f64;
            Bar::new(i as i64 * 60, price, price + 1.0, price - 1.0, price, 100.0)
        })
        .collect();

    let mut sma = build_checked("sma", &HashMap::from([("period".to_string(), 3.0)])).unwrap();
    let series = run_batch(&mut sma, &bars);

    // A hand-computed reference: SMA(3) of [100..109] confirms starting at bar index 2 (t=120).
    // Bar 5 (t=300) is deliberately marked `nan` (no Pine reference available there), and bar 9
    // (t=540) is intentionally omitted from the fixture entirely.
    let fixture_text = "\
        # SMA(3) reference values\n\
        0,nan\n\
        60,nan\n\
        120,101.0\n\
        180,102.0\n\
        240,103.0\n\
        300,nan\n\
        360,105.0\n\
        420,106.0\n\
        480,107.0\n\
    ";
    let fixture = ParityFixture::parse(fixture_text, 1e-9).unwrap();

    let report = kestrel_chartkit::parity::compare_series(&series, &fixture, |o| o.value);
    assert!(report.all_passed(), "mismatches: {:?}", report.mismatches());
    // 7 confirmed rows minus the 1 explicit `nan` row = 6 real matches.
    assert_eq!(report.matched_count(), 6);
}

#[test]
fn test_parity_harness_mtf_boundary_mode_against_a_resampled_series() {
    use kestrel_chartkit::timeframe::BarResampler;

    let bars: Vec<Bar> = (0..10)
        .map(|i| Bar::new(i as i64 * 60, 100.0, 101.0, 99.0, 100.0 + i as f64, 10.0))
        .collect();

    let mut resampler = BarResampler::new(Timeframe::Minute(5)).unwrap();
    let mut completed: Vec<(i64, f64)> = Vec::new();
    for bar in &bars {
        if let Some(c) = resampler.on_bar(bar).completed_bar {
            completed.push((c.timestamp, c.close));
        }
    }
    assert_eq!(
        completed.len(),
        1,
        "10 one-minute bars close exactly one 5-minute bucket"
    );

    let series: Vec<kestrel_chartkit::runner::TimestampedOutput> = completed
        .iter()
        .map(|&(ts, close)| kestrel_chartkit::runner::TimestampedOutput {
            timestamp: ts,
            output: Some(kestrel_chartkit::IndicatorOutput::new(close)),
        })
        .collect();

    // A fixture row at a non-boundary timestamp must be skipped, not treated as a failure.
    let fixture = ParityFixture::parse("30,999.0\n0,104.0", 1e-9).unwrap();
    let report = compare_series_at_timeframe_boundaries(
        &series,
        &fixture,
        |o| o.value,
        Timeframe::Minute(5),
        0,
    );

    assert!(report.all_passed(), "mismatches: {:?}", report.mismatches());
    assert_eq!(report.matched_count(), 1);
}
