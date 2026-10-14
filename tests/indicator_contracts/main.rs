//! Indicator contracts (plan 62): every catalog indicator run bar by bar against a complete
//! expected sequence from a generated fixture (`tests/fixtures/contract_<family>.txt`).
//!
//! One module per family. Test names start with the catalog name, so
//! `cargo test --test indicator_contracts rsi` checks one indicator. The coverage matrix below
//! lists, per indicator and area, whether a contract exists — a missing contract is reported as
//! not specified, never as passed.

#[path = "../common/mod.rs"]
mod common;

mod composites_a;
mod composites_b;
mod moving_averages;
mod oscillators;
mod pilot;
mod structure;
mod volatility;
mod volume;

use std::collections::{BTreeMap, HashMap};

use common::contract::{check, ContractCase};
use kestrel_chartkit::indicator::registry::{build_checked, catalog, RegistryError};

/// Whether an area of an indicator's contract is written down and checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    /// Cases fix this area completely.
    Specified,
    /// Cases fix part of it; the reason names what is missing.
    Partial(&'static str),
    /// No case fixes it; the reason says why.
    NotSpecified(&'static str),
}

/// One row of the coverage matrix.
///
/// - `values`: presence, value, `secondary` and the complete `extra` set on every bar.
/// - `alerts`: the complete set of alert kinds with their strength on every bar.
/// - `configuration`: inadmissible parameters are rejected.
#[derive(Debug, Clone, Copy)]
pub struct Coverage {
    pub indicator: &'static str,
    pub values: Status,
    pub alerts: Status,
    pub configuration: Status,
}

impl Coverage {
    pub const fn full(indicator: &'static str) -> Self {
        Self {
            indicator,
            values: Status::Specified,
            alerts: Status::Specified,
            configuration: Status::Specified,
        }
    }
}

pub fn assert_holds(case: &ContractCase) {
    let failures = check(case);
    assert!(
        failures.is_empty(),
        "{} deviations:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// A configuration outside the parameter contract is rejected, not built with a fallback.
pub fn assert_rejected(indicator: &str, params: &[(&str, f64)]) {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    match build_checked(indicator, &params) {
        Err(RegistryError::InvalidParameter { .. } | RegistryError::UnknownParameter { .. }) => {}
        Err(other) => panic!("{indicator} {params:?}: rejected for the wrong reason: {other}"),
        Ok(_) => panic!("{indicator} {params:?}: built, expected a rejection"),
    }
}

const FAMILIES: &[(&str, &[Coverage], &[&str])] = &[
    ("pilot", pilot::COVERAGE, pilot::PENDING),
    (
        "moving_averages",
        moving_averages::COVERAGE,
        moving_averages::PENDING,
    ),
    ("oscillators", oscillators::COVERAGE, oscillators::PENDING),
    ("volatility", volatility::COVERAGE, volatility::PENDING),
    ("volume", volume::COVERAGE, volume::PENDING),
    ("structure", structure::COVERAGE, structure::PENDING),
    (
        "composites_a",
        composites_a::COVERAGE,
        composites_a::PENDING,
    ),
    (
        "composites_b",
        composites_b::COVERAGE,
        composites_b::PENDING,
    ),
];

/// Every catalog indicator sits in exactly one family, either with its coverage or as pending.
/// A new catalog entry fails here until it is placed. Prints the progress per area
/// (`--nocapture`).
#[test]
fn coverage_matrix_names_every_catalog_indicator_once() {
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    let mut problems = Vec::new();
    for (family, coverage, pending) in FAMILIES {
        for name in coverage
            .iter()
            .map(|c| c.indicator)
            .chain(pending.iter().copied())
        {
            if let Some(other) = seen.insert(name, family) {
                problems.push(format!("{name}: listed in {other} and {family}"));
            }
        }
    }
    let catalog: Vec<&str> = catalog().iter().map(|e| e.name).collect();
    for name in &catalog {
        if !seen.contains_key(name) {
            problems.push(format!("{name}: in the catalog but in no family"));
        }
    }
    for name in seen.keys() {
        if !catalog.contains(name) {
            problems.push(format!("{name}: in a family but not in the catalog"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    let rows: Vec<&Coverage> = FAMILIES.iter().flat_map(|(_, c, _)| c.iter()).collect();
    let count = |area: fn(&Coverage) -> Status, wanted: fn(Status) -> bool| {
        rows.iter().filter(|c| wanted(area(c))).count()
    };
    let pending: usize = FAMILIES.iter().map(|(_, _, p)| p.len()).sum();
    for (label, area) in [
        (
            "values",
            (|c: &Coverage| c.values) as fn(&Coverage) -> Status,
        ),
        ("alerts", |c: &Coverage| c.alerts),
        ("configuration", |c: &Coverage| c.configuration),
    ] {
        println!(
            "{label:>13}: {} specified, {} partial, {} not specified, {pending} pending of {}",
            count(area, |s| s == Status::Specified),
            count(area, |s| matches!(s, Status::Partial(_))),
            count(area, |s| matches!(s, Status::NotSpecified(_))),
            catalog.len()
        );
    }
}
