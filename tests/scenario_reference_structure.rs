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
                if a.kind == "structure_break" {
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
                if a.kind == "structure_break" {
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
                if a.kind == "structure_break" {
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
// 2. Candle Story (Pinbars, Kangaroo Tails, Engulfing Patterns)
// ============================================================================
#[test]
fn test_scenario_candle_story() {
    // `min_range_atr` off: this fixture is three bars long, so the ATR is still warming up and a
    // size floor would suppress everything. The point here is the classification, not the filter.
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_reports_every_match() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_needs_the_trend_to_name_a_hammer() {
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
        let mut ind = build_checked("candle_story", &params).unwrap();
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
fn test_scenario_candle_story_advance_block() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_abandoned_baby() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_breakaway() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_kicking() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 141.0, 100.0, 101.0, 1000.0)); // bearish marubozu
    let out = ind
        .on_bar(&Bar::new(60, 200.0, 240.0, 199.0, 239.0, 1000.0)) // bullish marubozu, gapped up
        .expect("output");

    assert_eq!(out.extra["kicking"], 1.0);
}

/// Counterattack lines: opposite colours, matching closes.
#[test]
fn test_scenario_candle_story_counterattack_lines() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.5, 99.5, 110.0, 1000.0)); // bullish, close 110
    let out = ind
        .on_bar(&Bar::new(60, 130.0, 130.5, 109.5, 110.0, 1000.0)) // bearish, same close
        .expect("output");

    assert_eq!(out.extra["counterattack_lines"], 1.0);
}

/// Separating lines: opposite colours, matching opens.
#[test]
fn test_scenario_candle_story_separating_lines() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 100.5, 79.5, 80.0, 1000.0)); // bearish, open 100
    let out = ind
        .on_bar(&Bar::new(60, 100.0, 130.5, 99.5, 130.0, 1000.0)) // bullish, same open
        .expect("output");

    assert_eq!(out.extra["separating_lines"], 1.0);
}

/// Matching low: two bearish candles closing at (almost) the same price.
#[test]
fn test_scenario_candle_story_matching_low() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.5, 100.0, 1000.0)); // bearish, close 100
    let out = ind
        .on_bar(&Bar::new(60, 160.0, 160.5, 99.5, 100.0, 1000.0)) // bearish, same close
        .expect("output");

    assert_eq!(out.extra["matching_low"], 1.0);
}

/// Homing pigeon: a harami where both candles are bearish.
#[test]
fn test_scenario_candle_story_homing_pigeon() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 141.0, 99.0, 100.0, 1000.0)); // bearish, body [100, 140]
    let out = ind
        .on_bar(&Bar::new(60, 130.0, 131.0, 120.0, 125.0, 1000.0)) // bearish, inside the body
        .expect("output");

    assert_eq!(out.extra["homing_pigeon"], 1.0);
}

/// Tri-star: three dojis, the middle one isolated by gaps on both sides.
#[test]
fn test_scenario_candle_story_tri_star() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 101.0, 99.0, 100.1, 1000.0)); // doji
    ind.on_bar(&Bar::new(60, 150.0, 150.55, 149.65, 150.03, 1000.0)); // doji, gapped above
    let out = ind
        .on_bar(&Bar::new(120, 110.0, 110.5, 109.6, 110.05, 1000.0)) // doji, gapped below the middle
        .expect("output");

    assert_eq!(out.extra["tri_star"], 1.0);
}

/// Tasuki gap: two candles in trend direction with a gap, then a counter candle staying inside it.
#[test]
fn test_scenario_candle_story_tasuki_gap() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_upside_gap_two_crows() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 110.5, 99.5, 110.0, 1000.0)); // bullish, close 110
    ind.on_bar(&Bar::new(60, 140.0, 140.6, 129.9, 130.0, 1000.0)); // bearish, gapped up
    let out = ind
        .on_bar(&Bar::new(120, 145.0, 145.5, 114.5, 115.0, 1000.0)) // bearish, encloses, closes above 110
        .expect("output");

    assert_eq!(out.extra["upside_gap_two_crows"], 1.0);
}

/// Three stars in the south: three bearish candles, shrinking ranges, rising lows.
#[test]
fn test_scenario_candle_story_three_stars_in_the_south() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 140.0, 140.5, 99.5, 100.0, 1000.0)); // range 41, low 99.5
    ind.on_bar(&Bar::new(60, 125.0, 125.3, 109.8, 110.0, 1000.0)); // range 15.5, low 109.8
    let out = ind
        .on_bar(&Bar::new(120, 118.0, 118.2, 114.9, 115.0, 1000.0)) // range 3.3, low 114.9
        .expect("output");

    assert_eq!(out.extra["three_stars_in_the_south"], 1.0);
}

/// Deliberation: two large bullish candles, then a small third opening at the second's close.
#[test]
fn test_scenario_candle_story_deliberation() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

    ind.on_bar(&Bar::new(0, 100.0, 120.5, 99.5, 120.0, 1000.0));
    ind.on_bar(&Bar::new(60, 120.0, 140.5, 119.5, 140.0, 1000.0));
    let out = ind
        .on_bar(&Bar::new(120, 140.0, 141.0, 139.7, 140.2, 1000.0)) // small, opens at 140
        .expect("output");

    assert_eq!(out.extra["deliberation"], 1.0);
}

/// Stick sandwich: bearish, bullish, bearish — the outer candles closing at the same price.
#[test]
fn test_scenario_candle_story_stick_sandwich() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_unique_three_river_bottom() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_three_line_strike() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_concealing_baby_swallow() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_rising_three_methods_and_mat_hold() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_falling_three_methods() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
fn test_scenario_candle_story_ladder_bottom() {
    let params: HashMap<String, f64> = [("min_range_atr".to_string(), 0.0)].into();
    let mut ind = build_checked("candle_story", &params).unwrap();

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
            if a.kind == "liquidity_pool_stop_hunt" {
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
    for b in &bars {
        if let Some(out) = ind.on_bar(b) {
            last_sweep = out.extra["sweep"];
        }
    }

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
#[test]
fn test_scenario_wyckoff() {
    let mut ind = build_checked(
        "wyckoff",
        &HashMap::from([
            ("range_lookback".to_string(), 8.0),
            ("range_atr_max".to_string(), 5.0),
            ("min_range_bars".to_string(), 3.0),
        ]),
    )
    .unwrap();

    // 1. Warmup with oscillating range bars around 100.0
    for i in 0..20 {
        let offset = ((i % 4) as f64 - 1.5) * 2.0 * 0.3;
        let price = 100.0 + offset;
        let b = Bar::new(i as i64 * 60, price, price + 0.6, price - 0.6, price, 100.0);
        ind.on_bar(&b);
    }

    // 2. Feed the appropriate 3-bar resolution sequence (Accumulation: Spring -> SOS -> LPS, or Distribution: UTAD -> SOW -> LPSY)
    // Both drive the state machine through Phase C -> Phase D -> Phase E.
    let bars_seq = [
        Bar::new(2000, 101.5, 104.0, 101.0, 101.2, 100.0), // Phase C event
        Bar::new(2060, 101.0, 101.5, 94.0, 94.5, 100.0),   // Phase D event
        Bar::new(2120, 94.5, 97.0, 94.0, 95.0, 100.0),     // Phase E event
    ];

    let mut final_phase = 0.0;
    let mut alerts = Vec::new();
    for b in &bars_seq {
        if let Some(out) = ind.on_bar(b) {
            final_phase = out.value;
            for a in ind.alerts() {
                alerts.push(a.kind);
            }
        }
    }

    assert_eq!(
        final_phase, 5.0,
        "Wyckoff Phase must transition to Phase E (code 5.0) after complete structural sequence"
    );
    assert!(
        alerts
            .iter()
            .any(|k| k == "wyckoff_lastpointofsupply" || k == "wyckoff_lastpointofsupport"),
        "Must emit Wyckoff Phase E confirmation alert (LPS / LPSY)"
    );
}

#[test]
fn test_scenario_wyckoff_synthetic_accumulation() {
    use kestrel_chartkit::indicator::wyckoff::WyckoffBias;
    use kestrel_chartkit::synthetic::{wyckoff_schematic_bars, WyckoffGeneratorConfig};

    let mut ind = build_checked(
        "wyckoff",
        &HashMap::from([
            ("range_lookback".to_string(), 20.0),
            ("range_atr_max".to_string(), 5.0),
            ("min_range_bars".to_string(), 3.0),
        ]),
    )
    .unwrap();

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

    let mut ind = build_checked(
        "wyckoff",
        &HashMap::from([
            ("range_lookback".to_string(), 20.0),
            ("range_atr_max".to_string(), 5.0),
            ("min_range_bars".to_string(), 3.0),
        ]),
    )
    .unwrap();

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
    //     "running", plus a `zigzag_pivot_confirmed` alert for the just-confirmed swing low.
    let mut last_out = None;
    let mut confirm_alerts = Vec::new();
    for (i, &p) in prices.iter().enumerate() {
        let b = Bar::new(i as i64 * 60, p, p + 0.5, p - 0.5, p, 1000.0);
        if let Some(out) = ind.on_bar(&b) {
            last_out = Some(out);
        }
        for a in ind.alerts() {
            if a.kind == "zigzag_pivot_confirmed" {
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
