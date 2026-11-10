//! Structure Zones against `reference/kestrel_reference/fixtures/structure_zones.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::{BTreeMap, HashMap};

const GOLDEN: &str = include_str!("fixtures/golden_structure_zones.txt");
const CHECKPOINTS: [usize; 9] = [60, 150, 260, 333, 444, 555, 666, 777, 899];
const EVENTS: [&str; 6] = [
    "support_relevant",
    "resistance_relevant",
    "bull_support_hold",
    "bear_resistance_hold",
    "bear_support_break",
    "bull_resistance_break",
];

/// The fixture's series, as documented in its header.
fn series(with_volume: bool) -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..900 {
        let x = i as f64;
        let drift = 0.3 * (x / 70.0).sin();
        let mut close = prev + drift + 0.8 * (x / 5.3).sin() + 0.4 * (x / 1.9).cos();
        let mut open = prev + 0.3 * (x / 2.4).sin();
        let mut high = open.max(close) + 0.2 + 0.3 * (x / 3.1).sin().abs();
        let mut low = open.min(close) - 0.2 - 0.3 * (x / 2.6).cos().abs();
        let mut vol = 1000.0 + 600.0 * (x / 4.5).sin().powi(2);
        if i % 29 == 13 {
            low -= 1.6;
            vol *= 2.0;
        }
        if i % 31 == 21 {
            high += 1.6;
            vol *= 2.0;
        }
        if i % 47 == 33 {
            close = prev + 3.2;
            open = prev - 0.2;
            high = close + 0.2;
            low = open - 0.2;
            vol *= 2.8;
        }
        if !with_volume {
            vol = 0.0;
        }
        bars.push(Bar::new(i as i64 * 14400, open, high, low, close, vol));
        prev = close;
    }
    bars
}

/// Every `key=value` line of the fixture under `prefix`, with the prefix stripped.
fn entries(prefix: &str) -> HashMap<String, f64> {
    GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .filter_map(|(k, v)| {
            k.strip_prefix(prefix)
                .map(|rest| (rest.to_string(), v.parse::<f64>().unwrap()))
        })
        .collect()
}

fn check(prefix: &str, with_volume: bool) {
    let mut ind = build_checked("structure_zones", &HashMap::new()).unwrap();
    let val = |k: &str| common::golden_value(GOLDEN, &format!("{prefix}_{k}"));
    let tol = common::golden_value(GOLDEN, "vsz_tolerance");
    let mut first = None;
    let mut events: HashMap<&str, Vec<usize>> = HashMap::new();
    for (t, bar) in series(with_volume).iter().enumerate() {
        let out = ind.on_bar(bar);
        for a in ind.alerts() {
            let kind = EVENTS
                .iter()
                .find(|k| **k == a.kind)
                .expect("unknown alert");
            events.entry(kind).or_default().push(t);
        }
        let Some(out) = out else { continue };
        first.get_or_insert(t);
        if CHECKPOINTS.contains(&t) {
            let want = entries(&format!("{prefix}_cp{t}_"));
            let zone_keys = out
                .extra
                .keys()
                .filter(|k| k.starts_with("support_") || k.starts_with("resistance_"))
                .count();
            let want_zone_keys = want
                .keys()
                .filter(|k| k.starts_with("support_") || k.starts_with("resistance_"))
                .count();
            assert_eq!(zone_keys, want_zone_keys, "{prefix} cp{t} zone keys");
            for (k, e) in want {
                let got = if k == "zones" {
                    out.value
                } else {
                    *out.extra
                        .get(&k)
                        .unwrap_or_else(|| panic!("{prefix} cp{t}: missing {k}"))
                };
                common::assert_close(got, e, tol, &format!("{prefix} cp{t} {k}"));
            }
        }
    }
    assert_eq!(first, Some(val("first") as usize), "{prefix} first");
    for kind in EVENTS {
        let got = events.get(kind).cloned().unwrap_or_default();
        assert_eq!(
            got.len(),
            val(&format!("ev_{kind}_n")) as usize,
            "{prefix} {kind}: {got:?}"
        );
        for (j, b) in got.iter().take(8).enumerate() {
            assert_eq!(
                *b,
                val(&format!("ev_{kind}_{j}")) as usize,
                "{prefix} {kind} #{j}"
            );
        }
    }
}

#[test]
fn structure_zones_match_the_reference_with_volume() {
    check("v", true);
}

#[test]
fn structure_zones_match_the_reference_without_volume() {
    check("nv", false);
}

/// 24 bars around closes (1 above, 1 below: true range 2) with a pivot high of 100 at bar 13,
/// then bars given in full. Mirrored: every price negated, highs and lows exchanged.
fn zone_scenario(mirrored: bool) -> Vec<Bar> {
    let closes: [f64; 24] = [
        96.2, 96.4, 96.6, 96.8, 97.0, 97.2, 97.4, 97.6, 97.8, 98.0, 98.3, 98.6, 98.8, 99.0, 98.6,
        98.2, 97.8, 97.4, 97.0, 96.6, 96.3, 96.5, 96.9, 97.3,
    ];
    let tail: [(f64, f64, f64, f64); 14] = [
        (97.3, 98.6, 97.2, 98.0),
        (98.2, 99.9, 98.1, 99.5),
        (99.5, 100.1, 99.2, 99.6),
        (99.9, 101.0, 99.8, 100.8),
        (100.8, 102.0, 100.5, 101.5),
        (101.5, 101.8, 100.2, 100.9),
        (100.9, 101.5, 100.6, 101.2),
        (101.2, 101.6, 100.7, 101.0),
        (101.0, 101.5, 100.6, 101.3),
        (101.3, 101.7, 100.8, 101.1),
        (101.1, 101.8, 100.9, 101.6),
        (101.6, 102.1, 101.2, 101.9),
        (101.9, 102.4, 101.5, 102.2),
        (102.2, 102.5, 101.8, 102.3),
    ];
    let mut previous = closes[0];
    let mut ohlc: Vec<(f64, f64, f64, f64)> = closes
        .iter()
        .map(|&close| {
            let open = previous;
            previous = close;
            (
                open,
                (close + 1.0).max(open),
                (close - 1.0).min(open),
                close,
            )
        })
        .collect();
    ohlc.extend(tail);
    ohlc.into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c))| {
            let (o, h, l, c) = if mirrored {
                (-o, -l, -h, -c)
            } else {
                (o, h, l, c)
            };
            Bar::new(i as i64 * 14400, o, h, l, c, 0.0)
        })
        .collect()
}

/// Szenario mit von Hand hergeleiteten Sollwerten (ATR 2 bis Bar 23): Das Pivot-Hoch 100 der Bar
/// 13 begründet an Bar 20 eine Widerstandszone von 99,7 bis 100,3. Test an Bar 25, derselbe
/// Besuch an Bar 26, Bruch an Bar 27 mit dem Schluss 100,8, Bar 28 außerhalb, zweiter Test und
/// Rollentausch an Bar 29. Die gebrochene Zone bleibt mit ihrem Status in der Ausgabe. Im
/// Spiegelbild dasselbe für eine Unterstützungszone.
#[test]
fn the_list_follows_a_zone_through_test_break_and_role_reversal() {
    for mirrored in [false, true] {
        let sign = if mirrored { -1.0 } else { 1.0 };
        let mut ind = build_checked("structure_zones", &HashMap::new()).unwrap();
        let outputs: Vec<_> = zone_scenario(mirrored)
            .iter()
            .map(|bar| ind.on_bar(bar))
            .collect();
        let zone = |t: usize, field: &str| {
            let out = outputs[t].as_ref().expect("output");
            out.extra.get(&format!("zone_1_{field}")).copied()
        };
        let near = |got: Option<f64>, want: f64| (got.expect("key") - want).abs() < 1e-9;

        assert!(outputs[13].is_none() && outputs[14].is_some());
        assert!((outputs[14].as_ref().unwrap().extra["atr"] - 2.0).abs() < 1e-12);
        assert_eq!(outputs[19].as_ref().unwrap().value, 0.0);
        assert_eq!(zone(19, "lower"), None);

        // Bar 20: created, 0.15 x ATR on either side of the pivot.
        assert_eq!(outputs[20].as_ref().unwrap().value, 1.0);
        let (lower, upper) = if mirrored {
            (-100.3, -99.7)
        } else {
            (99.7, 100.3)
        };
        assert!(near(zone(20, "lower"), lower) && near(zone(20, "upper"), upper));
        assert_eq!(zone(20, "type"), Some(if mirrored { 1.0 } else { 2.0 }));
        assert_eq!(zone(20, "role"), Some(-sign));
        assert_eq!(zone(20, "status"), Some(0.0));
        assert_eq!(zone(20, "event"), Some(1.0));
        assert_eq!(zone(20, "age"), Some(0.0));
        // Bar 25: the first visit. Bar 26: still inside, no new test.
        assert_eq!(zone(24, "tests"), Some(0.0));
        assert_eq!(zone(25, "tests"), Some(1.0));
        assert_eq!(zone(25, "event"), Some(2.0));
        assert_eq!(zone(25, "status"), Some(1.0));
        assert_eq!(zone(26, "tests"), Some(1.0));
        assert_eq!(zone(26, "event"), Some(0.0));
        assert_eq!(zone(26, "break_bars_back"), None);
        // Bar 27: the close 100.8 beyond the far edge after a close of 99.6.
        assert_eq!(zone(27, "status"), Some(3.0));
        assert_eq!(zone(27, "event"), Some(8.0));
        assert_eq!(zone(27, "break_bars_back"), Some(0.0));
        assert!(near(zone(27, "break_close"), sign * 100.8));
        assert_eq!(zone(27, "role"), Some(-sign));
        // Bar 28: outside the zone, which stays listed as broken.
        assert_eq!(zone(28, "status"), Some(3.0));
        assert_eq!(zone(28, "event"), Some(0.0));
        assert_eq!(zone(28, "break_bars_back"), Some(1.0));
        assert_eq!(zone(28, "flip_bars_back"), None);
        // Bar 29: back in the zone (test 2) with a close beyond the midpoint: the role reversal.
        assert_eq!(zone(29, "tests"), Some(2.0));
        assert_eq!(zone(29, "event"), Some(18.0));
        assert_eq!(zone(29, "status"), Some(4.0));
        assert_eq!(zone(29, "role"), Some(sign));
        assert_eq!(zone(29, "flip_bars_back"), Some(0.0));
        assert_eq!(zone(29, "break_bars_back"), Some(2.0));
        assert_eq!(zone(37, "status"), Some(4.0));
        assert_eq!(zone(37, "age"), Some(17.0));
        assert!(near(zone(37, "break_close"), sign * 100.8));
    }
}

/// Die Liste der Zonen ist spiegelsymmetrisch, auf das Bit: Auf der negierten Reihe tauschen
/// Unterstützung und Widerstand die Rolle (Typen 1/2, 3/4, 5/6, 7/8), die Ränder wechseln
/// Vorzeichen und Seite, Status, Tests, Alter und Ereignisse bleiben. Zufallsreihen ohne Umsatz
/// (mit Umsatz ist der MFI auf negierten Preisen nicht definiert).
#[test]
fn the_zone_list_of_the_mirror_image_is_the_mirrored_list() {
    let (mut zones, mut events) = (0usize, [0usize; 5]);
    for seed in 1..=6u64 {
        let bars = common::generate_random_walk_bars(seed, 500, 100.0, 0.0, 1.5, 0.0);
        let mut plain = build_checked("structure_zones", &HashMap::new()).unwrap();
        let mut twin = build_checked("structure_zones", &HashMap::new()).unwrap();
        for (t, bar) in bars.iter().enumerate() {
            let mirror = Bar::new(
                bar.timestamp,
                -bar.open,
                -bar.low,
                -bar.high,
                -bar.close,
                0.0,
            );
            let (out, reflected) = (plain.on_bar(bar), twin.on_bar(&mirror));
            let (Some(out), Some(reflected)) = (out, reflected) else {
                continue;
            };
            let context = format!("seed {seed}, bar {t}");
            assert_eq!(out.value, reflected.value, "{context}");
            assert_eq!(out.extra["atr"], reflected.extra["atr"], "{context}");
            // One record per zone, as bit patterns so that the records can be sorted. Zones
            // created on the same bar are listed in a fixed order of their causes (swing high
            // before swing low, and so on), which the mirror image exchanges: the two lists hold
            // the same zones, not always at the same index.
            let records = |o: &kestrel_chartkit::indicator::IndicatorOutput, mirror: bool| {
                let mut zones: BTreeMap<usize, BTreeMap<String, u64>> = BTreeMap::new();
                for (key, value) in &o.extra {
                    let Some(rest) = key.strip_prefix("zone_") else {
                        continue;
                    };
                    let (index, field) = rest.split_once('_').unwrap();
                    let (field, value) = match (mirror, field) {
                        (true, "lower") => ("upper", -value),
                        (true, "upper") => ("lower", -value),
                        (true, "break_close" | "role") => (field, -value),
                        // 1/2, 3/4, 5/6, 7/8 are the support and resistance form of one type.
                        (true, "type") if value % 2.0 == 1.0 => (field, value + 1.0),
                        (true, "type") => (field, value - 1.0),
                        _ => (field, *value),
                    };
                    zones
                        .entry(index.parse().unwrap())
                        .or_default()
                        .insert(field.to_string(), (value + 0.0).to_bits());
                }
                let mut zones: Vec<_> = zones.into_values().collect();
                zones.sort();
                zones
            };
            let got = records(&out, false);
            assert_eq!(got, records(&reflected, true), "{context}");
            assert_eq!(got.len(), out.value as usize, "{context}");
            for zone in &got {
                for (bit, count) in events.iter_mut().enumerate() {
                    *count += usize::from(f64::from_bits(zone["event"]) as u32 & (1 << bit) != 0);
                }
            }
            zones += out.value as usize;
        }
    }
    // Recorded, so that the walks are known to exercise the lifecycle: zones summed over the
    // bars, and the events created, test, bounce, break and role reversal.
    assert_eq!((zones, events), (111960, [696, 4174, 362, 2911, 2096]));
}
