//! Swing Conviction Radar against `reference/kestrel_reference/fixtures/swing_conviction_radar.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_swing_conviction_radar.txt");
const CHECKPOINTS: [usize; 5] = [150, 299, 420, 560, 699];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// The fixture's series, as documented in its header.
fn series(with_volume: bool) -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..700 {
        let x = i as f64;
        let drift = if i < 200 {
            0.12
        } else if i < 330 {
            0.0
        } else if i < 480 {
            -0.18
        } else {
            0.10
        };
        let amp = 1.0 + 0.6 * (x / 47.0).sin();
        let close = prev
            + drift
            + amp * (1.1 * (x / 4.3).sin() + 0.5 * (x / 13.0).sin()) * 0.6
            + 0.25 * (x / 1.9).cos();
        let open = prev + 0.15 * (x / 2.7).sin();
        let high = open.max(close) + 0.3 + 0.25 * (x / 3.9).sin().abs();
        let low = open.min(close) - 0.3 - 0.25 * (x / 6.1).cos().abs();
        let vol = if with_volume {
            1000.0 + 600.0 * (x / 5.0).sin().powi(2) + 300.0 * (x / 17.0).cos()
        } else {
            0.0
        };
        bars.push(Bar::new(i as i64 * 3600, open, high, low, close, vol));
        prev = close;
    }
    bars
}

fn check(prefix: &str, with_volume: bool) {
    let mut ind = build_checked("swing_conviction_radar", &HashMap::new()).unwrap();
    let tol = expected("scr_tolerance");
    let key = |k: &str| expected(&format!("{prefix}_{k}"));
    let mut legs: HashMap<usize, (f64, f64)> = HashMap::new();
    for j in 0..(key("legs_n") as usize).min(12) {
        legs.insert(
            key(&format!("leg{j}_bar")) as usize,
            (key(&format!("leg{j}_dir")), key(&format!("leg{j}_conv"))),
        );
    }
    let mut first = None;
    let mut bear = Vec::new();
    let mut bull = Vec::new();
    for (t, bar) in series(with_volume).iter().enumerate() {
        let out = ind.on_bar(bar);
        for a in ind.alerts() {
            match a.kind.as_str() {
                "bear_conviction_divergence" => bear.push(t),
                "bull_conviction_divergence" => bull.push(t),
                k => panic!("unknown alert {k}"),
            }
        }
        let Some(out) = out else { continue };
        // Vor dem ersten Beinpaar steht 0 ohne Beinfelder; „first“ ist die
        // erste Bar mit beiden.
        if !out.extra.contains_key("bull_last") {
            continue;
        }
        first.get_or_insert(t);
        if let Some((dir, conv)) = legs.get(&t) {
            let got = if *dir > 0.0 {
                out.value
            } else {
                -out.secondary.unwrap()
            };
            common::assert_close(got, *conv, tol, &format!("{prefix} leg at {t}"));
        }
        if CHECKPOINTS.contains(&t) {
            let p = format!("cp{t}");
            common::assert_close(out.value, key(&format!("{p}_high_step")), tol, &p);
            common::assert_close(
                out.secondary.unwrap(),
                key(&format!("{p}_low_step")),
                tol,
                &p,
            );
            common::assert_close(
                out.extra["bull_prev"],
                key(&format!("{p}_bull_prev")),
                tol,
                &p,
            );
            common::assert_close(
                out.extra["bear_prev"],
                key(&format!("{p}_bear_prev")),
                tol,
                &p,
            );
        }
    }
    assert_eq!(first, Some(key("first") as usize), "{prefix} first");
    for (kind, got) in [("bear_div", bear), ("bull_div", bull)] {
        let n = key(&format!("{kind}_n")) as usize;
        let want: Vec<usize> = (0..n)
            .map(|j| key(&format!("{kind}_{j}")) as usize)
            .collect();
        assert_eq!(got, want, "{prefix} {kind}");
    }
}

#[test]
fn swing_conviction_matches_the_reference_with_volume() {
    check("v", true);
}

#[test]
fn swing_conviction_matches_the_reference_without_volume() {
    check("nv", false);
}
