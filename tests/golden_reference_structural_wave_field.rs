//! Structural Wave Field against `reference/kestrel_reference/fixtures/structural_wave_field.py`.

mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_structural_wave_field.txt");
const CHECKPOINTS: [usize; 9] = [200, 333, 480, 615, 777, 902, 1050, 1222, 1399];
const EVENTS: [&str; 19] = [
    "bull_flow_initiated",
    "bear_flow_initiated",
    "flow_transition",
    "bull_flow_flip",
    "bear_flow_flip",
    "bull_expansion",
    "bear_expansion",
    "correction_start",
    "compression",
    "bull_early_resync",
    "bear_early_resync",
    "bull_resync",
    "bear_resync",
    "late_expansion",
    "exhaustion",
    "failed_expansion",
    "bull_dominant_expansion",
    "bear_dominant_expansion",
    "weakening_waves",
];
const KEYS: [&str; 20] = [
    "direction",
    "strength",
    "confidence",
    "internal_wave",
    "wave_quality",
    "counter_pressure",
    "integrity",
    "retracement",
    "maturity",
    "compression",
    "resync_quality",
    "resync_stage",
    "exhaustion",
    "deterioration",
    "wave_decay",
    "impulse_context",
    "correction_context",
    "wave_context",
    "phase",
    "phase_confidence",
];

/// The fixture's series, as documented in its header.
fn series(prefix: &str) -> Vec<Bar> {
    let (amp, period, wiggle, with_volume) = if prefix == "v" {
        (0.8, 110.0, 11.0, true)
    } else {
        (0.45, 60.0, 6.3, false)
    };
    let mut bars = Vec::new();
    let mut prev = 100.0f64;
    for i in 0..1400usize {
        let x = i as f64;
        let drift = amp * (x / period).sin() + 0.15 * (x / 37.0).sin();
        let mut close = prev + drift + 0.7 * (x / wiggle).sin() + 0.35 * (x / 2.1).cos();
        if i % 300 == 250 {
            close = if (i / 300) % 2 == 0 {
                prev - 25.0
            } else {
                prev + 25.0
            };
        }
        let open = prev + 0.25 * (x / 1.8).sin();
        let high = open.max(close) + 0.2 + 0.25 * (x / 3.7).sin().abs();
        let low = open.min(close) - 0.2 - 0.25 * (x / 4.3).cos().abs();
        let vol = if with_volume {
            1000.0 + 700.0 * (x / 7.0).sin().powi(2) + 300.0 * (x / 19.0).cos()
        } else {
            0.0
        };
        bars.push(Bar::new(i as i64 * 3600, open, high, low, close, vol));
        prev = close;
    }
    bars
}

fn check(prefix: &str) {
    let mut ind = build_checked("structural_wave_field", &HashMap::new()).unwrap();
    let val = |k: &str| common::golden_value(GOLDEN, &format!("{prefix}_{k}"));
    let tol = common::golden_value(GOLDEN, "swf_tolerance");
    let mut first = None;
    let mut events: HashMap<&str, Vec<usize>> = HashMap::new();
    for (t, bar) in series(prefix).iter().enumerate() {
        let out = ind.on_bar(bar);
        for a in ind.alerts() {
            let kind = EVENTS
                .iter()
                .find(|k| **k == a.kind)
                .expect("unknown alert");
            events.entry(kind).or_default().push(t);
        }
        let Some(out) = out else { continue };
        first.get_or_insert(t);
        if CHECKPOINTS.contains(&t) {
            let p = format!("cp{t}");
            common::assert_close(out.value, val(&format!("{p}_drive")), tol, &p);
            common::assert_close(
                out.secondary.unwrap(),
                val(&format!("{p}_aligned")),
                tol,
                &p,
            );
            for k in KEYS {
                common::assert_close(
                    out.extra[k],
                    val(&format!("{p}_{k}")),
                    tol,
                    &format!("{prefix} {p} {k}"),
                );
            }
        }
    }
    assert_eq!(first, Some(val("first") as usize), "{prefix} first");
    for kind in EVENTS {
        let got = events.get(kind).cloned().unwrap_or_default();
        assert_eq!(
            got.len(),
            val(&format!("ev_{kind}_n")) as usize,
            "{prefix} {kind}: {got:?}"
        );
        for (j, b) in got.iter().take(8).enumerate() {
            assert_eq!(
                *b,
                val(&format!("ev_{kind}_{j}")) as usize,
                "{prefix} {kind} #{j}"
            );
        }
    }
}

#[test]
fn structural_wave_field_matches_the_reference_with_volume() {
    check("v");
}

#[test]
fn structural_wave_field_matches_the_reference_without_volume() {
    check("nv");
}
