//! Reaction Speed against `reference/kestrel_reference/fixtures/reaction_speed.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_reaction_speed.txt");
const CHECKPOINTS: [usize; 8] = [100, 230, 333, 450, 617, 690, 780, 899];

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

#[test]
fn reaction_speed_matches_the_reference_implementation() {
    let mut ind = build_checked("reaction_speed", &HashMap::new()).unwrap();
    let tol = expected("tolerance");
    let mut first = None;
    let mut finals = Vec::new();
    for (t, bar) in common::wave_series(900).iter().enumerate() {
        let out = ind.on_bar(bar);
        assert!(ind.alerts().is_empty());
        let Some(out) = out else { continue };
        first.get_or_insert(t);
        if let Some(f) = out.extra.get("final_score") {
            finals.push((t, out.extra["event_type"], out.extra["event_dir"], *f));
        }
        if CHECKPOINTS.contains(&t) {
            let p = format!("cp{t}");
            common::assert_close(
                out.value,
                expected(&format!("{p}_activity")),
                tol,
                "activity",
            );
            if has(&format!("{p}_reaction")) {
                for k in ["reaction", "speed", "validation", "event_dir"] {
                    common::assert_close(
                        out.extra[k],
                        expected(&format!("{p}_{k}")),
                        tol,
                        &format!("{p} {k}"),
                    );
                }
            } else {
                assert!(
                    !out.extra.contains_key("reaction"),
                    "{p}: no reaction expected"
                );
            }
        }
    }
    assert_eq!(first, Some(expected("first_reported") as usize));
    assert_eq!(finals.len(), expected("final_n") as usize);
    for (j, (b, typ, dir, score)) in finals.iter().take(10).enumerate() {
        assert_eq!(
            *b,
            expected(&format!("final_{j}_bar")) as usize,
            "final #{j} bar"
        );
        assert_eq!(
            *typ,
            expected(&format!("final_{j}_type")),
            "final #{j} type"
        );
        assert_eq!(*dir, expected(&format!("final_{j}_dir")), "final #{j} dir");
        common::assert_close(
            *score,
            expected(&format!("final_{j}_score")),
            tol,
            &format!("final #{j} score"),
        );
    }
}
