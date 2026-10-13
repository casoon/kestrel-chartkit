//! Tradability Filter against
//! `reference/kestrel_reference/fixtures/tradability_filter.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_tradability_filter.txt");
const CHECKPOINTS: [usize; 8] = [200, 333, 450, 600, 777, 900, 1200, 1499];
const EVENTS: [&str; 11] = [
    "no_trade",
    "compression",
    "bull_ready",
    "bear_ready",
    "bull_breakout",
    "bear_breakout",
    "bull_trend",
    "bear_trend",
    "aftermath",
    "bull_exhaustion",
    "bear_exhaustion",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn mte_matches_the_reference_implementation() {
    let mut ind = build_checked("tradability_filter", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    for (t, bar) in common::lcg_series(1500, 11).iter().enumerate() {
        let out = ind.on_bar(bar);
        if out.is_some() && first.is_none() {
            first = Some(t);
        }
        for a in ind.alerts() {
            assert!(
                EVENTS.contains(&a.kind.as_str()),
                "unknown alert {}",
                a.kind
            );
            events.entry(a.kind.clone()).or_default().push(t);
        }
        if let (Some(out), true) = (out.as_ref(), CHECKPOINTS.contains(&t)) {
            let p = format!("cp{t}");
            common::assert_close(
                out.value,
                expected(&format!("{p}_tradability")),
                tol,
                "tradability",
            );
            common::assert_close(
                out.secondary.unwrap(),
                expected(&format!("{p}_direction")),
                tol,
                "direction",
            );
            for (key, name) in [
                ("balance", "balance"),
                ("compression", "compression"),
                ("state", "state_code"),
                ("candidate", "candidate_code"),
            ] {
                common::assert_close(
                    out.extra[name],
                    expected(&format!("{p}_{key}")),
                    tol,
                    &format!("{p} {key}"),
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
            "{kind}: {got:?}"
        );
        for (j, b) in got.iter().take(6).enumerate() {
            assert_eq!(
                *b,
                expected(&format!("ev_{kind}_{j}")) as usize,
                "{kind} #{j}"
            );
        }
    }
}
