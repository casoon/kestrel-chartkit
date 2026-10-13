//! Reversal Setup Score against `reference/kestrel_reference/fixtures/reversal_setup_score.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_reversal_setup_score.txt");
const CHECKPOINTS: [usize; 8] = [90, 233, 377, 512, 640, 777, 901, 999];
const EVENTS: [&str; 17] = [
    "bull_review",
    "bear_review",
    "bull_action",
    "bear_action",
    "long_signal_invalidated",
    "short_signal_invalidated",
    "bull_candidate",
    "bear_candidate",
    "conflict",
    "bull_spring",
    "bear_upthrust",
    "bull_major_bos",
    "bear_major_bos",
    "bear_failed_bull_reversal",
    "bull_failed_bear_reversal",
    "bull_choch",
    "bear_choch",
];
const KEYS: [&str; 16] = [
    "bull_setup",
    "bull_conf",
    "bull_conf_decayed",
    "bull_quality",
    "bull_status",
    "bear_setup",
    "bear_conf",
    "bear_conf_decayed",
    "bear_quality",
    "bear_status",
    "conflict_count",
    "clx_state",
    "clx_direction",
    "price_zone",
    "efficiency_ratio",
    "rsi",
];

/// The fixture's series, as documented in its header.
fn series(with_volume: bool) -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..1000 {
        let x = i as f64;
        let drift = 0.25 * (x / 61.0).sin() + 0.1 * (x / 23.0).sin();
        let mut close = prev + drift + 0.9 * (x / 4.1).sin() + 0.5 * (x / 1.7).cos();
        let mut open = prev + 0.35 * (x / 2.2).sin();
        let mut high = open.max(close) + 0.25 + 0.3 * (x / 3.4).sin().abs();
        let mut low = open.min(close) - 0.25 - 0.3 * (x / 2.9).cos().abs();
        let mut vol = 1000.0 + 500.0 * (x / 5.5).sin().powi(2);
        if i % 37 == 20 {
            low -= 2.0;
            vol *= 2.5;
        }
        if i % 41 == 30 {
            high += 2.0;
            vol *= 2.5;
        }
        if i % 53 == 44 {
            close = prev - 3.5;
            open = prev + 0.5;
            high = open + 0.3;
            low = close - 0.3;
            vol *= 3.0;
        }
        if !with_volume {
            vol = 0.0;
        }
        bars.push(Bar::new(i as i64 * 14400, open, high, low, close, vol));
        prev = close;
    }
    bars
}

fn check(prefix: &str, with_volume: bool) {
    let mut ind = build_checked("reversal_setup_score", &HashMap::new()).unwrap();
    let val = |k: &str| common::golden_value(GOLDEN, &format!("{prefix}_{k}"));
    let tol = common::golden_value(GOLDEN, "vrs_tolerance");
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
            let p = format!("cp{t}");
            common::assert_close(out.value, val(&format!("{p}_bull_score")), tol, &p);
            common::assert_close(
                out.secondary.unwrap(),
                val(&format!("{p}_bear_score")),
                tol,
                &p,
            );
            for k in KEYS {
                common::assert_close(
                    out.extra[k],
                    val(&format!("{p}_{k}")),
                    tol,
                    &format!("{prefix} {p} {k}"),
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
fn reversal_setup_score_matches_the_reference_with_volume() {
    check("v", true);
}

#[test]
fn reversal_setup_score_matches_the_reference_without_volume() {
    check("nv", false);
}
