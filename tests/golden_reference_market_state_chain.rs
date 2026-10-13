//! Market State Chain against `reference/kestrel_reference/fixtures/market_state_chain.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_market_state_chain.txt");
const CHECKPOINTS: [usize; 8] = [150, 400, 640, 900, 1111, 1400, 1700, 1999];
const EVENTS: [&str; 8] = [
    "bull_reversal",
    "bull_breakout",
    "bull_continuation",
    "bear_reversal",
    "bear_breakout",
    "bear_continuation",
    "regime_change",
    "state_extended",
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
    let mut ind = build_checked("market_state_chain", params).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    for (t, bar) in common::lcg_series(2000, 4).iter().enumerate() {
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
            common::assert_close(out.value, expected(&format!("{p}_net")), tol, "net");
            for (key, name) in [
                ("state", "state_code"),
                ("p_bull", "p_bull"),
                ("p_bear", "p_bear"),
                ("p_neutral", "p_neutral"),
                ("reliability", "reliability"),
                ("bias", "bias"),
                ("exits", "exits"),
                ("persistence", "persistence"),
                ("dwell_ratio", "dwell_ratio"),
            ] {
                let k = format!("{p}_{key}");
                if has(&k) {
                    common::assert_close(out.extra[name], expected(&k), tol, &k);
                } else {
                    assert!(!out.extra.contains_key(name), "{k} not expected");
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
fn market_state_chain_matches_the_reference_implementation() {
    run(&HashMap::new(), "ev", true);
}

#[test]
fn relaxed_thresholds_reach_breakout_edges() {
    let params = HashMap::from([("conf_th".to_string(), 45.0), ("rel_th".to_string(), 3.0)]);
    run(&params, "rx_ev", false);
}
