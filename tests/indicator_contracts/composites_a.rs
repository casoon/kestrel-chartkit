//! Contracts of the composites a family, from `tests/fixtures/contract_composites_a.txt`.

use super::{
    assert_holds, assert_rejected, Coverage,
    Status::{Partial, Specified},
};
use crate::common::contract::{check, load, ContractCase};
use kestrel_chartkit::IndicatorUnit;

const FIXTURE: &str = include_str!("../fixtures/contract_composites_a.txt");

fn case(name: &str, indicator: &str, unit: IndicatorUnit) -> ContractCase {
    load(FIXTURE, name, indicator, unit)
}

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[];

pub const COVERAGE: &[Coverage] = &[
    Coverage::full("roc_context"),
    Coverage::full("smoothed_supertrend"),
    Coverage::full("reaction_speed"),
    Coverage::full("flow_exhaustion"),
    Coverage::full("coil_release"),
    Coverage::full("compression_zones"),
    Coverage {
        indicator: "price_average_relation",
        values: Partial(
            "ma_kind 1-3 (SMA/RMA/ALMA): the reference reads the MA from the first bar",
        ),
        alerts: Partial("ma_kind 1-3 (SMA/RMA/ALMA) not exercised"),
        configuration: Specified,
    },
    Coverage::full("overextension_risk"),
    Coverage::full("tradability_filter"),
    Coverage::full("market_state_chain"),
    Coverage::full("rsi_cycle_stats"),
];

// --- ROC Context -----------------------------------------------------------------------------

fn roc_context(name: &str) {
    assert_holds(&case(name, "roc_context", IndicatorUnit::Ratio));
}

#[test]
fn roc_context_defaults() {
    roc_context("roc_context_defaults");
}

#[test]
fn roc_context_short() {
    roc_context("roc_context_short");
}

#[test]
fn roc_context_sma() {
    roc_context("roc_context_sma");
}

#[test]
fn roc_context_rma() {
    roc_context("roc_context_rma");
}

#[test]
fn roc_context_flat() {
    roc_context("roc_context_flat");
}

#[test]
fn roc_context_rejects_invalid_configuration() {
    assert_rejected("roc_context", &[("roc_len", 0.0)]);
    assert_rejected("roc_context", &[("ma_kind", 3.0)]);
    assert_rejected("roc_context", &[("ob_level", 0.0)]);
    assert_rejected("roc_context", &[("os_level", 1.0)]);
    assert_rejected("roc_context", &[("ctx_len", 1.0)]);
    assert_rejected("roc_context", &[("length", 12.0)]);
}

// --- Smoothed Supertrend ---------------------------------------------------------------------

fn smoothed_supertrend(name: &str) {
    assert_holds(&case(name, "smoothed_supertrend", IndicatorUnit::Price));
}

#[test]
fn smoothed_supertrend_defaults() {
    smoothed_supertrend("smoothed_supertrend_defaults");
}

#[test]
fn smoothed_supertrend_mirrored() {
    smoothed_supertrend("smoothed_supertrend_mirrored");
}

#[test]
fn smoothed_supertrend_short() {
    smoothed_supertrend("smoothed_supertrend_short");
}

#[test]
fn smoothed_supertrend_lcg() {
    smoothed_supertrend("smoothed_supertrend_lcg");
}

#[test]
fn smoothed_supertrend_rejects_invalid_configuration() {
    assert_rejected("smoothed_supertrend", &[("factor", 0.1)]);
    assert_rejected("smoothed_supertrend", &[("atr_len", 0.0)]);
    assert_rejected("smoothed_supertrend", &[("adx_threshold", 60.0)]);
    assert_rejected("smoothed_supertrend", &[("touches", 6.0)]);
    assert_rejected("smoothed_supertrend", &[("slope_threshold", -0.1)]);
    assert_rejected("smoothed_supertrend", &[("multiplier", 3.0)]);
}

// --- Reaction Speed --------------------------------------------------------------------------

fn reaction_speed(name: &str) {
    assert_holds(&case(name, "reaction_speed", IndicatorUnit::Ratio));
}

#[test]
fn reaction_speed_defaults() {
    reaction_speed("reaction_speed_defaults");
}

#[test]
fn reaction_speed_lcg() {
    reaction_speed("reaction_speed_lcg");
}

#[test]
fn reaction_speed_short() {
    reaction_speed("reaction_speed_short");
}

#[test]
fn reaction_speed_rejects_invalid_configuration() {
    assert_rejected("reaction_speed", &[("atr_len", 0.0)]);
    assert_rejected("reaction_speed", &[("atr_base", 4.0)]);
    assert_rejected("reaction_speed", &[("swing_len", 1.0)]);
    assert_rejected("reaction_speed", &[("atr_move", 0.05)]);
    assert_rejected("reaction_speed", &[("react_window", 8.0)]);
}

// --- Flow Exhaustion -------------------------------------------------------------------------

fn flow_exhaustion(name: &str) {
    assert_holds(&case(name, "flow_exhaustion", IndicatorUnit::Ratio));
}

#[test]
fn flow_exhaustion_defaults() {
    flow_exhaustion("flow_exhaustion_defaults");
}

#[test]
fn flow_exhaustion_mirrored() {
    flow_exhaustion("flow_exhaustion_mirrored");
}

#[test]
fn flow_exhaustion_short() {
    flow_exhaustion("flow_exhaustion_short");
}

#[test]
fn flow_exhaustion_lag() {
    flow_exhaustion("flow_exhaustion_lag");
}

/// Without volume there is no MFI, so the first output comes later than `warmup_period()`, which
/// assumes volume from the first bar (plan 62 (b): a data-dependent edge without its own warmup
/// entry). Every per-bar expectation holds; the static warmup is the only reported deviation.
#[test]
fn flow_exhaustion_late_volume() {
    let failures = check(&case(
        "flow_exhaustion_late_volume",
        "flow_exhaustion",
        IndicatorUnit::Ratio,
    ));
    assert!(
        failures.len() == 1 && failures[0].contains("warmup_period expected 26"),
        "{failures:#?}"
    );
}

#[test]
fn flow_exhaustion_no_volume() {
    flow_exhaustion("flow_exhaustion_no_volume");
}

#[test]
fn flow_exhaustion_rejects_invalid_configuration() {
    assert_rejected("flow_exhaustion", &[("mfi_len", 1.0)]);
    assert_rejected("flow_exhaustion", &[("zone_lookback", 9.0)]);
    assert_rejected("flow_exhaustion", &[("confirm_bars", 6.0)]);
    assert_rejected("flow_exhaustion", &[("react_bars", 1.0)]);
    assert_rejected("flow_exhaustion", &[("pivot_left", 7.0)]);
}

// --- Coil Release ----------------------------------------------------------------------------

fn coil_release(name: &str) {
    assert_holds(&case(name, "coil_release", IndicatorUnit::Ratio));
}

#[test]
fn coil_release_defaults() {
    coil_release("coil_release_defaults");
}

#[test]
fn coil_release_mirrored() {
    coil_release("coil_release_mirrored");
}

#[test]
fn coil_release_short() {
    coil_release("coil_release_short");
}

#[test]
fn coil_release_rejects_invalid_configuration() {
    assert_rejected("coil_release", &[("len", 9.0)]);
    assert_rejected("coil_release", &[("break_len", 51.0)]);
    assert_rejected("coil_release", &[("thr_cont", 1.5)]);
    assert_rejected("coil_release", &[("chop_veto", 20.0)]);
    assert_rejected("coil_release", &[("length", 34.0)]);
}

// --- Compression Zones -----------------------------------------------------------------------

fn compression_zones(name: &str) {
    assert_holds(&case(name, "compression_zones", IndicatorUnit::Ratio));
}

#[test]
fn compression_zones_defaults() {
    compression_zones("compression_zones_defaults");
}

#[test]
fn compression_zones_short() {
    compression_zones("compression_zones_short");
}

#[test]
fn compression_zones_no_volume() {
    compression_zones("compression_zones_no_volume");
}

#[test]
fn compression_zones_rejects_invalid_configuration() {
    assert_rejected("compression_zones", &[("lookback", 7.0)]);
    assert_rejected("compression_zones", &[("min_zone_bars", 2.0)]);
    assert_rejected("compression_zones", &[("min_touches", 1.0)]);
    assert_rejected("compression_zones", &[("min_score", 90.0)]);
    assert_rejected("compression_zones", &[("watch", 20.0)]);
}

// --- Price-Average Relation ------------------------------------------------------------------

fn price_average_relation(name: &str) {
    assert_holds(&case(
        name,
        "price_average_relation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn price_average_relation_defaults() {
    price_average_relation("price_average_relation_defaults");
}

#[test]
fn price_average_relation_mirrored() {
    price_average_relation("price_average_relation_mirrored");
}

#[test]
fn price_average_relation_tiring_down() {
    price_average_relation("price_average_relation_tiring_down");
}

#[test]
fn price_average_relation_tiring_up() {
    price_average_relation("price_average_relation_tiring_up");
}

#[test]
fn price_average_relation_short() {
    price_average_relation("price_average_relation_short");
}

#[test]
fn price_average_relation_rejects_invalid_configuration() {
    assert_rejected("price_average_relation", &[("ma_len", 1.0)]);
    assert_rejected("price_average_relation", &[("ma_kind", 4.0)]);
    assert_rejected("price_average_relation", &[("atr_len", 1.0)]);
    assert_rejected("price_average_relation", &[("turn_level", 5.0)]);
    assert_rejected("price_average_relation", &[("turn_lookback", 151.0)]);
    assert_rejected("price_average_relation", &[("length", 50.0)]);
}

// --- Overextension Risk ----------------------------------------------------------------------

fn overextension_risk(name: &str) {
    assert_holds(&case(name, "overextension_risk", IndicatorUnit::Ratio));
}

#[test]
fn overextension_risk_defaults() {
    overextension_risk("overextension_risk_defaults");
}

#[test]
fn overextension_risk_range() {
    overextension_risk("overextension_risk_range");
}

#[test]
fn overextension_risk_low() {
    overextension_risk("overextension_risk_low");
}

#[test]
fn overextension_risk_first_touch() {
    overextension_risk("overextension_risk_first_touch");
}

#[test]
fn overextension_risk_no_volume() {
    overextension_risk("overextension_risk_no_volume");
}

#[test]
fn overextension_risk_rejects_invalid_configuration() {
    assert_rejected("overextension_risk", &[("w_stretch", -1.0)]);
    assert_rejected("overextension_risk", &[("base_threshold", 85.0)]);
    assert_rejected("overextension_risk", &[("confirm", 2.0)]);
    assert_rejected("overextension_risk", &[("ema_len", 1.0)]);
    assert_rejected("overextension_risk", &[("threshold", 40.0)]);
}

// --- Tradability Filter ----------------------------------------------------------------------

fn tradability_filter(name: &str) {
    assert_holds(&case(name, "tradability_filter", IndicatorUnit::Ratio));
}

#[test]
fn tradability_filter_defaults() {
    tradability_filter("tradability_filter_defaults");
}

#[test]
fn tradability_filter_short() {
    tradability_filter("tradability_filter_short");
}

#[test]
fn tradability_filter_loose() {
    tradability_filter("tradability_filter_loose");
}

#[test]
fn tradability_filter_rejects_invalid_configuration() {
    assert_rejected("tradability_filter", &[("quality_len", 7.0)]);
    assert_rejected("tradability_filter", &[("rank_len", 49.0)]);
    assert_rejected("tradability_filter", &[("balance_threshold", 95.0)]);
    assert_rejected("tradability_filter", &[("confirm_bars", 0.0)]);
    assert_rejected("tradability_filter", &[("min_state_bars", 3.0)]);
}

// --- Market State Chain ----------------------------------------------------------------------

fn market_state_chain(name: &str) {
    assert_holds(&case(name, "market_state_chain", IndicatorUnit::Ratio));
}

#[test]
fn market_state_chain_defaults() {
    market_state_chain("market_state_chain_defaults");
}

#[test]
fn market_state_chain_short() {
    market_state_chain("market_state_chain_short");
}

#[test]
fn market_state_chain_open() {
    market_state_chain("market_state_chain_open");
}

#[test]
fn market_state_chain_continuations() {
    market_state_chain("market_state_chain_continuations");
}

#[test]
fn market_state_chain_rejects_invalid_configuration() {
    assert_rejected("market_state_chain", &[("ema_len", 9.0)]);
    assert_rejected("market_state_chain", &[("rank_len", 19.0)]);
    assert_rejected("market_state_chain", &[("lookback", 1501.0)]);
    assert_rejected("market_state_chain", &[("min_dwell", 0.0)]);
    assert_rejected("market_state_chain", &[("conf_th", 101.0)]);
    assert_rejected("market_state_chain", &[("laplace", 1.0)]);
}

// --- RSI Cycle Statistics --------------------------------------------------------------------

fn rsi_cycle_stats(name: &str) {
    assert_holds(&case(name, "rsi_cycle_stats", IndicatorUnit::Ratio));
}

#[test]
fn rsi_cycle_stats_defaults() {
    rsi_cycle_stats("rsi_cycle_stats_defaults");
}

#[test]
fn rsi_cycle_stats_short() {
    rsi_cycle_stats("rsi_cycle_stats_short");
}

#[test]
fn rsi_cycle_stats_rejects_invalid_configuration() {
    assert_rejected("rsi_cycle_stats", &[("rsi_len", 1.0)]);
    assert_rejected("rsi_cycle_stats", &[("overbought", 50.0)]);
    assert_rejected("rsi_cycle_stats", &[("oversold", 50.0)]);
    assert_rejected("rsi_cycle_stats", &[("lookback", 99.0)]);
    assert_rejected("rsi_cycle_stats", &[("max_ci", 0.5)]);
    assert_rejected("rsi_cycle_stats", &[("max_samples", 1500.0)]);
}
