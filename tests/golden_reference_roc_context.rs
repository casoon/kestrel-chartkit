//! ROC Context against `reference/kestrel_reference/fixtures/roc_context.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_roc_context.txt");
const CHECKPOINTS: [usize; 8] = [80, 200, 333, 450, 617, 690, 780, 899];
const EVENTS: [&str; 8] = [
    "bull_cross",
    "bull_cross_weak",
    "bear_cross",
    "bear_cross_weak",
    "bull_zero_cross",
    "bear_zero_cross",
    "bull_stall",
    "bear_stall",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn roc_context_matches_the_reference_implementation() {
    let mut ind = build_checked("roc_context", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    let mut qualities = Vec::new();
    for (t, bar) in common::wave_series(900).iter().enumerate() {
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
        if let Some(out) = out.as_ref() {
            if let Some(q) = out.extra.get("quality") {
                qualities.push(*q);
            }
            if CHECKPOINTS.contains(&t) {
                let p = format!("cp{t}");
                common::assert_close(out.value, expected(&format!("{p}_line")), tol, "line");
                common::assert_close(
                    out.secondary.unwrap(),
                    expected(&format!("{p}_signal")),
                    tol,
                    "signal",
                );
                for (key, name) in [
                    ("ctx", "context"),
                    ("sentiment", "sentiment"),
                    ("stall", "stall"),
                    ("divergence", "divergence"),
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
    for (j, q) in qualities.iter().take(8).enumerate() {
        common::assert_close(
            *q,
            expected(&format!("q_{j}")),
            tol,
            &format!("quality #{j}"),
        );
    }
}
