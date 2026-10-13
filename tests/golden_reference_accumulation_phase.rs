//! Accumulation Phase against `reference/kestrel_reference/fixtures/accumulation_phase.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_accumulation_phase.txt");
const CHECKPOINTS: [usize; 8] = [76, 230, 274, 307, 569, 637, 700, 899];
const EVENTS: [&str; 6] = [
    "bull_accumulation_developing",
    "bull_accumulation_mature",
    "bull_breakout_ready",
    "bear_distribution_developing",
    "bear_distribution_mature",
    "bear_breakdown_ready",
];
const KEYS: [&str; 18] = [
    "accum_trend_loss",
    "accum_range",
    "accum_compression",
    "accum_liquidity",
    "accum_structure",
    "dist_trend_loss",
    "dist_range",
    "dist_compression",
    "dist_liquidity",
    "dist_structure",
    "accum_bars",
    "dist_bars",
    "accum_state",
    "dist_state",
    "efficiency",
    "bbw_ratio",
    "atr_ratio",
    "range_atr",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// The fixture's series, as documented in its header.
fn series() -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 150.0f64;
    for i in 0..900 {
        let x = i as f64;
        let (drift, amp) = if i < 150 {
            (-0.35, 0.6)
        } else if i < 330 {
            (0.0, 0.3)
        } else if i < 480 {
            (0.40, 0.6)
        } else if i < 680 {
            (0.0, 0.3)
        } else {
            (-0.38, 0.6)
        };
        let pull = if (150..330).contains(&i) {
            -0.2 * (prev - 97.0)
        } else if (480..680).contains(&i) {
            -0.2 * (prev - 157.0)
        } else {
            0.0
        };
        let close = prev + drift + pull + amp * ((x / 5.0).sin() + 0.6 * (x / 7.9).sin());
        let open = prev + 0.25 * (x / 1.9).sin();
        let mut high = open.max(close) + 0.3 + 0.3 * (x / 3.3).sin().abs();
        let mut low = open.min(close) - 0.3 - 0.3 * (x / 4.4).cos().abs();
        if (150..330).contains(&i) && i % 13 == 7 {
            low -= 1.8;
        }
        if (480..680).contains(&i) && i % 13 == 9 {
            high += 1.8;
        }
        bars.push(Bar::new(i as i64 * 14400, open, high, low, close, 0.0));
        prev = close;
    }
    bars
}

#[test]
fn accumulation_phase_matches_the_reference_implementation() {
    let mut ind = build_checked("accumulation_phase", &HashMap::new()).unwrap();
    let tol = expected("vacc_tolerance");
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
            common::assert_close(out.value, expected(&format!("{p}_accum")), tol, &p);
            common::assert_close(
                out.secondary.unwrap(),
                expected(&format!("{p}_dist")),
                tol,
                &p,
            );
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
