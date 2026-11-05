//! Exports the applicability declarations as one JSON document — `indicators` (each registry
//! indicator's `DataRequirements`), `rules` (the rule table `check_applicability` enforces,
//! with publication-ready explanations) plus `method_rules` (the rules of the named structure
//! methods, `Elliott` and `Wyckoff`, each with its own explanations) — for a separate docs project to render into a public
//! applicability matrix (see `plan/indikator-anwendbarkeit-und-serien-faehigkeiten.md`, "Export
//! für die Doku").
//!
//! This crate has no `serde_json` dependency and the schema here is a small, fully-known set of
//! bools/strings, so the JSON is hand-formatted rather than pulling in a dependency for it.
//!
//! Run with:
//! ```bash
//! cargo run --example export_applicability
//! ```

use kestrel_chartkit::applicability::{
    data_requirements, method_rule_catalog, rule_catalog, DataRequirements,
};
use kestrel_chartkit::indicator::registry::CANONICAL_INDICATOR_NAMES;

/// Two indicators are standalone modules, never wired into the single-bar registry/catalog (see
/// the note above `CANONICAL_INDICATOR_NAMES` in `src/indicator/registry.rs`), so they are not in
/// that list but still need their `DataRequirements` exported.
const NON_CATALOG_NAMES: &[&str] = &["elliott", "swing_structure"];

fn indicator_json(name: &str, requirements: &DataRequirements) -> String {
    format!(
        "{{\"name\":\"{}\",\"needs_real_volume\":{},\"needs_trade_direction\":{},\"session_sensitive\":{},\"roll_sensitive\":{},\"adjustment_sensitive\":{},\"needs_liquidity_depth\":{},\"structure_method\":{}}}",
        name,
        requirements.needs_real_volume,
        requirements.needs_trade_direction,
        requirements.session_sensitive,
        requirements.roll_sensitive,
        requirements.adjustment_sensitive,
        requirements.needs_liquidity_depth,
        requirements
            .structure_method
            .map_or("null".to_string(), |method| format!("\"{method:?}\"")),
    )
}

fn main() {
    let indicators: Vec<String> = CANONICAL_INDICATOR_NAMES
        .iter()
        .chain(NON_CATALOG_NAMES)
        .map(|&name| indicator_json(name, &data_requirements(name)))
        .collect();

    let rules: Vec<String> = rule_catalog()
        .iter()
        .map(|rule| {
            format!(
                "{{\"reason\":\"{:?}\",\"tier\":\"{:?}\",\"explanation\":\"{}\"}}",
                rule.reason, rule.tier, rule.explanation
            )
        })
        .collect();

    let method_rules: Vec<String> = method_rule_catalog()
        .iter()
        .map(|rule| {
            format!(
                "{{\"method\":\"{:?}\",\"reason\":\"{:?}\",\"tier\":\"{:?}\",\"explanation\":\"{}\"}}",
                rule.method, rule.reason, rule.tier, rule.explanation
            )
        })
        .collect();

    println!(
        "{{\"indicators\":[{}],\"rules\":[{}],\"method_rules\":[{}]}}",
        indicators.join(","),
        rules.join(","),
        method_rules.join(",")
    );
}
