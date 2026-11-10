mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

/// Pool-, Pivot- und ZigZag-Werte der handgebauten Folgen, hergeleitet in
/// `reference/kestrel_reference/structure.py`.
const STRUCTURE: &str = include_str!("fixtures/scenario_structure.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(STRUCTURE, key)
}

// ============================================================================
// 1. Break of Structure (BOS) / Change of Character (CHoCH)
// ============================================================================
#[test]
fn test_scenario_bos_choch() {
    let mut ind = build_checked(
        "bos_choch",
        &HashMap::from([("pivot_len".to_string(), 2.0)]),
    )
    .unwrap();

    // PivotLen = 2 means a pivot requires 2 bars before and 2 bars after (window = 5 bars).
    // Bars 0..4: Low at Bar 2 (price 90.0) -> confirmed at index 4
    // Bar 5: breaks 90.0 downwards (Close = 85.0) -> Bearish CHoCH (event_code = -2.0)
    let prices = [100.0, 95.0, 90.0, 95.0, 96.0, 85.0];

    let mut event_codes = Vec::new();
    let mut notes_collected = Vec::new();

    for (i, &p) in prices.iter().enumerate() {
        let b = Bar::new(i as i64 * 60, p, p + 1.0, p - 1.0, p, 1000.0);
        if let Some(out) = ind.on_bar(&b) {
            event_codes.push(out.value);
            for a in ind.alerts() {
                if a.kind.ends_with("_bos") || a.kind.ends_with("_choch") {
                    assert_structure_kind(&a.kind, &a.note);
                    notes_collected.push(a.note);
                }
            }
        }
    }

    assert!(
        event_codes.contains(&-2.0),
        "BOS/CHoCH must record a Bearish CHoCH event code (-2.0)"
    );
    assert!(
        notes_collected
            .iter()
            .any(|n| n.contains("Change of Character (CHoCH)")),
        "BOS/CHoCH must emit alert note with Change of Character (CHoCH)"
    );
}

#[test]
fn test_scenario_bos_choch_synthetic_bearish() {
    use kestrel_chartkit::synthetic::{bos_choch_swing_bars, SwingDirection};

    let mut ind = build_checked(
        "bos_choch",
        &HashMap::from([("pivot_len".to_string(), 2.0)]),
    )
    .unwrap();

    let bars = bos_choch_swing_bars(42, SwingDirection::Bearish, 2);
    let mut event_codes = Vec::new();
    let mut notes = Vec::new();

    for qb in &bars {
        if let Some(out) = ind.on_bar(&qb.bar) {
            event_codes.push(out.value);
            for a in ind.alerts() {
                if a.kind.ends_with("_bos") || a.kind.ends_with("_choch") {
                    assert_structure_kind(&a.kind, &a.note);
                    notes.push(a.note);
                }
            }
        }
    }

    assert!(
        event_codes.contains(&-2.0),
        "Synthetic Bearish swing sequence must trigger Bearish CHoCH (-2.0)"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Bearish Change of Character (CHoCH)")),
        "Must emit Bearish CHoCH alert"
    );
}

#[test]
fn test_scenario_bos_choch_synthetic_bullish() {
    use kestrel_chartkit::synthetic::{bos_choch_swing_bars, SwingDirection};

    let mut ind = build_checked(
        "bos_choch",
        &HashMap::from([("pivot_len".to_string(), 2.0)]),
    )
    .unwrap();

    let bars = bos_choch_swing_bars(42, SwingDirection::Bullish, 2);
    let mut event_codes = Vec::new();
    let mut notes = Vec::new();

    for qb in &bars {
        if let Some(out) = ind.on_bar(&qb.bar) {
            event_codes.push(out.value);
            for a in ind.alerts() {
                if a.kind.ends_with("_bos") || a.kind.ends_with("_choch") {
                    assert_structure_kind(&a.kind, &a.note);
                    notes.push(a.note);
                }
            }
        }
    }

    assert!(
        event_codes.contains(&2.0),
        "Synthetic Bullish swing sequence must trigger Bullish CHoCH (2.0)"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Bullish Change of Character (CHoCH)")),
        "Must emit Bullish CHoCH alert"
    );
}

// ============================================================================
// 2. Candle Classification (Pinbars, Kangaroo Tails, Engulfing Patterns)
// ============================================================================
#[test]
fn test_scenario_candle_classification() {
    // `min_range_atr` off: this fixture is three bars long, so the ATR is still warming up and a
    // size floor would suppress everything. The point here is the classification, not the filter.
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    // Bar 0: neutral bar
    let b0 = Bar::new(0, 100.0, 101.0, 99.0, 100.0, 1000.0);
    ind.on_bar(&b0);

    // Bar 1: bullish pinbar.
    // Range = 100.5 - 80.0 = 20.5
    // Lower wick = min(100.0, 100.2) - 80.0 = 20.0 → 97.5% of range, ≥ 55%
    // Close position = (100.2 - 80.0) / 20.5 = 98.5%, ≥ 65%
    let pinbar = Bar::new(60, 100.0, 100.5, 80.0, 100.2, 2000.0);
    let pinbar_out = ind.on_bar(&pinbar).expect("Pinbar should yield output");

    assert_eq!(pinbar_out.extra["bullish_pinbar"], 1.0);
    assert!(!pinbar_out.extra.contains_key("bearish_pinbar"));
    // The metrics come out too, so the flag can be checked instead of trusted.
    assert!((pinbar_out.extra["lower_wick_ratio"] - 20.0 / 20.5).abs() < 1e-12);
    assert!((pinbar_out.extra["close_position"] - 20.2 / 20.5).abs() < 1e-12);

    let alerts = ind.alerts();
    assert!(
        alerts
            .iter()
            .any(|a| a.kind == "bullish_pinbar" && a.strength >= 0.85),
        "Must emit high-confidence bullish_pinbar alert"
    );

    // Bar 2: bearish engulfing over the pinbar's body, and at the same time a bearish pinbar is
    // *not* present. Body = 10.5, previous body = 0.2, closes below the previous open.
    let engulfing = Bar::new(120, 100.5, 103.0, 88.0, 90.0, 3000.0);
    let eng_out = ind
        .on_bar(&engulfing)
        .expect("Engulfing should yield output");
    assert_eq!(eng_out.extra["bearish_engulfing"], 1.0);
    assert!(
        ind.alerts().iter().any(|a| a.kind == "bearish_engulfing"),
        "Must emit bearish_engulfing alert"
    );
}

/// A bar can be several things at once, and the engine has to say so.
///
/// This is what the previous single `pattern_type` slot could not express: whichever check ran
/// last overwrote the others, so the reported pattern depended on the order of the branches.
#[test]
fn test_scenario_candle_classification_reports_every_match() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    // A small bearish bar, then a large bullish one that both engulfs it and is a marubozu.
    ind.on_bar(&Bar::new(0, 100.0, 100.6, 99.6, 99.8, 1000.0));
    let out = ind
        .on_bar(&Bar::new(60, 99.7, 105.2, 99.6, 105.0, 2000.0))
        .expect("output");

    // body = 5.3, range = 5.6 → 94.6% ≥ 82%
    assert_eq!(out.extra["bullish_marubozu"], 1.0, "marubozu by body ratio");
    assert_eq!(out.extra["bullish_engulfing"], 1.0, "and engulfing at once");
    assert!(
        out.extra["pattern_count"] >= 2.0,
        "both are reported, not one overwriting the other"
    );
}

/// Hammer and hanging man are the same geometry; only the prior move separates them.
#[test]
fn test_scenario_candle_classification_needs_the_trend_to_name_a_hammer() {
    // Body 0.05, lower wick 3.0 (60× the body), upper wick 0.01 — hammer geometry either way.
    let shape = |t: i64, base: f64| Bar::new(t, base, base + 0.06, base - 3.0, base + 0.05, 1000.0);

    // Same shape after a decline and after an advance.
    let run = |steigend: bool| {
        let params: HashMap<String, f64> = [
            ("min_range_atr".to_string(), 0.0),
            ("trend_lookback".to_string(), 6.0),
            ("atr_len".to_string(), 5.0),
            ("trend_min_atr".to_string(), 1.0),
        ]
        .into();
        let mut ind = build_checked("candle_classification", &params).unwrap();
        for i in 0..14 {
            let base = if steigend {
                100.0 + i as f64 * 2.0
            } else {
                130.0 - i as f64 * 2.0
            };
            ind.on_bar(&Bar::new(
                i * 60,
                base,
                base + 0.8,
                base - 0.8,
                base + 0.4,
                1000.0,
            ));
        }
        let base = if steigend { 128.0 } else { 102.0 };
        ind.on_bar(&shape(14 * 60, base)).expect("output")
    };

    let nach_abverkauf = run(false);
    assert_eq!(nach_abverkauf.extra["hammer_shape"], 1.0);
    assert_eq!(
        nach_abverkauf.extra["hammer"], 1.0,
        "after a decline it is a hammer"
    );
    assert!(!nach_abverkauf.extra.contains_key("hanging_man"));

    let nach_anstieg = run(true);
    assert_eq!(nach_anstieg.extra["hammer_shape"], 1.0, "same geometry");
    assert_eq!(
        nach_anstieg.extra["hanging_man"], 1.0,
        "after an advance it is a hanging man"
    );
    assert!(!nach_anstieg.extra.contains_key("hammer"));
}

/// Advance block: three bullish candles, each with a smaller body and a longer upper wick than
/// the one before — an uptrend still climbing but with less conviction each bar.
#[test]
fn test_scenario_candle_classification_advance_block() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.0, 99.0, 110.0, 1000.0)); // body 10, upper wick 0
    ind.on_bar(&Bar::new(60, 110.0, 120.0, 109.0, 118.0, 1000.0)); // body 8, upper wick 2
    let out = ind
        .on_bar(&Bar::new(120, 118.0, 128.0, 117.0, 124.0, 1000.0)) // body 6, upper wick 4
        .expect("output");

    assert_eq!(out.extra["advance_block"], 1.0);
}

/// Abandoned baby: a candle in trend direction, a gap, a doji isolated by gaps on both sides,
/// then a candle against the trend on the far side of the second gap.
#[test]
fn test_scenario_candle_classification_abandoned_baby() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    // Bullish trend candle, high = 111.
    ind.on_bar(&Bar::new(0, 100.0, 111.0, 99.0, 110.0, 1000.0));
    // Doji, gapped above (low = 129.5 > 111) and above the third bar's high (121) on both sides.
    ind.on_bar(&Bar::new(60, 130.0, 130.5, 129.5, 130.05, 1000.0));
    // Bearish candle against the trend, gapped below the doji (high = 121 < 129.5).
    let out = ind
        .on_bar(&Bar::new(120, 120.0, 121.0, 115.0, 116.0, 1000.0))
        .expect("output");

    assert_eq!(out.extra["abandoned_baby"], 1.0);
}

/// Breakaway: a gap in trend direction, three small continuation candles, then a large counter
/// candle closing back into the gap.
#[test]
fn test_scenario_candle_classification_breakaway() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 121.0, 99.0, 120.0, 1000.0)); // large bullish, high 121
    ind.on_bar(&Bar::new(60, 125.0, 127.0, 124.0, 126.0, 1000.0)); // gap up (low 124 > 121), small
    ind.on_bar(&Bar::new(120, 126.0, 128.0, 125.0, 127.0, 1000.0)); // small continuation
    ind.on_bar(&Bar::new(180, 127.0, 129.0, 126.0, 128.0, 1000.0)); // small continuation
                                                                    // Large bearish candle closing into the gap (between 121 and 124).
    let out = ind
        .on_bar(&Bar::new(240, 128.0, 129.0, 115.0, 122.0, 1000.0))
        .expect("output");

    assert_eq!(out.extra["breakaway"], 1.0);
}

/// Kicking: two marubozu of opposite colour, the second gapping past the first's own extreme.
#[test]
fn test_scenario_candle_classification_kicking() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 141.0, 100.0, 101.0, 1000.0)); // bearish marubozu
    let out = ind
        .on_bar(&Bar::new(60, 200.0, 240.0, 199.0, 239.0, 1000.0)) // bullish marubozu, gapped up
        .expect("output");

    assert_eq!(out.extra["kicking"], 1.0);
}

/// Counterattack lines: opposite colours, matching closes.
#[test]
fn test_scenario_candle_classification_counterattack_lines() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.5, 99.5, 110.0, 1000.0)); // bullish, close 110
    let out = ind
        .on_bar(&Bar::new(60, 130.0, 130.5, 109.5, 110.0, 1000.0)) // bearish, same close
        .expect("output");

    assert_eq!(out.extra["counterattack_lines"], 1.0);
}

/// Separating lines: opposite colours, matching opens.
#[test]
fn test_scenario_candle_classification_separating_lines() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 100.5, 79.5, 80.0, 1000.0)); // bearish, open 100
    let out = ind
        .on_bar(&Bar::new(60, 100.0, 130.5, 99.5, 130.0, 1000.0)) // bullish, same open
        .expect("output");

    assert_eq!(out.extra["separating_lines"], 1.0);
}

/// Without the opening gap neither pattern exists: a continuous quote opens at the previous close,
/// and a near-doji pair of opposite colours then matches closes and opens by construction.
#[test]
fn test_scenario_candle_classification_no_counterattack_or_separating_without_gap() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 100.6, 99.6, 100.05, 1000.0)); // tiny bullish body
    let out = ind
        .on_bar(&Bar::new(60, 100.05, 100.5, 99.5, 100.0, 1000.0)) // opens at the close, tiny bearish
        .expect("output");

    assert!(
        !out.extra.contains_key("counterattack_lines"),
        "{:?}",
        out.extra
    );
    assert!(
        !out.extra.contains_key("separating_lines"),
        "{:?}",
        out.extra
    );
}

/// Matching low: two bearish candles closing at (almost) the same price.
#[test]
fn test_scenario_candle_classification_matching_low() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.5, 100.0, 1000.0)); // bearish, close 100
    let out = ind
        .on_bar(&Bar::new(60, 160.0, 160.5, 99.5, 100.0, 1000.0)) // bearish, same close
        .expect("output");

    assert_eq!(out.extra["matching_low"], 1.0);
}

/// Homing pigeon: a harami where both candles are bearish.
#[test]
fn test_scenario_candle_classification_homing_pigeon() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 141.0, 99.0, 100.0, 1000.0)); // bearish, body [100, 140]
    let out = ind
        .on_bar(&Bar::new(60, 130.0, 131.0, 120.0, 125.0, 1000.0)) // bearish, inside the body
        .expect("output");

    assert_eq!(out.extra["homing_pigeon"], 1.0);
}

/// Tri-star: three dojis, the middle one isolated by gaps on both sides.
#[test]
fn test_scenario_candle_classification_tri_star() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 101.0, 99.0, 100.1, 1000.0)); // doji
    ind.on_bar(&Bar::new(60, 150.0, 150.55, 149.65, 150.03, 1000.0)); // doji, gapped above
    let out = ind
        .on_bar(&Bar::new(120, 110.0, 110.5, 109.6, 110.05, 1000.0)) // doji, gapped below the middle
        .expect("output");

    assert_eq!(out.extra["tri_star"], 1.0);
}

/// Tasuki gap: two candles in trend direction with a gap, then a counter candle staying inside it.
#[test]
fn test_scenario_candle_classification_tasuki_gap() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.5, 99.5, 110.0, 1000.0)); // bullish, high 110.5
    ind.on_bar(&Bar::new(60, 130.0, 140.6, 129.7, 140.0, 1000.0)); // bullish, gapped up
    let out = ind
        .on_bar(&Bar::new(120, 150.0, 150.5, 119.5, 120.0, 1000.0)) // bearish, closes inside the gap
        .expect("output");

    assert_eq!(out.extra["tasuki_gap"], 1.0);
}

/// Upside gap two crows: a bullish candle, a gap up, then a bearish candle enclosed by a second
/// bearish candle that still closes above the pre-gap close.
#[test]
fn test_scenario_candle_classification_upside_gap_two_crows() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.5, 99.5, 110.0, 1000.0)); // bullish, close 110
    ind.on_bar(&Bar::new(60, 140.0, 140.6, 129.9, 130.0, 1000.0)); // bearish, gapped up
    let out = ind
        .on_bar(&Bar::new(120, 145.0, 145.5, 114.5, 115.0, 1000.0)) // bearish, encloses, closes above 110
        .expect("output");

    assert_eq!(out.extra["upside_gap_two_crows"], 1.0);
}

/// Three stars in the south: three bearish candles, shrinking ranges, rising lows.
#[test]
fn test_scenario_candle_classification_three_stars_in_the_south() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.5, 100.0, 1000.0)); // range 41, low 99.5
    ind.on_bar(&Bar::new(60, 125.0, 125.3, 109.8, 110.0, 1000.0)); // range 15.5, low 109.8
    let out = ind
        .on_bar(&Bar::new(120, 118.0, 118.2, 114.9, 115.0, 1000.0)) // range 3.3, low 114.9
        .expect("output");

    assert_eq!(out.extra["three_stars_in_the_south"], 1.0);
}

/// Deliberation: two large bullish candles, then a small third opening at the second's close.
#[test]
fn test_scenario_candle_classification_deliberation() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 120.5, 99.5, 120.0, 1000.0));
    ind.on_bar(&Bar::new(60, 120.0, 140.5, 119.5, 140.0, 1000.0));
    let out = ind
        .on_bar(&Bar::new(120, 140.0, 141.0, 139.7, 140.2, 1000.0)) // small, opens at 140
        .expect("output");

    assert_eq!(out.extra["deliberation"], 1.0);
}

/// Stick sandwich: bearish, bullish, bearish — the outer candles closing at the same price.
#[test]
fn test_scenario_candle_classification_stick_sandwich() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.5, 100.0, 1000.0)); // bearish, close 100
    ind.on_bar(&Bar::new(60, 100.0, 120.5, 99.5, 120.0, 1000.0)); // bullish
    let out = ind
        .on_bar(&Bar::new(120, 120.0, 120.5, 99.5, 100.0, 1000.0)) // bearish, same close as bar 0
        .expect("output");

    assert_eq!(out.extra["stick_sandwich"], 1.0);
}

/// Unique three river bottom: a large bearish candle, a bearish candle with a long lower wick at
/// a new low, then a small bullish candle holding above that low.
#[test]
fn test_scenario_candle_classification_unique_three_river_bottom() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.5, 100.0, 1000.0)); // low 99.5
    ind.on_bar(&Bar::new(60, 100.0, 100.5, 59.5, 95.0, 1000.0)); // new low, long lower wick
    let out = ind
        .on_bar(&Bar::new(120, 95.0, 95.5, 90.0, 95.3, 1000.0)) // small bullish, holds above 59.5
        .expect("output");

    assert_eq!(out.extra["unique_three_river_bottom"], 1.0);
}

/// Three-line strike: three same-direction candles making progress, then a single counter
/// candle whose range covers all three.
#[test]
fn test_scenario_candle_classification_three_line_strike() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.5, 99.5, 110.0, 1000.0));
    ind.on_bar(&Bar::new(60, 110.0, 120.5, 109.5, 120.0, 1000.0));
    ind.on_bar(&Bar::new(120, 120.0, 130.5, 119.5, 130.0, 1000.0));
    let out = ind
        .on_bar(&Bar::new(180, 130.0, 131.0, 89.0, 90.0, 1000.0)) // bearish, engulfs all three
        .expect("output");

    assert_eq!(out.extra["three_line_strike"], 1.0);
}

/// Concealing baby swallow: two bearish marubozu, then a bearish candle poking into the previous
/// body, then a candle that fully encloses it.
#[test]
fn test_scenario_candle_classification_concealing_baby_swallow() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.8, 100.0, 1000.0)); // bearish marubozu
    ind.on_bar(&Bar::new(60, 100.0, 100.3, 59.7, 60.0, 1000.0)); // bearish marubozu
    ind.on_bar(&Bar::new(120, 60.0, 75.0, 55.0, 58.0, 1000.0)); // upper wick into [60, 100]
    let out = ind
        .on_bar(&Bar::new(180, 58.0, 80.0, 40.0, 45.0, 1000.0)) // encloses [55, 75]
        .expect("output");

    assert_eq!(out.extra["concealing_baby_swallow"], 1.0);
}

/// Rising three methods: a large bullish candle, three small candles inside its range, then a
/// bullish close beyond its high. The same bars also read as a mat hold — the two definitions
/// overlap wherever the closing candle both breaks out and is the larger of the two directions.
#[test]
fn test_scenario_candle_classification_rising_three_methods_and_mat_hold() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 125.5, 99.2, 125.0, 1000.0)); // large bullish
    ind.on_bar(&Bar::new(60, 125.0, 125.3, 123.5, 124.0, 1000.0)); // small, contained
    ind.on_bar(&Bar::new(120, 124.0, 124.2, 122.5, 123.0, 1000.0)); // small, contained
    ind.on_bar(&Bar::new(180, 123.0, 123.2, 121.5, 122.0, 1000.0)); // small, contained
    let out = ind
        .on_bar(&Bar::new(240, 122.0, 146.5, 121.5, 146.0, 1000.0)) // closes past 125.5
        .expect("output");

    assert_eq!(out.extra["rising_three_methods"], 1.0);
    assert_eq!(out.extra["mat_hold"], 1.0);
}

/// Falling three methods: the mirrored shape.
#[test]
fn test_scenario_candle_classification_falling_three_methods() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.8, 114.5, 115.0, 1000.0)); // large bearish
    ind.on_bar(&Bar::new(60, 115.0, 116.5, 114.7, 116.0, 1000.0)); // small, contained
    ind.on_bar(&Bar::new(120, 116.0, 117.5, 115.8, 117.0, 1000.0)); // small, contained
    ind.on_bar(&Bar::new(180, 117.0, 118.5, 116.8, 118.0, 1000.0)); // small, contained
    let out = ind
        .on_bar(&Bar::new(240, 118.0, 118.5, 93.5, 94.0, 1000.0)) // closes past 114.5
        .expect("output");

    assert_eq!(out.extra["falling_three_methods"], 1.0);
}

/// Ladder bottom: three bearish candles with falling closes, a bearish candle with a long upper
/// wick, then a bullish candle gapping above it.
#[test]
fn test_scenario_candle_classification_ladder_bottom() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_classification", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 119.5, 120.0, 1000.0));
    ind.on_bar(&Bar::new(60, 120.0, 120.5, 104.5, 105.0, 1000.0));
    ind.on_bar(&Bar::new(120, 105.0, 105.5, 94.5, 95.0, 1000.0));
    ind.on_bar(&Bar::new(180, 95.0, 110.0, 89.5, 90.0, 1000.0)); // long upper wick
    let out = ind
        .on_bar(&Bar::new(240, 115.0, 120.0, 114.5, 119.0, 1000.0)) // bullish, opens above 110
        .expect("output");

    assert_eq!(out.extra["ladder_bottom"], 1.0);
}

// ============================================================================
// 3. Liquidity Fair Value Gap (FVG)
// ============================================================================
#[test]
fn test_scenario_liquidity_fvg() {
    let mut ind = build_checked(
        "liquidity_fvg",
        &HashMap::from([("lookback".to_string(), 5.0)]),
    )
    .unwrap();

    // Bar 0: High = 100.0
    let b0 = Bar::new(0, 95.0, 100.0, 94.0, 99.0, 1000.0);
    ind.on_bar(&b0);

    // Bar 1: Impulse upward
    let b1 = Bar::new(60, 101.0, 115.0, 101.0, 114.0, 5000.0);
    ind.on_bar(&b1);

    // Bar 2: Low = 106.0 -> Gap between Bar 2 Low (106) and Bar 0 High (100) = 6.0!
    let b2 = Bar::new(120, 108.0, 120.0, 106.0, 119.0, 2000.0);
    let out = ind.on_bar(&b2).expect("Bar 2 should yield output");

    assert_eq!(
        out.extra["fvg_type"], 1.0,
        "FVG type must be 1.0 (Bullish FVG)"
    );
    common::assert_close(out.extra["gap_size"], 6.0, 1e-9, "FVG Gap Size");

    let alerts = ind.alerts();
    assert!(
        alerts
            .iter()
            .any(|a| a.kind == "bullish_fvg" && a.note.contains("$100.00 - $106.00")),
        "Must emit bullish_fvg alert with exact zone boundaries"
    );
}

// ============================================================================
// 4. Liquidity Pools (Equal Highs / BSL & SSL Pools)
// ============================================================================
#[test]
fn test_scenario_liquidity_pools() {
    let mut ind = build_checked(
        "liquidity_pools",
        &HashMap::from([
            ("pivot_len".to_string(), 2.0),
            ("tolerance_pct".to_string(), 0.5),
        ]),
    )
    .unwrap();

    // Traced bar-by-bar (pivot_len=2 => 5-bar pivot window, tolerance_pct=0.5% => 0.005 relative
    // merge tolerance); the same values come out of the reference implementation in
    // `reference/kestrel_reference/structure.py` and stand in `scenario_structure.txt`:
    //   - Bar idx4 (window bars 0..4, mid=bar2 H=110.0): bar2 is a pivot high (no bar in the
    //     window has a higher high) => registers a BSL pool at 110.0. active_count=1.
    //   - Bar idx6 (window bars 2..6, mid=bar4 L=100.0): bar4 is a pivot low => registers an SSL
    //     pool at 100.0. In the SAME bar, the *current* bar's own high (110.2) already pierces
    //     the BSL@110.0 pool while its close (109.0) stays below it => immediate stop hunt on
    //     BSL@110.0 (the pool is consumed here, well before bar idx9 -- the pool price is fixed
    //     at pivot-detection time, not at the "Pivot High 2" bar's own close). active_count=1
    //     (BSL@110.0 now StopHunted, SSL@100.0 newly Active).
    //   - Bar idx8 (window bars 4..8, mid=bar6 H=110.2): bar6 is a pivot high. The old BSL@110.0
    //     pool is no longer Active (StopHunted), so no cluster-merge happens -- a fresh,
    //     independent BSL pool forms at 110.2. active_count=2.
    //   - Bar idx9 (current bar H=111.5/C=108.0): pierces and closes back below the BSL@110.2
    //     pool => second stop hunt, this time on BSL@110.2. active_count=1 (only SSL@100.0
    //     remains Active).
    let bars = vec![
        Bar::new(0, 100.0, 102.0, 98.0, 101.0, 1000.0),
        Bar::new(60, 102.0, 106.0, 101.0, 105.0, 1000.0),
        Bar::new(120, 105.0, 110.0, 104.0, 108.0, 1000.0), // Pivot High 1 (BSL@110.0 forms at idx4)
        Bar::new(180, 107.0, 108.0, 102.0, 104.0, 1000.0),
        Bar::new(240, 103.0, 105.0, 100.0, 102.0, 1000.0), // Pivot Low forms (SSL@100.0) at idx6
        Bar::new(300, 102.0, 107.0, 101.0, 106.0, 1000.0),
        Bar::new(360, 106.0, 110.2, 105.0, 109.0, 1000.0), // Stop hunt #1: BSL@110.0 swept & closed back below
        Bar::new(420, 108.0, 108.0, 103.0, 104.0, 1000.0),
        Bar::new(480, 103.0, 105.0, 101.0, 102.0, 1000.0), // Pivot High 2 forms fresh BSL@110.2 at idx8
        Bar::new(540, 103.0, 111.5, 102.0, 108.0, 2000.0), // Stop hunt #2: BSL@110.2 swept & closed back below
    ];

    let mut stop_hunts = Vec::new();
    let mut last_active_count = None;
    for b in &bars {
        if let Some(out) = ind.on_bar(b) {
            last_active_count = Some(out.value);
        }
        for a in ind.alerts() {
            // Both hunted pools are BSL (highs): the hunt turns down.
            if a.kind == "bear_pool_stop_hunt" {
                stop_hunts.push(a.note);
            }
        }
    }

    assert_eq!(
        stop_hunts.len(),
        expected("liquidity_pools_stop_hunt_count") as usize,
        "Must record exactly two stop hunts (BSL@110.0 then BSL@110.2): {stop_hunts:?}"
    );
    for (i, note) in stop_hunts.iter().enumerate() {
        let price = expected(&format!("liquidity_pools_stop_hunt_{}_price", i + 1));
        assert!(
            note.contains(&format!("{price:.4}")),
            "Stop hunt {} must be the BSL pool at {price:.4}: {note}",
            i + 1
        );
    }
    common::assert_close(
        last_active_count.expect("liquidity_pools should have produced output"),
        expected("liquidity_pools_active_last"),
        expected("scenario_structure_tolerance"),
        "Final active pool count (only SSL@100.0 remains Active)",
    );
}

// ============================================================================
// 5. Liquidity Sweeps
// ============================================================================
#[test]
fn test_scenario_liquidity_sweeps() {
    let mut ind = build_checked(
        "liquidity_sweeps",
        &HashMap::from([
            ("pivot_len".to_string(), 2.0),
            ("tolerance_pct".to_string(), 0.5),
        ]),
    )
    .unwrap();

    let bars = vec![
        Bar::new(0, 100.0, 102.0, 98.0, 101.0, 1000.0),
        Bar::new(60, 102.0, 106.0, 101.0, 105.0, 1000.0),
        Bar::new(120, 105.0, 110.0, 104.0, 108.0, 1000.0), // Pivot High (110.0)
        Bar::new(180, 106.0, 107.0, 102.0, 103.0, 1000.0),
        Bar::new(240, 103.0, 104.0, 100.0, 101.0, 1000.0), // Confirmed!
        Bar::new(300, 102.0, 111.5, 101.0, 107.0, 2000.0), // Sweep bar
    ];

    let mut last_sweep = 0.0;
    let mut kinds = Vec::new();
    for b in &bars {
        if let Some(out) = ind.on_bar(b) {
            last_sweep = out.extra["sweep"];
        }
        kinds.extend(ind.alerts().into_iter().map(|a| a.kind));
    }
    assert_eq!(kinds, ["bear_sweep"], "the swept high turns down");

    assert_eq!(
        last_sweep, -1.0,
        "Sweep state must be -1.0 (Bearish Liquidity Sweep)"
    );
}

// ============================================================================
// 6. Market Structure Breaks (MSB)
// ============================================================================
#[test]
fn test_scenario_market_structure_breaks() {
    let mut ind = build_checked(
        "market_structure_breaks",
        &HashMap::from([("lookback".to_string(), 2.0)]),
    )
    .unwrap();

    let bars = vec![
        Bar::new(0, 100.0, 102.0, 98.0, 101.0, 1000.0),
        Bar::new(60, 102.0, 106.0, 101.0, 105.0, 1000.0),
        Bar::new(120, 105.0, 110.0, 104.0, 108.0, 1000.0), // Swing High (110.0)
        Bar::new(180, 107.0, 108.0, 103.0, 104.0, 1000.0),
        Bar::new(240, 103.0, 104.0, 100.0, 101.0, 1000.0), // Confirmed!
        Bar::new(300, 102.0, 115.0, 102.0, 114.0, 2000.0), // Breakout bar
    ];

    let mut last_signal = 0.0;
    let mut alerts = Vec::new();
    for b in &bars {
        if let Some(out) = ind.on_bar(b) {
            last_signal = out.value;
            for a in ind.alerts() {
                alerts.push(a.kind);
            }
        }
    }

    assert_eq!(
        last_signal, 2.0,
        "Signal value must be 2.0 (Bullish Change of Character)"
    );
    assert!(
        alerts.iter().any(|k| k == "bullish_choch"),
        "Must emit bullish_choch alert"
    );
}

// ============================================================================
// 7. Institutional Order Block (OB)
// ============================================================================
#[test]
fn test_scenario_order_block() {
    let mut ind = build_checked(
        "order_block",
        &HashMap::from([("atr_len".to_string(), 5.0), ("min_disp".to_string(), 1.5)]),
    )
    .unwrap();

    // 6 warmup bars with steady small range (ATR ~ 2.0)
    for i in 0..6 {
        let b = Bar::new(i * 60, 100.0, 101.0, 99.0, 100.0, 1000.0);
        ind.on_bar(&b);
    }

    // Down candle (potential Demand Order Block)
    let down_bar = Bar::new(360, 101.0, 102.0, 97.0, 98.0, 1000.0);
    ind.on_bar(&down_bar);

    // Massive Bullish Displacement Candle (Body = 118 - 98 = 20 > 1.5 * 2)
    let disp_bar = Bar::new(420, 98.0, 120.0, 98.0, 118.0, 5000.0);
    let disp_out = ind
        .on_bar(&disp_bar)
        .expect("Displacement should yield output");

    assert!(
        disp_out.extra["active_count"] >= 1.0,
        "Must record at least 1 active order block"
    );
    common::assert_close(
        disp_out.extra["active_ob_top"],
        102.0,
        1e-9,
        "Order Block Top",
    );
    common::assert_close(
        disp_out.extra["active_ob_bottom"],
        97.0,
        1e-9,
        "Order Block Bottom",
    );
    assert!(
        ind.alerts().iter().any(|a| a.kind == "bullish_order_block"),
        "Must emit bullish_order_block alert"
    );

    // Pullback bar touching into the OB zone (low = 100.0 <= ob_top 102.0, close = 104.0 >= ob_bottom 97.0)
    let pullback = Bar::new(480, 110.0, 112.0, 100.0, 104.0, 1500.0);
    ind.on_bar(&pullback);
    assert!(
        ind.alerts().iter().any(|a| a.kind == "ob_retest_bullish"),
        "Must emit ob_retest_bullish alert upon zone test"
    );
}

// ============================================================================
// 8. Pivot Sets (Classic Pivot Levels)
// ============================================================================
#[test]
fn test_scenario_pivot_sets() {
    let mut ind = build_checked("pivot_sets", &HashMap::new()).unwrap();
    let b = Bar::new(0, 100.0, 110.0, 90.0, 105.0, 1000.0);
    let out = ind.on_bar(&b).expect("Pivot sets should output on bar");

    // Classic Pivot Points: H=110, L=90, C=105
    // P  = (110 + 90 + 105) / 3 = 305/3 = 101.666666667
    // R1 = 2P - L = 203.333333333 - 90 = 113.333333333
    // S1 = 2P - H = 203.333333333 - 110 = 93.333333333
    // R2 = P + (H - L) = 101.666666667 + 20 = 121.666666667
    // S2 = P - (H - L) = 101.666666667 - 20 = 81.666666667
    let expected_p = 305.0 / 3.0;
    let expected_r1 = 2.0 * expected_p - 90.0;
    let expected_s1 = 2.0 * expected_p - 110.0;
    let expected_r2 = expected_p + 20.0;
    let expected_s2 = expected_p - 20.0;

    common::assert_close(out.extra["p"], expected_p, 1e-9, "Classic Pivot P");
    common::assert_close(out.extra["r1"], expected_r1, 1e-9, "Classic Pivot R1");
    common::assert_close(out.extra["s1"], expected_s1, 1e-9, "Classic Pivot S1");
    common::assert_close(out.extra["r2"], expected_r2, 1e-9, "Classic Pivot R2");
    common::assert_close(out.extra["s2"], expected_s2, 1e-9, "Classic Pivot S2");
}

// ============================================================================
// 9. Pivots Structure (Fractal Swings & Bias Score)
// ============================================================================
#[test]
fn test_scenario_pivots_structure() {
    let mut ind = build_checked(
        "pivots_structure",
        &HashMap::from([
            ("left_bars".to_string(), 2.0),
            ("right_bars".to_string(), 2.0),
            ("score_window".to_string(), 3.0),
        ]),
    )
    .unwrap();

    // Traced bar-by-bar (left_bars=2, right_bars=2 => candidate_idx = bars.len()-1-right_bars
    // over the growing bar history, not a fixed sliding window); the same values come out of the
    // reference implementation in `reference/kestrel_reference/structure.py`:
    //   - At bars.len()=5 (bar idx4), candidate_idx=2 (bar2, H=110.0) is the first-ever pivot
    //     high. It only *seeds* `last_high` (no prior high to compare against yet) => no score
    //     contribution.
    //   - At bars.len()=7 (bar idx6), candidate_idx=4 (bar4, L=100.0) is the first-ever pivot
    //     low. Same seeding effect on `last_low` => still no score contribution.
    //   - No further pivots are found until bars.len()=10 (bar idx9, the last bar): candidate_idx
    //     =7 (bar7, H=120.0) is a pivot high, and this time `last_high` already holds the prior
    //     110.0 => cand_high(120.0) > prev(110.0) contributes +2.0. This is the *only* score ever
    //     pushed into `pivot_scores`, so score = (2.0 / (score_window=3 * 2.0)) * 100 =
    //     2.0/6.0*100 = 33.333...%, and every bar before it holds score=0.0 exactly (empty
    //     `pivot_scores`, not just "not yet positive").
    let bars = vec![
        Bar::new(0, 100.0, 102.0, 98.0, 101.0, 1000.0),
        Bar::new(60, 102.0, 105.0, 101.0, 104.0, 1000.0),
        Bar::new(120, 105.0, 110.0, 104.0, 108.0, 1000.0), // High 1 (110.0) -- seeds last_high, no score
        Bar::new(180, 106.0, 107.0, 102.0, 103.0, 1000.0),
        Bar::new(240, 103.0, 104.0, 100.0, 102.0, 1000.0), // Low (100.0) -- seeds last_low, no score
        Bar::new(300, 102.0, 108.0, 101.0, 107.0, 1000.0),
        Bar::new(360, 107.0, 115.0, 106.0, 114.0, 1000.0),
        Bar::new(420, 114.0, 120.0, 113.0, 118.0, 1000.0), // High 2 (120.0) confirmed at idx9 -> +2.0
        Bar::new(480, 116.0, 117.0, 110.0, 112.0, 1000.0),
        Bar::new(540, 112.0, 113.0, 108.0, 110.0, 1000.0),
    ];

    let mut scores = Vec::new();
    for b in &bars {
        if let Some(out) = ind.on_bar(b) {
            scores.push(out.value);
        }
    }

    assert_eq!(
        scores.len(),
        expected("pivots_structure_output_count") as usize,
        "Expected one output per bar from bars.len()>=5 onward"
    );
    for (i, &s) in scores.iter().enumerate() {
        common::assert_close(
            s,
            expected(&format!("pivots_structure_score_{}", i + 1)),
            expected("scenario_structure_tolerance"),
            "0.0 before the second pivot, then 2.0/(score_window*2.0)*100 after the Higher High",
        );
    }
}

// ============================================================================
// 10. Wyckoff State Machine (Phases A..E)
// ============================================================================

/// The textbook accumulation and the other hand-built Wyckoff series, as the contract fixture
/// carries them (`reference/kestrel_reference/fixtures/contract_structure.py`).
const WYCKOFF_CASES: &str = include_str!("fixtures/contract_structure.txt");

fn wyckoff_case(name: &str) -> common::contract::ContractCase {
    common::contract::load(
        WYCKOFF_CASES,
        name,
        "wyckoff",
        kestrel_chartkit::IndicatorUnit::Unitless,
    )
}

/// 0-based position of the bar with this timestamp.
fn bar_of(bars: &[Bar], timestamp: i64) -> usize {
    bars.iter()
        .position(|b| b.timestamp == timestamp)
        .expect("event on a bar of the series")
}

/// What a run of the typed state machine shows: the first bar of every phase it entered, and the
/// events of the range in play at the end with the bar each lies on and its price.
struct WyckoffRun {
    machine: kestrel_chartkit::indicator::wyckoff::WyckoffStateMachine,
    phase_starts: Vec<(usize, f64)>,
    events: Vec<(
        kestrel_chartkit::indicator::wyckoff::WyckoffEventKind,
        usize,
        f64,
    )>,
}

fn wyckoff_run(bars: &[Bar]) -> WyckoffRun {
    let mut machine = kestrel_chartkit::indicator::wyckoff::WyckoffStateMachine::with_defaults();
    let mut phase_starts = Vec::new();
    let mut previous = None;
    for (i, bar) in bars.iter().enumerate() {
        let value = machine.on_bar(bar).map(|o| o.value);
        if value != previous {
            if let Some(value) = value {
                phase_starts.push((i, value));
            }
            previous = value;
        }
    }
    let events = machine
        .events()
        .iter()
        .map(|e| (e.kind, bar_of(bars, e.timestamp), e.price))
        .collect();
    WyckoffRun {
        machine,
        phase_starts,
        events,
    }
}

fn assert_wyckoff_events(
    actual: &[(
        kestrel_chartkit::indicator::wyckoff::WyckoffEventKind,
        usize,
        f64,
    )],
    expected: &[(
        kestrel_chartkit::indicator::wyckoff::WyckoffEventKind,
        usize,
        f64,
    )],
) {
    assert_eq!(
        actual.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        expected.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        "event kinds and bars"
    );
    for (a, e) in actual.iter().zip(expected) {
        common::assert_close(a.2, e.2, 1e-9, &format!("price of {:?}", e.0));
    }
}

/// The textbook accumulation with the default thresholds. The expected bars are those the series
/// was built around (bars count from 0): Preliminary Support 16, Selling Climax 21 (low 94.0),
/// Automatic Rally 24 (high 100.0), Secondary Test 29, Spring 48 (low 93.4), Sign of Strength 53
/// (close 101.6), Last Point of Support 57 (low 100.1).
///
/// Two expectations follow from the documented rules rather than from the construction:
/// - The range is set on bar 26. The rally high is bar 24 (low 98.1); bar 25 closes at 98.7,
///   bar 26 at 97.4, the first close below that low.
/// - Bar 38 is a second Secondary Test: its low of 94.2 is within a quarter ATR (about 0.4) of
///   the range low, on a volume of 70 against the climax's 430 and 1.4 wide against 3.2.
#[test]
fn test_scenario_wyckoff_textbook_accumulation() {
    use kestrel_chartkit::indicator::wyckoff::{
        WyckoffBias, WyckoffEventKind as Kind, WyckoffPhase, WyckoffRangeSource,
    };

    let bars = wyckoff_case("wyckoff_textbook_accumulation").bars;
    assert_eq!(bars.len(), 62);
    let run = wyckoff_run(&bars);

    assert_wyckoff_events(
        &run.events,
        &[
            (Kind::PreliminarySupport, 16, 97.5),
            (Kind::SellingClimax, 21, 94.0),
            (Kind::AutomaticRally, 24, 100.0),
            (Kind::SecondaryTest, 29, 95.3),
            (Kind::SecondaryTest, 38, 95.4),
            (Kind::Spring, 48, 93.4),
            (Kind::SignOfStrength, 53, 101.6),
            (Kind::LastPointOfSupport, 57, 100.1),
        ],
    );
    // No output while the window of 20 bars fills, then no range, then A to E.
    assert_eq!(
        run.phase_starts,
        vec![
            (19, 0.0),
            (26, 1.0),
            (29, 2.0),
            (48, 3.0),
            (53, 4.0),
            (57, 5.0)
        ]
    );
    assert_eq!(run.machine.phase(), WyckoffPhase::E);
    assert_eq!(run.machine.bias(), Some(WyckoffBias::Accumulation));
    let range = run.machine.range().expect("range set");
    common::assert_close(range.low, 94.0, 1e-9, "range low: the climax low");
    common::assert_close(range.high, 100.0, 1e-9, "range high: the rally high");
    assert_eq!(range.source, WyckoffRangeSource::Climax);
    assert_eq!(run.machine.score().sequence_quality, 1.0);
    // The cause counts the bars of Phases A to D: bars 27 to 57, 31 bars of a range 6 wide.
    common::assert_close(run.machine.score().cause_score, 186.0, 1e-9, "cause");
}

/// The same series reflected at 200: the mirror events on the same bars, the range from 100
/// (the reaction low) to 106 (the climax high).
#[test]
fn test_scenario_wyckoff_textbook_distribution() {
    use kestrel_chartkit::indicator::wyckoff::{
        WyckoffBias, WyckoffEventKind as Kind, WyckoffPhase, WyckoffRangeSource,
    };

    let bars = wyckoff_case("wyckoff_textbook_distribution").bars;
    let run = wyckoff_run(&bars);

    assert_wyckoff_events(
        &run.events,
        &[
            (Kind::PreliminarySupply, 16, 102.5),
            (Kind::BuyingClimax, 21, 106.0),
            (Kind::AutomaticReaction, 24, 100.0),
            (Kind::SecondaryTest, 29, 104.7),
            (Kind::SecondaryTest, 38, 104.6),
            (Kind::Utad, 48, 106.6),
            (Kind::SignOfWeakness, 53, 98.4),
            (Kind::LastPointOfSupply, 57, 99.9),
        ],
    );
    assert_eq!(
        run.phase_starts,
        vec![
            (19, 0.0),
            (26, 1.0),
            (29, 2.0),
            (48, 3.0),
            (53, 4.0),
            (57, 5.0)
        ]
    );
    assert_eq!(run.machine.phase(), WyckoffPhase::E);
    assert_eq!(run.machine.bias(), Some(WyckoffBias::Distribution));
    let range = run.machine.range().expect("range set");
    common::assert_close(range.low, 100.0, 1e-9, "range low: the reaction low");
    common::assert_close(range.high, 106.0, 1e-9, "range high: the climax high");
    assert_eq!(range.source, WyckoffRangeSource::Climax);
}

/// The name of an `extra` key or alert kind on the other side of the mirror.
fn wyckoff_mirrored_name(name: &str) -> String {
    const PAIRS: [(&str, &str); 6] = [
        ("preliminarysupport", "preliminarysupply"),
        ("sellingclimax", "buyingclimax"),
        ("automaticrally", "automaticreaction"),
        ("spring", "utad"),
        ("signofstrength", "signofweakness"),
        ("lastpointofsupport", "lastpointofsupply"),
    ];
    let (prefix, event) = name.split_once('_').unwrap_or(("", name));
    for (a, b) in PAIRS {
        if event == a {
            return format!("{prefix}_{b}");
        }
        if event == b {
            return format!("{prefix}_{a}");
        }
    }
    name.to_string()
}

/// Accumulation and distribution are mirror images: reflecting any series at a price level
/// leaves phases, ATR, volume threshold, range source and cause untouched, negates bias and
/// climax, swaps the boundaries and turns every event into its counterpart on the same bar.
/// Checked on every hand-built Wyckoff series and on the random walk, with the parameters of
/// each case.
#[test]
fn test_scenario_wyckoff_mirror_symmetry() {
    const LEVEL: f64 = 200.0;
    for name in [
        "wyckoff_textbook_accumulation",
        "wyckoff_failed_breakout",
        "wyckoff_cycle",
        "wyckoff_fallback_spring",
        "wyckoff_no_volume",
        "wyckoff_no_dispersion",
        "wyckoff_fallback_carried",
        "wyckoff_lps_then_new_range",
        "wyckoff_random_walk",
        "wyckoff_pullback_too_deep",
        "wyckoff_tuned_cycle",
        "wyckoff_tuned_walk",
    ] {
        let case = wyckoff_case(name);
        let mut plain = build_checked("wyckoff", &case.params).unwrap();
        let mut mirrored = build_checked("wyckoff", &case.params).unwrap();
        let mut events = 0;
        for (i, bar) in case.bars.iter().enumerate() {
            let reflected = Bar::new(
                bar.timestamp,
                LEVEL - bar.open,
                LEVEL - bar.low,
                LEVEL - bar.high,
                LEVEL - bar.close,
                bar.volume,
            );
            let (a, b) = (plain.on_bar(bar), mirrored.on_bar(&reflected));
            let at = format!("{name} bar {i}");
            assert_eq!(a.is_some(), b.is_some(), "{at}: output present");
            let (Some(a), Some(b)) = (a, b) else {
                continue;
            };
            assert_eq!(a.value, b.value, "{at}: phase");
            assert_eq!(a.extra.len(), b.extra.len(), "{at}: extra keys");
            for (key, &value) in &a.extra {
                let (other_key, expected) = match key.as_str() {
                    "climax" | "bias" => (key.clone(), -value),
                    "range_high" => ("range_low".to_string(), LEVEL - value),
                    "range_low" => ("range_high".to_string(), LEVEL - value),
                    _ if key.starts_with("event_") => {
                        events += 1;
                        (wyckoff_mirrored_name(key), value)
                    }
                    _ => (key.clone(), value),
                };
                let actual = *b
                    .extra
                    .get(&other_key)
                    .unwrap_or_else(|| panic!("{at}: mirrored run lacks {other_key}"));
                common::assert_close(actual, expected, 1e-9, &format!("{at}: {other_key}"));
            }
            let kinds = |ind: &dyn Indicator, mirror: bool| {
                let mut kinds: Vec<String> = ind
                    .alerts()
                    .into_iter()
                    .map(|alert| {
                        if mirror {
                            wyckoff_mirrored_name(&alert.kind)
                        } else {
                            alert.kind
                        }
                    })
                    .collect();
                kinds.sort();
                kinds
            };
            assert_eq!(
                kinds(plain.as_ref(), true),
                kinds(mirrored.as_ref(), false),
                "{at}: alerts"
            );
        }
        assert!(events > 0, "{name}: the case reports events");
    }
}

/// Phase E does not end by a width rule: on the accumulation followed by its mirror image it
/// lasts through the whole markup (bars 57 to 87) and ends on bar 88, where the Automatic
/// Reaction of the distribution is confirmed and the next range is set.
#[test]
fn test_scenario_wyckoff_phase_e_ends_with_the_next_range() {
    use kestrel_chartkit::indicator::wyckoff::{WyckoffBias, WyckoffEventKind as Kind};

    let bars = wyckoff_case("wyckoff_cycle").bars;
    assert_eq!(bars.len(), 124);
    let run = wyckoff_run(&bars);
    assert_eq!(
        run.phase_starts,
        vec![
            (19, 0.0),
            (26, 1.0),
            (29, 2.0),
            (48, 3.0),
            (53, 4.0),
            (57, 5.0),
            // The second range: the first one's bars shifted by the 62 bars of the accumulation.
            (88, 1.0),
            (91, 2.0),
            (110, 3.0),
            (115, 4.0),
            (119, 5.0)
        ]
    );
    assert_eq!(run.machine.bias(), Some(WyckoffBias::Distribution));
    // Only the events of the range in play: the mirror at 214.3 puts the climax high at 120.3.
    assert_wyckoff_events(
        &run.events,
        &[
            (Kind::PreliminarySupply, 78, 116.8),
            (Kind::BuyingClimax, 83, 120.3),
            (Kind::AutomaticReaction, 86, 114.3),
            (Kind::SecondaryTest, 91, 119.0),
            (Kind::SecondaryTest, 100, 118.9),
            (Kind::Utad, 110, 120.9),
            (Kind::SignOfWeakness, 115, 112.7),
            (Kind::LastPointOfSupply, 119, 114.2),
        ],
    );
}

/// A breakout that fails: the bar after the Sign of Strength closes at 100.9, the next one at
/// 99.4, back below the range high of 100. Phase D returns to Phase B on that bar (56) and the
/// range stays in play; there is no Last Point of Support.
#[test]
fn test_scenario_wyckoff_failed_breakout_returns_to_phase_b() {
    use kestrel_chartkit::indicator::wyckoff::{WyckoffEventKind as Kind, WyckoffPhase};

    let bars = wyckoff_case("wyckoff_failed_breakout").bars;
    let run = wyckoff_run(&bars);
    assert_eq!(
        run.phase_starts,
        vec![
            (19, 0.0),
            (26, 1.0),
            (29, 2.0),
            (48, 3.0),
            (53, 4.0),
            (56, 2.0)
        ]
    );
    assert_eq!(run.machine.phase(), WyckoffPhase::B);
    assert_eq!(
        run.events.last().map(|e| (e.0, e.1)),
        Some((Kind::SignOfStrength, 53))
    );
    let range = run.machine.range().expect("the range stays in play");
    common::assert_close(range.low, 94.0, 1e-9, "range low");
    common::assert_close(range.high, 100.0, 1e-9, "range high");
}

/// A range without a climax and a series without volume. The closes swing between 100 and 102
/// at one constant volume, so no bar is a volume outlier: the width fallback sets the range on
/// the first bar with an output, with an open bias. Bar 33 undercuts the range and closes back
/// inside: a Spring, which makes it an accumulation. Bar 35 closes above the range without any
/// volume expansion: with the defaults that is no Sign of Strength, the output stays at Phase C
/// until a close more than one ATR above the range abandons it.
#[test]
fn test_scenario_wyckoff_without_volume() {
    use kestrel_chartkit::indicator::wyckoff::{
        WyckoffBias, WyckoffEventKind as Kind, WyckoffPhase, WyckoffRangeSource,
        WyckoffStateMachine,
    };

    let bars = wyckoff_case("wyckoff_no_volume").bars;
    assert!(bars.iter().all(|b| b.volume == 0.0));

    let mut machine = WyckoffStateMachine::with_defaults();
    let mut values = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        let out = machine.on_bar(bar);
        if let Some(out) = &out {
            assert_eq!(
                out.extra["climax"], 0.0,
                "bar {i}: no climax without volume"
            );
            assert!(
                !out.extra.contains_key("volume_threshold"),
                "bar {i}: no dispersion, no threshold"
            );
        }
        if i == 19 {
            let range = machine.range().expect("set on the first full window");
            assert_eq!(range.source, WyckoffRangeSource::Width);
            assert_eq!(machine.bias(), None, "bias open");
            assert_eq!(machine.phase(), WyckoffPhase::A);
        }
        if i == 33 {
            assert_eq!(machine.bias(), Some(WyckoffBias::Accumulation));
            assert_eq!(machine.phase(), WyckoffPhase::C);
        }
        values.push(out.map(|o| o.value));
    }
    assert_eq!(
        values[35],
        Some(3.0),
        "a close above the range without volume"
    );
    assert_eq!(machine.phase(), WyckoffPhase::Undefined);
    assert!(machine.range().is_none());
    assert!(machine
        .events()
        .iter()
        .all(|e| matches!(e.kind, Kind::SecondaryTest | Kind::Spring)));

    // The same prices with `breakout_volume` off: the close above the range is the Sign of
    // Strength, the pullback of bar 36 holds, bar 37 turns up: Last Point of Support, Phase E.
    let mut price_only = build_checked(
        "wyckoff",
        &HashMap::from([("breakout_volume".to_string(), 0.0)]),
    )
    .unwrap();
    let mut last = None;
    let mut kinds = Vec::new();
    for bar in &bars {
        last = price_only.on_bar(bar).map(|o| o.value).or(last);
        kinds.extend(price_only.alerts().into_iter().map(|a| a.kind));
    }
    assert_eq!(last, Some(5.0));
    kinds.retain(|k| k != "wyckoff_secondarytest");
    assert_eq!(
        kinds,
        [
            "wyckoff_spring",
            "wyckoff_signofstrength",
            "wyckoff_lastpointofsupport"
        ]
    );
}

/// Without dispersion there is no volume outlier. From bar 24 on every bar of this series trades
/// 100, so on bar 35 (the close above the range, on 101) the window holds 15 bars of 100 among
/// its 20: the MAD is zero. A threshold on the median would call 101 an outlier; here the bar is
/// no Sign of Strength, `volume_threshold` is absent, and the output stays at Phase C until the
/// range is abandoned. On volumes that keep varying, the same prices with 300 on that bar run on
/// to Phase E.
#[test]
fn test_scenario_wyckoff_without_dispersion_no_volume_is_an_outlier() {
    let case = wyckoff_case("wyckoff_no_dispersion");
    assert!(case.bars[24..]
        .iter()
        .enumerate()
        .all(|(k, b)| b.volume == if k == 11 { 101.0 } else { 100.0 }));
    let mut ind = build_checked("wyckoff", &HashMap::new()).unwrap();
    let mut kinds = Vec::new();
    let mut with_threshold = Vec::new();
    let mut values = Vec::new();
    for (i, bar) in case.bars.iter().enumerate() {
        let out = ind.on_bar(bar);
        if out
            .as_ref()
            .is_some_and(|o| o.extra.contains_key("volume_threshold"))
        {
            with_threshold.push(i);
        }
        values.push(out.map(|o| o.value));
        kinds.extend(ind.alerts().into_iter().map(|a| a.kind));
    }
    // Bars 19 to 23: at least 10 of the 20 volumes still differ from 100.
    assert_eq!(with_threshold, [19, 20, 21, 22, 23]);
    assert_eq!(values[33], Some(3.0), "the Spring");
    assert_eq!(values[35], Some(3.0), "the close above the range on 101");
    assert_eq!(values.last(), Some(&Some(0.0)), "abandoned");
    assert!(!kinds.iter().any(|k| k == "wyckoff_signofstrength"));

    let carried = wyckoff_case("wyckoff_fallback_carried");
    assert_eq!(carried.bars[35].volume, 300.0);
    let mut ind = build_checked("wyckoff", &HashMap::new()).unwrap();
    let mut last = None;
    for (i, bar) in carried.bars.iter().enumerate() {
        let out = ind.on_bar(bar);
        if let Some(out) = &out {
            assert!(out.extra.contains_key("volume_threshold"), "bar {i}");
        }
        if i == 35 {
            assert_eq!(out.as_ref().map(|o| o.value), Some(4.0), "Sign of Strength");
        }
        last = out.map(|o| o.value).or(last);
    }
    assert_eq!(last, Some(5.0));
}

/// A bar that reports an event or changes the phase sets no range in the same step. On the
/// textbook series with `range_atr_max = 5.9` the window of bar 57 is 9.0 wide (Spring low 93.4
/// to the high of 102.4 after the Sign of Strength), 5.81 ATR, so the width fallback applies on
/// the very bar that turns the pullback. That bar reports the Last Point of Support and Phase E
/// with the boundaries and the cause of the range it belongs to; the fallback sets the next
/// range on bar 58, from the same window extremes.
#[test]
fn test_scenario_wyckoff_reporting_bar_sets_no_range() {
    use kestrel_chartkit::indicator::wyckoff::{
        WyckoffEventKind as Kind, WyckoffParams, WyckoffPhase, WyckoffRangeSource,
        WyckoffStateMachine,
    };

    for (name, hold, alert, old, new) in [
        (
            "wyckoff_lps_then_new_range",
            Kind::LastPointOfSupport,
            "wyckoff_lastpointofsupport",
            (94.0, 100.0),
            (93.4, 102.4),
        ),
        (
            "wyckoff_lpsy_then_new_range",
            Kind::LastPointOfSupply,
            "wyckoff_lastpointofsupply",
            (100.0, 106.0),
            (97.6, 106.6),
        ),
    ] {
        let case = wyckoff_case(name);
        assert_eq!(
            case.params,
            HashMap::from([("range_atr_max".to_string(), 5.9)])
        );
        let mut machine = WyckoffStateMachine::with_params(WyckoffParams {
            range_atr_max: 5.9,
            ..WyckoffParams::default()
        });
        let mut outputs = Vec::new();
        for (i, bar) in case.bars.iter().enumerate() {
            outputs.push(machine.on_bar(bar));
            if i == 57 {
                assert_eq!(machine.phase(), WyckoffPhase::E, "{name}");
                assert_eq!(machine.events().last().map(|e| e.kind), Some(hold));
                assert_eq!(
                    machine.events().len(),
                    8,
                    "the events of the range are kept"
                );
                assert_eq!(machine.alerts().len(), 1);
                assert_eq!(machine.alerts()[0].kind, alert);
                let range = machine.range().unwrap();
                assert_eq!(range.source, WyckoffRangeSource::Climax);
            }
            if i == 58 {
                assert_eq!(machine.phase(), WyckoffPhase::A, "{name}");
                assert!(machine.events().is_empty() && machine.alerts().is_empty());
                assert_eq!(machine.bias(), None);
                assert_eq!(machine.range().unwrap().source, WyckoffRangeSource::Width);
            }
        }
        let reporting = outputs[57].as_ref().unwrap();
        assert_eq!(reporting.value, 5.0, "{name}: Phase E on the reporting bar");
        assert_eq!(reporting.extra["range_source"], 1.0);
        common::assert_close(reporting.extra["range_low"], old.0, 1e-9, "old range low");
        common::assert_close(reporting.extra["range_high"], old.1, 1e-9, "old range high");
        common::assert_close(reporting.extra["cause_score"], 186.0, 1e-9, "its cause");
        assert_eq!(reporting.extra[&alert.replace("wyckoff_", "event_")], 0.0);

        let next = outputs[58].as_ref().unwrap();
        assert_eq!(next.value, 1.0, "{name}: the next range, one bar later");
        assert_eq!(next.extra["range_source"], 2.0);
        assert_eq!(next.extra["bias"], 0.0);
        common::assert_close(next.extra["range_low"], new.0, 1e-9, "new range low");
        common::assert_close(next.extra["range_high"], new.1, 1e-9, "new range high");
        assert_eq!(next.extra["cause_score"], 0.0);
        assert!(next.extra.keys().all(|key| !key.starts_with("event_")));
    }
}

/// `extra["atr"]` is the ATR the registry `atr` reports as `extra["raw"]`: Wilder's average of
/// the true range over 14 bars, in price units. The `value` of `atr` is that number in percent
/// of the close, which is what makes the two look different at first sight: on bar 35 of the
/// textbook series the range of 6.0 is 3.64 ATR wide, and 6.0 divided by the percentage is 3.51.
#[test]
fn test_scenario_wyckoff_atr_is_the_registry_atr_in_price_units() {
    let bars = wyckoff_case("wyckoff_textbook_accumulation").bars;
    let mut wyckoff = build_checked("wyckoff", &HashMap::new()).unwrap();
    let mut atr = build_checked(
        "atr",
        &HashMap::from([("atr_len".to_string(), 14.0), ("sig_len".to_string(), 1.0)]),
    )
    .unwrap();
    let mut compared = 0;
    for (i, bar) in bars.iter().enumerate() {
        let (w, a) = (wyckoff.on_bar(bar), atr.on_bar(bar));
        let (Some(w), Some(a)) = (w, a) else {
            continue;
        };
        assert_eq!(w.extra["atr"], a.extra["raw"], "bar {i}");
        common::assert_close(
            a.value,
            100.0 * a.extra["raw"] / bar.close,
            1e-12,
            "atr value: percent of the close",
        );
        if i == 35 {
            common::assert_close(6.0 / w.extra["atr"], 3.64, 0.005, "range width in ATR");
            common::assert_close(6.0 / a.value, 3.51, 0.005, "the same by the percentage");
        }
        compared += 1;
    }
    assert_eq!(compared, bars.len() - 19, "every bar with a wyckoff output");
}

/// No look-ahead: the output of a bar is the same whether or not later bars follow.
#[test]
fn test_scenario_wyckoff_is_causal() {
    for name in ["wyckoff_cycle", "wyckoff_random_walk"] {
        let bars = wyckoff_case(name).bars;
        let mut full = build_checked("wyckoff", &HashMap::new()).unwrap();
        let outputs: Vec<_> = bars.iter().map(|b| full.on_bar(b)).collect();
        for cut in (20..bars.len()).step_by(7) {
            let mut prefix = build_checked("wyckoff", &HashMap::new()).unwrap();
            let mut last = None;
            for bar in &bars[..=cut] {
                last = prefix.on_bar(bar);
            }
            assert_eq!(last, outputs[cut], "{name}: bar {cut}");
        }
    }
}

/// One of the random walks of [`test_scenario_wyckoff_random_walks`]: 1000 bars from `seed`.
fn wyckoff_random_walk(seed: u64) -> Vec<Bar> {
    let mut state = seed;
    let mut uniform = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let round = |x: f64, digits: f64| (x * digits).round() / digits;
    let mut close = 100.0;
    (0..1000)
        .map(|i| {
            let open = close;
            close = round(open + (uniform() + uniform() + uniform() - 1.5) * 1.6, 1e4);
            let high = round(open.max(close) + uniform() * 0.6, 1e4);
            let low = round(open.min(close) - uniform() * 0.6, 1e4);
            let volume = round(1000.0 * (0.5 + uniform()) * (0.6 + 0.5 * (high - low)), 1e2);
            Bar::new(i * 60, open, high, low, close, volume)
        })
        .collect()
}

/// What the default thresholds report on series with no structure built in: 20 random walks of
/// 1000 bars each. A 64-bit linear congruential generator supplies uniform numbers; the close
/// moves by the sum of three of them minus 1.5, times 1.6, the wicks are up to 0.6, and the
/// volume is 1000 x (0.5 + u) x (0.6 + 0.5 x spread), so wide bars tend to trade more.
///
/// Recorded, not judged: the counts below are what the rules yield on these walks. Of the 19620
/// bars with an output, 228 are climax bars by the bar test and 21 of them become the climax of
/// a range; the width fallback sets 235 ranges. Three quarters of the bars carry no range. Most
/// ranges end abandoned: 149 Springs and UTADs lead to 12 Signs of Strength or Weakness and 4
/// Last Points of Support or Supply.
#[test]
fn test_scenario_wyckoff_random_walks() {
    let mut state_counts = [0usize; 6];
    let mut alerts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let (mut climax_bars, mut by_climax, mut by_width) = (0, 0, 0);
    for seed in 1..=20u64 {
        let mut ind = build_checked("wyckoff", &HashMap::new()).unwrap();
        let mut boundaries = None;
        for bar in wyckoff_random_walk(seed) {
            let Some(out) = ind.on_bar(&bar) else {
                continue;
            };
            state_counts[out.value as usize] += 1;
            climax_bars += usize::from(out.extra["climax"] != 0.0);
            let now = out
                .extra
                .get("range_high")
                .map(|high| (high.to_bits(), out.extra["range_low"].to_bits()));
            if now.is_some() && now != boundaries {
                match out.extra["range_source"] as i32 {
                    1 => by_climax += 1,
                    _ => by_width += 1,
                }
            }
            boundaries = now;
            for alert in ind.alerts() {
                *alerts.entry(alert.kind).or_default() += 1;
            }
        }
    }
    let count = |kind: &str| alerts.get(kind).copied().unwrap_or(0);
    assert_eq!(state_counts.iter().sum::<usize>(), 20 * 981);
    assert_eq!(state_counts, [14747, 1422, 920, 2364, 37, 130]);
    assert_eq!((climax_bars, by_climax, by_width), (228, 21, 235));
    assert_eq!(
        count("wyckoff_sellingclimax") + count("wyckoff_buyingclimax"),
        by_climax
    );
    assert_eq!(count("wyckoff_secondarytest"), 235);
    assert_eq!(count("wyckoff_spring") + count("wyckoff_utad"), 149);
    assert_eq!(
        count("wyckoff_signofstrength") + count("wyckoff_signofweakness"),
        12
    );
    assert_eq!(
        count("wyckoff_lastpointofsupport") + count("wyckoff_lastpointofsupply"),
        4
    );
}

// ----------------------------------------------------------------------------
// The transition table (`wyckoff::transitions`) against the behaviour
// ----------------------------------------------------------------------------

/// The names of all Wyckoff cases of the contract fixture.
fn wyckoff_case_names() -> Vec<&'static str> {
    let names: Vec<&str> = WYCKOFF_CASES
        .lines()
        .filter_map(|line| line.strip_prefix('@'))
        .filter_map(|head| head.split(' ').next())
        .filter(|name| name.starts_with("wyckoff_"))
        .collect();
    assert!(names.len() >= 17, "only {} Wyckoff cases", names.len());
    names
}

/// The registry parameters of a case as the typed parameter set.
fn wyckoff_params(
    params: &HashMap<String, f64>,
) -> kestrel_chartkit::indicator::wyckoff::WyckoffParams {
    let mut typed = kestrel_chartkit::indicator::wyckoff::WyckoffParams::default();
    for (key, &value) in params {
        match key.as_str() {
            "range_lookback" => typed.range_lookback = value as usize,
            "range_atr_max" => typed.range_atr_max = value,
            "min_range_bars" => typed.min_range_bars = value as usize,
            "atr_len" => typed.atr_len = value as usize,
            "volume_k" => typed.volume_k = value,
            "climax_range_atr" => typed.climax_range_atr = value,
            "st_atr" => typed.st_atr = value,
            "breakout_volume" => typed.breakout_volume = value != 0.0,
            "lps_atr" => typed.lps_atr = value,
            "abandon_atr" => typed.abandon_atr = value,
            other => panic!("unknown wyckoff parameter {other}"),
        }
    }
    typed
}

/// What the output of a bar says about the state: phase, bias and the range in play.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WyckoffShown {
    phase: f64,
    bias: f64,
    source: f64,
    boundaries: Option<(u64, u64)>,
}

impl WyckoffShown {
    const NOTHING: WyckoffShown = WyckoffShown {
        phase: 0.0,
        bias: 0.0,
        source: 0.0,
        boundaries: None,
    };

    fn of(out: &kestrel_chartkit::indicator::IndicatorOutput) -> Self {
        WyckoffShown {
            phase: out.value,
            bias: out.extra["bias"],
            source: out.extra["range_source"],
            boundaries: out
                .extra
                .get("range_high")
                .map(|high| (high.to_bits(), out.extra["range_low"].to_bits())),
        }
    }

    /// The bias as the transition table names it: `None` without a range.
    fn range_bias(&self) -> Option<kestrel_chartkit::indicator::wyckoff::WyckoffRangeBias> {
        use kestrel_chartkit::indicator::wyckoff::WyckoffRangeBias as Bias;
        self.boundaries.map(|_| {
            [Bias::Accumulation, Bias::Distribution, Bias::Open]
                .into_iter()
                .find(|bias| bias.code() == self.bias)
                .expect("a bias code")
        })
    }
}

/// Runs the typed state machine over `bars` and checks every bar against the transition table:
/// what the output shows (phase, bias, boundaries, events, alerts) is what the transition
/// reported by `last_transition` declares, and a bar without a transition shows no change. The
/// registry indicator runs alongside and must give the same output, so the bars checked here
/// are the ones the contract fixture fixes. Adds each transition taken, with the bias it was
/// taken from, to `taken`.
fn audit_wyckoff_transitions(
    name: &str,
    params: &HashMap<String, f64>,
    bars: &[Bar],
    taken: &mut std::collections::BTreeSet<(&'static str, &'static str)>,
) {
    use kestrel_chartkit::indicator::wyckoff::{
        WyckoffCondition, WyckoffPhase, WyckoffStateMachine,
    };

    let mut machine = WyckoffStateMachine::with_params(wyckoff_params(params));
    let mut registry = build_checked("wyckoff", params).unwrap();
    let mut before = WyckoffShown::NOTHING;
    for (i, bar) in bars.iter().enumerate() {
        let at = format!("{name} bar {i}");
        let out = machine.on_bar(bar);
        assert_eq!(out, registry.on_bar(bar), "{at}: typed and registry output");
        let transition = machine.last_transition();
        let Some(out) = out else {
            assert!(transition.is_none(), "{at}: a transition without output");
            continue;
        };
        let after = WyckoffShown::of(&out);
        let mut reported: Vec<&str> = out
            .extra
            .keys()
            .filter_map(|key| key.strip_prefix("event_"))
            .collect();
        reported.sort_unstable();
        let alerts: Vec<String> = machine.alerts().into_iter().map(|a| a.kind).collect();

        let Some(t) = transition else {
            assert_eq!(
                after, before,
                "{at}: a change without a declared transition"
            );
            assert!(reported.is_empty(), "{at}: events without a transition");
            assert!(alerts.is_empty(), "{at}: alerts without a transition");
            continue;
        };

        assert_eq!(t.from.code(), before.phase, "{at}: {} starts in", t.id);
        assert_eq!(t.to.code(), after.phase, "{at}: {} leads to", t.id);
        match before.range_bias() {
            Some(bias) => assert!(t.bias_from.contains(&bias), "{at}: {} from {bias:?}", t.id),
            None => assert!(t.bias_from.is_empty(), "{at}: {} without a range", t.id),
        }
        assert_eq!(t.bias_to, after.range_bias(), "{at}: {} bias after", t.id);

        // The events: all declared ones except the optional, nothing else, in declared order.
        let slug = |kind| format!("{kind:?}").to_lowercase();
        let declared: Vec<String> = t.events.iter().map(|&kind| slug(kind)).collect();
        for kind in &reported {
            assert!(
                declared.iter().any(|d| d == kind),
                "{at}: {} reports {kind}",
                t.id
            );
        }
        for &kind in t.events {
            assert!(
                t.optional_events.contains(&kind) || reported.contains(&slug(kind).as_str()),
                "{at}: {} lacks {kind:?}",
                t.id
            );
        }
        let in_order: Vec<String> = declared
            .iter()
            .filter(|d| reported.contains(&d.as_str()))
            .map(|d| format!("wyckoff_{d}"))
            .collect();
        assert_eq!(alerts, in_order, "{at}: {} alerts", t.id);

        // The range: set anew by the transitions into Phase A, gone after an abandonment,
        // untouched by every other one.
        match t.to {
            WyckoffPhase::A => {
                let source = match t.condition {
                    WyckoffCondition::CounterMoveConfirmed => 1.0,
                    WyckoffCondition::WidthFallback => 2.0,
                    other => panic!("{at}: {} sets a range by {other:?}", t.id),
                };
                assert_eq!(after.source, source, "{at}: {} range source", t.id);
                assert_eq!(out.extra["cause_score"], 0.0, "{at}: {} a new range", t.id);
            }
            WyckoffPhase::Undefined => {
                assert_eq!(
                    after,
                    WyckoffShown::NOTHING,
                    "{at}: {} leaves no range",
                    t.id
                )
            }
            _ => assert_eq!(
                (after.boundaries, after.source),
                (before.boundaries, before.source),
                "{at}: {} keeps the range",
                t.id
            ),
        }

        let bias_from = before.range_bias().map_or("-", |bias| bias.id());
        taken.insert((t.id, bias_from));
        before = after;
    }
}

/// A series reflected at 200: accumulation becomes distribution.
fn wyckoff_reflected(bars: &[Bar]) -> Vec<Bar> {
    bars.iter()
        .map(|bar| {
            Bar::new(
                bar.timestamp,
                200.0 - bar.open,
                200.0 - bar.low,
                200.0 - bar.high,
                200.0 - bar.close,
                bar.volume,
            )
        })
        .collect()
}

/// A re-accumulation, the one path no contract case and no random walk takes: a Selling Climax
/// and its Automatic Rally while Phase E of an accumulation runs. The textbook series (Phase E
/// from bar 57, range 94 to 100) continues with a markup of half a point per bar up to 114.9
/// (bar 83), three falling bars, and on bar 87 the widest bar on the highest volume, with the
/// lowest low of its window at 104.4. Bars 88 to 90 rally to 110.8, above the climax bar's high
/// of 109.8; bar 91 closes at 109.6, bar 92 at 108.4, the first close below the low of the
/// rally-high bar (108.8). No bar closes below 100, so Phase E lasts until bar 92 sets the new
/// range 104.4 to 110.8. Rows are (close, wick above the body, wick below it, volume); the open
/// is the previous close.
fn wyckoff_reaccumulation_bars() -> Vec<Bar> {
    let mut bars = wyckoff_case("wyckoff_textbook_accumulation").bars;
    let mut rows = Vec::new();
    let markup_volumes = [120.0, 100.0, 110.0, 95.0, 105.0, 115.0, 90.0, 100.0];
    for k in 0..22 {
        let close = 103.9 + 0.5 * (k + 1) as f64;
        rows.push((close, 0.2, 0.2, markup_volumes[k % markup_volumes.len()]));
    }
    rows.extend([
        (113.4, 0.2, 0.3, 150.0),
        (111.6, 0.2, 0.4, 190.0),
        (109.6, 0.2, 0.4, 230.0),
        (105.4, 0.2, 1.0, 520.0), // 87 Selling Climax
        (107.2, 0.3, 0.2, 280.0),
        (109.0, 0.3, 0.2, 220.0),
        (110.4, 0.4, 0.2, 180.0), // 90 Automatic Rally
        (109.6, 0.3, 0.3, 130.0),
        (108.4, 0.2, 0.4, 120.0), // 92 confirms it
        (107.5, 0.2, 0.3, 110.0),
        (106.4, 0.2, 0.3, 100.0),
        (105.2, 0.2, 0.6, 95.0),
    ]);
    let round = |x: f64| (x * 1e6).round() / 1e6;
    let last = bars.last().expect("the textbook series").clone();
    let (mut open, mut timestamp) = (last.close, last.timestamp);
    for (close, above, below, volume) in rows {
        let close = round(close);
        timestamp += 900;
        bars.push(Bar::new(
            timestamp,
            open,
            round(open.max(close) + above),
            round(open.min(close) - below),
            close,
            volume,
        ));
        open = close;
    }
    bars
}

/// The re-accumulation takes `e_a_selling_climax` on bar 92, and nothing between the Last Point
/// of Support and that bar. The Selling Climax has no Preliminary Support: the event is optional.
#[test]
fn test_scenario_wyckoff_reaccumulation_in_phase_e() {
    use kestrel_chartkit::indicator::wyckoff::{
        WyckoffBias, WyckoffEventKind as Kind, WyckoffPhase, WyckoffStateMachine,
    };

    let bars = wyckoff_reaccumulation_bars();
    assert_eq!(bars.len(), 96);
    let mut machine = WyckoffStateMachine::with_defaults();
    let mut taken = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        machine.on_bar(bar);
        if let Some(t) = machine.last_transition() {
            taken.push((i, t.id));
        }
        if i == 87 {
            assert_eq!(
                machine.phase(),
                WyckoffPhase::E,
                "the climax bar sets no range"
            );
        }
        if i == 92 {
            let range = machine.range().expect("the new range");
            common::assert_close(range.low, 104.4, 1e-9, "range low: the climax low");
            common::assert_close(range.high, 110.8, 1e-9, "range high: the rally high");
            assert_eq!(machine.bias(), Some(WyckoffBias::Accumulation));
            let events: Vec<_> = machine
                .events()
                .iter()
                .map(|e| (e.kind, bar_of(&bars, e.timestamp), e.price))
                .collect();
            assert_wyckoff_events(
                &events,
                &[
                    (Kind::SellingClimax, 87, 104.4),
                    (Kind::AutomaticRally, 90, 110.8),
                ],
            );
        }
    }
    assert_eq!(
        taken,
        [
            (26, "none_a_selling_climax"),
            (29, "a_b_secondary_test_accumulation"),
            (38, "b_b_secondary_test_accumulation"),
            (48, "b_c_spring"),
            (53, "c_d_sign_of_strength"),
            (57, "d_e_last_point_of_support"),
            (92, "e_a_selling_climax"),
            (95, "a_b_secondary_test_accumulation"),
        ]
    );
}

/// The transition table describes what the state machine does, and nothing else.
///
/// (a) On the textbook series, its mirror image, every contract case (each also reflected), the
/// re-accumulation of [`wyckoff_reaccumulation_bars`] and the random walks of the count test,
/// every bar is checked by [`audit_wyckoff_transitions`]:
/// each change of phase, bias or boundaries and each reported event belongs to one declared
/// transition whose start, end, bias and events are what the output shows, and a bar that takes
/// none shows no change. One transition per bar is also what keeps a bar that reports an event,
/// changes the phase or abandons a range from setting a range.
///
/// (b) Every declared transition is taken by at least one of these series, from every bias it
/// declares. A transition no series reaches would be a claim without evidence.
#[test]
fn test_scenario_wyckoff_transitions_describe_the_behaviour() {
    use kestrel_chartkit::indicator::wyckoff::transitions;

    let mut taken = std::collections::BTreeSet::new();
    let names = wyckoff_case_names();
    for required in [
        "wyckoff_textbook_accumulation",
        "wyckoff_textbook_distribution",
    ] {
        assert!(names.contains(&required), "{required} is a contract case");
    }
    for name in names {
        let case = wyckoff_case(name);
        audit_wyckoff_transitions(name, &case.params, &case.bars, &mut taken);
        audit_wyckoff_transitions(
            &format!("{name} reflected"),
            &case.params,
            &wyckoff_reflected(&case.bars),
            &mut taken,
        );
    }
    let reaccumulation = wyckoff_reaccumulation_bars();
    for (name, bars) in [
        ("re-accumulation", reaccumulation.clone()),
        (
            "re-accumulation reflected",
            wyckoff_reflected(&reaccumulation),
        ),
    ] {
        audit_wyckoff_transitions(name, &HashMap::new(), &bars, &mut taken);
    }
    for seed in 1..=20u64 {
        let bars = wyckoff_random_walk(seed);
        audit_wyckoff_transitions(&format!("walk {seed}"), &HashMap::new(), &bars, &mut taken);
    }

    let mut missing = Vec::new();
    for t in transitions() {
        let from: Vec<&str> = if t.bias_from.is_empty() {
            vec!["-"]
        } else {
            t.bias_from.iter().map(|bias| bias.id()).collect()
        };
        for bias in from {
            if !taken.contains(&(t.id, bias)) {
                missing.push(format!("{} from {bias}", t.id));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "transitions no series takes: {missing:#?}"
    );
    let declared: std::collections::BTreeSet<&str> = transitions().iter().map(|t| t.id).collect();
    for (id, _) in &taken {
        assert!(declared.contains(id), "{id} is not in the table");
    }
}

/// The schematic generator of the crate, read with the registry defaults.
#[test]
fn test_scenario_wyckoff_synthetic_accumulation() {
    use kestrel_chartkit::indicator::wyckoff::WyckoffBias;
    use kestrel_chartkit::synthetic::{wyckoff_schematic_bars, WyckoffGeneratorConfig};

    let mut ind = build_checked("wyckoff", &HashMap::new()).unwrap();
    let bars = wyckoff_schematic_bars(
        123,
        WyckoffBias::Accumulation,
        WyckoffGeneratorConfig::default(),
    );

    let mut final_phase = 0.0;
    let mut alerts = Vec::new();
    for qb in &bars {
        if let Some(out) = ind.on_bar(&qb.bar) {
            final_phase = out.value;
            for a in ind.alerts() {
                alerts.push(a.kind);
            }
        }
    }

    assert_eq!(
        final_phase, 5.0,
        "Wyckoff Accumulation sequence must reach Phase E (5.0)"
    );
    assert!(
        alerts.iter().any(|k| k == "wyckoff_lastpointofsupport"),
        "Must emit Last Point of Support alert"
    );
}

#[test]
fn test_scenario_wyckoff_synthetic_distribution() {
    use kestrel_chartkit::indicator::wyckoff::WyckoffBias;
    use kestrel_chartkit::synthetic::{wyckoff_schematic_bars, WyckoffGeneratorConfig};

    let mut ind = build_checked("wyckoff", &HashMap::new()).unwrap();
    let bars = wyckoff_schematic_bars(
        123,
        WyckoffBias::Distribution,
        WyckoffGeneratorConfig::default(),
    );

    let mut final_phase = 0.0;
    let mut alerts = Vec::new();
    for qb in &bars {
        if let Some(out) = ind.on_bar(&qb.bar) {
            final_phase = out.value;
            for a in ind.alerts() {
                alerts.push(a.kind);
            }
        }
    }

    assert_eq!(
        final_phase, 5.0,
        "Wyckoff Distribution sequence must reach Phase E (5.0)"
    );
    assert!(
        alerts.iter().any(|k| k == "wyckoff_lastpointofsupply"),
        "Must emit Last Point of Supply alert"
    );
}

// ============================================================================
// 11. ZigZag (Alternating Swing Legs)
// ============================================================================
#[test]
fn test_scenario_zigzag() {
    let mut ind = build_checked(
        "zigzag",
        &HashMap::from([
            ("depth".to_string(), 2.0),
            ("deviation_pct".to_string(), 5.0),
        ]),
    )
    .unwrap();

    let prices = [
        100.0, 110.0, 120.0, 115.0, 110.0, 105.0, 95.0, 90.0, 95.0, 100.0, 110.0, 120.0, 130.0,
        125.0, 120.0,
    ];

    let mut last_out = None;
    for (i, &p) in prices.iter().enumerate() {
        let b = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&b) {
            last_out = Some(out);
        }
    }

    let out = last_out.expect("ZigZag should yield output");
    assert_eq!(
        out.extra["direction"], 1.0,
        "Final ZigZag direction must be upward (1.0) after rally to 130.0"
    );
    common::assert_close(
        out.extra["last_pivot_price"],
        130.5,
        1e-9,
        "ZigZag Last Pivot Price",
    );
}

// ============================================================================
// 12. ZigZag Advanced (Dual Levels / State Tracking)
// ============================================================================
#[test]
fn test_scenario_zigzag_advanced() {
    let mut ind = build_checked(
        "zigzag_advanced",
        &HashMap::from([
            ("depth".to_string(), 2.0),
            ("backstep".to_string(), 1.0),
            ("deviation_pct".to_string(), 2.0),
            ("atr_len".to_string(), 5.0),
        ]),
    )
    .unwrap();

    let prices = [
        100.0, 105.0, 115.0, 110.0, 105.0, 98.0, 92.0, 90.0, 95.0, 102.0, 110.0, 118.0, 125.0,
        120.0, 115.0,
    ];

    // Traced bar-by-bar (depth=2 => mid_idx=2 of a 5-bar window, backstep=1, deviation_pct=2.0
    // => 2% threshold); the same values come out of the reference implementation in
    // `reference/kestrel_reference/structure.py`:
    //   - idx4 (mid=bar2, H=115.5=price+0.5): first-ever pivot high, seeds a running (unconfirmed)
    //     high node at 115.5. Stays unchanged through idx5..idx8 (no further pivot beats it).
    //   - idx9 (mid=bar7, L=89.5=price-0.5): pivot low with |89.5-115.5|/115.5=22.5% >> 2%
    //     threshold => confirms the 115.5 high node and starts a new running low leg at 89.5.
    //     Stays unchanged through idx10..idx13.
    //   - idx14 (mid=bar12, H=125.5=price+0.5, the LAST bar): pivot high with
    //     |125.5-89.5|/89.5=40.2% >> threshold, and backstep_ok holds (mid_bar_index=12 >=
    //     last_confirmed(7)+backstep(1)=8) => confirms the 89.5 low node and starts a new running
    //     high leg at 125.5. Final output: value=125.5 (not yet confirmed itself), state=
    //     "running", plus a `bull_swing_confirmed` alert for the just-confirmed swing low (the 115.5 high
    //     confirmed at idx9 raised `bear_swing_confirmed`).
    let mut last_out = None;
    let mut confirm_alerts = Vec::new();
    for (i, &p) in prices.iter().enumerate() {
        let b = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&b) {
            last_out = Some(out);
        }
        for a in ind.alerts() {
            // A confirmed swing low is a turn up, a swing high a turn down.
            if a.kind.ends_with("_swing_confirmed") {
                assert_eq!(
                    a.kind == "bull_swing_confirmed",
                    a.note.contains("swing low"),
                    "{} vs {}",
                    a.kind,
                    a.note
                );
                confirm_alerts.push(a.note);
            }
        }
    }

    let out = last_out.expect("Advanced ZigZag should produce output");
    common::assert_close(
        out.value,
        expected("zigzag_advanced_value_last"),
        expected("scenario_structure_tolerance"),
        "Final running leg price",
    );
    let running = expected("zigzag_advanced_running_last") == 1.0;
    assert_eq!(
        out.state.as_deref(),
        Some(if running { "running" } else { "confirmed" }),
        "Final bar starts a new unconfirmed leg, so state must be 'running'"
    );
    assert_eq!(
        confirm_alerts.len(),
        expected("zigzag_advanced_confirmations") as usize,
        "{confirm_alerts:?}"
    );
    let low = expected("zigzag_advanced_last_confirms_low") == 1.0;
    assert_eq!(
        confirm_alerts.last().map(String::as_str),
        Some(if low {
            "ZigZag confirmed a swing low"
        } else {
            "ZigZag confirmed a swing high"
        }),
        "Last bar must confirm the prior running swing low: {confirm_alerts:?}"
    );
}

/// Im ATR-Modus zählt der Preisabstand gegen ein Vielfaches der ATR, nicht die relative Änderung:
/// Beim Dreifachen bestätigen beide Ausschläge (26 und 36 Punkte), beim Sechsfachen keiner.
#[test]
fn test_scenario_zigzag_advanced_atr_mode() {
    use kestrel_chartkit::indicator::zigzag_advanced::{AdvancedZigZagEngine, ZigZagDeviationMode};

    let prices = [
        100.0, 105.0, 115.0, 110.0, 105.0, 98.0, 92.0, 90.0, 95.0, 102.0, 110.0, 118.0, 125.0,
        120.0, 115.0,
    ];
    let tol = expected("scenario_structure_tolerance");
    for mult in [3, 6] {
        let mut engine =
            AdvancedZigZagEngine::new(2, 1, ZigZagDeviationMode::AtrMultiple(mult as f64), 3);
        for (i, &p) in prices.iter().enumerate() {
            engine.on_bar(&Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0));
        }
        let confirmed: Vec<f64> = engine
            .nodes()
            .iter()
            .filter(|n| n.confirmed)
            .map(|n| n.price)
            .collect();
        assert_eq!(
            confirmed.len(),
            expected(&format!("zigzag_atr{mult}_confirmed_count")) as usize,
            "ATR x{mult}: {confirmed:?}"
        );
        for (i, &price) in confirmed.iter().enumerate() {
            common::assert_close(
                price,
                expected(&format!("zigzag_atr{mult}_confirmed_{}_price", i + 1)),
                tol,
                "bestätigter Knoten",
            );
        }
        common::assert_close(
            engine.current_leg().expect("laufender Schenkel").price,
            expected(&format!("zigzag_atr{mult}_running_price")),
            tol,
            "laufender Schenkel",
        );
    }
}

/// A structure alert names its direction and kind in the prefix and suffix, matching the note.
fn assert_structure_kind(kind: &str, note: &str) {
    let bull = note.starts_with("Bullish");
    assert_eq!(kind.starts_with("bull_"), bull, "{kind} vs {note}");
    assert_eq!(
        kind.ends_with("_choch"),
        note.contains("CHoCH"),
        "{kind} vs {note}"
    );
}
