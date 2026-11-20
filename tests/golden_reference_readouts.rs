//! Read-out indicators (plan 141) against `reference/kestrel_reference/fixtures/readouts.py`,
//! built by name through the registry and run over `series.wave_series(700)`.

mod common;

use std::collections::HashMap;

use kestrel_chartkit::indicator::registry::build_checked;

const GOLDEN: &str = include_str!("fixtures/golden_readouts.txt");
const BARS: usize = 700;

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

fn checkpoints() -> Vec<usize> {
    (0..value("meta_cp_count") as usize)
        .map(|k| value(&format!("meta_cp{k}")) as usize)
        .collect()
}

/// The `value` of the indicator on every bar (`None` where it has no output).
fn series(name: &str, params: &[(&str, f64)]) -> Vec<Option<f64>> {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut indicator = build_checked(name, &params).unwrap();
    common::wave_series(BARS)
        .iter()
        .map(|bar| indicator.on_bar(bar).map(|o| o.value))
        .collect()
}

/// Every checkpoint of `label` agrees: a value within the fixture tolerance, or no output exactly
/// where the fixture says `none`.
fn check(label: &str, name: &str, params: &[(&str, f64)]) {
    let values = series(name, params);
    let tolerance = value("readouts_tolerance");
    for cp in checkpoints() {
        let key = format!("{label}_cp{cp}");
        if has(&format!("{key}_none")) {
            assert_eq!(values[cp], None, "{key}: no output expected");
        } else {
            let actual = values[cp].unwrap_or_else(|| panic!("{key}: no output"));
            common::assert_close(actual, value(&key), tolerance, &key);
        }
    }
}

#[test]
fn test_golden_fractal_dimension() {
    check("fd_halves_default", "fractal_dimension", &[]);
    check(
        "fd_halves_odd",
        "fractal_dimension",
        &[("period", 9.0), ("method", 0.0)],
    );
    check(
        "fd_halves_min",
        "fractal_dimension",
        &[("period", 4.0), ("method", 0.0)],
    );
    check("fd_path_default", "fractal_dimension", &[("method", 1.0)]);
    check(
        "fd_path_short",
        "fractal_dimension",
        &[("period", 5.0), ("method", 1.0)],
    );
}

#[test]
fn test_golden_r_squared() {
    check("r2_default", "r_squared", &[]);
    check("r2_short", "r_squared", &[("period", 2.0)]);
    check("r2_long", "r_squared", &[("period", 60.0)]);
}

#[test]
fn test_golden_percent_rank() {
    check("pr_default", "percent_rank", &[]);
    check("pr_one", "percent_rank", &[("period", 1.0)]);
    check("pr_long", "percent_rank", &[("period", 100.0)]);
}

#[test]
fn test_golden_cycle_length() {
    check("cl_default", "cycle_length", &[]);
    check(
        "cl_fast",
        "cycle_length",
        &[("period", 5.0), ("history", 3.0)],
    );
}
