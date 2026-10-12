//! Compression Fractal Release against
//! `reference/kestrel_reference/fixtures/compression_fractal_release.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_compression_fractal_release.txt");
const CHECKPOINTS: [usize; 8] = [120, 300, 450, 600, 777, 900, 1200, 1499];
const EVENTS: [&str; 4] = [
    "bull_release",
    "bear_release",
    "bull_base_break",
    "bear_base_break",
];
const EXTRA: [(&str, &str); 8] = [
    ("complexity", "complexity"),
    ("fd", "fractal_dim"),
    ("eff", "efficiency"),
    ("chop", "chop"),
    ("coil", "coil"),
    ("coil_score", "coil_score"),
    ("release_dyn", "release_dyn"),
    ("trend", "trend"),
];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn cfr_matches_the_reference_implementation() {
    let mut ind = build_checked("compression_fractal_release", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut events: HashMap<String, Vec<usize>> = HashMap::new();
    for (t, bar) in common::lcg_series(1500, 3).iter().enumerate() {
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
            common::assert_close(out.value, expected(&format!("{p}_setup")), tol, "setup");
            for (key, name) in EXTRA {
                common::assert_close(
                    out.extra[name],
                    expected(&format!("{p}_{key}")),
                    tol,
                    &format!("{p} {key}"),
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
