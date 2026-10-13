//! Compression Zones against `reference/kestrel_reference/fixtures/compression_zones.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_compression_zones.txt");
const CHECKPOINTS: [usize; 8] = [100, 160, 328, 449, 587, 743, 1021, 1499];
const EVENTS: [&str; 2] = ["bull_breakout", "bear_breakout"];
const EXTRA: [&str; 7] = [
    "range_atr",
    "in_zone",
    "upper_touches",
    "lower_touches",
    "read",
    "armed",
    "hist_reuse",
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
fn compression_zones_match_the_reference_implementation() {
    let mut ind = build_checked("compression_zones", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    let mut zones = 0;
    let mut was_in_zone = false;
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
        let in_zone = out.as_ref().is_some_and(|o| o.extra["in_zone"] == 1.0);
        if in_zone && !was_in_zone && t > 0 {
            zones += 1;
        }
        was_in_zone = in_zone;
        if let (Some(out), true) = (out.as_ref(), CHECKPOINTS.contains(&t)) {
            let p = format!("cp{t}");
            common::assert_close(out.value, expected(&format!("{p}_score")), tol, "score");
            for k in EXTRA {
                common::assert_close(
                    out.extra[k],
                    expected(&format!("{p}_{k}")),
                    tol,
                    &format!("{p} {k}"),
                );
            }
            for k in ["zone_high", "zone_low"] {
                let key = format!("{p}_{k}");
                if has(&key) {
                    common::assert_close(out.extra[k], expected(&key), tol, &key);
                } else {
                    assert!(!out.extra.contains_key(k), "{key}: no zone expected");
                }
            }
        }
    }
    assert_eq!(first, Some(expected("first_reported") as usize));
    assert_eq!(zones, expected("zones_started") as usize);
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
