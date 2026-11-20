//! Read-out indicators (plan 141) against `reference/kestrel_reference/fixtures/readouts.py`,
//! built by name through the registry and run over `series.wave_series(700)`.

mod common;

use std::collections::HashMap;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::model::Bar;

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

#[test]
fn test_golden_price_location() {
    check("pl_default", "price_location", &[]);
    check(
        "pl_small",
        "price_location",
        &[
            ("atr_len", 5.0),
            ("ema_len", 8.0),
            ("range_len", 12.0),
            ("sma_len", 30.0),
            ("slope_bars", 4.0),
            ("vol_window", 25.0),
            ("vol_warmup", 40.0),
        ],
    );
}

/// Daily bars with explicit timestamps over calendar month and multi-week boundaries; the held
/// midpoint and the completion flag on every bar.
fn check_midpoint_scenario(label: &str, params: &[(&str, f64)]) {
    let key = |suffix: String| format!("pmscn_{label}_{suffix}");
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut indicator = build_checked("period_midpoint", &params).unwrap();
    let tolerance = value("readouts_tolerance");
    for k in 0..value(&key("n".to_string())) as usize {
        let bar = Bar::new(
            value(&key(format!("bar{k}_ts"))) as i64,
            100.0,
            value(&key(format!("bar{k}_high"))),
            value(&key(format!("bar{k}_low"))),
            100.0,
            1.0,
        );
        let output = indicator.on_bar(&bar);
        if has(&key(format!("out{k}_none"))) {
            assert!(output.is_none(), "{label} bar {k}: no output expected");
            continue;
        }
        let output = output.unwrap_or_else(|| panic!("{label} bar {k}: no output"));
        common::assert_close(
            output.value,
            value(&key(format!("out{k}"))),
            tolerance,
            &format!("{label} bar {k} midpoint"),
        );
        common::assert_close(
            output.extra["new_period"],
            value(&key(format!("new{k}"))),
            tolerance,
            &format!("{label} bar {k} new_period"),
        );
    }
}

#[test]
fn test_golden_period_midpoint_calendar_periods() {
    // unit 4 = week, 5 = month (the order of the unit choice).
    check_midpoint_scenario("month1", &[("unit", 5.0), ("count", 1.0)]);
    check_midpoint_scenario("month2", &[("unit", 5.0), ("count", 2.0)]);
    check_midpoint_scenario("week2", &[("unit", 4.0), ("count", 2.0)]);
    check_midpoint_scenario(
        "month_offset",
        &[
            ("unit", 5.0),
            ("count", 1.0),
            ("utc_offset_seconds", -3600.0),
        ],
    );
}

#[test]
fn test_golden_vwap_regime() {
    check("vr_default", "vwap_regime", &[]);
    check(
        "vr_small",
        "vwap_regime",
        &[
            ("window", 60.0),
            ("slope_lookback", 5.0),
            ("atr_len", 7.0),
            ("regime_window", 10.0),
            ("flat_threshold", 0.02),
            ("strong_threshold", 0.10),
        ],
    );
}

#[test]
fn test_golden_swing_structure() {
    check("ss_default", "swing_structure", &[]);
    check(
        "ss_small",
        "swing_structure",
        &[
            ("left", 3.0),
            ("right", 3.0),
            ("max_swings", 10.0),
            ("atr_len", 7.0),
        ],
    );
}
