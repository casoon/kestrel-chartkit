//! Contracts of the volume family, from `tests/fixtures/contract_volume.txt`
//! (`reference/kestrel_reference/fixtures/contract_volume.py`, whose docstring lists the
//! conventions the type documentation leaves open).

use super::{assert_holds, assert_rejected, Coverage, Status::*};
use crate::common::contract::{load, ContractCase};
use kestrel_chartkit::IndicatorUnit;

const FIXTURE: &str = include_str!("../fixtures/contract_volume.txt");

fn case(name: &str, indicator: &str, unit: IndicatorUnit) -> ContractCase {
    load(FIXTURE, name, indicator, unit)
}

/// Indicators of this family whose contract is not yet written. Moved into [`COVERAGE`] once
/// their cases exist; a name in neither list fails the matrix.
pub const PENDING: &[&str] = &["true_range_money_flow"];

pub const COVERAGE: &[Coverage] = &[
    Coverage::full("pvt"),
    Coverage::full("efi"),
    Coverage {
        indicator: "rvat",
        values: Partial(
            "bar_seconds 900 only (the contract bars are 15 minutes apart); missing slots and \
             negative volume not covered",
        ),
        alerts: Specified,
        configuration: Specified,
    },
    Coverage {
        indicator: "hires_volume_flow",
        values: Partial("bars without range not covered (not documented on the type)"),
        alerts: Specified,
        configuration: Specified,
    },
    Coverage {
        indicator: "extended_volume_profile",
        values: Partial(
            "value (POC) only; the profile, delta-profile and zone artifacts are not checked",
        ),
        alerts: Specified,
        configuration: Specified,
    },
    Coverage {
        indicator: "persistent_volume_profile",
        values: Partial(
            "value (POC) only; the profile, absorption and zone artifacts are not checked",
        ),
        alerts: Partial("bin_expired is not documented on the type; pinned to the catalog's rule"),
        configuration: Specified,
    },
    Coverage {
        indicator: "volume_profile",
        values: Specified,
        alerts: Partial(
            "price_at_poc/price_above_vah/price_below_val are not documented on the type; pinned \
             to the catalog's rule",
        ),
        configuration: Specified,
    },
    Coverage::full("money_flow_profile"),
    Coverage::full("tpo_profile"),
    Coverage::full("volume_flow_indicator"),
    Coverage::full("volume_zone_oscillator"),
    Coverage::full("volume_price_confirmation"),
    Coverage::full("market_facilitation"),
    Coverage::full("volume"),
    Coverage {
        indicator: "rvol",
        values: Specified,
        alerts: Partial(
            "extreme_rvol (>= 2.5) is not documented on the type; pinned to the catalog's rule",
        ),
        configuration: Specified,
    },
    Coverage::full("obv"),
    Coverage::full("cmf"),
    Coverage::full("acc_dist"),
    Coverage::full("cvd"),
    Coverage::full("klinger"),
    Coverage::full("eom"),
    Coverage::full("nvi"),
    Coverage::full("pvi"),
    Coverage::full("chaikin_oscillator"),
];

// --- Volume, RVOL, OBV, CMF, A/D -------------------------------------------------------------

#[test]
fn volume_defaults() {
    assert_holds(&case("volume_defaults", "volume", IndicatorUnit::Volume));
}

#[test]
fn volume_short_zero() {
    assert_holds(&case("volume_short_zero", "volume", IndicatorUnit::Volume));
}

#[test]
fn volume_rejects_invalid_configuration() {
    assert_rejected("volume", &[("ma_period", 0.0)]);
    assert_rejected("volume", &[("ma_period", 2.5)]);
    assert_rejected("volume", &[("period", 20.0)]);
}

#[test]
fn rvol_defaults() {
    assert_holds(&case("rvol_defaults", "rvol", IndicatorUnit::Ratio));
}

#[test]
fn rvol_nan_volume() {
    assert_holds(&case("rvol_nan_volume", "rvol", IndicatorUnit::Ratio));
}

#[test]
fn rvol_threshold() {
    assert_holds(&case("rvol_threshold", "rvol", IndicatorUnit::Ratio));
}

#[test]
fn rvol_rejects_invalid_configuration() {
    assert_rejected("rvol", &[("period", 0.0)]);
    assert_rejected("rvol", &[("period", f64::NAN)]);
    assert_rejected("rvol", &[("ma_period", 20.0)]);
}

#[test]
fn obv_equal_closes() {
    assert_holds(&case("obv_equal_closes", "obv", IndicatorUnit::Volume));
}

#[test]
fn obv_nan_volume() {
    assert_holds(&case("obv_nan_volume", "obv", IndicatorUnit::Volume));
}

#[test]
fn obv_rejects_invalid_configuration() {
    assert_rejected("obv", &[("period", 1.0)]);
}

#[test]
fn cmf_defaults() {
    assert_holds(&case("cmf_defaults", "cmf", IndicatorUnit::Ratio));
}

#[test]
fn cmf_short() {
    assert_holds(&case("cmf_short", "cmf", IndicatorUnit::Ratio));
}

#[test]
fn cmf_nan_volume() {
    assert_holds(&case("cmf_nan_volume", "cmf", IndicatorUnit::Ratio));
}

#[test]
fn cmf_zero_window() {
    assert_holds(&case("cmf_zero_window", "cmf", IndicatorUnit::Ratio));
}

#[test]
fn cmf_rejects_invalid_configuration() {
    assert_rejected("cmf", &[("period", 0.0)]);
    assert_rejected("cmf", &[("period", 10001.0)]);
    assert_rejected("cmf", &[("length", 20.0)]);
}

#[test]
fn acc_dist_shaped() {
    assert_holds(&case("acc_dist_shaped", "acc_dist", IndicatorUnit::Volume));
}

#[test]
fn acc_dist_rejects_invalid_configuration() {
    assert_rejected("acc_dist", &[("period", 1.0)]);
}

// --- CVD and the high-resolution flow --------------------------------------------------------

#[test]
fn cvd_shaped() {
    assert_holds(&case("cvd_shaped", "cvd", IndicatorUnit::Volume));
}

/// A bar without range has no close location: its volume splits evenly (share 0.5).
#[test]
fn cvd_zero_range() {
    assert_holds(&case("cvd_zero_range", "cvd", IndicatorUnit::Volume));
}

#[test]
fn cvd_rejects_invalid_configuration() {
    assert_rejected("cvd", &[("period", 1.0)]);
}

#[test]
fn hires_volume_flow_defaults() {
    assert_holds(&case(
        "hires_volume_flow_defaults",
        "hires_volume_flow",
        IndicatorUnit::Volume,
    ));
}

#[test]
fn hires_volume_flow_short() {
    assert_holds(&case(
        "hires_volume_flow_short",
        "hires_volume_flow",
        IndicatorUnit::Volume,
    ));
}

#[test]
fn hires_volume_flow_rejects_invalid_configuration() {
    assert_rejected("hires_volume_flow", &[("window_len", 1.0)]);
    assert_rejected("hires_volume_flow", &[("window_len", 20.5)]);
    assert_rejected("hires_volume_flow", &[("window", 20.0)]);
}

// --- Klinger, Chaikin oscillator, EOM, PVT, EFI, NVI/PVI ------------------------------------

#[test]
fn klinger_defaults() {
    assert_holds(&case("klinger_defaults", "klinger", IndicatorUnit::Volume));
}

#[test]
fn klinger_short() {
    assert_holds(&case("klinger_short", "klinger", IndicatorUnit::Volume));
}

#[test]
fn klinger_rejects_invalid_configuration() {
    assert_rejected("klinger", &[("fast_len", 0.0)]);
    assert_rejected("klinger", &[("fast_len", 55.0), ("slow_len", 55.0)]);
    assert_rejected("klinger", &[("fast_len", 60.0), ("slow_len", 55.0)]);
    assert_rejected("klinger", &[("signal_len", 0.0)]);
    assert_rejected("klinger", &[("signal", 13.0)]);
}

#[test]
fn chaikin_oscillator_defaults() {
    assert_holds(&case(
        "chaikin_oscillator_defaults",
        "chaikin_oscillator",
        IndicatorUnit::Volume,
    ));
}

#[test]
fn chaikin_oscillator_short() {
    assert_holds(&case(
        "chaikin_oscillator_short",
        "chaikin_oscillator",
        IndicatorUnit::Volume,
    ));
}

#[test]
fn chaikin_oscillator_rejects_invalid_configuration() {
    assert_rejected("chaikin_oscillator", &[("fast_len", 0.0)]);
    assert_rejected("chaikin_oscillator", &[("fast_len", 10.0)]);
    assert_rejected("chaikin_oscillator", &[("fast", 3.0)]);
}

#[test]
fn eom_defaults() {
    assert_holds(&case("eom_defaults", "eom", IndicatorUnit::Unitless));
}

#[test]
fn eom_edges() {
    assert_holds(&case("eom_edges", "eom", IndicatorUnit::Unitless));
}

#[test]
fn eom_rejects_invalid_configuration() {
    assert_rejected("eom", &[("period", 0.0)]);
    assert_rejected("eom", &[("volume_divisor", 0.5)]);
    assert_rejected("eom", &[("volume_divisor", f64::INFINITY)]);
    assert_rejected("eom", &[("divisor", 10000.0)]);
}

#[test]
fn pvt_shaped() {
    assert_holds(&case("pvt_shaped", "pvt", IndicatorUnit::Volume));
}

#[test]
fn pvt_rejects_invalid_configuration() {
    assert_rejected("pvt", &[("period", 1.0)]);
}

#[test]
fn efi_defaults() {
    assert_holds(&case("efi_defaults", "efi", IndicatorUnit::Unitless));
}

#[test]
fn efi_short() {
    assert_holds(&case("efi_short", "efi", IndicatorUnit::Unitless));
}

#[test]
fn efi_rejects_invalid_configuration() {
    assert_rejected("efi", &[("ema_len", 0.0)]);
    assert_rejected("efi", &[("ema_len", 1.5)]);
    assert_rejected("efi", &[("length", 13.0)]);
}

#[test]
fn nvi_shaped() {
    assert_holds(&case("nvi_shaped", "nvi", IndicatorUnit::Unitless));
}

#[test]
fn nvi_rejects_invalid_configuration() {
    assert_rejected("nvi", &[("start", 1000.0)]);
}

#[test]
fn pvi_shaped() {
    assert_holds(&case("pvi_shaped", "pvi", IndicatorUnit::Unitless));
}

#[test]
fn pvi_rejects_invalid_configuration() {
    assert_rejected("pvi", &[("start", 1000.0)]);
}

// --- RVAT ------------------------------------------------------------------------------------

#[test]
fn rvat_two_days() {
    assert_holds(&case("rvat_two_days", "rvat", IndicatorUnit::Ratio));
}

#[test]
fn rvat_offset() {
    assert_holds(&case("rvat_offset", "rvat", IndicatorUnit::Ratio));
}

#[test]
fn rvat_rejects_invalid_configuration() {
    assert_rejected("rvat", &[("days", 0.0)]);
    assert_rejected("rvat", &[("days", 1001.0)]);
    assert_rejected("rvat", &[("day_start_offset", 86_401.0)]);
    assert_rejected("rvat", &[("bar_seconds", 0.5)]);
    assert_rejected("rvat", &[("bar_seconds", 86_401.0)]);
    assert_rejected("rvat", &[("lookback_days", 10.0)]);
}

// --- Volume profiles -------------------------------------------------------------------------

#[test]
fn volume_profile_defaults() {
    assert_holds(&case(
        "volume_profile_defaults",
        "volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn volume_profile_small() {
    assert_holds(&case(
        "volume_profile_small",
        "volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn volume_profile_zero_volume() {
    assert_holds(&case(
        "volume_profile_zero_volume",
        "volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn volume_profile_flat() {
    assert_holds(&case(
        "volume_profile_flat",
        "volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn volume_profile_rejects_invalid_configuration() {
    assert_rejected("volume_profile", &[("lookback", 0.0)]);
    assert_rejected("volume_profile", &[("num_bins", 0.0)]);
    assert_rejected("volume_profile", &[("num_bins", 1001.0)]);
    assert_rejected("volume_profile", &[("bins", 30.0)]);
}

#[test]
fn extended_volume_profile_defaults() {
    assert_holds(&case(
        "extended_volume_profile_defaults",
        "extended_volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn extended_volume_profile_flat() {
    assert_holds(&case(
        "extended_volume_profile_flat",
        "extended_volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn extended_volume_profile_rejects_invalid_configuration() {
    assert_rejected("extended_volume_profile", &[("lookback", 0.0)]);
    assert_rejected("extended_volume_profile", &[("num_bins", 0.0)]);
    assert_rejected("extended_volume_profile", &[("bins", 30.0)]);
}

#[test]
fn persistent_volume_profile_defaults() {
    assert_holds(&case(
        "persistent_volume_profile_defaults",
        "persistent_volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn persistent_volume_profile_small() {
    assert_holds(&case(
        "persistent_volume_profile_small",
        "persistent_volume_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn persistent_volume_profile_rejects_invalid_configuration() {
    assert_rejected("persistent_volume_profile", &[("lookback", 0.0)]);
    assert_rejected("persistent_volume_profile", &[("bin_width", 0.0)]);
    assert_rejected("persistent_volume_profile", &[("bin_width", 2_000_000.0)]);
    assert_rejected("persistent_volume_profile", &[("width", 1.0)]);
}

#[test]
fn money_flow_profile_defaults() {
    assert_holds(&case(
        "money_flow_profile_defaults",
        "money_flow_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn money_flow_profile_short() {
    assert_holds(&case(
        "money_flow_profile_short",
        "money_flow_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn money_flow_profile_rejects_invalid_configuration() {
    assert_rejected("money_flow_profile", &[("lookback", 1.0)]);
    assert_rejected("money_flow_profile", &[("rows", 0.0)]);
    assert_rejected("money_flow_profile", &[("va_pct", 1.5)]);
    assert_rejected("money_flow_profile", &[("va_pct", -0.1)]);
    assert_rejected("money_flow_profile", &[("num_bins", 25.0)]);
}

// --- tpo_profile -----------------------------------------------------------------------

#[test]
fn tpo_profile_defaults() {
    assert_holds(&case(
        "tpo_profile_defaults",
        "tpo_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn tpo_profile_short() {
    assert_holds(&case(
        "tpo_profile_short",
        "tpo_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn tpo_profile_tie_high() {
    assert_holds(&case(
        "tpo_profile_tie_high",
        "tpo_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn tpo_profile_tie_low() {
    assert_holds(&case(
        "tpo_profile_tie_low",
        "tpo_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn tpo_profile_one_bin() {
    assert_holds(&case(
        "tpo_profile_one_bin",
        "tpo_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn tpo_profile_flat() {
    assert_holds(&case(
        "tpo_profile_flat",
        "tpo_profile",
        IndicatorUnit::Price,
    ));
}

#[test]
fn tpo_profile_rejects_invalid_configuration() {
    assert_rejected("tpo_profile", &[("lookback", 0.0)]);
    assert_rejected("tpo_profile", &[("num_bins", 0.0)]);
    assert_rejected("tpo_profile", &[("lookback", 2.5)]);
    assert_rejected("tpo_profile", &[("bins", 10.0)]);
}

// --- volume_flow_indicator ---------------------------------------------------

#[test]
fn volume_flow_indicator_defaults() {
    assert_holds(&case(
        "volume_flow_indicator_defaults",
        "volume_flow_indicator",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_flow_indicator_short() {
    assert_holds(&case(
        "volume_flow_indicator_short",
        "volume_flow_indicator",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_flow_indicator_no_cutoff() {
    assert_holds(&case(
        "volume_flow_indicator_no_cutoff",
        "volume_flow_indicator",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_flow_indicator_zero_volume() {
    assert_holds(&case(
        "volume_flow_indicator_zero_volume",
        "volume_flow_indicator",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_flow_indicator_high_level() {
    assert_holds(&case(
        "volume_flow_indicator_high_level",
        "volume_flow_indicator",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_flow_indicator_rejects_invalid_configuration() {
    assert_rejected("volume_flow_indicator", &[("period", 1.0)]);
    assert_rejected("volume_flow_indicator", &[("period", 2.5)]);
    assert_rejected("volume_flow_indicator", &[("coef", -0.1)]);
    assert_rejected("volume_flow_indicator", &[("coef", 10.5)]);
    assert_rejected("volume_flow_indicator", &[("vcoef", 0.0)]);
    assert_rejected("volume_flow_indicator", &[("vcoef", 101.0)]);
    assert_rejected("volume_flow_indicator", &[("smoothing", 0.0)]);
    assert_rejected("volume_flow_indicator", &[("lookback", 5.0)]);
}

// --- volume_zone_oscillator --------------------------------------------------

#[test]
fn volume_zone_oscillator_defaults() {
    assert_holds(&case(
        "volume_zone_oscillator_defaults",
        "volume_zone_oscillator",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn volume_zone_oscillator_one() {
    assert_holds(&case(
        "volume_zone_oscillator_one",
        "volume_zone_oscillator",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn volume_zone_oscillator_five() {
    assert_holds(&case(
        "volume_zone_oscillator_five",
        "volume_zone_oscillator",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn volume_zone_oscillator_rejects_invalid_configuration() {
    assert_rejected("volume_zone_oscillator", &[("period", 0.0)]);
    assert_rejected("volume_zone_oscillator", &[("period", 1.5)]);
    assert_rejected("volume_zone_oscillator", &[("length", 14.0)]);
}

// --- volume_price_confirmation -----------------------------------------------

#[test]
fn volume_price_confirmation_defaults() {
    assert_holds(&case(
        "volume_price_confirmation_defaults",
        "volume_price_confirmation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_price_confirmation_small() {
    assert_holds(&case(
        "volume_price_confirmation_small",
        "volume_price_confirmation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_price_confirmation_minimal() {
    assert_holds(&case(
        "volume_price_confirmation_minimal",
        "volume_price_confirmation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_price_confirmation_zero_volume() {
    assert_holds(&case(
        "volume_price_confirmation_zero_volume",
        "volume_price_confirmation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_price_confirmation_high_level() {
    assert_holds(&case(
        "volume_price_confirmation_high_level",
        "volume_price_confirmation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn volume_price_confirmation_rejects_invalid_configuration() {
    assert_rejected("volume_price_confirmation", &[("short", 20.0)]);
    assert_rejected("volume_price_confirmation", &[("short", 25.0)]);
    assert_rejected("volume_price_confirmation", &[("short", 0.0)]);
    assert_rejected(
        "volume_price_confirmation",
        &[("long", 1.0), ("short", 1.0)],
    );
    assert_rejected("volume_price_confirmation", &[("smoothing", 0.0)]);
    assert_rejected("volume_price_confirmation", &[("smoothing", 2.5)]);
    assert_rejected("volume_price_confirmation", &[("window", 5.0)]);
}

// --- market_facilitation -----------------------------------------------------

#[test]
fn market_facilitation_shaped() {
    assert_holds(&case(
        "market_facilitation_shaped",
        "market_facilitation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn market_facilitation_edges() {
    assert_holds(&case(
        "market_facilitation_edges",
        "market_facilitation",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn market_facilitation_rejects_invalid_configuration() {
    assert_rejected("market_facilitation", &[("period", 1.0)]);
}
