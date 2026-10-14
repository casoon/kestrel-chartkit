//! Plan 62 (b): `warmup_period()` is the 1-based bar of the first output — a contract, not an
//! estimate. How long the start keeps its imprint is a separate measure (`settle`).
//!
//! Checked on the settle reference paths for the default parameters and for every whole-number
//! parameter raised on its own, so the formula has to follow the parameters, not just match the
//! defaults.

use std::collections::HashMap;

mod common;

use common::PATH_DEPENDENT_FIRST_OUTPUT;
use kestrel_chartkit::indicator::{registry, settle};

/// Bar spacing of the reference paths. Indicators that think in time (`rvat`) get it as their
/// `bar_seconds` parameter, so their bar count matches the paths.
const BAR_SECONDS: i64 = 900;

fn first_output(
    name: &str,
    params: &HashMap<String, f64>,
    bars: &[kestrel_chartkit::Bar],
) -> Option<usize> {
    let mut indicator = registry::build_checked(name, params).unwrap();
    bars.iter()
        .position(|bar| indicator.on_bar(bar).is_some())
        .map(|i| i + 1)
}

fn variants(defaults: &HashMap<String, f64>) -> Vec<(String, HashMap<String, f64>)> {
    let mut defaults = defaults.clone();
    if defaults.contains_key("bar_seconds") {
        defaults.insert("bar_seconds".to_string(), BAR_SECONDS as f64);
    }
    let mut out = vec![("defaults".to_string(), defaults.clone())];
    for (key, value) in &defaults {
        if key != "bar_seconds" && value.fract() == 0.0 && *value >= 2.0 {
            let mut params = defaults.clone();
            params.insert(key.clone(), value + 3.0);
            out.push((format!("{key}={}", value + 3.0), params));
        }
    }
    out
}

#[test]
fn warmup_is_the_bar_of_the_first_output() {
    let paths: Vec<_> = settle::REFERENCE_SHAPES
        .iter()
        .map(|s| s.series(BAR_SECONDS))
        .collect();
    let mut failures = Vec::new();
    for entry in registry::catalog() {
        for (label, params) in variants(&entry.default_params) {
            let Ok(indicator) = registry::build_checked(entry.name, &params) else {
                continue;
            };
            let warmup = indicator.warmup_period();
            let firsts: Vec<_> = paths
                .iter()
                .map(|bars| first_output(entry.name, &params, bars))
                .collect();
            let path_dependent = PATH_DEPENDENT_FIRST_OUTPUT.contains(&entry.name);
            let broken = |first: &usize| {
                if path_dependent {
                    *first < warmup
                } else {
                    *first != warmup
                }
            };
            if firsts.iter().flatten().any(broken) {
                failures.push(format!(
                    "{} [{label}]: warmup {warmup}, first output {firsts:?}",
                    entry.name
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
