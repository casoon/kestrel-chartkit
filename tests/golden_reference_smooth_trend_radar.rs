//! Smooth Trend Radar against `reference/kestrel_reference/fixtures/smooth_trend_radar.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_smooth_trend_radar.txt");
const CHECKPOINTS: [usize; 7] = [200, 333, 450, 617, 700, 780, 899];
const EVENTS: [&str; 6] = [
    "bull_flip",
    "bear_flip",
    "bull_rejection",
    "bear_rejection",
    "bull_rejection_strong",
    "bear_rejection_strong",
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
fn smooth_trend_radar_matches_the_reference_implementation() {
    let mut ind = build_checked("smooth_trend_radar", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
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
        if let (Some(out), true) = (out.as_ref(), CHECKPOINTS.contains(&t)) {
            let p = format!("cp{t}");
            common::assert_close(
                out.value,
                expected(&format!("{p}_baseline")),
                tol,
                "baseline",
            );
            for k in [
                "trend",
                "st_lower",
                "st_upper",
                "strength_rank",
                "distance_rank",
            ] {
                let key = format!("{p}_{k}");
                if has(&key) {
                    common::assert_close(out.extra[k], expected(&key), tol, &key);
                } else {
                    assert!(!out.extra.contains_key(k), "{key} reported too early");
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
}
