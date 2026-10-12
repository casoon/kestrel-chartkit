//! Market Motion DNA against `reference/kestrel_reference/fixtures/market_motion_dna.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_market_motion_dna.txt");
const CHECKPOINTS: [usize; 6] = [60, 177, 290, 404, 515, 699];
const EVENTS: [&str; 4] = [
    "bull_impulse",
    "bear_impulse",
    "correction_started",
    "character_lost",
];
const KEYS: [&str; 21] = [
    "correction",
    "confidence",
    "maturity",
    "bars",
    "direction",
    "class",
    "leg_start",
    "efficiency",
    "overlap",
    "retracement",
    "commitment",
    "wick_clean",
    "momentum_expansion",
    "tr_expansion",
    "ma_support",
    "cleanliness",
    "acceptance",
    "internal_swings",
    "avg_internal_swing_atr",
    "progression_ratio",
    "sub_leg_growth",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// The fixture's series, as documented in its header.
fn series() -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..700 {
        let x = i as f64;
        let phase = (i / 35) % 4;
        let drift = [1.0, -0.12, -1.0, 0.10][phase];
        let chop = [0.3, 0.9, 0.3, 1.0][phase];
        let close = prev + drift + chop * (x * 1.3).sin() + 0.2 * (x / 3.7).cos();
        let open = prev + 0.3 * chop * (x / 1.9).sin();
        let high = open.max(close) + 0.15 + 0.2 * (x / 2.3).sin().abs();
        let low = open.min(close) - 0.15 - 0.2 * (x / 3.1).cos().abs();
        bars.push(Bar::new(i as i64 * 3600, open, high, low, close, 0.0));
        prev = close;
    }
    bars
}

#[test]
fn market_motion_dna_matches_the_reference_implementation() {
    let mut ind = build_checked("market_motion_dna", &HashMap::new()).unwrap();
    let tol = expected("mmd_tolerance");
    let mut first = None;
    let mut events: HashMap<&str, Vec<usize>> = HashMap::new();
    for (t, bar) in series().iter().enumerate() {
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
            let p = format!("cp{t}");
            common::assert_close(out.value, expected(&format!("{p}_impulse")), tol, &p);
            for k in KEYS {
                common::assert_close(
                    out.extra[k],
                    expected(&format!("{p}_{k}")),
                    tol,
                    &format!("{p} {k}"),
                );
            }
        }
    }
    assert_eq!(first, Some(expected("first_reported") as usize));
    for kind in EVENTS {
        let got = events.get(kind).cloned().unwrap_or_default();
        assert_eq!(
            got.len(),
            expected(&format!("ev_{kind}_n")) as usize,
            "{kind} count: {got:?}"
        );
        for (j, b) in got.iter().take(10).enumerate() {
            assert_eq!(
                *b,
                expected(&format!("ev_{kind}_{j}")) as usize,
                "{kind} #{j}"
            );
        }
    }
}

#[test]
fn market_motion_dna_truncates_long_legs_like_the_reference() {
    let params: HashMap<String, f64> = [("max_leg_bars".to_string(), 20.0)].into();
    let mut ind = build_checked("market_motion_dna", &params).unwrap();
    let tol = expected("mmd_tolerance");
    for (t, bar) in series().iter().enumerate() {
        let out = ind.on_bar(bar);
        if !CHECKPOINTS.contains(&t) {
            continue;
        }
        let out = out.unwrap();
        let p = format!("short_cp{t}");
        common::assert_close(out.value, expected(&format!("{p}_impulse")), tol, &p);
        for k in [
            "bars",
            "class",
            "efficiency",
            "overlap",
            "retracement",
            "commitment",
            "wick_clean",
            "momentum_expansion",
            "tr_expansion",
            "ma_support",
            "cleanliness",
            "acceptance",
        ] {
            common::assert_close(
                out.extra[k],
                expected(&format!("{p}_{k}")),
                tol,
                &format!("{p} {k}"),
            );
        }
    }
}
