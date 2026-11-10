//! Equal-Level Liquidity against `reference/kestrel_reference/fixtures/equal_level_liquidity.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_equal_level_liquidity.txt");
const CHECKPOINTS: [usize; 8] = [60, 250, 270, 345, 440, 635, 730, 799];
const EVENTS: [&str; 12] = [
    "bear_bsl_sweep",
    "bull_bsl_breakout",
    "bear_bsl_stop_hunt",
    "bull_ssl_sweep",
    "bear_ssl_breakout",
    "bull_ssl_stop_hunt",
    "bear_reclaim",
    "bear_premium_reclaim",
    "bull_reclaim",
    "bull_premium_reclaim",
    "primary_bsl_near",
    "primary_ssl_near",
];

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

/// The fixture's series, as documented in its header.
fn series(with_volume: bool) -> Vec<Bar> {
    let steps = [2.5, -1.2, 3.5, -4.0];
    let mut bars = Vec::new();
    for i in 0..800usize {
        let x = i as f64;
        let k = i / 96;
        let level = 100.0 + (0..k).map(|j| steps[j % 4]).sum::<f64>();
        let tau = 2.0 * std::f64::consts::PI;
        let mut close = level + 2.5 * (tau * x / 32.0).sin() + 0.04 * (x / 2.9).sin();
        let open = level + 2.5 * (tau * (x - 0.6) / 32.0).sin();
        let mut high = open.max(close) + 0.35 + 0.03 * (x / 5.3).sin().abs();
        let mut low = open.min(close) - 0.35 - 0.03 * (x / 4.1).cos().abs();
        let mut vol = 1000.0 + 200.0 * (x / 6.0).sin().powi(2);
        if i % 96 == 72 && k % 4 < 2 {
            high += 1.6;
            vol *= 2.2;
        }
        if i % 96 == 88 && k % 4 < 2 {
            low -= 1.6;
            vol *= 2.2;
        }
        if i % 192 == 104 {
            close = high - 0.05;
            vol *= 1.6;
        }
        if !with_volume {
            vol = 0.0;
        }
        bars.push(Bar::new(i as i64 * 3600, open, high, low, close, vol));
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

/// Whether `key` belongs to the list of levels: `atr`, `tolerance`, `bsl_levels`, `ssl_levels`
/// and the per-level keys `bsl_<i>_*` / `ssl_<i>_*`.
fn is_list_key(key: &str) -> bool {
    matches!(key, "atr" | "tolerance" | "bsl_levels" | "ssl_levels")
        || ["bsl_", "ssl_"].iter().any(|side| {
            key.strip_prefix(side)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        })
}

fn check(prefix: &str, with_volume: bool) {
    let mut ind = build_checked("equal_level_liquidity", &HashMap::new()).unwrap();
    let key = |k: &str| format!("{prefix}_{k}");
    let val = |k: &str| common::golden_value(GOLDEN, &key(k));
    let tol = common::golden_value(GOLDEN, "lqh_tolerance");
    let mut first = None;
    let mut events: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut scores = Vec::new();
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
        for side in ["bear", "bull"] {
            if let Some(s) = out.extra.get(&format!("{side}_reclaim_score")) {
                scores.push((t, *s));
            }
        }
        if CHECKPOINTS.contains(&t) {
            let p = format!("cp{t}");
            common::assert_close(out.value, val(&format!("{p}_bias")), tol, &p);
            for name in [
                "primary_bsl",
                "secondary_bsl",
                "primary_ssl",
                "secondary_ssl",
            ] {
                let k = format!("{p}_{name}");
                assert_eq!(out.extra.contains_key(name), has(&key(&k)), "{prefix} {k}");
                if has(&key(&k)) {
                    common::assert_close(out.extra[name], val(&k), tol, &k);
                    let ks = format!("{name}_score");
                    common::assert_close(out.extra[&ks], val(&format!("{p}_{ks}")), tol, &k);
                }
            }
            // The list of levels: every key of the fixture, and no other in the output.
            let listed = entries(&key(&format!("{p}_list_")));
            assert!(listed.len() >= 4, "{prefix} {p}: list keys in the fixture");
            for (k, want) in &listed {
                let got = out
                    .extra
                    .get(k)
                    .unwrap_or_else(|| panic!("{prefix} {p}: missing {k}"));
                common::assert_close(*got, *want, tol, &format!("{prefix} {p} {k}"));
            }
            let in_output = out.extra.keys().filter(|k| is_list_key(k)).count();
            assert_eq!(in_output, listed.len(), "{prefix} {p}: list keys");
            for k in [
                "bsl_count",
                "ssl_count",
                "recent_bull_score",
                "recent_bear_score",
                "volume_ratio",
            ] {
                common::assert_close(
                    out.extra[k],
                    val(&format!("{p}_{k}")),
                    tol,
                    &format!("{p} {k}"),
                );
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
        for (j, b) in got.iter().take(10).enumerate() {
            assert_eq!(
                *b,
                val(&format!("ev_{kind}_{j}")) as usize,
                "{prefix} {kind} #{j}"
            );
        }
    }
    for (j, (t, s)) in scores.iter().take(10).enumerate() {
        assert_eq!(
            *t,
            val(&format!("score{j}_bar")) as usize,
            "{prefix} score #{j}"
        );
        common::assert_close(
            *s,
            val(&format!("score{j}_value")),
            tol,
            &format!("{prefix} score #{j}"),
        );
    }
}

#[test]
fn equal_level_liquidity_matches_the_reference_with_volume() {
    check("v", true);
}

#[test]
fn equal_level_liquidity_matches_the_reference_without_volume() {
    check("nv", false);
}

/// Bars around closes: the open is the previous close, high and low lie 1 beyond the close (or
/// at the open), no volume. Every true range is 2, so the ATR is 2 and the tolerance 0.24.
fn level_scenario() -> Vec<Bar> {
    let closes: [f64; 75] = [
        95.0, 95.6, 96.2, 96.8, 97.3, 97.8, 98.2, 98.5, 98.8, 99.0, 98.5, 98.0, 97.4, 96.8, 96.2,
        95.8, 95.5, 95.3, 95.6, 96.0, 96.6, 97.2, 97.8, 98.4, 98.9, 99.2, 98.7, 98.1, 97.5, 96.9,
        96.4, 96.2, 96.0, 95.9, 96.1, 96.4, 96.8, 97.2, 97.6, 98.0, 98.3, 98.4, 98.0, 97.5, 97.0,
        96.5, 96.1, 95.8, 95.6, 95.5, 95.7, 96.1, 96.6, 97.1, 97.6, 98.0, 98.4, 98.85, 98.4, 97.9,
        97.4, 96.9, 96.5, 96.2, 96.0, 95.9, 96.0, 96.8, 97.6, 98.4, 99.0, 100.0, 99.3, 98.6, 97.9,
    ];
    let mut previous = closes[0];
    closes
        .iter()
        .enumerate()
        .map(|(i, &close)| {
            let open = previous;
            previous = close;
            let (high, low) = ((close + 1.0).max(open), (close - 1.0).min(open));
            Bar::new(i as i64 * 3600, open, high, low, close, 0.0)
        })
        .collect()
}

/// Szenario mit von Hand hergeleiteten Sollwerten (ATR 2, Toleranz 0,24): vier Pivot-Hochs bei
/// 100,00, 100,20, 99,40 und 99,85. Die ersten beiden bilden ein Niveau bei 100,10; das dritte
/// liegt 0,70 entfernt, das vierte 0,25 und damit 0,01 zu weit. Das Niveau wird getestet,
/// erreicht (Sweep) und zurückerobert; jedes Ereignis steht an seiner Bar in der Ausgabe.
#[test]
fn the_list_names_every_level_its_tests_and_its_events() {
    let params = HashMap::from([("list_min_touches".to_string(), 1.0)]);
    let mut ind = build_checked("equal_level_liquidity", &params).unwrap();
    let mut by_default = build_checked("equal_level_liquidity", &HashMap::new()).unwrap();
    let outputs: Vec<_> = level_scenario()
        .iter()
        .map(|bar| (ind.on_bar(bar), by_default.on_bar(bar)))
        .collect();
    let level = |t: usize, i: usize, field: &str| {
        let out = outputs[t].0.as_ref().expect("output");
        out.extra.get(&format!("bsl_{i}_{field}")).copied()
    };
    let near = |got: Option<f64>, want: f64| (got.expect("key") - want).abs() < 1e-9;

    let first = outputs[13].0.as_ref().expect("first output with the ATR");
    assert!(outputs[12].0.is_none());
    assert!((first.extra["atr"] - 2.0).abs() < 1e-12);
    assert!((first.extra["tolerance"] - 0.24).abs() < 1e-12);
    assert_eq!(first.extra["bsl_levels"], 0.0);

    // Bar 17 confirms the pivot high of bar 9: level 1, a single pivot.
    assert_eq!(outputs[16].0.as_ref().unwrap().extra["bsl_levels"], 0.0);
    assert!(near(level(17, 1, "price"), 100.0));
    assert_eq!(level(17, 1, "touches"), Some(1.0));
    assert_eq!(level(17, 1, "event"), Some(1.0));
    assert_eq!(level(17, 1, "first_bars_back"), Some(8.0));
    // With the default (2 pivots) the single pivot is not listed.
    assert_eq!(outputs[17].1.as_ref().unwrap().extra["bsl_levels"], 0.0);
    // Bar 24 (high 99.9) reaches 100.00 - 0.24 after bar 23 (high 99.4) did not.
    assert_eq!(level(23, 1, "tests"), Some(0.0));
    assert_eq!(level(24, 1, "tests"), Some(1.0));
    assert_eq!(level(24, 1, "event"), Some(2.0));
    // Bar 33 confirms the pivot high 100.20: 0.20 from the level, it joins.
    assert!(near(level(33, 1, "price"), 100.1));
    assert_eq!(level(33, 1, "touches"), Some(2.0));
    assert_eq!(level(33, 1, "event"), Some(1.0));
    assert_eq!(level(33, 1, "first_bars_back"), Some(24.0));
    assert_eq!(level(33, 1, "last_bars_back"), Some(8.0));
    assert_eq!(outputs[33].1.as_ref().unwrap().extra["bsl_levels"], 1.0);
    // Bars 49 and 65 confirm 99.40 and 99.85: outside the tolerance, each its own level.
    assert!(near(level(49, 2, "price"), 99.4));
    assert!(near(level(65, 3, "price"), 99.85));
    assert_eq!(level(65, 1, "touches"), Some(2.0));
    assert_eq!(outputs[65].0.as_ref().unwrap().extra["bsl_levels"], 3.0);
    // Bar 70 (high 100.0) reaches the band 99.86..100.34 after bar 69 (high 99.4) did not.
    assert_eq!(level(70, 1, "tests"), Some(2.0));
    assert_eq!(level(70, 1, "event"), Some(2.0));
    assert_eq!(level(70, 1, "status"), Some(0.0));
    // Bar 71 (high 101.0, close 100.0): beyond 100.10 + 0.05 x 2, closed back: a sweep.
    assert_eq!(level(71, 1, "status"), Some(1.0));
    assert_eq!(level(71, 1, "event"), Some(4.0));
    assert_eq!(level(71, 1, "swept_bars_back"), Some(0.0));
    assert_eq!(level(70, 1, "swept_bars_back"), None);
    // Bar 72 (high 100.3, close 99.3): at least 0.2 back below the level: a reclaim.
    assert_eq!(level(72, 1, "status"), Some(3.0));
    assert_eq!(level(72, 1, "event"), Some(32.0));
    assert_eq!(level(72, 1, "swept_bars_back"), Some(1.0));
    // The swept level stays listed with its status; the single pivots took part in nothing.
    assert_eq!(level(74, 1, "status"), Some(3.0));
    assert_eq!(level(74, 1, "event"), Some(0.0));
    assert_eq!(level(74, 2, "status"), Some(0.0));
    assert_eq!(level(74, 3, "status"), Some(0.0));
    // The list changes no other output.
    for (t, (listed, plain)) in outputs.iter().enumerate() {
        let (Some(listed), Some(plain)) = (listed, plain) else {
            continue;
        };
        assert_eq!(listed.value, plain.value, "bar {t}");
        for (key, value) in &plain.extra {
            if !is_list_key(key) {
                assert_eq!(listed.extra.get(key), Some(value), "bar {t}: {key}");
            }
        }
    }
}

/// Die gespiegelte Reihe (jeder Preis negiert, Hoch und Tief getauscht) ergibt die gespiegelte
/// Ausgabe, auf das Bit: BSL und SSL tauschen die Plätze, Preise und der Bias wechseln das
/// Vorzeichen. Zufallsreihen ohne und mit wechselndem Umsatz, enge und weite Toleranz.
#[test]
fn the_mirror_image_gives_the_mirrored_output() {
    let swap = |name: &str| {
        name.replace("bsl", "\u{0}")
            .replace("ssl", "bsl")
            .replace('\u{0}', "ssl")
            .replace("bull", "\u{0}")
            .replace("bear", "bull")
            .replace('\u{0}', "bear")
    };
    let is_price = |key: &str| {
        key.ends_with("_price")
            || matches!(
                key,
                "primary_bsl" | "secondary_bsl" | "primary_ssl" | "secondary_ssl"
            )
    };
    let (mut alerts, mut listed, mut events) = (0usize, 0usize, 0usize);
    for seed in 1..=12u64 {
        for with_volume in [false, true] {
            let mut state = seed;
            let bars: Vec<Bar> = common::generate_random_walk_bars(seed, 500, 100.0, 0.0, 1.5, 0.0)
                .into_iter()
                .map(|b| {
                    state = state
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let factor = 0.4 + ((state >> 33) as f64 / (1u64 << 31) as f64) * 2.0;
                    let volume = if with_volume { 1000.0 * factor } else { 0.0 };
                    Bar::new(b.timestamp, b.open, b.high, b.low, b.close, volume)
                })
                .collect();
            for (tolerance, min_list) in [(0.12, 2.0), (0.6, 1.0)] {
                let params = HashMap::from([
                    ("pivot_len".to_string(), 3.0),
                    ("eq_tolerance_atr".to_string(), tolerance),
                    ("list_min_touches".to_string(), min_list),
                ]);
                let mut plain = build_checked("equal_level_liquidity", &params).unwrap();
                let mut twin = build_checked("equal_level_liquidity", &params).unwrap();
                for (t, bar) in bars.iter().enumerate() {
                    let mirror = Bar::new(
                        bar.timestamp,
                        -bar.open,
                        -bar.low,
                        -bar.high,
                        -bar.close,
                        bar.volume,
                    );
                    let (out, reflected) = (plain.on_bar(bar), twin.on_bar(&mirror));
                    let context = format!("seed {seed}, volume {with_volume}, bar {t}");
                    let mut kinds: Vec<String> =
                        plain.alerts().into_iter().map(|a| a.kind).collect();
                    let mut swapped: Vec<String> =
                        twin.alerts().iter().map(|a| swap(&a.kind)).collect();
                    kinds.sort();
                    swapped.sort();
                    assert_eq!(kinds, swapped, "{context}");
                    alerts += kinds.len();
                    let (Some(out), Some(reflected)) = (out, reflected) else {
                        continue;
                    };
                    let expected: HashMap<String, f64> = reflected
                        .extra
                        .iter()
                        .map(|(key, value)| {
                            let key = swap(key);
                            let value = if is_price(&key) { -value } else { *value };
                            (key, value)
                        })
                        .collect();
                    assert_eq!(out.value, -reflected.value, "{context}");
                    assert_eq!(out.extra, expected, "{context}");
                    listed += (out.extra["bsl_levels"] + out.extra["ssl_levels"]) as usize;
                    events += out
                        .extra
                        .iter()
                        .filter(|(k, v)| k.ends_with("_event") && **v != 0.0)
                        .count();
                }
            }
        }
    }
    // Recorded, so that the walks are known to exercise the rules: alerts, listed levels summed
    // over the bars, and levels with an event on a bar.
    assert_eq!((alerts, listed, events), (2393, 170502, 11177));
}
