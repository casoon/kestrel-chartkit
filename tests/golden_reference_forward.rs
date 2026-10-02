//! Golden-reference tests for the forward-path statistics: conditional phase end (Kaplan–Meier),
//! first passage, forward move, empirical bands and their coverage. Values from
//! `reference/kestrel_reference/fixtures/forward.py`.

mod common;

use kestrel_chartkit::analytics::forward::{
    band_coverage, conditional_end_probability, empirical_band, first_passage, forward_move, Band,
    Passage, PhaseDuration, Tie,
};
use kestrel_chartkit::Bar;

const GOLDEN: &str = include_str!("fixtures/golden_forward.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn tolerance() -> f64 {
    value("forward_tolerance")
}

fn close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance() * expected.abs().max(1.0),
        "{what}: {actual} vs {expected}"
    );
}

fn bars(prefix: &str) -> Vec<Bar> {
    let n = value(&format!("{prefix}_len")) as usize;
    (0..n)
        .map(|j| {
            let key = |f: &str| value(&format!("{prefix}_b{j}_{f}"));
            Bar {
                timestamp: j as i64,
                open: key("close"),
                high: key("high"),
                low: key("low"),
                close: key("close"),
                volume: 0.0,
            }
        })
        .collect()
}

#[test]
fn test_conditional_end_matches_the_bar_by_bar_product() {
    let cases = value("meta_km_case_count") as usize;
    assert!(cases > 0);
    for i in 0..cases {
        let key = |f: &str| value(&format!("km{i}_{f}"));
        let durations: Vec<PhaseDuration> = (0..key("n") as usize)
            .map(|j| PhaseDuration {
                bars: key(&format!("d{j}_bars")) as usize,
                ended: key(&format!("d{j}_ended")) == 1.0,
            })
            .collect();
        let got =
            conditional_end_probability(&durations, key("age") as usize, key("horizon") as usize)
                .unwrap_or_else(|| panic!("km{i} must produce an estimate"));
        close(
            got.probability,
            key("probability"),
            &format!("km{i} probability"),
        );
        close(got.std_error, key("std_error"), &format!("km{i} std_error"));
        assert_eq!(got.at_risk, key("at_risk") as usize, "km{i} at_risk");
    }
}

#[test]
fn test_first_passage_matches_the_scan() {
    let cases = value("meta_passage_case_count") as usize;
    for i in 0..cases {
        let prefix = format!("fp{i}");
        let key = |f: &str| value(&format!("{prefix}_{f}"));
        let tie = if key("tie") == 1.0 {
            Tie::UpperFirst
        } else {
            Tie::LowerFirst
        };
        let got = first_passage(
            &bars(&prefix),
            key("start") as usize,
            key("upper"),
            key("lower"),
            key("horizon") as usize,
            tie,
        )
        .unwrap_or_else(|| panic!("{prefix} has a complete path"));
        let expected = match key("kind") as u8 {
            1 => Passage::Upper {
                bars: key("bars") as usize,
            },
            2 => Passage::Lower {
                bars: key("bars") as usize,
            },
            _ => Passage::Neither,
        };
        assert_eq!(got, expected, "{prefix}");
    }
}

#[test]
fn test_band_matches_statistics_quantiles() {
    let cases = value("meta_band_case_count") as usize;
    for i in 0..cases {
        let key = |f: &str| value(&format!("band{i}_{f}"));
        let samples: Vec<f64> = (0..key("len") as usize)
            .map(|j| key(&format!("v{j}")))
            .collect();
        let band = empirical_band(&samples, key("coverage")).expect("band");
        close(band.lower, key("lower"), &format!("band{i} lower"));
        close(band.upper, key("upper"), &format!("band{i} upper"));
    }
}

#[test]
fn test_forward_move_matches() {
    let cases = value("meta_move_case_count") as usize;
    for i in 0..cases {
        let prefix = format!("move{i}");
        let key = |f: &str| value(&format!("{prefix}_{f}"));
        let got = forward_move(
            &bars(&prefix),
            key("start") as usize,
            key("horizon") as usize,
        )
        .expect("complete path");
        close(
            got.close_change,
            key("close_change"),
            &format!("{prefix} close_change"),
        );
        close(got.max_up, key("max_up"), &format!("{prefix} max_up"));
        close(got.max_down, key("max_down"), &format!("{prefix} max_down"));
    }
}

#[test]
fn test_coverage_counts_bounds_as_inside() {
    let n = value("cov_len") as usize;
    let pairs: Vec<(Band, f64)> = (0..n)
        .map(|j| {
            let key = |f: &str| value(&format!("cov_p{j}_{f}"));
            (
                Band {
                    lower: key("lower"),
                    upper: key("upper"),
                    coverage: 0.8,
                },
                key("outcome"),
            )
        })
        .collect();
    close(
        band_coverage(&pairs).unwrap(),
        value("cov_coverage"),
        "coverage",
    );
}
