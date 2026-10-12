//! Oscillator Cycle Statistics against
//! `reference/kestrel_reference/fixtures/oscillator_cycle_statistics.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_oscillator_cycle_statistics.txt");
const CHECKPOINTS: [usize; 8] = [300, 1000, 1700, 2222, 2900, 3333, 3700, 3999];
const EVENTS: [&str; 5] = [
    "cycle_success",
    "cycle_failure",
    "cycle_timeout",
    "bull_cycle_opportunity",
    "bear_cycle_opportunity",
];
const STATES: [&str; 8] = [
    "idle",
    "low_data",
    "stalled",
    "uncertain",
    "late",
    "opportunity",
    "watch",
    "wait",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

fn run(params: &HashMap<String, f64>, prefix: &str, check_values: bool) {
    let mut ind = build_checked("oscillator_cycle_statistics", params).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    for (t, bar) in common::lcg_series(4000, 9).iter().enumerate() {
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
        if let (Some(out), true) = (out.as_ref(), check_values && CHECKPOINTS.contains(&t)) {
            let p = format!("cp{t}");
            common::assert_close(out.value, expected(&format!("{p}_osc")), tol, "osc");
            common::assert_close(
                out.extra["cycle_dir"],
                expected(&format!("{p}_dir")),
                tol,
                "dir",
            );
            let state = out.state.as_deref().unwrap();
            assert_eq!(
                STATES[expected(&format!("{p}_state")) as usize],
                state,
                "{p} state"
            );
            for k in ["progress", "conditional", "cond_n", "edge"] {
                let key = format!("{p}_{k}");
                if has(&key) {
                    common::assert_close(out.extra[k], expected(&key), tol, &key);
                } else {
                    assert!(!out.extra.contains_key(k), "{key} not expected");
                }
            }
        }
    }
    assert_eq!(first, Some(expected("first_reported") as usize));
    for kind in EVENTS {
        let got = events.get(kind).cloned().unwrap_or_default();
        assert_eq!(
            got.len(),
            expected(&format!("{prefix}_{kind}_n")) as usize,
            "{prefix} {kind}: {got:?}"
        );
        for (j, b) in got.iter().take(6).enumerate() {
            assert_eq!(
                *b,
                expected(&format!("{prefix}_{kind}_{j}")) as usize,
                "{prefix} {kind} #{j}"
            );
        }
    }
}

#[test]
fn cycle_statistics_match_the_reference_implementation() {
    run(&HashMap::new(), "ev", true);
}

#[test]
fn relaxed_gates_reach_opportunities() {
    let params = HashMap::from([
        ("min_samples".to_string(), 5.0),
        ("max_ci".to_string(), 60.0),
    ]);
    run(&params, "rx_ev", false);
}
