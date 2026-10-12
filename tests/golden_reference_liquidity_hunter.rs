//! Liquidity Hunter against `reference/kestrel_reference/fixtures/liquidity_hunter.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_liquidity_hunter.txt");
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

fn check(prefix: &str, with_volume: bool) {
    let mut ind = build_checked("liquidity_hunter", &HashMap::new()).unwrap();
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
fn liquidity_hunter_matches_the_reference_with_volume() {
    check("v", true);
}

#[test]
fn liquidity_hunter_matches_the_reference_without_volume() {
    check("nv", false);
}
