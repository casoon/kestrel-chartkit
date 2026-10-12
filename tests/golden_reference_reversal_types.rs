//! Reversal type research labels against `reference/kestrel_reference/fixtures/reversal_types.py`.

mod common;

use kestrel_chartkit::evaluation::reversal_types::{
    classify_reversals, ReversalKind, ReversalTypeConfig,
};

const GOLDEN: &str = include_str!("fixtures/golden_reversal_types.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn code(kind: ReversalKind) -> f64 {
    match kind {
        ReversalKind::Snapback => 1.0,
        ReversalKind::Grind => 2.0,
        ReversalKind::Fake => 3.0,
        ReversalKind::Chop => 4.0,
    }
}

#[test]
fn reversal_labels_match_the_reference_implementation() {
    let bars = common::lcg_series(1500, 6);
    let labels = classify_reversals(&bars, &ReversalTypeConfig::default());
    let tol = expected("tolerance");
    assert_eq!(labels.len(), expected("label_n") as usize);
    for k in 1..=4 {
        let n = labels.iter().filter(|l| code(l.kind) == k as f64).count();
        assert_eq!(n, expected(&format!("kind_{k}_n")) as usize, "kind {k}");
    }
    for (j, l) in labels.iter().take(25).enumerate() {
        let p = format!("lab_{j}");
        assert_eq!(
            l.pivot_index,
            expected(&format!("{p}_pivot")) as usize,
            "{p} pivot"
        );
        assert_eq!(
            l.known_index,
            expected(&format!("{p}_known")) as usize,
            "{p} known"
        );
        assert_eq!(
            f64::from(l.side),
            expected(&format!("{p}_side")),
            "{p} side"
        );
        assert_eq!(code(l.kind), expected(&format!("{p}_kind")), "{p} kind");
        common::assert_close(l.pivot_price, expected(&format!("{p}_price")), tol, "price");
        common::assert_close(l.mfe_r, expected(&format!("{p}_mfe")), tol, "mfe");
        common::assert_close(l.mae_r, expected(&format!("{p}_mae")), tol, "mae");
        common::assert_close(l.quality, expected(&format!("{p}_quality")), tol, "quality");
        for (flag, name) in [
            (l.divergence, "div"),
            (l.extreme, "ext"),
            (l.trend_against, "ta"),
            (l.chop, "chop"),
        ] {
            assert_eq!(
                f64::from(u8::from(flag)),
                expected(&format!("{p}_{name}")),
                "{p} {name}"
            );
        }
    }
    // A label is never known before its evaluation window has closed.
    for l in &labels {
        assert_eq!(l.known_index, l.pivot_index + 5 + 24);
    }
}
