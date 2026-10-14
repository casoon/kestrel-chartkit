//! Contracts of the composites b family, from `tests/fixtures/contract_composites_b.txt`.

use super::{assert_holds, assert_rejected, Coverage, Status};
use crate::common::contract::{load, ContractCase};
use kestrel_chartkit::IndicatorUnit;

const FIXTURE: &str = include_str!("../fixtures/contract_composites_b.txt");

fn case(name: &str, indicator: &str, unit: IndicatorUnit) -> ContractCase {
    load(FIXTURE, name, indicator, unit)
}

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[];

/// Undocumented alert strengths are pinned at the implemented convention, stated per indicator
/// in `contract_composites_b.py`; those rows say so in their alert status.
pub const COVERAGE: &[Coverage] = &[
    Coverage::full("buy_sell_pressure"),
    Coverage {
        indicator: "trend_quality",
        values: Status::Specified,
        alerts: Status::Specified,
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "multi_factor",
        values: Status::Specified,
        alerts: Status::Specified,
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "leg_conviction",
        values: Status::Specified,
        alerts: Status::Partial("strength (conviction drop / 100, 0.05..1) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "sweep_reversal_score",
        values: Status::Partial("a bar firing long and short (long_*/short_* fields) not cased"),
        alerts: Status::Partial("strength (quality / 2) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "trend_break_risk",
        values: Status::Specified,
        alerts: Status::Partial(
            "break_critical and bull_break_pressure never reached; strengths not documented",
        ),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "equal_level_liquidity",
        values: Status::Specified,
        alerts: Status::Partial(
            "strengths (0.6 events, score / 100 reclaims, 0.4 near) not documented",
        ),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "leg_character",
        values: Status::Specified,
        alerts: Status::Partial("strengths (score / 100, 0.3 character_lost) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "accumulation_phase",
        values: Status::Specified,
        alerts: Status::Partial(
            "breakout/breakdown ready never reached; strengths (0.4/0.6/0.8) not documented",
        ),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "reversal_setup_score",
        values: Status::Specified,
        alerts: Status::Partial("bull/bear_action never reached; strengths not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "structure_zones",
        values: Status::Partial(
            "real volume from the first bar moves the first output to bar 20 (warmup_period 15); \
             cased with volume starting after the warm-up",
        ),
        alerts: Status::Partial("strength (0.5) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "structural_flow",
        values: Status::Specified,
        alerts: Status::Partial(
            "bull/bear_flow_flip never reached; strength (flow strength, 0.05..1) not documented",
        ),
        configuration: Status::Specified,
    },
];

// --- Buy/sell pressure -----------------------------------------------------------------------

#[test]
fn buy_sell_pressure_defaults() {
    assert_holds(&case(
        "buy_sell_pressure_defaults",
        "buy_sell_pressure",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn buy_sell_pressure_period3() {
    assert_holds(&case(
        "buy_sell_pressure_period3",
        "buy_sell_pressure",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn buy_sell_pressure_edges() {
    assert_holds(&case(
        "buy_sell_pressure_edges",
        "buy_sell_pressure",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn buy_sell_pressure_rejects_invalid_configuration() {
    assert_rejected("buy_sell_pressure", &[("period", 0.0)]);
    assert_rejected("buy_sell_pressure", &[("period", 1.5)]);
    assert_rejected("buy_sell_pressure", &[("length", 14.0)]);
}

// --- Trend quality ---------------------------------------------------------------------------

#[test]
fn trend_quality_defaults() {
    assert_holds(&case(
        "trend_quality_defaults",
        "trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_period5() {
    assert_holds(&case(
        "trend_quality_period5",
        "trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_novolume() {
    assert_holds(&case(
        "trend_quality_novolume",
        "trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_flat() {
    assert_holds(&case(
        "trend_quality_flat",
        "trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_rejects_invalid_configuration() {
    assert_rejected("trend_quality", &[("period", 1.0)]);
    assert_rejected("trend_quality", &[("period", 10001.0)]);
    assert_rejected("trend_quality", &[("perod", 14.0)]);
}

// --- Multi-factor ----------------------------------------------------------------------------

#[test]
fn multi_factor_defaults() {
    assert_holds(&case(
        "multi_factor_defaults",
        "multi_factor",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn multi_factor_period4() {
    assert_holds(&case(
        "multi_factor_period4",
        "multi_factor",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn multi_factor_rejects_invalid_configuration() {
    assert_rejected("multi_factor", &[("period", 1.0)]);
    assert_rejected("multi_factor", &[("period", 2.5)]);
    assert_rejected("multi_factor", &[("rsi_len", 14.0)]);
}

// --- Leg conviction --------------------------------------------------------------------------

#[test]
fn leg_conviction_defaults() {
    assert_holds(&case(
        "leg_conviction_defaults",
        "leg_conviction",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_conviction_short_pivots() {
    assert_holds(&case(
        "leg_conviction_short_pivots",
        "leg_conviction",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_conviction_novolume() {
    assert_holds(&case(
        "leg_conviction_novolume",
        "leg_conviction",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_conviction_rejects_invalid_configuration() {
    assert_rejected("leg_conviction", &[("pivot_left", 0.0)]);
    assert_rejected("leg_conviction", &[("max_leg", 1001.0)]);
    assert_rejected("leg_conviction", &[("speed_ref", 0.0)]);
    assert_rejected("leg_conviction", &[("w_volume", 1.5)]);
    assert_rejected("leg_conviction", &[("pivot_len", 5.0)]);
}

// --- Sweep reversal score --------------------------------------------------------------------

#[test]
fn sweep_reversal_score_defaults() {
    assert_holds(&case(
        "sweep_reversal_score_defaults",
        "sweep_reversal_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn sweep_reversal_score_short_windows() {
    assert_holds(&case(
        "sweep_reversal_score_short_windows",
        "sweep_reversal_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn sweep_reversal_score_rejects_invalid_configuration() {
    assert_rejected("sweep_reversal_score", &[("swing_len", 1.0)]);
    assert_rejected("sweep_reversal_score", &[("min_quality_long", 3.0)]);
    assert_rejected("sweep_reversal_score", &[("atr_low_factor", -0.1)]);
    assert_rejected("sweep_reversal_score", &[("cooldown", 0.5)]);
    assert_rejected("sweep_reversal_score", &[("sl_atr_buffer", 0.3)]);
}

// --- Trend break risk ------------------------------------------------------------------------

#[test]
fn trend_break_risk_defaults() {
    assert_holds(&case(
        "trend_break_risk_defaults",
        "trend_break_risk",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn trend_break_risk_fast() {
    assert_holds(&case(
        "trend_break_risk_fast",
        "trend_break_risk",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn trend_break_risk_crash() {
    assert_holds(&case(
        "trend_break_risk_crash",
        "trend_break_risk",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn trend_break_risk_rejects_invalid_configuration() {
    assert_rejected("trend_break_risk", &[("bias_len", 1.0)]);
    assert_rejected("trend_break_risk", &[("near_atr", 0.0)]);
    assert_rejected("trend_break_risk", &[("smooth_len", 101.0)]);
    assert_rejected("trend_break_risk", &[("w_near", 25.0)]);
}

// --- Equal-level liquidity -------------------------------------------------------------------

#[test]
fn equal_level_liquidity_defaults() {
    assert_holds(&case(
        "equal_level_liquidity_defaults",
        "equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn equal_level_liquidity_novolume() {
    assert_holds(&case(
        "equal_level_liquidity_novolume",
        "equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn equal_level_liquidity_tuned() {
    assert_holds(&case(
        "equal_level_liquidity_tuned",
        "equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn equal_level_liquidity_rejects_invalid_configuration() {
    assert_rejected("equal_level_liquidity", &[("pivot_len", 0.0)]);
    assert_rejected("equal_level_liquidity", &[("min_touches", 21.0)]);
    assert_rejected("equal_level_liquidity", &[("max_distance_atr", 0.05)]);
    assert_rejected("equal_level_liquidity", &[("eq_tolerance_atr", -0.1)]);
    assert_rejected("equal_level_liquidity", &[("premium_score", 65.0)]);
}

// --- Accumulation phase ----------------------------------------------------------------------

#[test]
fn accumulation_phase_base() {
    assert_holds(&case(
        "accumulation_phase_base",
        "accumulation_phase",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn accumulation_phase_top() {
    assert_holds(&case(
        "accumulation_phase_top",
        "accumulation_phase",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn accumulation_phase_fast() {
    assert_holds(&case(
        "accumulation_phase_fast",
        "accumulation_phase",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn accumulation_phase_rejects_invalid_configuration() {
    assert_rejected("accumulation_phase", &[("lookback", 9.0)]);
    assert_rejected("accumulation_phase", &[("min_phase_bars", 2.0)]);
    assert_rejected("accumulation_phase", &[("er_range", 0.01)]);
    assert_rejected("accumulation_phase", &[("bbw_compress", 6.0)]);
    assert_rejected("accumulation_phase", &[("pivot_len", 3.0)]);
}

// --- Leg character ---------------------------------------------------------------------------

#[test]
fn leg_character_defaults() {
    assert_holds(&case(
        "leg_character_defaults",
        "leg_character",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_character_short_legs() {
    assert_holds(&case(
        "leg_character_short_legs",
        "leg_character",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_character_rejects_invalid_configuration() {
    assert_rejected("leg_character", &[("max_leg_bars", 19.0)]);
    assert_rejected("leg_character", &[("mature_bars", 1.0)]);
    assert_rejected("leg_character", &[("impulse_threshold", 49.0)]);
    assert_rejected("leg_character", &[("change_buffer", 26.0)]);
    assert_rejected("leg_character", &[("min_separation", 18.0)]);
}

// --- Reversal setup score --------------------------------------------------------------------

#[test]
fn reversal_setup_score_volume() {
    assert_holds(&case(
        "reversal_setup_score_volume",
        "reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_novolume() {
    assert_holds(&case(
        "reversal_setup_score_novolume",
        "reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_candidates() {
    assert_holds(&case(
        "reversal_setup_score_candidates",
        "reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_long_invalidated() {
    assert_holds(&case(
        "reversal_setup_score_long_invalidated",
        "reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_rejects_invalid_configuration() {
    assert_rejected("reversal_setup_score", &[("rsi_len", 14.0)]);
    assert_rejected("reversal_setup_score", &[("momentum_mix", 0.0)]);
}

// --- Structure zones -------------------------------------------------------------------------

#[test]
fn structure_zones_volume() {
    assert_holds(&case(
        "structure_zones_volume",
        "structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_holds() {
    assert_holds(&case(
        "structure_zones_holds",
        "structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_novolume() {
    assert_holds(&case(
        "structure_zones_novolume",
        "structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_rejects_invalid_configuration() {
    assert_rejected("structure_zones", &[("atr_len", 14.0)]);
    assert_rejected("structure_zones", &[("zone_atr", 3.0)]);
}

// --- Structural flow -------------------------------------------------------------------------

#[test]
fn structural_flow_volume() {
    assert_holds(&case(
        "structural_flow_volume",
        "structural_flow",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structural_flow_novolume() {
    assert_holds(&case(
        "structural_flow_novolume",
        "structural_flow",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structural_flow_transitions() {
    assert_holds(&case(
        "structural_flow_transitions",
        "structural_flow",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structural_flow_rejects_invalid_configuration() {
    assert_rejected("structural_flow", &[("preset", 1.0)]);
    assert_rejected("structural_flow", &[("len_slow", 96.0)]);
}
