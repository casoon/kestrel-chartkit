//! Flow Exhaustion against `reference/kestrel_reference/fixtures/flow_exhaustion.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_flow_exhaustion.txt");
const CHECKPOINTS: [usize; 7] = [150, 260, 333, 450, 617, 780, 899];
const EVENTS: [&str; 4] = [
    "bull_exhaustion",
    "bear_exhaustion",
    "bull_divergence",
    "bear_divergence",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

#[test]
fn flow_exhaustion_matches_the_reference_implementation() {
    let mut ind = build_checked("flow_exhaustion", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    let mut scores = Vec::new();
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
            if let Some(s) = out.extra.get("divergence_score") {
                scores.push(*s);
            }
            if CHECKPOINTS.contains(&t) {
                let p = format!("cp{t}");
                common::assert_close(out.value, expected(&format!("{p}_mfi")), tol, "mfi");
                common::assert_close(
                    out.secondary.unwrap(),
                    expected(&format!("{p}_k")),
                    tol,
                    "k",
                );
                if has(&format!("{p}_upper")) {
                    for k in ["upper", "lower"] {
                        common::assert_close(out.extra[k], expected(&format!("{p}_{k}")), tol, k);
                    }
                } else {
                    assert!(!out.extra.contains_key("upper"), "{p}: zones too early");
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
    for (j, s) in scores.iter().take(8).enumerate() {
        assert_eq!(*s, expected(&format!("score_{j}")), "score #{j}");
    }
}
