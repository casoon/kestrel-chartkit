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
    Coverage::full("kestrel_buy_sell_pressure"),
    Coverage {
        indicator: "kestrel_trend_quality",
        values: Status::Specified,
        alerts: Status::Specified,
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_multi_factor",
        values: Status::Specified,
        alerts: Status::Specified,
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_leg_conviction",
        values: Status::Specified,
        alerts: Status::Partial("strength (conviction drop / 100, 0.05..1) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_sweep_reversal_score",
        values: Status::Partial("a bar firing long and short (long_*/short_* fields) not cased"),
        alerts: Status::Partial("strength (quality / 2) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_trend_break_risk",
        values: Status::Specified,
        alerts: Status::Partial(
            "break_critical and bull_break_pressure never reached; strengths not documented",
        ),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_equal_level_liquidity",
        values: Status::Specified,
        alerts: Status::Partial(
            "strengths (0.6 events, score / 100 reclaims, 0.4 near) not documented",
        ),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_leg_character",
        values: Status::Specified,
        alerts: Status::Partial("strengths (score / 100, 0.3 character_lost) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_accumulation_phase",
        values: Status::Specified,
        alerts: Status::Partial(
            "breakout/breakdown ready never reached; strengths (0.4/0.6/0.8) not documented",
        ),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_reversal_setup_score",
        values: Status::Specified,
        alerts: Status::Partial("bull/bear_action never reached; strengths not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_structure_zones",
        values: Status::Partial(
            "real volume from the first bar moves the first output to bar 20 (warmup_period 15); \
             cased with volume starting after the warm-up",
        ),
        alerts: Status::Partial("strength (0.5) not documented"),
        configuration: Status::Specified,
    },
    Coverage {
        indicator: "kestrel_structural_flow",
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
        "kestrel_buy_sell_pressure",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn buy_sell_pressure_period3() {
    assert_holds(&case(
        "buy_sell_pressure_period3",
        "kestrel_buy_sell_pressure",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn buy_sell_pressure_edges() {
    assert_holds(&case(
        "buy_sell_pressure_edges",
        "kestrel_buy_sell_pressure",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn buy_sell_pressure_rejects_invalid_configuration() {
    assert_rejected("kestrel_buy_sell_pressure", &[("period", 0.0)]);
    assert_rejected("kestrel_buy_sell_pressure", &[("period", 1.5)]);
    assert_rejected("kestrel_buy_sell_pressure", &[("length", 14.0)]);
}

// --- Trend quality ---------------------------------------------------------------------------

#[test]
fn trend_quality_defaults() {
    assert_holds(&case(
        "trend_quality_defaults",
        "kestrel_trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_period5() {
    assert_holds(&case(
        "trend_quality_period5",
        "kestrel_trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_novolume() {
    assert_holds(&case(
        "trend_quality_novolume",
        "kestrel_trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_flat() {
    assert_holds(&case(
        "trend_quality_flat",
        "kestrel_trend_quality",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn trend_quality_rejects_invalid_configuration() {
    assert_rejected("kestrel_trend_quality", &[("period", 1.0)]);
    assert_rejected("kestrel_trend_quality", &[("period", 10001.0)]);
    assert_rejected("kestrel_trend_quality", &[("perod", 14.0)]);
}

// --- Multi-factor ----------------------------------------------------------------------------

#[test]
fn multi_factor_defaults() {
    assert_holds(&case(
        "multi_factor_defaults",
        "kestrel_multi_factor",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn multi_factor_period4() {
    assert_holds(&case(
        "multi_factor_period4",
        "kestrel_multi_factor",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn multi_factor_rejects_invalid_configuration() {
    assert_rejected("kestrel_multi_factor", &[("period", 1.0)]);
    assert_rejected("kestrel_multi_factor", &[("period", 2.5)]);
    assert_rejected("kestrel_multi_factor", &[("rsi_len", 14.0)]);
}

// --- Leg conviction --------------------------------------------------------------------------

#[test]
fn leg_conviction_defaults() {
    assert_holds(&case(
        "leg_conviction_defaults",
        "kestrel_leg_conviction",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_conviction_short_pivots() {
    assert_holds(&case(
        "leg_conviction_short_pivots",
        "kestrel_leg_conviction",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_conviction_novolume() {
    assert_holds(&case(
        "leg_conviction_novolume",
        "kestrel_leg_conviction",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_conviction_rejects_invalid_configuration() {
    assert_rejected("kestrel_leg_conviction", &[("pivot_left", 0.0)]);
    assert_rejected("kestrel_leg_conviction", &[("max_leg", 1001.0)]);
    assert_rejected("kestrel_leg_conviction", &[("speed_ref", 0.0)]);
    assert_rejected("kestrel_leg_conviction", &[("w_volume", 1.5)]);
    assert_rejected("kestrel_leg_conviction", &[("pivot_len", 5.0)]);
}

// --- Sweep reversal score --------------------------------------------------------------------

#[test]
fn sweep_reversal_score_defaults() {
    assert_holds(&case(
        "sweep_reversal_score_defaults",
        "kestrel_sweep_reversal_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn sweep_reversal_score_short_windows() {
    assert_holds(&case(
        "sweep_reversal_score_short_windows",
        "kestrel_sweep_reversal_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn sweep_reversal_score_rejects_invalid_configuration() {
    assert_rejected("kestrel_sweep_reversal_score", &[("swing_len", 1.0)]);
    assert_rejected("kestrel_sweep_reversal_score", &[("min_quality_long", 3.0)]);
    assert_rejected("kestrel_sweep_reversal_score", &[("atr_low_factor", -0.1)]);
    assert_rejected("kestrel_sweep_reversal_score", &[("cooldown", 0.5)]);
    assert_rejected("kestrel_sweep_reversal_score", &[("sl_atr_buffer", 0.3)]);
}

// --- Trend break risk ------------------------------------------------------------------------

#[test]
fn trend_break_risk_defaults() {
    assert_holds(&case(
        "trend_break_risk_defaults",
        "kestrel_trend_break_risk",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn trend_break_risk_fast() {
    assert_holds(&case(
        "trend_break_risk_fast",
        "kestrel_trend_break_risk",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn trend_break_risk_crash() {
    assert_holds(&case(
        "trend_break_risk_crash",
        "kestrel_trend_break_risk",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn trend_break_risk_rejects_invalid_configuration() {
    assert_rejected("kestrel_trend_break_risk", &[("bias_len", 1.0)]);
    assert_rejected("kestrel_trend_break_risk", &[("near_atr", 0.0)]);
    assert_rejected("kestrel_trend_break_risk", &[("smooth_len", 101.0)]);
    assert_rejected("kestrel_trend_break_risk", &[("w_near", 25.0)]);
}

// --- Equal-level liquidity -------------------------------------------------------------------

#[test]
fn equal_level_liquidity_defaults() {
    assert_holds(&case(
        "equal_level_liquidity_defaults",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn equal_level_liquidity_novolume() {
    assert_holds(&case(
        "equal_level_liquidity_novolume",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn equal_level_liquidity_tuned() {
    assert_holds(&case(
        "equal_level_liquidity_tuned",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn equal_level_liquidity_listed() {
    assert_holds(&case(
        "equal_level_liquidity_listed",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    ));
}

/// `field` of bar `bar` (0-based) as the fixture expects it.
fn expected(case: &ContractCase, bar: usize, field: &str) -> Option<f64> {
    let output = case.expected[bar].output.as_ref().expect("an output");
    output.extra.get(field).copied()
}

fn expected_alerts(case: &ContractCase, bar: usize) -> Vec<&str> {
    case.expected[bar]
        .alerts
        .keys()
        .map(String::as_str)
        .collect()
}

/// Von Hand hergeleitet (ATR genau 2, Toleranz 0,10 · 2 = 0,2): Die Pivot-Hochs 100,00 und 100,20
/// liegen genau auf der Toleranz und bilden ein Niveau aus zwei Pivots bei 100,10. Danach liegen
/// ein Hoch auf dem Bandrand (Test), ein Hoch auf der Sweep-Marke (Sweep), ein Schluss auf der
/// Rückkehr-Marke und ein Hoch auf dem Niveau (Reclaim) je genau auf ihrer Marke.
#[test]
fn equal_level_liquidity_tolerance_edge() {
    let case = case(
        "equal_level_liquidity_tolerance_edge",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    );
    let level = |bar: usize, field: &str| expected(&case, bar, &format!("bsl_1_{field}"));
    let near = |got: Option<f64>, want: f64| (got.expect("field") - want).abs() < 1e-9;
    assert_eq!(expected(&case, 13, "atr"), Some(2.0));
    assert!(near(expected(&case, 13, "tolerance"), 0.2));
    assert_eq!(level(21, "price"), Some(100.0));
    assert_eq!(level(21, "touches"), Some(1.0));
    // Bar 22 bestätigt das Pivot-Hoch 100,20 (Bar 19): Abstand 0,20, es gehört zum Niveau.
    assert_eq!(case.bars[19].high, 100.2);
    assert_eq!(expected(&case, 22, "bsl_levels"), Some(1.0));
    assert!(near(level(22, "price"), 100.1));
    assert_eq!(level(22, "touches"), Some(2.0));
    assert_eq!(level(22, "event"), Some(1.0));
    assert_eq!(level(22, "first_bars_back"), Some(12.0));
    assert_eq!(level(22, "last_bars_back"), Some(3.0));
    // Bar 24: Schluss 98,10, genau 1 ATR unter dem Niveau.
    assert_eq!(case.bars[24].close, 98.1);
    assert_eq!(expected_alerts(&case, 24), ["primary_bsl_near"]);
    // Bar 25: Hoch 99,90 auf dem unteren Bandrand 100,10 - 0,20, nach einer Bar darunter.
    assert_eq!((case.bars[24].high, case.bars[25].high), (99.0, 99.9));
    assert_eq!(level(24, "tests"), Some(1.0));
    assert_eq!(level(25, "tests"), Some(2.0));
    assert_eq!(level(25, "event"), Some(2.0));
    // Bar 26: Hoch 100,20 auf der Sweep-Marke 100,10 + 0,10, Schluss 100,00 darunter.
    assert_eq!((case.bars[26].high, case.bars[26].close), (100.2, 100.0));
    assert_eq!(level(26, "status"), Some(1.0));
    assert_eq!(level(26, "event"), Some(4.0));
    assert_eq!(expected_alerts(&case, 26), ["bear_bsl_sweep"]);
    // Bar 27: Schluss 99,90 auf der Rückkehr-Marke 100,10 - 0,20, Hoch 100,10 auf dem Niveau.
    assert_eq!((case.bars[27].high, case.bars[27].close), (100.1, 99.9));
    assert_eq!(level(27, "status"), Some(3.0));
    assert_eq!(level(27, "event"), Some(32.0));
    assert_eq!(expected_alerts(&case, 27), ["bear_reclaim"]);
    // Verkaufsseite: Pivot-Tiefs 96,40 und 96,50 ergeben ein Niveau bei 96,45.
    assert!(near(expected(&case, 26, "ssl_1_price"), 96.45));
    assert_eq!(expected(&case, 26, "ssl_1_touches"), Some(2.0));
    assert_holds(&case);
}

/// Das Spiegelbild (jeder Preis negiert): dieselben Ereignisse auf der Verkaufsseite.
#[test]
fn equal_level_liquidity_negated_tolerance_edge() {
    let case = case(
        "equal_level_liquidity_negated_tolerance_edge",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    );
    let level = |bar: usize, field: &str| expected(&case, bar, &format!("ssl_1_{field}"));
    assert_eq!(case.bars[19].low, -100.2);
    assert_eq!(expected(&case, 22, "ssl_levels"), Some(1.0));
    assert!((level(22, "price").unwrap() + 100.1).abs() < 1e-9);
    assert_eq!(level(22, "touches"), Some(2.0));
    assert_eq!(expected_alerts(&case, 24), ["primary_ssl_near"]);
    assert_eq!(level(25, "tests"), Some(2.0));
    assert_eq!(level(26, "status"), Some(1.0));
    assert_eq!(expected_alerts(&case, 26), ["bull_ssl_sweep"]);
    assert_eq!(level(27, "status"), Some(3.0));
    assert_eq!(expected_alerts(&case, 27), ["bull_reclaim"]);
    assert_holds(&case);
}

/// Knapp darunter und knapp darüber: 0,19 Abstand ergibt ein Niveau (Preis 100,095), 0,21 zwei.
#[test]
fn equal_level_liquidity_tolerance_inside_and_outside() {
    let inside = case(
        "equal_level_liquidity_tolerance_inside",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    );
    assert_eq!(inside.bars[19].high, 100.19);
    assert_eq!(expected(&inside, 22, "bsl_levels"), Some(1.0));
    assert_eq!(expected(&inside, 22, "bsl_1_touches"), Some(2.0));
    assert!((expected(&inside, 22, "bsl_1_price").unwrap() - 100.095).abs() < 1e-9);
    assert_holds(&inside);

    let outside = case(
        "equal_level_liquidity_tolerance_outside",
        "kestrel_equal_level_liquidity",
        IndicatorUnit::Unitless,
    );
    assert_eq!(outside.bars[19].high, 100.21);
    assert_eq!(expected(&outside, 22, "bsl_levels"), Some(2.0));
    assert_eq!(expected(&outside, 22, "bsl_1_price"), Some(100.0));
    assert_eq!(expected(&outside, 22, "bsl_1_touches"), Some(1.0));
    assert_eq!(expected(&outside, 22, "bsl_2_price"), Some(100.21));
    assert_eq!(expected(&outside, 22, "bsl_2_touches"), Some(1.0));
    assert_holds(&outside);
}

#[test]
fn equal_level_liquidity_rejects_invalid_configuration() {
    assert_rejected(
        "kestrel_equal_level_liquidity",
        &[("list_min_touches", 0.0)],
    );
    assert_rejected("kestrel_equal_level_liquidity", &[("pivot_len", 0.0)]);
    assert_rejected("kestrel_equal_level_liquidity", &[("min_touches", 21.0)]);
    assert_rejected(
        "kestrel_equal_level_liquidity",
        &[("max_distance_atr", 0.05)],
    );
    assert_rejected(
        "kestrel_equal_level_liquidity",
        &[("eq_tolerance_atr", -0.1)],
    );
    assert_rejected("kestrel_equal_level_liquidity", &[("premium_score", 65.0)]);
}

// --- Accumulation phase ----------------------------------------------------------------------

#[test]
fn accumulation_phase_base() {
    assert_holds(&case(
        "accumulation_phase_base",
        "kestrel_accumulation_phase",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn accumulation_phase_top() {
    assert_holds(&case(
        "accumulation_phase_top",
        "kestrel_accumulation_phase",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn accumulation_phase_fast() {
    assert_holds(&case(
        "accumulation_phase_fast",
        "kestrel_accumulation_phase",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn accumulation_phase_rejects_invalid_configuration() {
    assert_rejected("kestrel_accumulation_phase", &[("lookback", 9.0)]);
    assert_rejected("kestrel_accumulation_phase", &[("min_phase_bars", 2.0)]);
    assert_rejected("kestrel_accumulation_phase", &[("er_range", 0.01)]);
    assert_rejected("kestrel_accumulation_phase", &[("bbw_compress", 6.0)]);
    assert_rejected("kestrel_accumulation_phase", &[("pivot_len", 3.0)]);
}

// --- Leg character ---------------------------------------------------------------------------

#[test]
fn leg_character_defaults() {
    assert_holds(&case(
        "leg_character_defaults",
        "kestrel_leg_character",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_character_short_legs() {
    assert_holds(&case(
        "leg_character_short_legs",
        "kestrel_leg_character",
        IndicatorUnit::Ratio,
    ));
}

#[test]
fn leg_character_rejects_invalid_configuration() {
    assert_rejected("kestrel_leg_character", &[("max_leg_bars", 19.0)]);
    assert_rejected("kestrel_leg_character", &[("mature_bars", 1.0)]);
    assert_rejected("kestrel_leg_character", &[("impulse_threshold", 49.0)]);
    assert_rejected("kestrel_leg_character", &[("change_buffer", 26.0)]);
    assert_rejected("kestrel_leg_character", &[("min_separation", 18.0)]);
}

// --- Reversal setup score --------------------------------------------------------------------

#[test]
fn reversal_setup_score_volume() {
    assert_holds(&case(
        "reversal_setup_score_volume",
        "kestrel_reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_novolume() {
    assert_holds(&case(
        "reversal_setup_score_novolume",
        "kestrel_reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_candidates() {
    assert_holds(&case(
        "reversal_setup_score_candidates",
        "kestrel_reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_long_invalidated() {
    assert_holds(&case(
        "reversal_setup_score_long_invalidated",
        "kestrel_reversal_setup_score",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn reversal_setup_score_rejects_invalid_configuration() {
    assert_rejected("kestrel_reversal_setup_score", &[("rsi_len", 14.0)]);
    assert_rejected("kestrel_reversal_setup_score", &[("momentum_mix", 0.0)]);
}

// --- Structure zones -------------------------------------------------------------------------

#[test]
fn structure_zones_volume() {
    assert_holds(&case(
        "structure_zones_volume",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_holds() {
    assert_holds(&case(
        "structure_zones_holds",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_novolume() {
    assert_holds(&case(
        "structure_zones_novolume",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_lifecycle() {
    assert_holds(&case(
        "structure_zones_lifecycle",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structure_zones_mirrored_lifecycle() {
    assert_holds(&case(
        "structure_zones_mirrored_lifecycle",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    ));
}

/// Von Hand hergeleitet (ATR genau 2): Swing-Hochs im Abstand von genau 0,1 ATR sind keine
/// gleichen Hochs (die Regel verlangt weniger); ein Hoch auf dem unteren Zonenrand ist ein Test,
/// ein Schluss auf dem oberen Rand kein Bruch.
#[test]
fn structure_zones_edge() {
    let case = case(
        "structure_zones_edge",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    );
    let zone = |bar: usize, field: &str| expected(&case, bar, &format!("zone_1_{field}"));
    let near = |got: Option<f64>, want: f64| (got.expect("field") - want).abs() < 1e-9;
    assert_eq!(expected(&case, 18, "atr"), Some(2.0));
    // Bar 18 bestätigt das Swing-Hoch 100,10; das davor liegt bei 99,90: kein EQH (Bit 16384).
    assert_eq!((case.bars[10].high, case.bars[15].high), (99.9, 100.1));
    assert_eq!(expected(&case, 18, "event_code"), Some(1.0));
    // Bar 21: Swing-Tief 97,00 nach 96,90, Abstand 0,10: EQL (Bit 32768).
    assert_eq!(expected(&case, 21, "event_code"), Some(32770.0));
    // Bar 22: Zone 1 um das Major-Hoch 100,10, 0,3 ATR breit.
    assert!(near(zone(22, "lower"), 99.8) && near(zone(22, "upper"), 100.4));
    assert_eq!(zone(22, "role"), Some(-1.0));
    assert_eq!(zone(22, "event"), Some(1.0));
    // Bar 24 (Hoch 99,79) bleibt darunter, Bar 25 (Hoch 99,80) liegt auf dem unteren Rand.
    assert_eq!((case.bars[24].high, case.bars[25].high), (99.79, 99.8));
    assert_eq!(zone(24, "tests"), Some(0.0));
    assert_eq!(zone(25, "tests"), Some(1.0));
    assert_eq!(zone(25, "status"), Some(1.0));
    assert_eq!(zone(25, "event"), Some(2.0));
    // Bar 27 schließt bei 100,40 auf dem oberen Rand: kein Bruch; Major-BOS (4 + 16), Zone 2.
    assert_eq!(case.bars[27].close, 100.4);
    assert_eq!(zone(27, "status"), Some(1.0));
    assert_eq!(zone(27, "break_bars_back"), None);
    assert_eq!(expected(&case, 27, "event_code"), Some(20.0));
    assert_eq!(expected(&case, 27, "zone_2_type"), Some(5.0));
    // Bar 28 schließt bei 100,90 darüber: Bruch. Bar 29: Rollentausch.
    assert_eq!(zone(28, "status"), Some(3.0));
    assert_eq!(zone(28, "event"), Some(8.0));
    assert_eq!(zone(28, "break_close"), Some(100.9));
    assert_eq!(zone(29, "status"), Some(4.0));
    assert_eq!(zone(29, "role"), Some(1.0));
    assert_eq!(zone(29, "event"), Some(16.0));
    assert_holds(&case);
}

/// Das Spiegelbild: Swing-Tiefs im Abstand 0,1 ATR sind keine gleichen Tiefs, die Zone ist eine
/// Unterstützung, getestet auf dem oberen Rand, nicht gebrochen auf dem unteren.
#[test]
fn structure_zones_negated_edge() {
    let case = case(
        "structure_zones_negated_edge",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    );
    let zone = |bar: usize, field: &str| expected(&case, bar, &format!("zone_1_{field}"));
    assert_eq!(expected(&case, 18, "event_code"), Some(2.0));
    assert_eq!(expected(&case, 21, "event_code"), Some(16385.0));
    assert_eq!(zone(22, "role"), Some(1.0));
    assert_eq!(zone(24, "tests"), Some(0.0));
    assert_eq!(zone(25, "tests"), Some(1.0));
    assert_eq!(zone(27, "status"), Some(1.0));
    assert_eq!(zone(28, "status"), Some(3.0));
    assert_eq!(zone(29, "status"), Some(4.0));
    assert_eq!(zone(29, "role"), Some(-1.0));
    assert_holds(&case);
}

/// Knapp darunter: 0,19 Abstand ist weniger als 0,1 ATR, also EQH (1 + 16384).
#[test]
fn structure_zones_equal_highs() {
    let case = case(
        "structure_zones_equal_highs",
        "kestrel_structure_zones",
        IndicatorUnit::Unitless,
    );
    assert_eq!((case.bars[10].high, case.bars[15].high), (99.91, 100.1));
    assert_eq!(expected(&case, 18, "event_code"), Some(16385.0));
    assert_holds(&case);
}

#[test]
fn structure_zones_rejects_invalid_configuration() {
    assert_rejected("kestrel_structure_zones", &[("atr_len", 14.0)]);
    assert_rejected("kestrel_structure_zones", &[("zone_atr", 3.0)]);
}

// --- Structural flow -------------------------------------------------------------------------

#[test]
fn structural_flow_volume() {
    assert_holds(&case(
        "structural_flow_volume",
        "kestrel_structural_flow",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structural_flow_novolume() {
    assert_holds(&case(
        "structural_flow_novolume",
        "kestrel_structural_flow",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structural_flow_transitions() {
    assert_holds(&case(
        "structural_flow_transitions",
        "kestrel_structural_flow",
        IndicatorUnit::Unitless,
    ));
}

#[test]
fn structural_flow_rejects_invalid_configuration() {
    assert_rejected("kestrel_structural_flow", &[("preset", 1.0)]);
    assert_rejected("kestrel_structural_flow", &[("len_slow", 96.0)]);
}
