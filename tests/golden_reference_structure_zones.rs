//! Structure Zones against `reference/kestrel_reference/fixtures/structure_zones.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

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
