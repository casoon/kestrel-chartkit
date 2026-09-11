//! Golden-reference tests for the sample statistics used in trade evaluation: the Wilson score
//! interval and the longest run. Values from `reference/kestrel_reference/fixtures/sample_stats.py`.

mod common;

use kestrel_chartkit::{longest_run, wilson_interval};

const GOLDEN: &str = include_str!("fixtures/golden_sample_stats.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn tolerance() -> f64 {
    value("sample_stats_tolerance")
}

#[test]
fn test_wilson_interval_matches_the_quadratic_roots() {
    let cases = value("meta_wilson_case_count") as usize;
    assert!(cases > 0);
    for i in 0..cases {
        let key = |name: &str| value(&format!("wilson{i}_{name}"));
        let got = wilson_interval(key("successes") as usize, key("trials") as usize, key("z"))
            .unwrap_or_else(|| panic!("case {i} must produce an interval"));
        for (field, actual) in [
            ("estimate", got.estimate),
            ("lower", got.lower),
            ("upper", got.upper),
        ] {
            let expected = key(field);
            assert!(
                (actual - expected).abs() <= tolerance(),
                "wilson{i}_{field}: {actual} vs {expected}"
            );
        }
    }
}

/// Where Wald has zero width — no wins, all wins — Wilson must not: the whole point of the
/// choice. Checked against the fixture's own Wald values rather than asserted in prose.
#[test]
fn test_wilson_keeps_width_where_wald_collapses() {
    let cases = value("meta_wilson_case_count") as usize;
    let mut collapsed = 0;
    for i in 0..cases {
        let key = |name: &str| value(&format!("wilson{i}_{name}"));
        if key("wald_upper") - key("wald_lower") == 0.0 {
            collapsed += 1;
            assert!(
                key("upper") - key("lower") > 0.1,
                "wilson{i} keeps a real width"
            );
        }
    }
    assert!(
        collapsed >= 2,
        "fixture must contain the p = 0 and p = 1 cases"
    );
}

#[test]
fn test_wilson_rejects_what_has_no_interval() {
    assert!(wilson_interval(0, 0, 1.96).is_none());
    assert!(wilson_interval(5, 4, 1.96).is_none());
    assert!(wilson_interval(1, 2, 0.0).is_none());
    assert!(wilson_interval(1, 2, f64::NAN).is_none());
}

#[test]
fn test_longest_negative_run_matches_the_reference() {
    let cases = value("meta_run_case_count") as usize;
    assert!(cases > 0);
    for i in 0..cases {
        let len = value(&format!("run{i}_len")) as usize;
        let values: Vec<f64> = (0..len).map(|j| value(&format!("run{i}_v{j}"))).collect();
        let expected = value(&format!("run{i}_longest_negative")) as usize;
        assert_eq!(longest_run(&values, |v| *v < 0.0), expected, "run{i}");
    }
}
