//! Golden-reference tests for the two-series indicators `rolling_beta` and `pair_spread`. Values
//! from `reference/kestrel_reference/fixtures/pair_regression.py` (exact rational arithmetic, plus
//! the two worked examples by hand).

mod common;

use std::collections::HashMap;

use kestrel_chartkit::cross_asset::{compute_pair_spread, compute_rolling_beta};
use kestrel_chartkit::indicator::pair::{build_pair_checked, run_pair_batch, WithReference};
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::runner::{run_batch, TimestampedOutput};

const GOLDEN: &str = include_str!("fixtures/golden_pair_regression.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn near(got: f64, key: &str) {
    let want = value(key);
    let tolerance = value("pair_regression_tolerance");
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
    (series, reference, out)
}

#[test]
fn test_rolling_beta_matches_the_reference() {
    let cases = value("meta_rb_cases") as usize;
    assert!(cases >= 7);
    let mut outputs = 0;
    for i in 0..cases {
        let case = format!("rb{i}");
        let (_, _, out) = run("rolling_beta", &case);
        for (j, entry) in out.iter().enumerate() {
            let expected = value(&format!("{case}_out{j}")) == 1.0;
            assert_eq!(entry.output.is_some(), expected, "{case} bar {j}");
            let Some(output) = &entry.output else {
                continue;
            };
            outputs += 1;
            near(output.value, &format!("{case}_beta{j}"));
            near(output.extra["alpha"], &format!("{case}_alpha{j}"));
            near(output.extra["correlation"], &format!("{case}_corr{j}"));
            near(output.extra["r_squared"], &format!("{case}_r2{j}"));
            assert_eq!(output.extra.len(), 3, "{case} bar {j}");
        }
    }
    assert!(outputs > 25);
}

/// The worked example: returns of 2 %, 4 %, 6 % against 1 %, 2 %, 3 % give beta 2, alpha 0 and a
/// perfect fit.
#[test]
fn test_rolling_beta_worked_example() {
    let (_, _, out) = run("rolling_beta", "rb0");
    assert!(out[..3].iter().all(|entry| entry.output.is_none()));
    let output = out[3].output.as_ref().unwrap();
    assert!((output.value - 2.0).abs() < 1e-9);
    assert!(output.extra["alpha"].abs() < 1e-9);
    assert!((output.extra["r_squared"] - 1.0).abs() < 1e-9);
}

#[test]
fn test_pair_spread_matches_the_reference() {
    let cases = value("meta_ps_cases") as usize;
    assert!(cases >= 7);
    let mut outputs = 0;
    for i in 0..cases {
        let case = format!("ps{i}");
        let (_, _, out) = run("pair_spread", &case);
        for (j, entry) in out.iter().enumerate() {
            let expected = value(&format!("{case}_out{j}")) == 1.0;
            assert_eq!(entry.output.is_some(), expected, "{case} bar {j}");
            let Some(output) = &entry.output else {
                continue;
            };
            outputs += 1;
            near(output.value, &format!("{case}_z{j}"));
            near(output.extra["hedge_ratio"], &format!("{case}_hedge{j}"));
            near(output.extra["spread"], &format!("{case}_spread{j}"));
            near(output.extra["mean_spread"], &format!("{case}_mean{j}"));
            near(output.extra["std_spread"], &format!("{case}_std{j}"));
            assert_eq!(output.extra.len(), 4, "{case} bar {j}");
        }
    }
    assert!(outputs > 25);
}

/// The worked example: series 1, 3, 2 against 1, 2, 3 have hedge ratio 0.5 and the last spread 0.5
/// stands one deviation of sqrt(0.5) below the mean 1.
#[test]
fn test_pair_spread_worked_example() {
    let (_, _, out) = run("pair_spread", "ps0");
    assert!(out[..2].iter().all(|entry| entry.output.is_none()));
    let output = out[2].output.as_ref().unwrap();
    assert!((output.extra["hedge_ratio"] - 0.5).abs() < 1e-12);
    assert!((output.value + 0.5_f64.sqrt()).abs() < 1e-12);
}

/// A series that does not move has hedge ratio 0 and equal spreads: the z-score is 0, not absent.
#[test]
fn test_pair_spread_of_a_constant_series_is_zero() {
    let (_, _, out) = run("pair_spread", "ps6");
    let output = out[3].output.as_ref().unwrap();
    assert_eq!(output.value, 0.0);
    assert_eq!(output.extra["hedge_ratio"], 0.0);
}

/// A bar whose timestamp the reference lacks is skipped; the windows run over the paired bars
/// only, so the same run on the paired bars alone gives the same outputs in the same order.
#[test]
fn test_bars_without_a_partner_are_skipped_and_do_not_fill_a_window() {
    for (name, case) in [("rolling_beta", "rb3"), ("pair_spread", "ps4")] {
        let (series, reference, out) = run(name, case);
        let stamps: Vec<i64> = reference.iter().map(|bar| bar.timestamp).collect();
        assert!(!stamps.contains(&4) && !stamps.contains(&9), "{case}");
        assert!(out[4].output.is_none() && out[9].output.is_none(), "{case}");

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
}

/// The streamed value is the snapshot helper on the window ending at that bar: the pair spread on
/// the last `len` closes, the beta on the last `len` returns.
#[test]
fn test_streamed_values_equal_the_snapshot_helpers_on_every_window() {
    let len = value("ps2_len") as usize;
    let (series, reference, out) = run("pair_spread", "ps2");
    for (j, entry) in out.iter().enumerate() {
        let Some(output) = &entry.output else {
            continue;
        };
        let own: Vec<f64> = series[j + 1 - len..=j].iter().map(|b| b.close).collect();
        let other: Vec<f64> = reference[j + 1 - len..=j].iter().map(|b| b.close).collect();
        let snapshot = compute_pair_spread(&own, &other).unwrap();
        assert_eq!(output.value, snapshot.residual_z_score);
        assert_eq!(output.extra["hedge_ratio"], snapshot.hedge_ratio);
    }

    let len = value("rb1_len") as usize;
    let (series, reference, out) = run("rolling_beta", "rb1");
    let returns = |bars: &[Bar]| -> Vec<f64> {
        bars.windows(2)
            .map(|w| (w[1].close - w[0].close) / w[0].close)
            .collect()
    };
    let (own, other) = (returns(&series), returns(&reference));
    for (j, entry) in out.iter().enumerate() {
        let Some(output) = &entry.output else {
            continue;
        };
        // The return of bar j is `own[j - 1]`; the window ends there.
        let fit = compute_rolling_beta(&own[j - len..j], &other[j - len..j]).unwrap();
        assert_eq!(output.value, fit.beta);
        assert_eq!(output.extra["alpha"], fit.alpha);
    }
}

/// Bound to the reference, the indicator runs through the one-series runner with the same result.
#[test]
fn test_with_reference_reproduces_the_fixture_through_run_batch() {
    for (name, case) in [("rolling_beta", "rb3"), ("pair_spread", "ps4")] {
        let (series, reference, direct) = run(name, case);
        let params = HashMap::from([("len".to_string(), value(&format!("{case}_len")))]);
        let mut bound = WithReference::new(build_pair_checked(name, &params).unwrap(), &reference);
        assert_eq!(run_batch(&mut bound, &series), direct, "{case}");
    }
}

/// Resetting returns an indicator to its initial state.
#[test]
fn test_reset_restarts_the_window() {
    for (name, case) in [("rolling_beta", "rb1"), ("pair_spread", "ps2")] {
        let (series, reference, first) = run(name, case);
        let params = HashMap::from([("len".to_string(), value(&format!("{case}_len")))]);
        let mut indicator = build_pair_checked(name, &params).unwrap();
        let _ = run_pair_batch(indicator.as_mut(), &series, &reference);
        indicator.reset();
        let again = run_pair_batch(indicator.as_mut(), &series, &reference);
        assert_eq!(again, first, "{name}");
    }
}
