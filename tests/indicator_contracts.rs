//! Indicator contracts (plan 62, pilot): SMA, RSI, ATR and the order block, each run bar by bar
//! against a complete expected sequence from `tests/fixtures/contract_pilot.txt`.
//!
//! Test names start with the catalog name, so `cargo test --test indicator_contracts rsi` checks
//! one indicator. Not specified in the pilot (and therefore not checked here): the RSI context
//! line with its divergences and the exhaustion alerts — both need more history than the cases
//! carry —, the ATR smoothing modes other than Wilder, and source mapping.

mod common;

use std::collections::HashMap;

use common::contract::{check, load, ContractCase};
use kestrel_chartkit::indicator::registry::{build_checked, RegistryError};
use kestrel_chartkit::IndicatorUnit;

const FIXTURE: &str = include_str!("fixtures/contract_pilot.txt");

fn case(name: &str, indicator: &str, unit: IndicatorUnit) -> ContractCase {
    load(FIXTURE, name, indicator, unit)
}

fn assert_holds(case: &ContractCase) {
    let failures = check(case);
    assert!(
        failures.is_empty(),
        "{} deviations:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// A configuration outside the parameter contract is rejected, not built with a fallback.
fn assert_rejected(indicator: &str, params: &[(&str, f64)]) {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    match build_checked(indicator, &params) {
        Err(RegistryError::InvalidParameter { .. } | RegistryError::UnknownParameter { .. }) => {}
        Err(other) => panic!("{indicator} {params:?}: rejected for the wrong reason: {other}"),
        Ok(_) => panic!("{indicator} {params:?}: built, expected a rejection"),
    }
}

// --- SMA -------------------------------------------------------------------------------------

#[test]
fn sma_rising() {
    assert_holds(&case("sma_rising", "sma", IndicatorUnit::Price));
}

#[test]
fn sma_single() {
    assert_holds(&case("sma_single", "sma", IndicatorUnit::Price));
}

#[test]
fn sma_constant() {
    assert_holds(&case("sma_constant", "sma", IndicatorUnit::Price));
}

#[test]
fn sma_rejects_invalid_configuration() {
    assert_rejected("sma", &[("period", 0.0)]);
    assert_rejected("sma", &[("period", 2.5)]);
    assert_rejected("sma", &[("period", f64::NAN)]);
    assert_rejected("sma", &[("peroid", 3.0)]);
}

// --- RSI -------------------------------------------------------------------------------------

#[test]
fn rsi_rawline() {
    assert_holds(&case("rsi_rawline", "rsi", IndicatorUnit::Ratio));
}

#[test]
fn rsi_defaults() {
    assert_holds(&case("rsi_defaults", "rsi", IndicatorUnit::Ratio));
}

#[test]
fn rsi_flatrise() {
    assert_holds(&case("rsi_flatrise", "rsi", IndicatorUnit::Ratio));
}

#[test]
fn rsi_rejects_invalid_configuration() {
    assert_rejected("rsi", &[("rsi_len", 0.0)]);
    assert_rejected("rsi", &[("oversold", 80.0), ("overbought", 70.0)]);
    assert_rejected("rsi", &[("overbought", 120.0)]);
    assert_rejected("rsi", &[("period", 14.0)]);
}

// --- ATR -------------------------------------------------------------------------------------

#[test]
fn atr_gaps() {
    assert_holds(&case("atr_gaps", "atr", IndicatorUnit::Ratio));
}

#[test]
fn atr_rejects_invalid_configuration() {
    assert_rejected("atr", &[("atr_len", 0.0)]);
    assert_rejected("atr", &[("smoothing", 4.0)]);
    assert_rejected("atr", &[("length", 14.0)]);
}

// --- Order block -----------------------------------------------------------------------------

#[test]
fn order_block_cycle() {
    assert_holds(&case(
        "order_block_cycle",
        "order_block",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn order_block_rejects_invalid_configuration() {
    assert_rejected("order_block", &[("atr_len", 0.0)]);
    assert_rejected("order_block", &[("min_disp", 0.0)]);
    assert_rejected("order_block", &[("min_displacement", 1.0)]);
}

// --- The check itself ------------------------------------------------------------------------
//
// A contract test is only worth something if a wrong expectation fails it. Each mutation changes
// the expected side of a passing case, never the implementation.

fn fails(case: &ContractCase, what: &str) {
    assert!(!check(case).is_empty(), "mutation not detected: {what}");
}

#[test]
fn contract_detects_a_wrong_value() {
    let mut c = case("sma_rising", "sma", IndicatorUnit::Price);
    c.expected[3].output.as_mut().unwrap().value += 1e-6;
    fails(&c, "value off by 1e-6");
}

#[test]
fn contract_detects_a_missing_field() {
    let mut c = case("atr_gaps", "atr", IndicatorUnit::Ratio);
    let bar = c
        .expected
        .iter_mut()
        .rev()
        .find(|b| b.output.is_some())
        .unwrap();
    bar.output.as_mut().unwrap().extra.remove("raw");
    fails(&c, "extra[raw] produced but not expected");

    let mut c = case("atr_gaps", "atr", IndicatorUnit::Ratio);
    let bar = c
        .expected
        .iter_mut()
        .rev()
        .find(|b| b.output.is_some())
        .unwrap();
    bar.output
        .as_mut()
        .unwrap()
        .extra
        .insert("upper".into(), 1.0);
    fails(&c, "required field missing from the output");
}

#[test]
fn contract_detects_a_wrong_alert_direction() {
    let mut c = case("order_block_cycle", "order_block", IndicatorUnit::Unitless);
    let bar = c
        .expected
        .iter_mut()
        .find(|b| b.alerts.contains_key("bullish_order_block"))
        .unwrap();
    let strength = bar.alerts.remove("bullish_order_block").unwrap();
    bar.alerts.insert("bearish_order_block".into(), strength);
    fails(&c, "bullish alert expected as bearish");
}

#[test]
fn contract_detects_a_shifted_first_output() {
    let mut c = case("rsi_rawline", "rsi", IndicatorUnit::Ratio);
    let first = c.expected.iter().position(|b| b.output.is_some()).unwrap();
    c.expected[first].output = None;
    fails(&c, "first output one bar later");
}

#[test]
fn contract_detects_a_wrong_unit_and_configuration() {
    fails(&case("sma_rising", "sma", IndicatorUnit::Ratio), "unit");

    let mut c = case("sma_rising", "sma", IndicatorUnit::Price);
    c.params.insert("period".into(), 0.0);
    fails(&c, "inadmissible configuration");
}
