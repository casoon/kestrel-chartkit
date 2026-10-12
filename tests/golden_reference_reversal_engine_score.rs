//! Reversal Engine Score against `reference/kestrel_reference/fixtures/reversal_engine_score.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_reversal_engine_score.txt");
const CHECKPOINTS: [usize; 5] = [100, 333, 517, 750, 899];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

/// The fixture's series, as documented in its header.
fn series() -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..900 {
        let x = i as f64;
        let drift = if i < 300 {
            0.08
        } else if i < 600 {
            -0.10
        } else {
            0.03
        };
        let close = prev + drift + 0.9 * (x / 5.0).sin() + 0.4 * (x / 2.2).cos();
        let open = prev + 0.1 * (x / 3.3).sin();
        let mut high = open.max(close) + 0.3 + 0.2 * (x / 7.0).sin().abs();
        let mut low = open.min(close) - 0.3 - 0.2 * (x / 4.0).cos().abs();
        if i % 23 == 11 {
            low -= 3.0;
        }
        if i % 29 == 17 {
            high += 3.0;
        }
        bars.push(Bar::new(i as i64 * 900, open, high, low, close, 0.0));
        prev = close;
    }
    bars
}

#[test]
fn reversal_engine_matches_the_reference_implementation() {
    let mut ind = build_checked("reversal_engine_score", &HashMap::new()).unwrap();
    let tol = expected("res_tolerance");
    let mut first = None;
    let mut signals: HashMap<&str, Vec<usize>> = HashMap::new();
    for (t, bar) in series().iter().enumerate() {
        let out = ind.on_bar(bar);
        let Some(out) = out else { continue };
        first.get_or_insert(t);
        let alerts = ind.alerts();
        let both = alerts.len() == 2;
        for a in alerts {
            let kind = match a.kind.as_str() {
                "bull_sweep_reversal" => "long",
                "bear_sweep_reversal" => "short",
                k => panic!("unknown alert {k}"),
            };
            let list = signals.entry(kind).or_default();
            let j = list.len();
            list.push(t);
            if j < 8 {
                let pre = if both {
                    format!("{kind}_")
                } else {
                    String::new()
                };
                let p = format!("sig_{kind}{j}");
                common::assert_close(
                    out.extra[&format!("{kind}_quality")],
                    expected(&format!("{p}_quality")),
                    tol,
                    &p,
                );
                for k in ["entry", "stop", "take"] {
                    common::assert_close(
                        out.extra[&format!("{pre}{k}")],
                        expected(&format!("{p}_{k}")),
                        tol,
                        &format!("{p} {k}"),
                    );
                }
            }
        }
        if CHECKPOINTS.contains(&t) {
            let p = format!("cp{t}");
            for k in [
                "long_quality",
                "short_quality",
                "rsi",
                "atr",
                "atr_base",
                "swing_low",
                "swing_high",
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
    for kind in ["long", "short"] {
        let n = expected(&format!("sig_{kind}_n")) as usize;
        let want: Vec<usize> = (0..n)
            .map(|j| expected(&format!("sig_{kind}{j}_bar")) as usize)
            .collect();
        assert_eq!(
            signals.get(kind).cloned().unwrap_or_default(),
            want,
            "{kind}"
        );
    }
}
