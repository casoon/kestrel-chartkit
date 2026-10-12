//! MARE against `reference/kestrel_reference/fixtures/market_average_relationship.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_market_average_relationship.txt");
const CHECKPOINTS: [usize; 6] = [200, 333, 450, 617, 780, 899];
const EVENTS: [&str; 14] = [
    "bull_pullback",
    "bear_pullback",
    "bull_momentum_release",
    "bear_momentum_release",
    "do_not_chase_up",
    "do_not_chase_down",
    "bear_exhaustion",
    "bull_exhaustion",
    "bear_relationship_break",
    "bull_relationship_break",
    "bull_divergence",
    "bear_divergence",
    "bull_turn",
    "bear_turn",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// The fixture's series, as documented in its header.
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
fn mare_matches_the_reference_implementation() {
    let mut ind = build_checked("market_average_relationship", &HashMap::new()).unwrap();
    let tol = expected("mare_tolerance");
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
            common::assert_close(
                out.value,
                expected(&format!("{p}_relationship")),
                tol,
                &format!("{p} relationship"),
            );
            common::assert_close(
                out.secondary.unwrap(),
                expected(&format!("{p}_signal")),
                tol,
                &format!("{p} signal"),
            );
            for k in [
                "trend",
                "respect",
                "extension",
                "compression",
                "acceleration",
                "exhaustion",
                "ma",
                "distance_atr",
                "respect_samples",
            ] {
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
        for (j, b) in got.iter().take(6).enumerate() {
            assert_eq!(
                *b,
                expected(&format!("ev_{kind}_{j}")) as usize,
                "{kind} #{j}"
            );
        }
    }
}
