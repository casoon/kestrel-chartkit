//! Golden-reference tests for the CUSUM against `reference/kestrel_reference/fixtures/changepoint.py`
//! (closed-form suffix sums in exact rational arithmetic, plus the two worked examples by hand).

mod common;

use kestrel_chartkit::{cusum, CusumState};

const GOLDEN: &str = include_str!("fixtures/golden_changepoint.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

struct Case {
    x: Vec<f64>,
    mu0: f64,
    k: f64,
    h: f64,
}

fn case(i: usize) -> Case {
    let key = |name: &str| value(&format!("cusum{i}_{name}"));
    let n = key("n") as usize;
    Case {
        x: (0..n).map(|j| key(&format!("x{j}"))).collect(),
        mu0: key("mu0"),
        k: key("k"),
        h: key("h"),
    }
}

fn alarm(raw: f64) -> Option<usize> {
    (raw >= 0.0).then_some(raw as usize)
}

#[test]
fn test_cusum_matches_the_reference_sums_and_alarms() {
    let cases = value("meta_case_count") as usize;
    assert!(cases >= 5);
    let tol = value("changepoint_tolerance");
    for i in 0..cases {
        let c = case(i);
        let got = cusum(&c.x, c.mu0, c.k, c.h);
        assert_eq!(got.upper.len(), c.x.len());
        assert_eq!(got.lower.len(), c.x.len());
        for j in 0..c.x.len() {
            let up = value(&format!("cusum{i}_upper{j}"));
            let lo = value(&format!("cusum{i}_lower{j}"));
            assert!((got.upper[j] - up).abs() <= tol, "case {i} upper[{j}]");
            assert!((got.lower[j] - lo).abs() <= tol, "case {i} lower[{j}]");
        }
        assert_eq!(
            got.first_alarm_upper,
            alarm(value(&format!("cusum{i}_alarm_upper"))),
            "case {i} upper alarm"
        );
        assert_eq!(
            got.first_alarm_lower,
            alarm(value(&format!("cusum{i}_alarm_lower"))),
            "case {i} lower alarm"
        );
    }
}

/// The documented worked example: S reaches exactly 4.0 at the tenth value, which is not above
/// h = 4, so the alarm comes one value later.
#[test]
fn test_cusum_strict_threshold_at_the_worked_example() {
    let c = case(0);
    let got = cusum(&c.x, c.mu0, c.k, c.h);
    assert_eq!(got.first_alarm_upper, Some(10));
    assert_eq!(got.first_alarm_lower, None);
    assert!((got.upper[9] - 4.0).abs() < 1e-12);
}

/// A single outlier below k + h neither alarms nor leaves a trace; the sum decays to 0.
#[test]
fn test_cusum_outlier_below_k_plus_h_does_not_alarm() {
    let c = case(1);
    let got = cusum(&c.x, c.mu0, c.k, c.h);
    let peak = got.upper.iter().copied().fold(0.0, f64::max);
    assert!((peak - 3.8).abs() < 1e-12);
    assert_eq!(got.first_alarm_upper, None);
    assert_eq!(*got.upper.last().unwrap(), 0.0);
}

#[test]
fn test_cusum_lower_side_mirrors_the_upper_side() {
    let up = cusum(&case(0).x, 0.0, 0.5, 4.0);
    let down = cusum(&case(2).x, 0.0, 0.5, 4.0);
    assert_eq!(up.first_alarm_upper, down.first_alarm_lower);
    for (a, b) in up.upper.iter().zip(&down.lower) {
        assert!((a - b).abs() < 1e-12);
    }
    for (a, b) in up.lower.iter().zip(&down.upper) {
        assert!((a - b).abs() < 1e-12);
    }
}

#[test]
fn test_cusum_sum_equal_to_the_threshold_does_not_alarm() {
    let c = case(4);
    let got = cusum(&c.x, c.mu0, c.k, c.h);
    assert_eq!(got.upper, vec![1.0, 2.0, 3.0]);
    assert_eq!(got.first_alarm_upper, None);
}

#[test]
fn test_cusum_non_finite_values_leave_the_sums_unchanged() {
    let got = cusum(&[1.5, f64::NAN, 1.5, f64::INFINITY], 0.0, 0.5, 1.5);
    assert_eq!(got.upper, vec![1.0, 1.0, 2.0, 2.0]);
    assert_eq!(got.first_alarm_upper, Some(2));
    let mut state = CusumState::default();
    state.step(f64::NAN, 0.0, 0.5);
    assert_eq!(state, CusumState::default());
}

#[test]
fn test_cusum_of_nothing_is_empty() {
    let got = cusum(&[], 0.0, 0.5, 4.0);
    assert!(got.upper.is_empty() && got.lower.is_empty());
    assert_eq!((got.first_alarm_upper, got.first_alarm_lower), (None, None));
}
