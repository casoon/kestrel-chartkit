mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

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

// ============================================================================
// 2. Candle Story (Pinbars, Kangaroo Tails, Engulfing Patterns)
// ============================================================================
#[test]
fn test_scenario_candle_story() {
    let mut ind = build_checked("candle_story", &HashMap::new()).unwrap();

    // Bar 0: neutral bar
    let b0 = Bar::new(0, 100.0, 101.0, 99.0, 100.0, 1000.0);
    ind.on_bar(&b0);

    // Bar 1: Bullish Kangaroo Tail (Pinbar Reversal):
    // Range = 100.5 - 80.0 = 20.5
    // Lower Wick = min(100.0, 100.2) - 80.0 = 20.0 (20.0 / 20.5 = 97.5% >= 55%)
    // Close Pos = (100.2 - 80.0) / 20.5 = 98.5% >= 65%
    let pinbar = Bar::new(60, 100.0, 100.5, 80.0, 100.2, 2000.0);
    let pinbar_out = ind.on_bar(&pinbar).expect("Pinbar should yield output");

    assert_eq!(
        pinbar_out.extra["pattern_type"], 1.0,
        "Pattern type code must be 1.0 (Bullish Kangaroo Tail)"
    );
    let alerts = ind.alerts();
    assert!(
        alerts
            .iter()
            .any(|a| a.kind == "bullish_kangaroo_tail" && a.strength >= 0.85),
        "Must emit high-confidence bullish_kangaroo_tail alert"
    );

    // Bar 2: Bearish Engulfing Bar (body > prev_body, close < open, closes below prev open, body/range = 10.5/15 = 70% < 82%)
    let engulfing = Bar::new(120, 100.5, 103.0, 88.0, 90.0, 3000.0);
    let eng_out = ind
        .on_bar(&engulfing)
        .expect("Engulfing should yield output");
    assert_eq!(
        eng_out.extra["pattern_type"], 4.0,
        "Pattern type code must be 4.0 (Bearish Engulfing)"
    );
    assert!(
        ind.alerts().iter().any(|a| a.kind == "bearish_engulfing"),
        "Must emit bearish_engulfing alert"
    );
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

    // Traced bar-by-bar against `LiquidityPoolEngine` (pivot_len=2 => 5-bar pivot window,
    // tolerance_pct=0.5% => 0.005 relative merge tolerance), independently confirmed via a
    // from-scratch Python re-implementation of the exact algorithm:
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
        2,
        "Must record exactly two stop hunts (BSL@110.0 then BSL@110.2): {stop_hunts:?}"
    );
    assert!(
        stop_hunts[0].contains("110.0000"),
        "First stop hunt must be the original BSL pool at 110.0000: {}",
        stop_hunts[0]
    );
    assert!(
        stop_hunts[1].contains("110.2000"),
        "Second stop hunt must be the later, independently-formed BSL pool at 110.2000: {}",
        stop_hunts[1]
    );
    common::assert_close(
        last_active_count.expect("liquidity_pools should have produced output"),
        1.0,
        1e-9,
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

    // Traced bar-by-bar against `PivotStructureEngine` (left_bars=2, right_bars=2 =>
    // candidate_idx = bars.len()-1-right_bars over the ever-growing bar history, not a fixed
    // sliding window), independently confirmed via a from-scratch Python re-implementation:
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
        6,
        "Expected one output per bar from bars.len()>=5 onward"
    );
    for &s in &scores[..scores.len() - 1] {
        common::assert_close(
            s,
            0.0,
            1e-9,
            "Score must stay exactly 0.0 before the second pivot",
        );
    }
    common::assert_close(
        *scores.last().unwrap(),
        2.0 / 6.0 * 100.0,
        1e-9,
        "Final score must be exactly 2.0/(score_window*2.0)*100 after the Higher-High pivot",
    );
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

    // Traced bar-by-bar against `AdvancedZigZagEngine` (depth=2 => mid_idx=2 of a 5-bar window,
    // backstep=1, deviation_pct=2.0 => 2% threshold), independently confirmed via a from-scratch
    // Python re-implementation:
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
    common::assert_close(out.value, 125.5, 1e-9, "Final running leg price");
    assert_eq!(
        out.state.as_deref(),
        Some("running"),
        "Final bar starts a new unconfirmed leg, so state must be 'running'"
    );
    assert_eq!(
        confirm_alerts.last().map(String::as_str),
        Some("ZigZag confirmed a swing low"),
        "Last bar must confirm the prior running swing low: {confirm_alerts:?}"
    );
}
