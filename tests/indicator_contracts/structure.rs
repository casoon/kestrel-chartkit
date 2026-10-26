//! Contracts of the structure family, from `tests/fixtures/contract_structure.txt`.
//!
//! Expected sequences come from `reference/kestrel_reference/fixtures/contract_structure.py`:
//! the documented rule of each detector as a second implementation, run over hand-built
//! scenarios whose decisive bars the generator names. Conventions the documentation leaves open
//! are stated there.

use super::{assert_holds, assert_rejected, Coverage, Status};
use crate::common::contract::load;
use kestrel_chartkit::IndicatorUnit;

const FIXTURE: &str = include_str!("../fixtures/contract_structure.txt");

fn holds(name: &str, indicator: &str, unit: IndicatorUnit) {
    assert_holds(&load(FIXTURE, name, indicator, unit));
}

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &[];

const DIVERGENCE_OSCILLATORS: &str =
    "oscillators RSI (0) and Williams %R (3); WaveTrend, Stochastic RSI and CCI not specified";

pub const COVERAGE: &[Coverage] = &[
    Coverage {
        indicator: "candle_classification",
        values: Status::Partial(
            "cases of at most two bars: one- and two-bar patterns, metrics, range_atr and \
             trend_context; the three- to five-bar patterns are not specified",
        ),
        alerts: Status::Partial(
            "one- and two-bar patterns; the three- to five-bar patterns are not specified",
        ),
        configuration: Status::Specified,
    },
    Coverage::full("liquidity_fvg"),
    Coverage::full("market_structure_breaks"),
    Coverage::full("pivots_structure"),
    Coverage::full("zigzag"),
    Coverage {
        indicator: "pivot_sets",
        values: Status::Specified,
        alerts: Status::Specified,
        configuration: Status::Partial(
            "the registry builds Classic only and takes no parameter; the other set types \
             (typed-only `pivot_type`) are not specified",
        ),
    },
    Coverage::full("bos_choch"),
    Coverage::full("liquidity_sweeps"),
    Coverage::full("liquidity_pools"),
    Coverage::full("zigzag_advanced"),
    Coverage::full("zone_rejection"),
    Coverage::full("double_pattern"),
    Coverage::full("bottom_formation"),
    Coverage {
        indicator: "pivot_divergence",
        values: Status::Partial(DIVERGENCE_OSCILLATORS),
        alerts: Status::Partial(DIVERGENCE_OSCILLATORS),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "oscillator_pivot_divergence",
        values: Status::Partial(DIVERGENCE_OSCILLATORS),
        alerts: Status::Partial(DIVERGENCE_OSCILLATORS),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "wavetrend_divergence",
        values: Status::NotSpecified(FESTE_OSZILLATOREN),
        alerts: Status::NotSpecified(FESTE_OSZILLATOREN),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "stoch_rsi_divergence",
        values: Status::NotSpecified(FESTE_OSZILLATOREN),
        alerts: Status::NotSpecified(FESTE_OSZILLATOREN),
        configuration: Status::Specified,
    },
    Coverage::full("wyckoff"),
];

const FESTE_OSZILLATOREN: &str =
    "oscillator_pivot_divergence fixed to WaveTrend / Stochastic RSI; the reference generator has \
     neither oscillator (see DIVERGENCE_OSCILLATORS), the engine is fixed by the RSI and Williams %R \
     cases";

// --- liquidity_fvg ---------------------------------------------------------------------------

#[test]
fn liquidity_fvg_defaults() {
    holds(
        "liquidity_fvg_defaults",
        "liquidity_fvg",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_fvg_short() {
    holds(
        "liquidity_fvg_short",
        "liquidity_fvg",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_fvg_touch() {
    holds(
        "liquidity_fvg_touch",
        "liquidity_fvg",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_fvg_rejects_invalid_configuration() {
    assert_rejected("liquidity_fvg", &[("lookback", 0.0)]);
    assert_rejected("liquidity_fvg", &[("lookback", 2.5)]);
    assert_rejected("liquidity_fvg", &[("lookback", f64::NAN)]);
    assert_rejected("liquidity_fvg", &[("look_back", 20.0)]);
}

// --- market_structure_breaks -----------------------------------------------------------------

#[test]
fn market_structure_breaks_defaults() {
    holds(
        "market_structure_breaks_defaults",
        "market_structure_breaks",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn market_structure_breaks_short() {
    holds(
        "market_structure_breaks_short",
        "market_structure_breaks",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn market_structure_breaks_equal_highs() {
    holds(
        "market_structure_breaks_equal_highs",
        "market_structure_breaks",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn market_structure_breaks_rejects_invalid_configuration() {
    assert_rejected("market_structure_breaks", &[("lookback", 0.0)]);
    assert_rejected("market_structure_breaks", &[("lookback", 10001.0)]);
    assert_rejected("market_structure_breaks", &[("pivot_len", 5.0)]);
}

// --- pivots_structure ------------------------------------------------------------------------

#[test]
fn pivots_structure_defaults() {
    holds(
        "pivots_structure_defaults",
        "pivots_structure",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn pivots_structure_short() {
    holds(
        "pivots_structure_short",
        "pivots_structure",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn pivots_structure_rejects_invalid_configuration() {
    assert_rejected("pivots_structure", &[("left_bars", 0.0)]);
    assert_rejected("pivots_structure", &[("right_bars", 0.0)]);
    assert_rejected("pivots_structure", &[("score_window", 0.0)]);
    assert_rejected("pivots_structure", &[("left", 5.0)]);
}

// --- zigzag ----------------------------------------------------------------------------------

#[test]
fn zigzag_defaults() {
    holds("zigzag_defaults", "zigzag", IndicatorUnit::Price);
}

#[test]
fn zigzag_fine() {
    holds("zigzag_fine", "zigzag", IndicatorUnit::Price);
}

#[test]
fn zigzag_rejects_invalid_configuration() {
    assert_rejected("zigzag", &[("depth", 1.0)]);
    assert_rejected("zigzag", &[("deviation_pct", 0.0)]);
    assert_rejected("zigzag", &[("deviation_pct", 101.0)]);
    assert_rejected("zigzag", &[("deviation", 5.0)]);
}

// --- pivot_sets ------------------------------------------------------------------------------

#[test]
fn pivot_sets_days() {
    holds("pivot_sets_days", "pivot_sets", IndicatorUnit::Price);
}

#[test]
fn pivot_sets_rejects_invalid_configuration() {
    // `pivot_type` is an enum only `build_typed` reads; the plain registry takes no parameter.
    assert_rejected("pivot_sets", &[("pivot_type", 1.0)]);
    assert_rejected("pivot_sets", &[("period", 1.0)]);
}

// --- bos_choch -------------------------------------------------------------------------------

#[test]
fn bos_choch_defaults() {
    holds("bos_choch_defaults", "bos_choch", IndicatorUnit::Unitless);
}

#[test]
fn bos_choch_short() {
    holds("bos_choch_short", "bos_choch", IndicatorUnit::Unitless);
}

#[test]
fn bos_choch_equal_highs() {
    holds(
        "bos_choch_equal_highs",
        "bos_choch",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn bos_choch_rejects_invalid_configuration() {
    assert_rejected("bos_choch", &[("pivot_len", 1.0)]);
    assert_rejected("bos_choch", &[("pivot_len", 5.5)]);
    assert_rejected("bos_choch", &[("lookback", 5.0)]);
}

// --- liquidity_sweeps ------------------------------------------------------------------------

#[test]
fn liquidity_sweeps_defaults() {
    holds(
        "liquidity_sweeps_defaults",
        "liquidity_sweeps",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_sweeps_tight() {
    holds(
        "liquidity_sweeps_tight",
        "liquidity_sweeps",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_sweeps_rejects_invalid_configuration() {
    assert_rejected("liquidity_sweeps", &[("pivot_len", 1.0)]);
    assert_rejected("liquidity_sweeps", &[("tolerance_pct", 0.0)]);
    assert_rejected("liquidity_sweeps", &[("tolerance_pct", 101.0)]);
    assert_rejected("liquidity_sweeps", &[("tolerance", 0.2)]);
}

// --- liquidity_pools -------------------------------------------------------------------------

#[test]
fn liquidity_pools_defaults() {
    holds(
        "liquidity_pools_defaults",
        "liquidity_pools",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_pools_tight() {
    holds(
        "liquidity_pools_tight",
        "liquidity_pools",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn liquidity_pools_rejects_invalid_configuration() {
    assert_rejected("liquidity_pools", &[("pivot_len", 1.0)]);
    assert_rejected("liquidity_pools", &[("tolerance_pct", 0.0)]);
    assert_rejected("liquidity_pools", &[("tolerance_pct", f64::INFINITY)]);
    assert_rejected("liquidity_pools", &[("tolerance", 0.2)]);
}

// --- zigzag_advanced -------------------------------------------------------------------------

#[test]
fn zigzag_advanced_defaults() {
    holds(
        "zigzag_advanced_defaults",
        "zigzag_advanced",
        IndicatorUnit::Price,
    );
}

#[test]
fn zigzag_advanced_coarse() {
    holds(
        "zigzag_advanced_coarse",
        "zigzag_advanced",
        IndicatorUnit::Price,
    );
}

#[test]
fn zigzag_advanced_backstep() {
    holds(
        "zigzag_advanced_backstep",
        "zigzag_advanced",
        IndicatorUnit::Price,
    );
}

#[test]
fn zigzag_advanced_rejects_invalid_configuration() {
    assert_rejected("zigzag_advanced", &[("depth", 0.0)]);
    assert_rejected("zigzag_advanced", &[("deviation_pct", 0.0)]);
    assert_rejected("zigzag_advanced", &[("atr_len", 0.0)]);
    // deviation_mode is 0 (percent) or 1 (ATR multiples); percent ends at 100.
    assert_rejected("zigzag_advanced", &[("deviation_mode", 2.0)]);
    assert_rejected("zigzag_advanced", &[("deviation_pct", 150.0)]);
    assert_rejected("zigzag_advanced", &[("back_step", 2.0)]);
}

// --- zone_rejection --------------------------------------------------------------------------

#[test]
fn zone_rejection_defaults() {
    holds(
        "zone_rejection_defaults",
        "zone_rejection",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn zone_rejection_strict() {
    holds(
        "zone_rejection_strict",
        "zone_rejection",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn zone_rejection_rejects_invalid_configuration() {
    assert_rejected("zone_rejection", &[("pivot_len", 0.0)]);
    assert_rejected("zone_rejection", &[("atr_len", 0.0)]);
    assert_rejected("zone_rejection", &[("tolerance_atr", -0.1)]);
    assert_rejected("zone_rejection", &[("max_width_atr", 101.0)]);
    assert_rejected("zone_rejection", &[("min_touches", 0.0)]);
    assert_rejected("zone_rejection", &[("touches", 2.0)]);
}

// --- double_pattern --------------------------------------------------------------------------

#[test]
fn double_pattern_defaults() {
    holds(
        "double_pattern_defaults",
        "double_pattern",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn double_pattern_fast() {
    holds(
        "double_pattern_fast",
        "double_pattern",
        IndicatorUnit::Unitless,
    );
}

/// The documentation confirms "within `max_wait` bars after forming"; the implementation still
/// confirms on bar `max_wait + 1` (the expiry check runs after the neckline check). Bars 30 and
/// 58: formed on 26 and 54, `max_wait` 3, the neckline close comes 4 bars later.
#[test]
fn double_pattern_expiry() {
    holds(
        "double_pattern_expiry",
        "double_pattern",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn double_pattern_rejects_invalid_configuration() {
    assert_rejected("double_pattern", &[("pivot_len", 0.0)]);
    assert_rejected("double_pattern", &[("atr_len", 0.0)]);
    assert_rejected("double_pattern", &[("tolerance_atr", -1.0)]);
    assert_rejected("double_pattern", &[("min_depth_atr", 100.5)]);
    assert_rejected("double_pattern", &[("max_gap", 1.0)]);
    assert_rejected("double_pattern", &[("max_wait", 0.0)]);
    assert_rejected("double_pattern", &[("wait", 50.0)]);
}

// --- bottom_formation ------------------------------------------------------------------------

#[test]
fn bottom_formation_defaults() {
    holds(
        "bottom_formation_defaults",
        "bottom_formation",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn bottom_formation_mirrored() {
    holds(
        "bottom_formation_mirrored",
        "bottom_formation",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn bottom_formation_expiry() {
    holds(
        "bottom_formation_expiry",
        "bottom_formation",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn bottom_formation_rejects_invalid_configuration() {
    assert_rejected("bottom_formation", &[("pivot_len", 0.0)]);
    assert_rejected("bottom_formation", &[("max_stage_bars", 0.0)]);
    assert_rejected("bottom_formation", &[("retest_atr", -0.5)]);
    assert_rejected("bottom_formation", &[("retest", 0.5)]);
}

// --- pivot_divergence, oscillator_pivot_divergence -------------------------------------------

#[test]
fn pivot_divergence_defaults() {
    holds(
        "pivot_divergence_defaults",
        "pivot_divergence",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn pivot_divergence_williams() {
    holds(
        "pivot_divergence_williams",
        "pivot_divergence",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn pivot_divergence_rejects_invalid_configuration() {
    assert_rejected("pivot_divergence", &[("oscillator", 5.0)]);
    assert_rejected("pivot_divergence", &[("left", 0.0)]);
    assert_rejected("pivot_divergence", &[("right", 1001.0)]);
    assert_rejected(
        "pivot_divergence",
        &[("min_distance", 20.0), ("max_distance", 20.0)],
    );
    assert_rejected("pivot_divergence", &[("max_prior_pivots", 0.0)]);
    assert_rejected("pivot_divergence", &[("lookback", 7.0)]);
}

#[test]
fn oscillator_pivot_divergence_defaults() {
    holds(
        "oscillator_pivot_divergence_defaults",
        "oscillator_pivot_divergence",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn oscillator_pivot_divergence_williams() {
    holds(
        "oscillator_pivot_divergence_williams",
        "oscillator_pivot_divergence",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn fixed_oscillator_divergences_reject_invalid_configuration() {
    for name in ["wavetrend_divergence", "stoch_rsi_divergence"] {
        assert_rejected(name, &[("left", 0.0)]);
        assert_rejected(name, &[("min_distance", 61.0), ("max_distance", 60.0)]);
        assert_rejected(name, &[("oscillator", 1.0)]);
    }
}

#[test]
fn oscillator_pivot_divergence_rejects_invalid_configuration() {
    assert_rejected("oscillator_pivot_divergence", &[("oscillator", 4.5)]);
    assert_rejected("oscillator_pivot_divergence", &[("left", 0.0)]);
    assert_rejected(
        "oscillator_pivot_divergence",
        &[("min_distance", 61.0), ("max_distance", 60.0)],
    );
    assert_rejected(
        "oscillator_pivot_divergence",
        &[("max_prior_pivots", 101.0)],
    );
    assert_rejected("oscillator_pivot_divergence", &[("pivots", 1.0)]);
}

// --- wyckoff ---------------------------------------------------------------------------------

#[test]
fn wyckoff_accumulation() {
    holds("wyckoff_accumulation", "wyckoff", IndicatorUnit::Unitless);
}

#[test]
fn wyckoff_distribution() {
    holds("wyckoff_distribution", "wyckoff", IndicatorUnit::Unitless);
}

#[test]
fn wyckoff_climax() {
    holds("wyckoff_climax", "wyckoff", IndicatorUnit::Unitless);
}

#[test]
fn wyckoff_failed_breakout() {
    holds(
        "wyckoff_failed_breakout",
        "wyckoff",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn wyckoff_wide() {
    holds("wyckoff_wide", "wyckoff", IndicatorUnit::Unitless);
}

/// With `range_lookback` below 14 the window is full before the ATR(14) exists: the first output
/// comes with bar 14, `warmup_period()` says `range_lookback` (10). Plan 62: warmup = first
/// output.
#[test]
fn wyckoff_short_lookback() {
    holds("wyckoff_short_lookback", "wyckoff", IndicatorUnit::Unitless);
}

#[test]
fn wyckoff_rejects_invalid_configuration() {
    assert_rejected("wyckoff", &[("range_lookback", 2.0)]);
    assert_rejected("wyckoff", &[("range_atr_max", 0.05)]);
    assert_rejected("wyckoff", &[("min_range_bars", 1.0)]);
    assert_rejected("wyckoff", &[("lookback", 20.0)]);
}

// --- candle_classification -------------------------------------------------------------------

#[test]
fn candle_classification_pinbar() {
    holds(
        "candle_classification_pinbar",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_engulfing() {
    holds(
        "candle_classification_engulfing",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_harami_cross() {
    holds(
        "candle_classification_harami_cross",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_kicking() {
    holds(
        "candle_classification_kicking",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_tweezer() {
    holds(
        "candle_classification_tweezer",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_homing_pigeon() {
    holds(
        "candle_classification_homing_pigeon",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_gravestone() {
    holds(
        "candle_classification_gravestone",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_flat() {
    holds(
        "candle_classification_flat",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_hammer_trend() {
    holds(
        "candle_classification_hammer_trend",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_hanging_man() {
    holds(
        "candle_classification_hanging_man",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_size_floor() {
    holds(
        "candle_classification_size_floor",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_thresholds() {
    holds(
        "candle_classification_thresholds",
        "candle_classification",
        IndicatorUnit::Unitless,
    );
}

#[test]
fn candle_classification_rejects_invalid_configuration() {
    assert_rejected("candle_classification", &[("pin_wick_min", 0.05)]);
    assert_rejected("candle_classification", &[("pin_close_pos", 0.4)]);
    assert_rejected("candle_classification", &[("marubozu_body_min", 1.1)]);
    assert_rejected("candle_classification", &[("doji_body_max", 0.5)]);
    assert_rejected("candle_classification", &[("tweezer_tolerance", 0.2)]);
    assert_rejected("candle_classification", &[("atr_len", 0.0)]);
    assert_rejected("candle_classification", &[("trend_lookback", 0.0)]);
    assert_rejected("candle_classification", &[("trend_min_atr", -1.0)]);
    assert_rejected("candle_classification", &[("pin_wick", 0.55)]);
}
