//! Golden-reference tests for the sample statistics used in trade evaluation: the Wilson score
//! interval and the longest run. Values from `reference/kestrel_reference/fixtures/sample_stats.py`.

mod common;

use kestrel_chartkit::{
    block_difference, block_mean, longest_run, nearest_rank_quantile, wilson_interval,
};

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

#[test]
fn test_nearest_rank_quantile_matches_the_reference() {
    let cases = value("meta_quantile_case_count") as usize;
    assert!(cases > 0);
    for i in 0..cases {
        let key = |name: &str| value(&format!("q{i}_{name}"));
        let values: Vec<f64> = (0..key("n") as usize)
            .map(|j| key(&format!("v{j}")))
            .collect();
        assert_eq!(
            nearest_rank_quantile(&values, key("q")),
            Some(key("result")),
            "q{i}"
        );
    }
    assert_eq!(nearest_rank_quantile(&[], 0.5), None);
}

fn series(name: &str) -> Vec<(i64, f64)> {
    let key = |f: &str| value(&format!("block_{name}_{f}"));
    (0..key("n") as usize)
        .map(|j| (key(&format!("t{j}")) as i64, key(&format!("x{j}"))))
        .collect()
}

#[test]
fn test_block_statistics_match_the_reference() {
    const DAY: i64 = 86_400_000;
    for name in ["a", "b", "single"] {
        let key = |f: &str| value(&format!("block_{name}_{f}"));
        let got = block_mean(&series(name), DAY).expect("values");
        assert!((got.mean - key("mean")).abs() <= tolerance(), "{name} mean");
        let se = got.std_error.expect("two or more blocks");
        assert!((se - key("se")).abs() <= tolerance(), "{name} se");
        assert_eq!(got.blocks, key("blocks") as usize, "{name} blocks");
    }
    // One value per block: the clustered error is the textbook s/sqrt(n).
    let single = block_mean(&series("single"), DAY).expect("values");
    assert!(
        (single.std_error.expect("se") - value("block_single_textbook_se")).abs() <= tolerance()
    );
    let (diff, se) = block_difference(&series("a"), &series("b"), DAY).expect("both series");
    assert!((diff - value("block_diff_value")).abs() <= tolerance());
    assert!((se.expect("se") - value("block_diff_se")).abs() <= tolerance());
    assert!(block_mean(&[], DAY).is_none());
    assert_eq!(
        block_mean(&[(0, 1.0), (1, 2.0)], DAY).unwrap().std_error,
        None
    );
}
