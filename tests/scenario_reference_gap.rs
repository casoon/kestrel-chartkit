//! Scenarios of the gap detector on the hand-built series of the contract fixture
//! (`reference/kestrel_reference/fixtures/contract_structure.py`, cases `gap_*`): the worked
//! example with its numbers by hand, the mirror image of every case, and the typed gaps against
//! the `extra` fields.

mod common;

use kestrel_chartkit::indicator::gap::{Gap, GapDirection, GapEngine, GapParams, GapReference};
use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::IndicatorUnit;

const CASES: &str = include_str!("fixtures/contract_structure.txt");

const ALL_CASES: [&str; 5] = [
    "gap_defaults",
    "gap_close_reference",
    "gap_horizon",
    "gap_flat",
    "gap_walk",
];

fn case(name: &str) -> common::contract::ContractCase {
    common::contract::load(CASES, name, "gap", IndicatorUnit::Unitless)
}

/// The worked example, by hand. Bar 19 (high 101, low 99, close 100) ends nineteen bars whose
/// true range is 2 each, so its ATR(14) is 2. Bar 20 opens at 104 with high 105 and low 103.5:
///
/// - gap up from the high 101 to the open 104: size 3, that is 3 / 2 = 1.5 ATR;
/// - its low 103.5 has gone 0.5 of the 3 back into the gap: a sixth filled;
/// - its true range is max(105 - 103.5, |105 - 100|, |103.5 - 100|) = 5, so the ATR rises to
///   2 + (5 - 2) / 14 = 2.2142857..., which bar 21 reports as the ATR of its previous bar;
/// - bar 26 is the first with a low (100.9) at or below 101: closed after 6 bars.
#[test]
fn test_scenario_gap_worked_example_by_hand() {
    let bars = case("gap_defaults").bars;
    let mut engine = GapEngine::with_defaults();
    let outputs: Vec<_> = bars.iter().map(|bar| engine.on_bar(bar)).collect();

    assert!(outputs[..14].iter().all(Option::is_none));
    assert!(outputs[14..].iter().all(Option::is_some));
    assert_eq!(engine.warmup_period(), 15);

    let bar19 = &bars[18];
    assert_eq!((bar19.high, bar19.low, bar19.close), (101.0, 99.0, 100.0));

    let bar20 = outputs[19].as_ref().unwrap();
    assert_eq!(bar20.value, 1.0);
    assert_eq!(bar20.extra["atr_prev"], 2.0);
    assert_eq!(bar20.extra["gap_bottom"], 101.0);
    assert_eq!(bar20.extra["gap_top"], 104.0);
    assert_eq!(bar20.extra["size"], 3.0);
    assert_eq!(bar20.extra["size_atr"], 1.5);
    assert_eq!(bar20.extra["open_count"], 1.0);
    common::assert_close(
        bar20.extra["open_1_fill"],
        1.0 / 6.0,
        1e-12,
        "fill on bar 20",
    );

    let bar21 = outputs[20].as_ref().unwrap();
    common::assert_close(
        bar21.extra["atr_prev"],
        2.0 + 3.0 / 14.0,
        1e-12,
        "ATR of bar 20",
    );

    // Bars 21 to 25 leave the gap open; the gap down of bar 23 closes on bar 26 as well.
    for output in &outputs[20..25] {
        let extra = &output.as_ref().unwrap().extra;
        assert_eq!(extra["closed_count"], 0.0);
        assert!(extra["open_count"] >= 1.0);
    }
    let bar26 = outputs[25].as_ref().unwrap();
    assert_eq!(bar26.value, 0.0);
    assert_eq!(bar26.extra["closed_count"], 2.0);
    assert_eq!(bar26.extra["closed_1_bars"], 3.0);
    assert_eq!(bar26.extra["closed_2_bars"], 6.0);
    assert_eq!(bar26.extra["open_count"], 0.0);
}

fn mirrored_key(key: &str) -> String {
    if let Some(prefix) = key.strip_suffix("_top") {
        format!("{prefix}_bottom")
    } else if let Some(prefix) = key.strip_suffix("_bottom") {
        format!("{prefix}_top")
    } else {
        key.to_string()
    }
}

/// Gaps up and gaps down are mirror images: reflecting a series at a price level negates the
/// direction, swaps and reflects the edges and leaves the ATR, the sizes, the filled fractions,
/// the counts and the bars of every closing untouched; every alert turns into its counterpart on
/// the same bar. Checked on every case of the fixture with its parameters. The level is a power
/// of two, so the reflection of the exactly representable case prices is exact and the gap that
/// sits on the minimum size stays on it.
#[test]
fn test_scenario_gap_mirror_symmetry() {
    const LEVEL: f64 = 256.0;
    for name in ALL_CASES {
        let case = case(name);
        let mut plain = build_checked("gap", &case.params).unwrap();
        let mut mirrored = build_checked("gap", &case.params).unwrap();
        let mut gaps = 0;
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
            let at = format!("{name} bar {}", i + 1);
            assert_eq!(a.is_some(), b.is_some(), "{at}: output present");
            let (Some(a), Some(b)) = (a, b) else {
                continue;
            };
            assert_eq!(b.value, -a.value, "{at}: direction");
            gaps += usize::from(a.value != 0.0);
            assert_eq!(a.extra.len(), b.extra.len(), "{at}: extra keys");
            for (key, &value) in &a.extra {
                let other_key = mirrored_key(key);
                let expected = if key.ends_with("_top") || key.ends_with("_bottom") {
                    LEVEL - value
                } else if key.ends_with("_direction") {
                    -value
                } else {
                    value
                };
                let actual = *b
                    .extra
                    .get(&other_key)
                    .unwrap_or_else(|| panic!("{at}: mirrored run lacks {other_key}"));
                common::assert_close(actual, expected, 1e-9, &format!("{at}: {other_key}"));
            }
            let kinds = |indicator: &dyn Indicator, mirror: bool| {
                let mut kinds: Vec<String> = indicator
                    .alerts()
                    .into_iter()
                    .map(|alert| match (mirror, alert.kind.as_str()) {
                        (true, "gap_up") => "gap_down".to_string(),
                        (true, "gap_down") => "gap_up".to_string(),
                        _ => alert.kind,
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
        assert!(gaps > 0, "{name}: the case forms gaps");
    }
}

/// The typed gaps say the same as the `extra` fields on every bar of every case: the open gaps
/// youngest first under `open_<n>_*`, the gaps closed on the bar under `closed_<n>_bars`.
#[test]
fn test_scenario_gap_typed_gaps_match_the_extra_fields() {
    for name in ALL_CASES {
        let case = case(name);
        let params = GapParams {
            reference: if case.params.get("reference") == Some(&1.0) {
                GapReference::Close
            } else {
                GapReference::Range
            },
            atr_len: case.params.get("atr_len").map_or(14, |v| *v as usize),
            min_atr: case.params.get("min_atr").copied().unwrap_or(0.0),
            max_bars: case.params.get("max_bars").map_or(100, |v| *v as usize),
        };
        let mut engine = GapEngine::new(params);
        for (i, bar) in case.bars.iter().enumerate() {
            let at = format!("{name} bar {}", i + 1);
            let Some(output) = engine.on_bar(bar) else {
                assert!(engine.open_gaps().is_empty(), "{at}: no gap before the ATR");
                continue;
            };
            let extra = &output.extra;
            let open: Vec<Gap> = engine.open_gaps().iter().rev().copied().collect();
            assert_eq!(extra["open_count"], open.len() as f64, "{at}: open");
            for (n, gap) in open.iter().enumerate() {
                let key = |field: &str| format!("open_{}_{field}", n + 1);
                assert_eq!(extra[&key("direction")], gap.direction.code(), "{at}");
                assert_eq!(extra[&key("top")], gap.top, "{at}");
                assert_eq!(extra[&key("bottom")], gap.bottom, "{at}");
                assert_eq!(extra[&key("fill")], gap.fill, "{at}");
                assert_eq!(extra[&key("bars")], (i + 1 - gap.bar) as f64, "{at}");
                assert!(gap.closed_after.is_none() && gap.fill < 1.0, "{at}: open");
                assert!(gap.top > gap.bottom, "{at}: edges");
            }
            let closed: Vec<Gap> = engine.closed_gaps().iter().rev().copied().collect();
            assert_eq!(extra["closed_count"], closed.len() as f64, "{at}: closed");
            for (n, gap) in closed.iter().enumerate() {
                let bars = extra[&format!("closed_{}_bars", n + 1)];
                assert_eq!(gap.closed_after, Some(bars as usize), "{at}");
                assert_eq!(gap.fill, 1.0, "{at}: a closed gap is filled");
                assert_eq!(gap.bar + bars as usize, i + 1, "{at}: gap bar");
            }
            // The gap formed on this bar is the youngest open one, or the youngest closed one.
            if output.value != 0.0 {
                let formed = open
                    .iter()
                    .chain(&closed)
                    .find(|gap| gap.bar == i + 1)
                    .unwrap_or_else(|| panic!("{at}: the gap formed here is followed"));
                let direction = if output.value > 0.0 {
                    GapDirection::Up
                } else {
                    GapDirection::Down
                };
                assert_eq!(formed.direction, direction, "{at}");
                assert_eq!(formed.top, extra["gap_top"], "{at}");
                assert_eq!(formed.bottom, extra["gap_bottom"], "{at}");
                assert_eq!(formed.size(), extra["size"], "{at}");
                assert_eq!(formed.size_atr, extra.get("size_atr").copied(), "{at}");
            }
        }
    }
}

/// The stricter reading of a gap, the whole bar beyond the previous one, is the set of gaps
/// against the range that are not closed on their own bar: on the random walk every gap bar
/// either lies wholly beyond the previous bar and stays open, or reaches back and closes at once.
#[test]
fn test_scenario_gap_full_bar_gaps_are_those_not_closed_on_their_own_bar() {
    let case = case("gap_walk");
    let mut engine = build_checked("gap", &case.params).unwrap();
    let (mut full_bar, mut closed_at_once) = (0, 0);
    for (i, bar) in case.bars.iter().enumerate() {
        let Some(output) = engine.on_bar(bar) else {
            continue;
        };
        if output.value == 0.0 {
            continue;
        }
        let previous = &case.bars[i - 1];
        let beyond = bar.low > previous.high || bar.high < previous.low;
        let closed_here = (1..=output.extra["closed_count"] as usize)
            .any(|n| output.extra[&format!("closed_{n}_bars")] == 0.0);
        assert_eq!(beyond, !closed_here, "bar {}", i + 1);
        full_bar += usize::from(beyond);
        closed_at_once += usize::from(closed_here);
    }
    assert!(full_bar > 0 && closed_at_once > 0);
}
