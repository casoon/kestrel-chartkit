//! Golden-reference tests for the L2-regularised logistic regression. Values from
//! `reference/kestrel_reference/fixtures/logistic.py`.

mod common;

use kestrel_chartkit::LogisticRegression;

const GOLDEN: &str = include_str!("fixtures/golden_logistic.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn test_fit_matches_the_independent_newton_solution() {
    let tol = value("logistic_tolerance");
    for i in 0..value("meta_case_count") as usize {
        let key = |f: &str| value(&format!("lr{i}_{f}"));
        let (n, k) = (key("n") as usize, key("k") as usize);
        let x: Vec<Vec<f64>> = (0..n)
            .map(|r| (0..k).map(|c| key(&format!("x{r}_{c}"))).collect())
            .collect();
        let y: Vec<bool> = (0..n).map(|r| key(&format!("y{r}")) == 1.0).collect();
        let m = LogisticRegression::fit(&x, &y, key("l2")).unwrap_or_else(|| panic!("lr{i} fits"));
        let close = |a: f64, b: f64, what: &str| {
            assert!(
                (a - b).abs() <= tol * b.abs().max(1.0),
                "lr{i} {what}: {a} vs {b}"
            )
        };
        close(m.intercept, key("intercept"), "intercept");
        for c in 0..k {
            close(m.weights[c], key(&format!("w{c}")), &format!("w{c}"));
        }
        for (r, row) in x.iter().enumerate() {
            close(m.predict(row), key(&format!("p{r}")), &format!("p{r}"));
        }
    }
}
