//! Structure Break Risk against `reference/kestrel_reference/fixtures/structure_break_risk.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_structure_break_risk.txt");
const CHECKPOINTS: [usize; 7] = [150, 260, 400, 470, 600, 700, 899];
const EVENTS: [&str; 8] = [
    "bear_break_pressure",
    "bull_break_pressure",
    "bear_break_critical",
    "bull_break_critical",
    "bear_structure_break",
    "bull_structure_break",
    "bear_divergence",
    "bull_divergence",
];
const STATES: [&str; 6] = [
    "no_trend",
    "structure_broken",
    "break_pressure",
    "critical",
    "watch",
    "quiet",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// The fixture's series, as documented in its header (the MARE series).
fn series() -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..900 {
        let x = i as f64;
        let mut drift = if i < 220 {
            0.18
        } else if i < 380 {
            -0.02
        } else if i < 470 {
            -0.30
        } else {
            0.12
        };
        let mut amp = 1.0;
        if (560..650).contains(&i) {
            drift = 0.0;
            amp = 0.08;
        } else if (650..690).contains(&i) {
            drift = 0.45;
        }
        let base = prev + drift + amp * (0.9 * (x / 3.1).sin() + 0.6 * (x / 11.0).sin());
        let close = base + amp * 0.35 * (x / 1.7).cos();
        let open = prev + 0.2 * (x / 2.3).sin();
        let high = open.max(close) + 0.4 + 0.3 * (x / 4.7).sin().abs();
        let low = open.min(close) - 0.4 - 0.3 * (x / 5.3).cos().abs();
        bars.push(Bar::new(i as i64 * 3600, open, high, low, close, 0.0));
        prev = close;
    }
    bars
}

#[test]
fn structure_break_risk_matches_the_reference_implementation() {
    let mut ind = build_checked("structure_break_risk", &HashMap::new()).unwrap();
    let tol = expected("sbr_tolerance");
    let mut first = None;
    let mut events: HashMap<&str, Vec<usize>> = HashMap::new();
    for (t, bar) in series().iter().enumerate() {
        let out = ind.on_bar(bar);
        if out.is_some() && first.is_none() {
            first = Some(t);
        }
        for a in ind.alerts() {
            let kind = EVENTS
                .iter()
                .find(|k| **k == a.kind)
                .expect("unknown alert");
            events.entry(kind).or_default().push(t);
        }
        if let (Some(out), true) = (out.as_ref(), CHECKPOINTS.contains(&t)) {
            let p = format!("cp{t}");
            common::assert_close(out.value, expected(&format!("{p}_risk")), tol, &p);
            for (k, e) in [
                ("risk_raw", "risk_raw"),
                ("trend_dir", "trend"),
                ("near", "near"),
                ("sfp", "sfp"),
                ("structure", "structure"),
                ("divergence", "divergence"),
                ("rsi", "rsi"),
                ("ema", "ema"),
                ("break_level", "break_level"),
                ("distance_atr", "distance"),
            ] {
                common::assert_close(
                    out.extra[k],
                    expected(&format!("{p}_{e}")),
                    tol,
                    &format!("{p} {k}"),
                );
            }
            let state = STATES[expected(&format!("{p}_state")) as usize];
            assert_eq!(out.state.as_deref(), Some(state), "{p} state");
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
        for (j, b) in got.iter().take(8).enumerate() {
            assert_eq!(
                *b,
                expected(&format!("ev_{kind}_{j}")) as usize,
                "{kind} #{j}"
            );
        }
    }
}
