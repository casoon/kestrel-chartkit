//! Reality Check and SPA against `reference/kestrel_reference/fixtures/data_snooping.py`.

mod common;

use kestrel_chartkit::{data_snooping, SnoopingConfig};

const GOLDEN: &str = include_str!("fixtures/golden_data_snooping.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn test_data_snooping_matches_the_reference() {
    let tol = value("data_snooping_tolerance");
    for name in ["clear", "noise", "bad_rules"] {
        let k = |f: &str| value(&format!("{name}_{f}"));
        let (kk, t) = (k("k") as usize, k("t") as usize);
        let series: Vec<Vec<f64>> = (0..kk)
            .map(|j| (0..t).map(|i| k(&format!("k{j}_t{i}"))).collect())
            .collect();
        let r = data_snooping(
            &series,
            SnoopingConfig {
                mean_block: k("mean_block"),
                resamples: k("resamples") as usize,
                seed: k("seed") as u64,
            },
        )
        .expect("valid input");
        assert_eq!(r.best, k("best") as usize, "{name}");
        assert!((r.best_mean - k("best_mean")).abs() <= tol, "{name} mean");
        assert_eq!(r.p_reality_check, k("p_rc"), "{name} RC");
        assert_eq!(r.p_spa, k("p_spa"), "{name} SPA");
    }
    // The clear winner survives, noise does not; SPA is not more conservative than RC when bad
    // rules are among the candidates.
    assert!(value("clear_p_spa") < 0.05);
    assert!(value("noise_p_spa") > 0.1);
    assert!(value("bad_rules_p_spa") <= value("bad_rules_p_rc"));
}
