//! Golden-reference tests for the two-series indicators (`relative_strength`, `correlation`) and
//! the pairing of two bar series by timestamp. Values from
//! `reference/kestrel_reference/fixtures/pair_indicators.py` (exact rational arithmetic, plus the
//! two worked examples by hand).

mod common;

use std::collections::HashMap;

use kestrel_chartkit::cross_asset::{correlation_matrix, CloseSample};
use kestrel_chartkit::indicator::pair::{build_pair_checked, run_pair_batch, WithReference};
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::runner::{run_batch, TimestampedOutput};

const GOLDEN: &str = include_str!("fixtures/golden_pair_indicators.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|line| line.split_once('=').is_some_and(|(k, _)| k == key))
}

fn near(got: f64, key: &str) {
    let want = value(key);
    let tolerance = value("pair_indicators_tolerance");
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{key}: got {got}, reference {want}"
    );
}

/// The bars of one side of a case; open, high and low repeat the close.
fn bars(case: &str, side: &str) -> Vec<Bar> {
    let n = value(&format!("{case}_{side}_n")) as usize;
    (0..n)
        .map(|j| {
            let close = value(&format!("{case}_{side}_close{j}"));
            let timestamp = value(&format!("{case}_{side}_ts{j}")) as i64;
            Bar::new(timestamp, close, close, close, close, 0.0)
        })
        .collect()
}

fn run(name: &str, case: &str) -> (Vec<Bar>, Vec<Bar>, Vec<TimestampedOutput>) {
    let params = HashMap::from([("len".to_string(), value(&format!("{case}_len")))]);
    let mut indicator = build_pair_checked(name, &params).unwrap();
    let (series, reference) = (bars(case, "a"), bars(case, "b"));
    let out = run_pair_batch(indicator.as_mut(), &series, &reference);
    assert_eq!(out.len(), series.len(), "{case}: one entry per bar");
    for (entry, bar) in out.iter().zip(&series) {
        assert_eq!(entry.timestamp, bar.timestamp, "{case}");
    }
    (series, reference, out)
}

#[test]
fn test_relative_strength_matches_the_reference() {
    let cases = value("meta_rs_cases") as usize;
    assert!(cases >= 6);
    let mut outputs = 0;
    for i in 0..cases {
        let case = format!("rs{i}");
        let (_, _, out) = run("relative_strength", &case);
        for (j, entry) in out.iter().enumerate() {
            let expected = value(&format!("{case}_out{j}")) == 1.0;
            assert_eq!(entry.output.is_some(), expected, "{case} bar {j}");
            let Some(output) = &entry.output else {
                continue;
            };
            outputs += 1;
            near(output.value, &format!("{case}_alpha{j}"));
            near(output.extra["alpha_pct"], &format!("{case}_alpha{j}"));
            near(output.extra["ratio"], &format!("{case}_ratio{j}"));
            near(output.extra["own_perf_pct"], &format!("{case}_own{j}"));
            near(output.extra["bench_perf_pct"], &format!("{case}_bench{j}"));
            assert_eq!(output.extra.len(), 4, "{case} bar {j}");
        }
    }
    assert!(outputs > 30);
}

/// The worked example of the documentation: over two bars the series rises from 100 to 110, the
/// reference falls from 50 to 44.
#[test]
fn test_relative_strength_worked_example() {
    let (_, _, out) = run("relative_strength", "rs0");
    assert!(out[0].output.is_none() && out[1].output.is_none());
    let output = out[2].output.as_ref().unwrap();
    assert!((output.value - 22.0).abs() < 1e-9);
    assert!((output.extra["ratio"] - 2.5).abs() < 1e-12);
    assert!((output.extra["own_perf_pct"] - 10.0).abs() < 1e-9);
    assert!((output.extra["bench_perf_pct"] + 12.0).abs() < 1e-9);
}

#[test]
fn test_correlation_matches_the_reference() {
    let cases = value("meta_corr_cases") as usize;
    assert!(cases >= 9);
    let mut outputs = 0;
    for i in 0..cases {
        let case = format!("corr{i}");
        let (_, _, out) = run("correlation", &case);
        for (j, entry) in out.iter().enumerate() {
            let expected = value(&format!("{case}_out{j}")) == 1.0;
            assert_eq!(entry.output.is_some(), expected, "{case} bar {j}");
            let Some(output) = &entry.output else {
                continue;
            };
            outputs += 1;
            near(output.value, &format!("{case}_value{j}"));
            assert!((-1.0..=1.0).contains(&output.value), "{case} bar {j}");
            assert!(output.extra.is_empty(), "{case} bar {j}");
        }
    }
    assert!(outputs > 40);
}

/// The worked example: returns of 1 %, 2 %, 3 % against 2 %, 1 %, 3 % correlate with 0.5.
#[test]
fn test_correlation_worked_example() {
    let (_, _, out) = run("correlation", "corr0");
    assert!(out[..3].iter().all(|entry| entry.output.is_none()));
    assert!((out[3].output.as_ref().unwrap().value - 0.5).abs() < 1e-9);
}

/// A bar whose timestamp the reference lacks is skipped: it has no output, and the windows run
/// over the paired bars only. The reference fixture holds the expected values; here the shape.
#[test]
fn test_bars_without_a_partner_are_skipped_and_do_not_fill_a_window() {
    for (name, case) in [("relative_strength", "rs3"), ("correlation", "corr3")] {
        let (series, reference, out) = run(name, case);
        let stamps: Vec<i64> = reference.iter().map(|bar| bar.timestamp).collect();
        assert!(!stamps.contains(&4) && !stamps.contains(&9), "{case}");
        assert!(stamps.contains(&16) && stamps.contains(&17), "{case}");
        assert_eq!(stamps.iter().filter(|t| **t == 6).count(), 2, "{case}");
        assert!(out[4].output.is_none() && out[9].output.is_none(), "{case}");

        // The same run on the paired bars alone gives the same outputs in the same order.
        let paired: Vec<Bar> = series
            .iter()
            .filter(|bar| stamps.contains(&bar.timestamp))
            .cloned()
            .collect();
        let params = HashMap::from([("len".to_string(), value(&format!("{case}_len")))]);
        let mut indicator = build_pair_checked(name, &params).unwrap();
        let dense = run_pair_batch(indicator.as_mut(), &paired, &reference);
        let with_partner: Vec<&TimestampedOutput> = out
            .iter()
            .filter(|entry| stamps.contains(&entry.timestamp))
            .collect();
        assert_eq!(dense.iter().collect::<Vec<_>>(), with_partner, "{case}");
    }
    // The reference's first sample at timestamp 6 (60.0) is replaced by the later one.
    assert!(has("rs3_ratio6"));
    let (series, reference, out) = run("relative_strength", "rs3");
    let later = reference.iter().rfind(|bar| bar.timestamp == 6).unwrap();
    let ratio = out[6].output.as_ref().unwrap().extra["ratio"];
    assert!((ratio - series[6].close / later.close).abs() < 1e-12);
}

/// The streamed value is `correlation_matrix` of the bars up to that point: the same pairing, the
/// same returns, the same window.
#[test]
fn test_correlation_equals_correlation_matrix_on_every_prefix() {
    for i in 0..value("meta_corr_cases") as usize {
        let case = format!("corr{i}");
        let len = value(&format!("{case}_len")) as usize;
        let (series, reference, out) = run("correlation", &case);
        let samples = |bars: &[Bar], until: i64| -> Vec<CloseSample> {
            bars.iter()
                .filter(|bar| bar.timestamp <= until)
                .map(|bar| CloseSample {
                    timestamp: bar.timestamp,
                    close: bar.close,
                })
                .collect()
        };
        for entry in &out {
            let Some(output) = &entry.output else {
                continue;
            };
            let matrix = correlation_matrix(
                &[
                    ("a".to_string(), samples(&series, entry.timestamp)),
                    ("b".to_string(), samples(&reference, entry.timestamp)),
                ],
                len,
            );
            assert_eq!(matrix.len(), 1, "{case} at {}", entry.timestamp);
            assert_eq!(
                matrix[0].corr, output.value,
                "{case} at {}",
                entry.timestamp
            );
        }
    }
}

/// Bound to the reference, the indicator runs through the one-series runner with the same result.
#[test]
fn test_with_reference_reproduces_the_fixture_through_run_batch() {
    for (name, case) in [("relative_strength", "rs3"), ("correlation", "corr3")] {
        let (series, reference, direct) = run(name, case);
        let params = HashMap::from([("len".to_string(), value(&format!("{case}_len")))]);
        let mut bound = WithReference::new(build_pair_checked(name, &params).unwrap(), &reference);
        assert_eq!(run_batch(&mut bound, &series), direct, "{case}");
    }
}
