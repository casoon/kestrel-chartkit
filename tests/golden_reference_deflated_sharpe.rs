//! Golden-reference tests for the standard normal quantile and the deflated Sharpe ratio. Values
//! from `reference/kestrel_reference/fixtures/deflated_sharpe.py`.

mod common;

use kestrel_chartkit::{deflated_sharpe, normal_quantile};

const GOLDEN: &str = include_str!("fixtures/golden_deflated_sharpe.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn test_normal_quantile_matches_statistics_inv_cdf() {
    let tol = value("quantile_tolerance");
    for i in 0..value("meta_quantile_case_count") as usize {
        let p = value(&format!("q{i}_p"));
        let expected = value(&format!("q{i}_value"));
        let got = normal_quantile(p).expect("p in (0, 1)");
        assert!(
            (got - expected).abs() <= tol * expected.abs().max(1.0),
            "q{i}: {got} vs {expected}"
        );
    }
    assert_eq!(normal_quantile(0.0), None);
    assert_eq!(normal_quantile(1.0), None);
}

#[test]
fn test_deflated_sharpe_matches_the_paper_formula() {
    let tol = value("dsr_tolerance");
    for i in 0..value("meta_dsr_case_count") as usize {
        let key = |f: &str| value(&format!("dsr{i}_{f}"));
        let returns: Vec<f64> = (0..key("len") as usize)
            .map(|j| key(&format!("v{j}")))
            .collect();
        let got = deflated_sharpe(&returns, key("trials") as usize, key("variance"))
            .unwrap_or_else(|| panic!("dsr{i} computable"));
        for (name, actual) in [
            ("sharpe", got.sharpe),
            ("sr0", got.expected_max_sharpe),
            ("probability", got.probability),
        ] {
            let expected = key(name);
            assert!(
                (actual - expected).abs() <= tol * expected.abs().max(1.0),
                "dsr{i}_{name}: {actual} vs {expected}"
            );
        }
    }
}

/// More trials raise the bar: the same returns are less convincing as the best of 168 than as
/// the only strategy tried.
#[test]
fn test_more_trials_deflate() {
    let r: Vec<f64> = (0..40)
        .map(|i| if i % 3 == 0 { -1.0 } else { 0.8 })
        .collect();
    let one = deflated_sharpe(&r, 1, 0.04).unwrap();
    let many = deflated_sharpe(&r, 168, 0.04).unwrap();
    assert!(many.probability < one.probability);
    assert!(many.expected_max_sharpe > 0.0);
}
