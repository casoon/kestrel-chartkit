//! Golden-reference tests for the intrabar cumulative volume delta (`intrabar_cvd`). Values from
//! `reference/kestrel_reference/fixtures/intrabar_cvd.py`, a second implementation of the booking
//! rule: both anchors (the daily one with and without an offset), both policies for an unchanged
//! close, parents with and without children, day changes and unchanged closes.

mod common;

use std::collections::HashMap;

use kestrel_chartkit::indicator::other_input::{build_intrabar_checked, IntrabarIndicator};
use kestrel_chartkit::indicator::registry::{build_checked, RegistryError};
use kestrel_chartkit::intrabar::IntrabarGroup;
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::DeltaProvenance;

const GOLDEN: &str = include_str!("fixtures/golden_intrabar_cvd.txt");

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
    let tolerance = value("intrabar_cvd_tolerance");
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{key}: got {got}, reference {want}"
    );
}

fn params(case: &str) -> HashMap<String, f64> {
    [
        ("anchor", "anchor"),
        ("anchor_offset_seconds", "offset"),
        ("unchanged_policy", "policy"),
    ]
    .into_iter()
    .filter(|(_, suffix)| has(&format!("{case}_{suffix}")))
    .map(|(name, suffix)| (name.to_string(), value(&format!("{case}_{suffix}"))))
    .collect()
}

/// A step of a case as the indicator reads it: a parent with its children, or a parent without.
enum Step {
    Group(IntrabarGroup),
    Missing(Bar),
}

fn steps(case: &str) -> Vec<Step> {
    (0..value(&format!("{case}_n")) as usize)
        .map(|j| {
            let stamp = value(&format!("{case}_s{j}_ts")) as i64;
            if value(&format!("{case}_s{j}_kind")) == 0.0 {
                let children = (0..value(&format!("{case}_s{j}_m")) as usize)
                    .map(|k| {
                        let open = value(&format!("{case}_s{j}_c{k}_open"));
                        let close = value(&format!("{case}_s{j}_c{k}_close"));
                        let volume = value(&format!("{case}_s{j}_c{k}_volume"));
                        Bar::new(
                            stamp + k as i64 * 60,
                            open,
                            open.max(close),
                            open.min(close),
                            close,
                            volume,
                        )
                    })
                    .collect();
                Step::Group(IntrabarGroup {
                    parent_timestamp: stamp,
                    children,
                })
            } else {
                let volume = value(&format!("{case}_s{j}_pv"));
                Step::Missing(Bar::new(stamp, 100.0, 101.0, 99.0, 100.0, volume))
            }
        })
        .collect()
}

fn feed(
    indicator: &mut dyn IntrabarIndicator,
    steps: &[Step],
) -> Vec<kestrel_chartkit::IntrabarDelta> {
    steps
        .iter()
        .map(|step| match step {
            Step::Group(group) => indicator.on_group(group),
            Step::Missing(parent) => indicator.on_missing_children(parent),
        })
        .collect()
}

#[test]
fn test_intrabar_cvd_matches_the_reference() {
    let cases = value("meta_cases") as usize;
    assert!(cases >= 20);
    let mut parents = 0;
    let mut without_children = 0;
    let mut unattributed = 0;
    let mut daily = 0;
    let mut carrying = 0;
    for i in 0..cases {
        let case = format!("ib{i}");
        let params = params(&case);
        daily += usize::from(params.get("anchor") == Some(&1.0));
        carrying += usize::from(params.get("unchanged_policy") == Some(&1.0));
        let steps = steps(&case);
        let mut indicator = build_intrabar_checked("intrabar_cvd", &params).unwrap();
        let out = feed(indicator.as_mut(), &steps);
        for (j, delta) in out.iter().enumerate() {
            let key = |name: &str| format!("{case}_s{j}_{name}");
            parents += 1;
            assert_eq!(delta.timestamp as f64, value(&key("ts")), "{case} step {j}");
            assert_eq!(delta.provenance, DeltaProvenance::IntrabarShape);
            let children = match &steps[j] {
                Step::Group(group) => group.children.len(),
                Step::Missing(_) => {
                    without_children += 1;
                    0
                }
            };
            assert_eq!(delta.children, children, "{case} step {j}");
            near(delta.delta, &key("delta"));
            near(delta.buy_volume, &key("buy"));
            near(delta.sell_volume, &key("sell"));
            near(delta.unattributed_volume, &key("un"));
            near(delta.cumulative_open, &key("cum_open"));
            near(delta.cumulative_high, &key("cum_high"));
            near(delta.cumulative_low, &key("cum_low"));
            near(delta.cumulative_close, &key("cum_close"));
            if delta.unattributed_volume > 0.0 {
                unattributed += 1;
            }
        }

        // The run after `reset` is the same run.
        indicator.reset();
        assert_eq!(
            feed(indicator.as_mut(), &steps),
            out,
            "{case}: run after reset"
        );
    }
    assert!(parents > 250, "{parents}");
    assert!(without_children >= 10, "{without_children}");
    assert!(unattributed >= 30, "{unattributed}");
    assert!(daily >= 8 && carrying >= 6, "{daily} {carrying}");
}

/// Without parameters the anchor is continuous and an unchanged close is unattributed: the same
/// run as with the choices named by position.
#[test]
fn test_defaults_are_continuous_and_unattributed() {
    let steps = steps("ib0");
    assert!(params("ib0").is_empty());
    let mut by_default = build_intrabar_checked("intrabar_cvd", &HashMap::new()).unwrap();
    let mut named = build_intrabar_checked(
        "intrabar_cvd",
        &HashMap::from([
            ("anchor".to_string(), 0.0),
            ("anchor_offset_seconds".to_string(), 0.0),
            ("unchanged_policy".to_string(), 0.0),
        ]),
    )
    .unwrap();
    assert_eq!(
        feed(by_default.as_mut(), &steps),
        feed(named.as_mut(), &steps)
    );
}

/// The two policies and the two anchors change the result on the same steps: no choice is
/// ignored.
#[test]
fn test_every_choice_changes_the_result() {
    let run = |case: &str| {
        let mut indicator = build_intrabar_checked("intrabar_cvd", &params(case)).unwrap();
        feed(indicator.as_mut(), &steps(case))
    };
    // ib0 and ib1 share their steps (default and carry), ib4 and ib6 theirs (daily and continuous).
    assert_eq!(value("ib0_n"), value("ib1_n"));
    assert_ne!(run("ib0"), run("ib1"));
    assert_eq!(value("ib4_n"), value("ib6_n"));
    assert_ne!(run("ib4"), run("ib6"));
    // ib4 and ib5 differ in the policy under the daily anchor.
    assert_ne!(run("ib4"), run("ib5"));
}

#[test]
fn test_intrabar_cvd_rejects_inadmissible_configuration() {
    let rejected = |params: &[(&str, f64)]| {
        let map: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        match build_intrabar_checked("intrabar_cvd", &map) {
            Err(
                RegistryError::InvalidParameter { .. } | RegistryError::UnknownParameter { .. },
            ) => {}
            Err(other) => panic!("{params:?}: rejected for the wrong reason: {other}"),
            Ok(_) => panic!("{params:?}: built, expected a rejection"),
        }
    };
    for anchor in [2.0, 0.5, -1.0, f64::NAN, f64::INFINITY] {
        rejected(&[("anchor", anchor)]);
    }
    for policy in [2.0, 0.5, -1.0, f64::NAN] {
        rejected(&[("unchanged_policy", policy)]);
    }
    for offset in [
        86_400.5,
        -86_400.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        rejected(&[("anchor_offset_seconds", offset)]);
    }
    rejected(&[("period", 5.0)]);
    rejected(&[("anchor", 1.0), ("offset", 3600.0)]);
    for offset in [86_400.0, -86_400.0] {
        let map = HashMap::from([("anchor_offset_seconds".to_string(), offset)]);
        assert!(build_intrabar_checked("intrabar_cvd", &map).is_ok());
    }
}

/// The entry belongs to the intrabar builder: the others refuse it by name, and the intrabar
/// builder refuses a one-series entry.
#[test]
fn test_intrabar_cvd_is_built_only_by_the_intrabar_builder() {
    assert!(matches!(
        build_checked("intrabar_cvd", &HashMap::new()),
        Err(RegistryError::WrongInput { .. })
    ));
    assert!(matches!(
        build_intrabar_checked("sma", &HashMap::new()),
        Err(RegistryError::WrongInput { .. })
    ));
}
