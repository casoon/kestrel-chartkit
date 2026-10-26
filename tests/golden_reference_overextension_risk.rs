//! Overextension Risk against `reference/kestrel_reference/fixtures/overextension_risk.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_overextension_risk.txt");
const CHECKPOINTS: [usize; 8] = [120, 230, 333, 450, 617, 690, 780, 899];
const EVENTS: [&str; 6] = [
    "bear_do_not_chase",
    "bull_do_not_chase",
    "bear_trend_exhaustion",
    "bull_trend_exhaustion",
    "bear_range_fade",
    "bull_range_fade",
];
const EXTRA: [&str; 10] = [
    "stretch_top",
    "stretch_bot",
    "exhaust_top",
    "exhaust_bot",
    "react_top",
    "react_bot",
    "vol_factor",
    "top_thresh",
    "bot_thresh",
    "regime",
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn overextension_risk_matches_the_reference_implementation() {
    let mut ind = build_checked("overextension_risk", &HashMap::new()).unwrap();
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
            common::assert_close(out.value, expected(&format!("{p}_top")), tol, "top");
            common::assert_close(
                out.secondary.unwrap(),
                expected(&format!("{p}_bot")),
                tol,
                "bot",
            );
            for k in EXTRA {
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
