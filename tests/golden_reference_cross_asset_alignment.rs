//! Golden-reference tests for aligning several close series on their shared timestamps: the
//! alignment itself, aligned percentage returns and market breadth over aligned closes. Values
//! from `reference/kestrel_reference/fixtures/cross_asset_alignment.py`.

mod common;

use kestrel_chartkit::cross_asset::{
    align_closes, aligned_returns, market_breadth_from_closes, CloseSample,
};

const GOLDEN: &str = include_str!("fixtures/golden_cross_asset_alignment.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn tolerance() -> f64 {
    value("cross_asset_alignment_tolerance")
}

fn case_count() -> usize {
    let cases = value("meta_case_count") as usize;
    assert!(cases > 0);
    cases
}

fn series(i: usize) -> Vec<(String, Vec<CloseSample>)> {
    let key = |name: String| value(&format!("case{i}_{name}"));
    (0..key("series".into()) as usize)
        .map(|k| {
            let samples = (0..key(format!("s{k}_len")) as usize)
                .map(|j| CloseSample {
                    timestamp: key(format!("s{k}_ts{j}")) as i64,
                    close: key(format!("s{k}_close{j}")),
                })
                .collect();
            (format!("s{k}"), samples)
        })
        .collect()
}

#[test]
fn test_align_closes_keeps_only_shared_timestamps() {
    for i in 0..case_count() {
        let input = series(i);
        let aligned = align_closes(&input);
        let shared = value(&format!("case{i}_shared_count")) as usize;
        let expected: Vec<i64> = (0..shared)
            .map(|j| value(&format!("case{i}_shared{j}")) as i64)
            .collect();
        assert_eq!(aligned.timestamps, expected, "case{i}");
        assert_eq!(aligned.closes.len(), input.len(), "case{i}");
        for (name, closes) in &aligned.closes {
            assert_eq!(closes.len(), shared, "case{i} {name}");
        }
    }
}

#[test]
fn test_aligned_returns_match_the_reference() {
    for i in 0..case_count() {
        let lookback = value(&format!("case{i}_lookback")) as usize;
        let got = aligned_returns(&series(i), lookback);
        let count = value(&format!("case{i}_return_count")) as usize;
        let stamps: Vec<i64> = (0..count)
            .map(|j| value(&format!("case{i}_return_ts{j}")) as i64)
            .collect();
        assert_eq!(got.timestamps, stamps, "case{i}");
        for (k, (_, returns)) in got.returns.iter().enumerate() {
            assert_eq!(returns.len(), count, "case{i} s{k}");
            for (j, actual) in returns.iter().enumerate() {
                common::assert_close(
                    *actual,
                    value(&format!("case{i}_s{k}_return{j}")),
                    tolerance(),
                    &format!("case{i}_s{k}_return{j}"),
                );
            }
        }
    }
}

#[test]
fn test_market_breadth_from_closes_matches_the_reference() {
    let mut defined = 0;
    for i in 0..case_count() {
        let lookback = value(&format!("case{i}_lookback")) as usize;
        let got = market_breadth_from_closes(&series(i), lookback);
        if value(&format!("case{i}_breadth")) == 0.0 {
            assert!(got.is_none(), "case{i} has no breadth");
            continue;
        }
        defined += 1;
        let got = got.unwrap_or_else(|| panic!("case{i} must produce breadth"));
        let key = |name: &str| value(&format!("case{i}_{name}"));
        assert_eq!(got.total_active, key("total") as usize, "case{i}");
        assert_eq!(got.advancing, key("advancing") as usize, "case{i}");
        assert_eq!(got.declining, key("declining") as usize, "case{i}");
        assert_eq!(got.unchanged, key("unchanged") as usize, "case{i}");
        for (field, actual) in [
            ("ad_ratio", got.advance_decline_ratio),
            ("net_pct", got.net_advancing_pct),
            (
                "above_ma",
                got.pct_above_ma.expect("every member has a reference"),
            ),
        ] {
            common::assert_close(actual, key(field), tolerance(), &format!("case{i}_{field}"));
        }
    }
    assert!(
        defined >= 2,
        "fixture must contain cases with defined breadth"
    );
}

/// The point of aligning: a bar missing in one series must not shift the others. Measured on
/// unaligned closes, B's gap at timestamp 3 would pair A's change 2→3 with B's change 2→4.
#[test]
fn test_a_missing_bar_drops_the_period_for_every_series() {
    let input = series(0);
    let got = aligned_returns(&input, usize::MAX);
    assert!(
        !got.timestamps.contains(&3),
        "timestamp 3 is missing in one series"
    );
    assert!(
        !got.timestamps.contains(&0),
        "timestamp 0 is missing in one series"
    );
}
