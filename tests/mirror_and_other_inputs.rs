//! Two acceptance checks the per-family contracts do not cover.
//!
//! **Mirror:** a series reflected about a level `C` (`x' = 2C - x`, high and low swap) is the same
//! market seen from the other side. An entry has to answer with the reflected value (price-level
//! outputs), the negated value (signed distances), the complement (ranks and positions) or the same
//! value (shape measures) — any other answer means long and short are treated differently.
//!
//! **Pair and transform inputs:** the robustness and reset cases `robustness_fuzzing.rs` runs for
//! bar indicators, for the entries that take two series or turn bars into candles.

mod common;

use std::collections::HashMap;

use common::*;
use kestrel_chartkit::{build_checked, build_pair_checked, build_transform_checked, Bar};

const LEVEL: f64 = 400.0;

fn mirror(bars: &[Bar]) -> Vec<Bar> {
    bars.iter()
        .map(|b| {
            Bar::new(
                b.timestamp,
                2.0 * LEVEL - b.open,
                2.0 * LEVEL - b.low,
                2.0 * LEVEL - b.high,
                2.0 * LEVEL - b.close,
                b.volume,
            )
        })
        .collect()
}

/// How a value of the original maps onto the value of the mirrored series.
#[derive(Clone, Copy)]
enum Answer {
    Reflected,
    Negated,
    Complement(f64),
    Same,
}

impl Answer {
    fn expected(self, original: f64) -> f64 {
        match self {
            Answer::Reflected => 2.0 * LEVEL - original,
            Answer::Negated => -original,
            Answer::Complement(total) => total - original,
            Answer::Same => original,
        }
    }
}

fn series() -> Vec<(&'static str, Vec<Bar>)> {
    vec![("wave", wave_series(600)), ("lcg", lcg_series(600, 7))]
}

fn assert_mirrored(
    name: &str,
    field: &str,
    answer: Answer,
    a: Option<f64>,
    b: Option<f64>,
    at: String,
) {
    match (a, b) {
        (None, None) => {}
        (Some(x), Some(y)) => {
            let want = answer.expected(x);
            assert!(
                (y - want).abs() <= 1e-9 * want.abs().max(1.0),
                "{name}.{field} {at}: original {x}, mirrored {y}, expected {want}"
            );
        }
        _ => panic!("{name}.{field} {at}: output on one side only ({a:?} / {b:?})"),
    }
}

fn check_bars_entry(name: &str, params: &[(&str, f64)], fields: &[(&str, Answer)]) {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    for (label, bars) in series() {
        let mirrored = mirror(&bars);
        let mut a = build_checked(name, &params).unwrap();
        let mut b = build_checked(name, &params).unwrap();
        for (i, (x, y)) in bars.iter().zip(&mirrored).enumerate() {
            let (oa, ob) = (a.on_bar(x), b.on_bar(y));
            for &(field, answer) in fields {
                let pick = |o: &Option<kestrel_chartkit::IndicatorOutput>| {
                    o.as_ref().and_then(|o| {
                        if field == "value" {
                            Some(o.value)
                        } else {
                            o.extra.get(field).copied()
                        }
                    })
                };
                assert_mirrored(
                    name,
                    field,
                    answer,
                    pick(&oa),
                    pick(&ob),
                    format!("{label}@{i}"),
                );
            }
        }
    }
}

#[test]
fn smoothers_answer_with_the_reflected_level() {
    for name in [
        "rma",
        "alma",
        "jma",
        "super_smoother",
        "ultimate_smoother",
        "mama",
    ] {
        check_bars_entry(name, &[], &[("value", Answer::Reflected)]);
    }
    for (phase, power) in [(-100.0, 1.0), (100.0, 3.0)] {
        check_bars_entry(
            "jma",
            &[("phase", phase), ("power", power)],
            &[("value", Answer::Reflected)],
        );
    }
    check_bars_entry(
        "alma",
        &[("offset", 0.3), ("sigma", 2.0)],
        &[("value", Answer::Reflected)],
    );
}

#[test]
fn oscillators_answer_with_the_negation_or_the_complement() {
    for name in [
        "roofing_filter",
        "reflex",
        "trendflex",
        "cyber_cycle",
        "correlation_trend",
    ] {
        check_bars_entry(name, &[], &[("value", Answer::Negated)]);
    }
    check_bars_entry("cyber_cycle", &[], &[("trigger", Answer::Negated)]);
    check_bars_entry("laguerre_rsi", &[], &[("value", Answer::Complement(1.0))]);
}

#[test]
fn shape_measures_read_the_same_on_both_sides() {
    check_bars_entry("r_squared", &[], &[("value", Answer::Same)]);
    check_bars_entry("fractal_dimension", &[], &[("value", Answer::Same)]);
    check_bars_entry(
        "fractal_dimension",
        &[("method", 1.0)],
        &[("value", Answer::Same)],
    );
    check_bars_entry(
        "cycle_length",
        &[],
        &[
            ("value", Answer::Same),
            ("bars_since_crossing", Answer::Same),
        ],
    );
    check_bars_entry("opening_range", &[], &[("value", Answer::Same)]);
}

#[test]
fn positions_answer_with_the_negation_or_the_complement() {
    // No two closes of these series are equal, so the rank has no ties to break.
    check_bars_entry("percent_rank", &[], &[("value", Answer::Complement(100.0))]);
    check_bars_entry(
        "price_location",
        &[],
        &[
            ("value", Answer::Negated),
            ("ema", Answer::Reflected),
            ("sma", Answer::Reflected),
            ("range_position", Answer::Complement(1.0)),
            ("atr", Answer::Same),
        ],
    );
    check_bars_entry("period_midpoint", &[], &[("value", Answer::Reflected)]);
}

#[test]
fn heikin_ashi_candles_mirror_with_high_and_low_swapped() {
    for (label, bars) in series() {
        let mirrored = mirror(&bars);
        let mut a = build_transform_checked("heikin_ashi", &HashMap::new()).unwrap();
        let mut b = build_transform_checked("heikin_ashi", &HashMap::new()).unwrap();
        for (i, (x, y)) in bars.iter().zip(&mirrored).enumerate() {
            let (ca, cb) = (a.update(x), b.update(y));
            let at = || format!("{label}@{i}");
            // The mirrored high is the reflection of the original low, and the other way round.
            for (field, original, mirrored) in [
                ("open", ca.open, cb.open),
                ("close", ca.close, cb.close),
                ("high", ca.low, cb.high),
                ("low", ca.high, cb.low),
            ] {
                assert_mirrored(
                    "heikin_ashi",
                    field,
                    Answer::Reflected,
                    Some(original),
                    Some(mirrored),
                    at(),
                );
            }
        }
    }
}

#[test]
fn pair_spread_answers_with_the_negated_z_score() {
    let own = wave_series(400);
    let reference = lcg_series(400, 11);
    let (own_m, reference_m) = (mirror(&own), mirror(&reference));
    let mut a = build_pair_checked("pair_spread", &HashMap::new()).unwrap();
    let mut b = build_pair_checked("pair_spread", &HashMap::new()).unwrap();
    for i in 0..own.len() {
        let (oa, ob) = (
            a.on_bars(&own[i], &reference[i]),
            b.on_bars(&own_m[i], &reference_m[i]),
        );
        let value = |o: &Option<kestrel_chartkit::IndicatorOutput>| o.as_ref().map(|o| o.value);
        let extra = |o: &Option<kestrel_chartkit::IndicatorOutput>, k: &str| {
            o.as_ref().and_then(|o| o.extra.get(k).copied())
        };
        let at = format!("@{i}");
        assert_mirrored(
            "pair_spread",
            "value",
            Answer::Negated,
            value(&oa),
            value(&ob),
            at.clone(),
        );
        assert_mirrored(
            "pair_spread",
            "hedge_ratio",
            Answer::Same,
            extra(&oa, "hedge_ratio"),
            extra(&ob, "hedge_ratio"),
            at,
        );
    }
}

fn pair_inputs() -> Vec<(&'static str, Vec<Bar>, Vec<Bar>)> {
    vec![
        (
            "short",
            generate_flat_spread_bars(3, 100.0, 1.0, 1000.0),
            generate_flat_spread_bars(3, 50.0, 1.0, 1000.0),
        ),
        (
            "flat",
            generate_flat_bars(200, 100.0, 1000.0),
            generate_flat_bars(200, 50.0, 1000.0),
        ),
        ("nan", generate_nan_bars(200), wave_series(200)),
        ("zero", generate_flat_bars(200, 0.0, 0.0), wave_series(200)),
        ("wave", wave_series(300), lcg_series(300, 3)),
    ]
}

#[test]
fn pair_entries_survive_degenerate_input_and_reset_cleanly() {
    for name in [
        "rolling_beta",
        "pair_spread",
        "correlation",
        "relative_strength",
    ] {
        for (label, own, reference) in pair_inputs() {
            let mut indicator = build_pair_checked(name, &HashMap::new()).unwrap();
            let run = |indicator: &mut Box<dyn kestrel_chartkit::PairIndicator>| -> Vec<Option<Vec<u64>>> {
                own.iter()
                    .zip(&reference)
                    .map(|(a, b)| {
                        indicator.on_bars(a, b).map(|o| {
                            let mut bits = vec![o.value.to_bits()];
                            let mut keys: Vec<_> = o.extra.keys().cloned().collect();
                            keys.sort();
                            bits.extend(keys.iter().map(|k| o.extra[k].to_bits()));
                            bits
                        })
                    })
                    .collect()
            };
            let first = run(&mut indicator);
            // NaN bars break the input contract (`Bar::validate`): only no panic and a clean
            // reset are required there, as for the bar indicators in `robustness_fuzzing.rs`.
            if label == "nan" {
                indicator.reset();
                assert_eq!(
                    run(&mut indicator),
                    first,
                    "{name} {label}: reset does not start over"
                );
                continue;
            }
            for (i, bits) in first.iter().enumerate() {
                if let Some(bits) = bits {
                    assert!(
                        bits.iter().all(|b| f64::from_bits(*b).is_finite()),
                        "{name} {label}@{i}: non-finite output"
                    );
                }
            }
            indicator.reset();
            assert_eq!(
                run(&mut indicator),
                first,
                "{name} {label}: reset does not start over"
            );
        }
    }
}

#[test]
fn heikin_ashi_survives_degenerate_input_and_resets_cleanly() {
    for (label, bars) in [
        ("short", generate_flat_spread_bars(1, 100.0, 1.0, 1000.0)),
        ("flat", generate_flat_bars(50, 100.0, 1000.0)),
        ("nan", generate_nan_bars(50)),
        ("wave", wave_series(200)),
    ] {
        let mut transform = build_transform_checked("heikin_ashi", &HashMap::new()).unwrap();
        let run = |t: &mut Box<dyn kestrel_chartkit::BarTransform>| -> Vec<[u64; 4]> {
            bars.iter()
                .map(|b| {
                    let c = t.update(b);
                    [
                        c.open.to_bits(),
                        c.high.to_bits(),
                        c.low.to_bits(),
                        c.close.to_bits(),
                    ]
                })
                .collect()
        };
        let first = run(&mut transform);
        if label != "nan" {
            for c in &first {
                let [o, h, l, cl] = c.map(f64::from_bits);
                assert!(
                    l <= o.min(cl) && h >= o.max(cl),
                    "heikin_ashi {label}: low/high do not bound the body"
                );
            }
        }
        transform.reset();
        assert_eq!(
            run(&mut transform),
            first,
            "heikin_ashi {label}: reset does not start over"
        );
    }
}
