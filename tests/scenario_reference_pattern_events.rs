//! Scenario fixtures of the catalog entries `chart_patterns` and `harmonic_patterns`.
//!
//! Every series is built from waypoints: flat bars (open = high = low = close) on a straight line
//! between swing prices ten bars apart, so each waypoint is a ZigZag node at exactly that price.
//! With the entries' defaults (`depth` 3, `backstep` 2, 1 % deviation) a node is confirmed three
//! bars after the next waypoint, which makes the expected bar of every event a hand count: the
//! pattern's last anchor is confirmed on bar `next waypoint + 3`, and the event is dated to that
//! bar or to the later bar on which the detector's rule is first met.
//!
//! Harmonic zone prices are the documented projections evaluated outside the crate from the ratio
//! table: the XA retracement of the row's `d` centre, the BC extension of the row's `cd` centre
//! and the AB=CD projection (Gartley: 120 - 0.785 * 20 = 104.3, 113.92 - 1.374 * 6.28 =
//! 105.29128, 113.92 + (107.64 - 120) = 101.56). Detector wiring, not the detector rules, is what
//! these scenarios fix: the rules themselves are the `ChartPatternDetector` and `HarmonicDetector`
//! module tests.

use std::collections::HashMap;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::{Indicator, IndicatorOutput};
use kestrel_chartkit::model::Bar;

type Waypoints = &'static [(usize, f64)];

fn bars(waypoints: Waypoints) -> Vec<Bar> {
    let mut out = Vec::new();
    for pair in waypoints.windows(2) {
        let ((i0, p0), (i1, p1)) = (pair[0], pair[1]);
        for i in i0..i1 {
            let p = p0 + (p1 - p0) * (i - i0) as f64 / (i1 - i0) as f64;
            out.push(Bar::new(i as i64 * 60, p, p, p, p, 100.0));
        }
    }
    let (i, p) = *waypoints.last().unwrap();
    out.push(Bar::new(i as i64 * 60, p, p, p, p, 100.0));
    out
}

/// The same market with every price negated: highs become lows.
fn mirrored(series: &[Bar]) -> Vec<Bar> {
    series
        .iter()
        .map(|b| Bar::new(b.timestamp, -b.open, -b.low, -b.high, -b.close, b.volume))
        .collect()
}

struct Step {
    bar: usize,
    output: IndicatorOutput,
    kinds: Vec<String>,
}

fn run(entry: &str, params: &[(&str, f64)], series: &[Bar]) -> Vec<Step> {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut indicator = build_checked(entry, &params).unwrap();
    let mut out = Vec::new();
    for (bar, b) in series.iter().enumerate() {
        if let Some(output) = indicator.on_bar(b) {
            let kinds = indicator.alerts().into_iter().map(|a| a.kind).collect();
            out.push(Step { bar, output, kinds });
        }
    }
    out
}

/// The bars that raised alerts, with the alert kinds in the order raised.
fn events(steps: &[Step]) -> Vec<(usize, Vec<String>)> {
    steps
        .iter()
        .filter(|s| !s.kinds.is_empty())
        .map(|s| (s.bar, s.kinds.clone()))
        .collect()
}

fn expect(steps: &[Step], expected: &[(usize, &[&str])]) {
    let want: Vec<(usize, Vec<String>)> = expected
        .iter()
        .map(|(bar, kinds)| (*bar, kinds.iter().map(|k| k.to_string()).collect()))
        .collect();
    assert_eq!(events(steps), want);
}

fn extra(steps: &[Step], bar: usize, key: &str) -> f64 {
    let step = steps.iter().find(|s| s.bar == bar).unwrap();
    *step
        .output
        .extra
        .get(key)
        .unwrap_or_else(|| panic!("no extra {key} on bar {bar}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// --- chart patterns: one scenario per geometry -----------------------------------------------

const CHART: &str = "chart_patterns";

/// Highs 120 / 120.5 within 2 %, the trough 105 between them is the neckline. The second high is
/// confirmed with the low at 40 (bar 43); the close through 105 came on bar 38, so the event is
/// dated to bar 43.
const DOUBLE_TOP: Waypoints = &[
    (0, 100.0),
    (10, 120.0),
    (20, 105.0),
    (30, 120.5),
    (40, 100.0),
    (50, 110.0),
];
const DOUBLE_BOTTOM: Waypoints = &[
    (0, 120.0),
    (10, 100.0),
    (20, 115.0),
    (30, 99.5),
    (40, 120.0),
    (50, 110.0),
];
/// Three highs on one level; the neckline is the lower trough, 105. Besides the triple top the
/// other instances in these nodes confirm too: the double top over the first two highs (first
/// close below 105 on bar 58, found long before), the double top over the last two (neckline 108,
/// found with the third high on bar 63), a rising wedge and a triangle over four nodes.
const TRIPLE_TOP: Waypoints = &[
    (0, 100.0),
    (10, 120.0),
    (20, 105.0),
    (30, 120.5),
    (40, 108.0),
    (50, 119.8),
    (60, 100.0),
    (70, 110.0),
];
const TRIPLE_BOTTOM: Waypoints = &[
    (0, 120.0),
    (10, 100.0),
    (20, 115.0),
    (30, 99.5),
    (40, 112.0),
    (50, 100.2),
    (60, 120.0),
    (70, 110.0),
];
/// Head 128 over shoulders 118 and 117.5, troughs 108 and 109: the sloping neckline is crossed at
/// bar 54, the right shoulder is confirmed on bar 63.
const HEAD_AND_SHOULDERS: Waypoints = &[
    (0, 100.0),
    (10, 118.0),
    (20, 108.0),
    (30, 128.0),
    (40, 109.0),
    (50, 117.5),
    (60, 100.0),
    (70, 110.0),
];
const INVERSE_HEAD_AND_SHOULDERS: Waypoints = &[
    (0, 130.0),
    (10, 112.0),
    (20, 122.0),
    (30, 102.0),
    (40, 121.0),
    (50, 112.5),
    (60, 130.0),
    (70, 120.0),
];
/// Low 100, failed high 112, new low 95 below the first: the 1-2-3 is found on bar 33+10 = 43 and
/// confirmed by the first close under 95 on bar 47 (105 - 1.5 * 7 = 94.5).
const ONE_TWO_THREE: Waypoints = &[
    (0, 110.0),
    (10, 100.0),
    (20, 112.0),
    (30, 95.0),
    (40, 105.0),
    (50, 90.0),
    (60, 100.0),
];
/// Lows 100 / 96 / 88 (bars 10, 30, 50) and highs 112 / 106 (bars 20, 40). The Wolfe rules: 96 is
/// below 100; 106 is inside the 1-2 range 100..112; line 1-3 falls 0.2 per bar and line 2-4 falls
/// 0.3, so they converge; line 1-3 is at 92 on bar 50 and point 5 at 88 undercuts it. Point 5 is
/// confirmed with the next swing, three bars after the waypoint 60, on bar 63; the first close
/// after it (bar 51, 89.2) is above point 5 and below the 1-4 line there (100 + 0.2 * 41 =
/// 108.2), so the rule confirms at once and the event, raised on bar 63, carries the target of
/// bar 51. The first three nodes (100, 112, 96) are a bearish 1-2-3 as well: node 3 is confirmed
/// on bar 43, and the first close under 96 is bar 46 (106 - 1.8 * 6 = 95.2).
const WOLFE_WAVE: Waypoints = &[
    (0, 110.0),
    (10, 100.0),
    (20, 112.0),
    (30, 96.0),
    (40, 106.0),
    (50, 88.0),
    (60, 100.0),
    (70, 98.0),
];
/// Falling highs 120 / 115 over rising lows 100 / 104: the upper boundary (111.25 on bar 45) is
/// crossed on bar 45, the last low is confirmed on bar 53.
const TRIANGLE: Waypoints = &[
    (0, 100.0),
    (10, 120.0),
    (20, 100.0),
    (30, 115.0),
    (40, 104.0),
    (50, 122.0),
    (60, 110.0),
];
const TRIANGLE_DOWN: Waypoints = &[
    (0, 110.0),
    (10, 100.0),
    (20, 120.0),
    (30, 105.0),
    (40, 114.0),
    (50, 95.0),
    (60, 105.0),
];
/// Equal highs at 120 over rising lows: the flat top is broken upward.
const ASCENDING_TRIANGLE: Waypoints = &[
    (0, 100.0),
    (10, 120.0),
    (20, 100.0),
    (30, 120.0),
    (40, 108.0),
    (50, 130.0),
    (60, 115.0),
];
const DESCENDING_TRIANGLE: Waypoints = &[
    (0, 120.0),
    (10, 100.0),
    (20, 120.0),
    (30, 100.0),
    (40, 112.0),
    (50, 90.0),
    (60, 105.0),
];
/// Rising highs and falling lows: a widening range, left upward.
const BROADENING: Waypoints = &[
    (0, 105.0),
    (10, 110.0),
    (20, 100.0),
    (30, 115.0),
    (40, 95.0),
    (50, 125.0),
    (60, 105.0),
];
const BROADENING_DOWN: Waypoints = &[
    (0, 105.0),
    (10, 100.0),
    (20, 110.0),
    (30, 95.0),
    (40, 118.0),
    (50, 85.0),
    (60, 100.0),
];
/// Both boundaries rise, the lower faster (0.3 against 0.15 a bar): a rising wedge, left downward.
const RISING_WEDGE: Waypoints = &[
    (0, 105.0),
    (10, 100.0),
    (20, 110.0),
    (30, 106.0),
    (40, 113.0),
    (50, 98.0),
    (60, 105.0),
];
const FALLING_WEDGE: Waypoints = &[
    (0, 115.0),
    (10, 120.0),
    (20, 110.0),
    (30, 114.0),
    (40, 107.0),
    (50, 120.0),
    (60, 112.0),
];
/// Pole 100 -> 124 (bars 10 to 20), then highs 124 / 121 and lows 116 / 113: both channel lines
/// fall 0.15 a bar, so they are parallel, and the deepest low gives back 11 of the pole's 24 (12
/// allowed). The last low is confirmed on bar 63; the first close above the upper line is bar 54
/// (113 + 1.7 * 4 = 119.8 against 124 - 0.15 * 34 = 118.9), where the lower line is at 112.4.
const FLAG: Waypoints = &[
    (0, 110.0),
    (10, 100.0),
    (20, 124.0),
    (30, 116.0),
    (40, 121.0),
    (50, 113.0),
    (60, 130.0),
    (70, 120.0),
];
/// Rims 120 (bar 10) and 119 (bar 50) over the bottom 100 (bar 30), handle 112 on bar 58: above
/// the cup's midpoint 109.5, and 8 bars after the right rim against a cup of 40. The handle is
/// confirmed on bar 73; the first close over the rim is bar 65 (112 + 13 / 12 * 7 = 119.58). The
/// rims, bottom and handle are also a converging window (highs 120 -> 119, lows 100 -> 112), so
/// the same close breaks a triangle out upward.
const CUP_AND_HANDLE: Waypoints = &[
    (0, 100.0),
    (10, 120.0),
    (30, 100.0),
    (50, 119.0),
    (58, 112.0),
    (70, 125.0),
    (80, 118.0),
];

#[test]
fn double_top_confirms_on_the_bar_its_second_high_is_fixed() {
    let steps = run(CHART, &[], &bars(DOUBLE_TOP));
    expect(&steps, &[(43, &["bear_double_top"])]);
    assert_eq!(
        steps.iter().find(|s| s.bar == 42).unwrap().output.value,
        0.0
    );
    assert_eq!(
        steps.iter().find(|s| s.bar == 43).unwrap().output.value,
        -1.0
    );
    assert_eq!(extra(&steps, 43, "pattern"), 9.0);
    assert_eq!(extra(&steps, 43, "level"), 105.0);
    assert_eq!(extra(&steps, 43, "invalidation"), 120.5);
    assert_eq!(extra(&steps, 43, "confirmed"), 1.0);
    // The direction stays until another pattern confirms.
    assert_eq!(steps.last().unwrap().output.value, -1.0);
}

#[test]
fn double_bottom_confirms_on_the_bar_its_second_low_is_fixed() {
    let steps = run(CHART, &[], &bars(DOUBLE_BOTTOM));
    expect(&steps, &[(43, &["bull_double_bottom"])]);
    assert_eq!(extra(&steps, 43, "pattern"), 10.0);
    assert_eq!(extra(&steps, 43, "level"), 115.0);
    assert_eq!(extra(&steps, 43, "invalidation"), 99.5);
}

#[test]
fn triple_top_and_the_instances_inside_it_each_confirm_once() {
    let steps = run(CHART, &[], &bars(TRIPLE_TOP));
    expect(
        &steps,
        &[
            (55, &["bear_rising_wedge"]),
            (58, &["bear_double_top"]),
            (63, &["bear_triangle", "bear_double_top", "bear_triple_top"]),
        ],
    );
}

#[test]
fn triple_bottom_and_the_instances_inside_it_each_confirm_once() {
    let steps = run(CHART, &[], &bars(TRIPLE_BOTTOM));
    expect(
        &steps,
        &[
            (55, &["bull_falling_wedge"]),
            (58, &["bull_double_bottom"]),
            (
                63,
                &["bull_triangle", "bull_double_bottom", "bull_triple_bottom"],
            ),
        ],
    );
}

#[test]
fn head_and_shoulders_confirms_with_the_right_shoulder() {
    let steps = run(CHART, &[], &bars(HEAD_AND_SHOULDERS));
    // Trough 108, head 128, trough 109 and right shoulder 117.5 are also an inverse cup and
    // handle: the shoulder stays below the midpoint 118.5 and took 10 bars against 20.
    expect(
        &steps,
        &[(
            63,
            &[
                "bear_triangle",
                "bear_head_and_shoulders",
                "bear_inverse_cup_and_handle",
            ],
        )],
    );
    assert!(close(extra(&steps, 63, "confirmed"), 3.0));
}

#[test]
fn inverse_head_and_shoulders_confirms_with_the_right_shoulder() {
    let steps = run(CHART, &[], &bars(INVERSE_HEAD_AND_SHOULDERS));
    // Shoulder 122, head 102, shoulder 121 and trough 112.5: a cup and handle as well.
    expect(
        &steps,
        &[(
            63,
            &[
                "bull_triangle",
                "bull_inverse_head_and_shoulders",
                "bull_cup_and_handle",
            ],
        )],
    );
}

#[test]
fn one_two_three_confirms_on_the_first_close_beyond_the_third_node() {
    let steps = run(CHART, &[], &bars(ONE_TWO_THREE));
    expect(
        &steps,
        &[(47, &["bear_one_two_three"]), (59, &["bull_falling_wedge"])],
    );
    assert_eq!(extra(&steps, 47, "pattern"), 7.0);
    assert_eq!(extra(&steps, 47, "level"), 95.0);
    assert_eq!(extra(&steps, 47, "invalidation"), 112.0);
}

#[test]
fn wolfe_wave_confirms_with_point_five_and_projects_the_one_four_line() {
    let steps = run(CHART, &[], &bars(WOLFE_WAVE));
    expect(
        &steps,
        &[
            (46, &["bear_one_two_three"]),
            (53, &["bear_falling_wedge"]),
            (63, &["bull_wolfe_wave"]),
        ],
    );
    assert_eq!(extra(&steps, 63, "pattern"), 8.0);
    assert_eq!(extra(&steps, 63, "invalidation"), 88.0);
    assert!(close(extra(&steps, 63, "target"), 108.2));
    let step = steps.iter().find(|s| s.bar == 63).unwrap();
    assert_eq!(step.output.value, 1.0);
}

#[test]
fn flag_breaks_out_in_the_direction_of_its_pole() {
    let steps = run(CHART, &[], &bars(FLAG));
    expect(&steps, &[(63, &["bull_flag"])]);
    assert_eq!(extra(&steps, 63, "pattern"), 15.0);
    assert!(close(extra(&steps, 63, "level"), 118.9));
    assert!(close(extra(&steps, 63, "invalidation"), 112.4));
    assert!(close(extra(&steps, 63, "fit"), 1.0));
    let down = run(CHART, &[], &mirrored(&bars(FLAG)));
    expect(&down, &[(63, &["bear_flag"])]);
}

#[test]
fn cup_and_handle_confirms_over_the_right_rim() {
    let steps = run(CHART, &[], &bars(CUP_AND_HANDLE));
    expect(&steps, &[(73, &["bull_triangle", "bull_cup_and_handle"])]);
    let step = steps.iter().find(|s| s.bar == 73).unwrap();
    assert_eq!(step.output.value, 1.0);
    let inverse = run(CHART, &[], &mirrored(&bars(CUP_AND_HANDLE)));
    expect(
        &inverse,
        &[(73, &["bear_triangle", "bear_inverse_cup_and_handle"])],
    );
}

#[test]
fn symmetrical_triangle_breaks_out_on_either_side() {
    let up = run(CHART, &[], &bars(TRIANGLE));
    expect(&up, &[(53, &["bull_triangle"])]);
    assert_eq!(extra(&up, 53, "pattern"), 1.0);
    // Upper boundary 120 - 0.25 * (45 - 10) on the bar of the close through it.
    assert!(close(extra(&up, 53, "level"), 111.25));
    let down = run(CHART, &[], &bars(TRIANGLE_DOWN));
    expect(&down, &[(53, &["bear_triangle"])]);
    assert!(close(extra(&down, 53, "level"), 108.5));
}

#[test]
fn ascending_and_descending_triangle_need_the_flat_boundary() {
    let asc = run(CHART, &[], &bars(ASCENDING_TRIANGLE));
    expect(&asc, &[(53, &["bull_ascending_triangle"])]);
    assert_eq!(extra(&asc, 53, "pattern"), 2.0);
    assert_eq!(extra(&asc, 53, "level"), 120.0);
    let desc = run(CHART, &[], &bars(DESCENDING_TRIANGLE));
    expect(&desc, &[(53, &["bear_descending_triangle"])]);
    assert_eq!(extra(&desc, 53, "pattern"), 3.0);
    assert_eq!(extra(&desc, 53, "level"), 100.0);
}

#[test]
fn broadening_formation_breaks_out_on_either_side() {
    let up = run(CHART, &[], &bars(BROADENING));
    expect(&up, &[(53, &["bull_broadening"])]);
    assert_eq!(extra(&up, 53, "pattern"), 4.0);
    let down = run(CHART, &[], &bars(BROADENING_DOWN));
    expect(&down, &[(53, &["bear_broadening"])]);
}

#[test]
fn rising_and_falling_wedge_break_out() {
    let rising = run(CHART, &[], &bars(RISING_WEDGE));
    expect(&rising, &[(53, &["bear_rising_wedge"])]);
    assert_eq!(extra(&rising, 53, "pattern"), 5.0);
    let falling = run(CHART, &[], &bars(FALLING_WEDGE));
    expect(&falling, &[(53, &["bull_falling_wedge"])]);
    assert_eq!(extra(&falling, 53, "pattern"), 6.0);
}

// --- chart patterns: what must not fire ------------------------------------------------------

#[test]
fn a_second_high_outside_the_tolerance_is_not_a_double_top() {
    // 125 against 120 is 4.2 %: outside the 2 % default, inside a 5 % tolerance.
    let near_miss: Waypoints = &[
        (0, 100.0),
        (10, 120.0),
        (20, 105.0),
        (30, 125.0),
        (40, 100.0),
        (50, 110.0),
    ];
    let strict = run(CHART, &[], &bars(near_miss));
    expect(&strict, &[]);
    assert!(strict.iter().all(|s| s.output.value == 0.0));
    let loose = run(CHART, &[("tolerance_pct", 5.0)], &bars(near_miss));
    expect(&loose, &[(43, &["bear_double_top"])]);
}

#[test]
fn parallel_boundaries_are_a_channel_not_a_pattern() {
    let channel: Waypoints = &[
        (0, 100.0),
        (10, 120.0),
        (20, 100.0),
        (30, 120.0),
        (40, 100.0),
        (50, 120.0),
        (60, 110.0),
    ];
    expect(&run(CHART, &[], &bars(channel)), &[]);
}

#[test]
fn a_head_that_does_not_overshoot_is_no_head_and_shoulders() {
    // Head 118 on the level of the shoulders: the double and triple tops of that level are
    // there, a head and shoulders is not.
    let flat_head: Waypoints = &[
        (0, 100.0),
        (10, 118.0),
        (20, 108.0),
        (30, 118.0),
        (40, 109.0),
        (50, 117.5),
        (60, 100.0),
        (70, 110.0),
    ];
    let steps = run(CHART, &[], &bars(flat_head));
    assert!(events(&steps)
        .iter()
        .all(|(_, kinds)| !kinds.iter().any(|k| k.contains("shoulders"))));
    assert!(events(&steps)
        .iter()
        .any(|(_, kinds)| kinds.contains(&"bear_triple_top".to_string())));
}

#[test]
fn an_instance_the_rule_invalidates_raises_nothing() {
    // Double top over 120 / 120.5 with the second trough at 108, above the neckline 105. The close
    // at 121.6 on bar 48 is above the pattern's own extreme: the rule gives it up, and the fall
    // to 99 later raises no double top. Both live instances (the double top and a bullish 1-2-3)
    // are followed until bar 47; on bar 48 the 1-2-3 confirms and the double top is gone.
    let series: Waypoints = &[
        (0, 100.0),
        (10, 120.0),
        (20, 105.0),
        (30, 120.5),
        (40, 108.0),
        (50, 125.0),
        (60, 112.0),
        (70, 99.0),
        (80, 110.0),
    ];
    let steps = run(CHART, &[], &bars(series));
    // The bearish Wolfe wave over the highs 120 / 120.5 / 125 (bars 10, 30, 50) and the lows
    // 105 / 108 (bars 20, 40): 120.5 is above 120; 108 is inside the 1-2 range 105..120; line 1-3
    // rises 0.025 per bar and line 2-4 0.15, so they converge; line 1-3 is at 121 on bar 50 and
    // point 5 at 125 overshoots it. Point 5 is confirmed with the next swing (the low 99 on bar
    // 70), on bar 73.
    expect(
        &steps,
        &[
            (48, &["bull_one_two_three"]),
            (53, &["bull_rising_wedge"]),
            (73, &["bear_wolfe_wave"]),
        ],
    );
    let active = |bar: usize| steps.iter().find(|s| s.bar == bar).unwrap().output.extra["active"];
    assert_eq!(active(43), 2.0);
    assert_eq!(active(47), 2.0);
    assert_eq!(active(48), 0.0);
}

#[test]
fn an_instance_expires_after_pending_swings_confirmed_nodes() {
    // The first double top (120 / 120.5, neckline 105) is found on bar 43. Three more nodes are
    // confirmed before price closes under 105 on bar 77 (113 - 1.3 * 7 = 103.9), so with the
    // default of two it has expired and raises nothing; with ten it confirms on bar 77.
    let series: Waypoints = &[
        (0, 100.0),
        (10, 120.0),
        (20, 105.0),
        (30, 120.5),
        (40, 108.0),
        (50, 114.0),
        (60, 109.0),
        (70, 113.0),
        (80, 100.0),
        (90, 110.0),
    ];
    let default = run(CHART, &[], &bars(series));
    expect(
        &default,
        &[
            (57, &["bear_rising_wedge"]),
            (63, &["bear_triangle"]),
            (73, &["bull_triangle"]),
            (83, &["bear_triangle", "bear_double_top"]),
        ],
    );
    let patient = run(CHART, &[("pending_swings", 10.0)], &bars(series));
    expect(
        &patient,
        &[
            (57, &["bear_rising_wedge"]),
            (63, &["bear_triangle"]),
            (73, &["bull_triangle"]),
            (77, &["bear_double_top"]),
            (83, &["bear_triangle", "bear_double_top"]),
        ],
    );
}

// --- harmonic patterns -----------------------------------------------------------------------

const HARMONIC: &str = "harmonic_patterns";

/// X = 100, A = 120 and the three prices of a row; D is a swing low confirmed on bar 63 (the
/// rally to 125 is the next waypoint).
fn xabcd(b: f64, c: f64, d: f64) -> Vec<Bar> {
    let waypoints = vec![
        (0, 110.0),
        (10, 100.0),
        (20, 120.0),
        (30, b),
        (40, c),
        (50, d),
        (60, 125.0),
        (70, 110.0),
    ];
    // `bars` takes a static slice; the few waypoints of a test are leaked.
    bars(Box::leak(waypoints.into_boxed_slice()))
}

/// (kind, code, B, C, D, PRZ low, PRZ high): ratios inside the row of the table on every leg.
/// The zone is the span of the XA retracement at the row's `d` centre, the BC extension at its
/// `cd` centre and the AB=CD projection.
const ROWS: &[(&str, f64, f64, f64, f64, f64, f64)] = &[
    ("gartley", 1.0, 107.64, 113.92, 104.3, 101.56, 105.29128),
    ("bat", 2.0, 111.18, 117.33, 102.3, 102.3, 108.51),
    ("butterfly", 3.0, 104.3, 111.29, 94.96, 92.52, 96.48518),
    ("crab", 4.0, 108.82, 117.32, 88.0, 88.0, 106.14),
    ("deep_crab", 5.0, 102.3, 110.18, 89.0, 87.09948, 92.48),
    ("shark", 6.0, 110.0, 122.52, 100.2, 98.36892, 112.52),
];

#[test]
fn every_row_of_the_table_confirms_when_its_d_node_is_fixed() {
    for (name, code, b, c, d, prz_low, prz_high) in ROWS {
        let steps = run(HARMONIC, &[], &xabcd(*b, *c, *d));
        let kind = format!("bull_{name}");
        expect(&steps, &[(63, &[kind.as_str()])]);
        assert_eq!(extra(&steps, 63, "pattern"), *code, "{name}");
        assert!(
            close(extra(&steps, 63, "prz_low"), *prz_low),
            "{name} prz_low"
        );
        assert!(
            close(extra(&steps, 63, "prz_high"), *prz_high),
            "{name} prz_high"
        );
        assert_eq!(extra(&steps, 63, "d"), *d, "{name}");
        assert!(close(extra(&steps, 63, "invalidation"), *prz_low), "{name}");
        assert_eq!(
            steps.iter().find(|s| s.bar == 63).unwrap().output.value,
            1.0
        );
    }
}

#[test]
fn a_low_x_gives_the_bearish_pattern_in_the_mirror_image() {
    for (name, code, b, c, d, prz_low, prz_high) in ROWS {
        let steps = run(HARMONIC, &[], &mirrored(&xabcd(*b, *c, *d)));
        let kind = format!("bear_{name}");
        expect(&steps, &[(63, &[kind.as_str()])]);
        assert_eq!(extra(&steps, 63, "pattern"), *code);
        assert!(close(extra(&steps, 63, "prz_low"), -prz_high));
        assert!(close(extra(&steps, 63, "prz_high"), -prz_low));
        assert_eq!(
            steps.iter().find(|s| s.bar == 63).unwrap().output.value,
            -1.0
        );
    }
}

#[test]
fn a_b_leg_between_the_rows_is_no_pattern() {
    // B at 0.70 of XA: above Gartley's 0.65 + 0.03 and below Butterfly's 0.75 - 0.03.
    expect(&run(HARMONIC, &[], &xabcd(106.0, 112.0, 104.3)), &[]);
}

#[test]
fn the_ratio_tolerance_decides_whether_the_pattern_exists() {
    // B at 0.66: outside Gartley's 0.586..0.65, inside it widened by the default 0.03.
    let series = xabcd(106.8, 113.92, 104.3);
    expect(&run(HARMONIC, &[], &series), &[(63, &["bull_gartley"])]);
    expect(&run(HARMONIC, &[("ratio_tolerance", 0.0)], &series), &[]);
}

#[test]
fn a_close_through_the_zone_gives_the_structure_up() {
    // Bat with D at 101.2 (0.94 of XA, inside the widened 0.85..0.92), below the zone 102.3..108.51.
    // With the rally to 125 it confirms; when the swing after D reaches only 102.25 every close
    // since D has stayed under the zone and the rule gives the structure up.
    let waypoints = |e: f64| -> Vec<Bar> {
        let w = vec![
            (0, 110.0),
            (10, 100.0),
            (20, 120.0),
            (30, 111.18),
            (40, 117.33),
            (50, 101.2),
            (60, e),
            (70, 95.0),
            (80, 110.0),
        ];
        bars(Box::leak(w.into_boxed_slice()))
    };
    expect(
        &run(HARMONIC, &[], &waypoints(125.0)),
        &[(63, &["bull_bat"])],
    );
    expect(&run(HARMONIC, &[], &waypoints(102.25)), &[]);
}

// --- mirror image ---------------------------------------------------------------------------

fn mirror_base(base: &str) -> &str {
    match base {
        "double_top" => "double_bottom",
        "double_bottom" => "double_top",
        "triple_top" => "triple_bottom",
        "triple_bottom" => "triple_top",
        "head_and_shoulders" => "inverse_head_and_shoulders",
        "inverse_head_and_shoulders" => "head_and_shoulders",
        "ascending_triangle" => "descending_triangle",
        "descending_triangle" => "ascending_triangle",
        "rising_wedge" => "falling_wedge",
        "falling_wedge" => "rising_wedge",
        "cup_and_handle" => "inverse_cup_and_handle",
        "inverse_cup_and_handle" => "cup_and_handle",
        other => other,
    }
}

fn mirror_kind(kind: &str) -> String {
    let (side, base) = kind.split_once('_').unwrap();
    let side = if side == "bull" { "bear" } else { "bull" };
    format!("{side}_{}", mirror_base(base))
}

fn mirror_code(code: f64) -> f64 {
    match code as u32 {
        2 => 3.0,
        3 => 2.0,
        5 => 6.0,
        6 => 5.0,
        9 => 10.0,
        10 => 9.0,
        11 => 12.0,
        12 => 11.0,
        13 => 14.0,
        14 => 13.0,
        16 => 17.0,
        17 => 16.0,
        _ => code,
    }
}

#[test]
fn the_mirror_image_of_every_chart_scenario_gives_the_mirrored_events() {
    let scenarios: &[Waypoints] = &[
        DOUBLE_TOP,
        DOUBLE_BOTTOM,
        TRIPLE_TOP,
        TRIPLE_BOTTOM,
        HEAD_AND_SHOULDERS,
        INVERSE_HEAD_AND_SHOULDERS,
        ONE_TWO_THREE,
        WOLFE_WAVE,
        TRIANGLE,
        TRIANGLE_DOWN,
        ASCENDING_TRIANGLE,
        DESCENDING_TRIANGLE,
        BROADENING,
        BROADENING_DOWN,
        RISING_WEDGE,
        FALLING_WEDGE,
        FLAG,
        CUP_AND_HANDLE,
    ];
    for (n, waypoints) in scenarios.iter().enumerate() {
        let original = run(CHART, &[], &bars(waypoints));
        let mirror = run(CHART, &[], &mirrored(&bars(waypoints)));
        assert!(!events(&original).is_empty(), "scenario {n} raises nothing");
        assert_eq!(original.len(), mirror.len(), "scenario {n}");
        for (a, b) in original.iter().zip(&mirror) {
            let mirrored_kinds: Vec<String> = a.kinds.iter().map(|k| mirror_kind(k)).collect();
            assert_eq!(b.kinds, mirrored_kinds, "scenario {n} bar {}", a.bar);
            assert_eq!(
                b.output.value, -a.output.value,
                "scenario {n} bar {}",
                a.bar
            );
            for key in ["active", "confirmed", "fit"] {
                assert_eq!(
                    a.output.extra.get(key),
                    b.output.extra.get(key),
                    "{n} {key}"
                );
            }
            if let Some(code) = a.output.extra.get("pattern") {
                assert_eq!(
                    b.output.extra["pattern"],
                    mirror_code(*code),
                    "scenario {n}"
                );
            }
            for key in ["level", "invalidation", "target"] {
                if let Some(v) = a.output.extra.get(key) {
                    assert!(close(b.output.extra[key], -v), "scenario {n} {key}");
                }
            }
        }
    }
}

// --- state ----------------------------------------------------------------------------------

#[test]
fn reset_starts_over() {
    let series = bars(DOUBLE_TOP);
    let mut indicator = build_checked(CHART, &HashMap::new()).unwrap();
    let first: Vec<Option<f64>> = series
        .iter()
        .map(|b| indicator.on_bar(b).map(|o| o.value))
        .collect();
    indicator.reset();
    let second: Vec<Option<f64>> = series
        .iter()
        .map(|b| indicator.on_bar(b).map(|o| o.value))
        .collect();
    assert_eq!(first, second);
}
