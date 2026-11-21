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

/// Like `check_bars_entry`, for a pair of lines that swap places: the original's `field` is the
/// reflected `mirrored_field` of the mirrored series (an upper line becomes the reflected lower one).
fn check_bars_swapped_pair(name: &str, params: &[(&str, f64)], field: &str, mirrored_field: &str) {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    for (label, bars) in series() {
        let mirrored = mirror(&bars);
        let mut a = build_checked(name, &params).unwrap();
        let mut b = build_checked(name, &params).unwrap();
        for (i, (x, y)) in bars.iter().zip(&mirrored).enumerate() {
            let pick = |o: &Option<kestrel_chartkit::IndicatorOutput>, f: &str| {
                o.as_ref().and_then(|o| {
                    if f == "value" {
                        Some(o.value)
                    } else {
                        o.extra.get(f).copied()
                    }
                })
            };
            let (oa, ob) = (a.on_bar(x), b.on_bar(y));
            assert_mirrored(
                name,
                &format!("{field}<->{mirrored_field}"),
                Answer::Reflected,
                pick(&oa, field),
                pick(&ob, mirrored_field),
                format!("{label}@{i}"),
            );
        }
    }
}

#[test]
fn n2_trend_lines_answer_with_the_reflected_level() {
    for name in [
        "zlema",
        "range_filter",
        "halftrend",
        "ssl_channel",
        "nadaraya_watson",
    ] {
        check_bars_entry(name, &[], &[("value", Answer::Reflected)]);
    }
    check_bars_entry("frama", &[], &[("value", Answer::Reflected)]);
    check_bars_entry("frama", &[], &[("dimension", Answer::Same)]);
    check_bars_entry("ssl_channel", &[], &[("ssl_down", Answer::Reflected)]);
    // Short settings, so that the directions decide inside the series.
    let short = [("amplitude", 2.0), ("atr_len", 5.0)];
    check_bars_entry("halftrend", &short, &[("value", Answer::Reflected)]);
    check_bars_entry("halftrend", &short, &[("trend", Answer::Negated)]);
    check_bars_entry(
        "range_filter",
        &[("period", 5.0)],
        &[("direction", Answer::Negated)],
    );
    check_bars_entry("ssl_channel", &[], &[("hlv", Answer::Negated)]);
    check_bars_swapped_pair("range_filter", &[], "upper", "lower");
    check_bars_swapped_pair("range_filter", &[], "lower", "upper");
    check_bars_swapped_pair("halftrend", &short, "atr_high", "atr_low");
    check_bars_swapped_pair("halftrend", &short, "atr_low", "atr_high");
    check_bars_swapped_pair("nadaraya_watson", &[], "upper", "lower");
    check_bars_swapped_pair("nadaraya_watson", &[], "lower", "upper");
}

#[test]
fn n2_oscillators_answer_with_the_negation_or_the_complement() {
    check_bars_entry("squeeze_momentum", &[], &[("value", Answer::Negated)]);
    check_bars_entry("squeeze_momentum", &[], &[("squeeze", Answer::Same)]);
    check_bars_entry(
        "schaff_trend_cycle",
        &[],
        &[("value", Answer::Complement(100.0))],
    );
    check_bars_entry(
        "qqe",
        &[],
        &[
            ("value", Answer::Complement(100.0)),
            ("trail", Answer::Complement(100.0)),
        ],
    );
    // The trend is a state label: the reflected series is up where the original is down.
    for (label, bars) in series() {
        let mirrored = mirror(&bars);
        let mut a = build_checked("qqe", &HashMap::new()).unwrap();
        let mut b = build_checked("qqe", &HashMap::new()).unwrap();
        for (i, (x, y)) in bars.iter().zip(&mirrored).enumerate() {
            let (sa, sb) = (
                a.on_bar(x).and_then(|o| o.state),
                b.on_bar(y).and_then(|o| o.state),
            );
            let opposite = sa.as_deref().map(|s| match s {
                "up" => "down",
                "down" => "up",
                other => other,
            });
            assert_eq!(sb.as_deref(), opposite, "qqe state {label}@{i}");
        }
    }
    check_bars_entry(
        "relative_momentum_index",
        &[],
        &[("value", Answer::Complement(100.0))],
    );
    check_bars_entry("demarker", &[], &[("value", Answer::Complement(1.0))]);
}

/// MOST and OTT keep their distances as a percent of the level: no mirror, but scaling every
/// price by `k` scales the price outputs by `k` and leaves the direction alone.
#[test]
fn most_and_ott_are_scale_invariant() {
    let scaled = |bars: &[Bar], k: f64| -> Vec<Bar> {
        bars.iter()
            .map(|b| {
                Bar::new(
                    b.timestamp,
                    k * b.open,
                    k * b.high,
                    k * b.low,
                    k * b.close,
                    b.volume,
                )
            })
            .collect()
    };
    for (name, price_fields) in [("most", &["ema"][..]), ("ott", &["var", "stop"][..])] {
        for (label, bars) in series() {
            let big = scaled(&bars, 2.0);
            let mut a = build_checked(name, &HashMap::new()).unwrap();
            let mut b = build_checked(name, &HashMap::new()).unwrap();
            for (i, (x, y)) in bars.iter().zip(&big).enumerate() {
                let (oa, ob) = (a.on_bar(x), b.on_bar(y));
                match (oa, ob) {
                    (None, None) => {}
                    (Some(oa), Some(ob)) => {
                        let at = format!("{name} {label}@{i}");
                        assert!(
                            (2.0 * oa.value - ob.value).abs() <= 1e-9 * ob.value.abs().max(1.0),
                            "{at}"
                        );
                        for f in price_fields {
                            assert!(
                                (2.0 * oa.extra[*f] - ob.extra[*f]).abs()
                                    <= 1e-9 * ob.extra[*f].abs().max(1.0),
                                "{at} {f}"
                            );
                        }
                        assert_eq!(oa.extra["direction"], ob.extra["direction"], "{at}");
                    }
                    _ => panic!("{name} {label}@{i}: output on one side only"),
                }
            }
        }
    }
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
        "kestrel_price_location",
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

/// The N3 statistics and volatility estimators work on ratios of prices: multiplying every price
/// by `k` leaves every value and extra unchanged. `ar1_half_life` regresses raw prices on raw
/// prices, so reflecting the series about a level leaves its value and `phi` unchanged too.
#[test]
fn n3_entries_are_scale_invariant() {
    let scaled = |bars: &[Bar], k: f64| -> Vec<Bar> {
        bars.iter()
            .map(|b| {
                Bar::new(
                    b.timestamp,
                    k * b.open,
                    k * b.high,
                    k * b.low,
                    k * b.close,
                    b.volume,
                )
            })
            .collect()
    };
    for name in [
        "parkinson",
        "rogers_satchell",
        "yang_zhang",
        "variance_ratio",
        "autocorrelation",
        "hurst_rs",
        "ar1_half_life",
    ] {
        for (label, bars) in series() {
            let big = scaled(&bars, 3.0);
            let mut a = build_checked(name, &HashMap::new()).unwrap();
            let mut b = build_checked(name, &HashMap::new()).unwrap();
            let mut published = 0;
            for (i, (x, y)) in bars.iter().zip(&big).enumerate() {
                let at = format!("{name} {label}@{i}");
                match (a.on_bar(x), b.on_bar(y)) {
                    (None, None) => {}
                    (Some(oa), Some(ob)) => {
                        published += 1;
                        assert!(
                            (oa.value - ob.value).abs() <= 1e-9 * ob.value.abs().max(1.0),
                            "{at}"
                        );
                        assert_eq!(oa.extra.len(), ob.extra.len(), "{at}");
                        for (key, va) in &oa.extra {
                            assert!(
                                (va - ob.extra[key]).abs() <= 1e-9 * ob.extra[key].abs().max(1.0),
                                "{at} {key}"
                            );
                        }
                    }
                    _ => panic!("{at}: output on one side only"),
                }
            }
            assert!(published > 0, "{name} {label} never published");
        }
    }
}

#[test]
fn ar1_half_life_reads_the_same_on_both_sides() {
    check_bars_entry(
        "ar1_half_life",
        &[("period", 20.0)],
        &[("value", Answer::Same), ("phi", Answer::Same)],
    );
}

/// Compares the alerts of every bar of a series with those of its mirror: each kind of the
/// original answers as the kind `pairs` maps it to. Returns how many alerts were compared.
fn check_alerts_mirrored(
    name: &str,
    params: &[(&str, f64)],
    bars: &[Bar],
    pairs: &[(&str, &str)],
) -> usize {
    let params: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mirrored = mirror(bars);
    let mut a = build_checked(name, &params).unwrap();
    let mut b = build_checked(name, &params).unwrap();
    let mut seen = 0;
    for (i, (x, y)) in bars.iter().zip(&mirrored).enumerate() {
        a.on_bar(x);
        b.on_bar(y);
        let mut want: Vec<String> = a
            .alerts()
            .iter()
            .map(|alert| {
                pairs
                    .iter()
                    .find(|(from, _)| *from == alert.kind)
                    .unwrap_or_else(|| panic!("{name}@{i}: unexpected alert {}", alert.kind))
                    .1
                    .to_string()
            })
            .collect();
        let mut got: Vec<String> = b.alerts().iter().map(|alert| alert.kind.clone()).collect();
        want.sort();
        got.sort();
        assert_eq!(want, got, "{name}@{i}");
        seen += want.len();
    }
    seen
}

/// A buy setup with its countdown (see the unit tests of `setup_countdown`): a rise of five bars, a
/// fall that completes the setup on bar 13, a countdown with a deferred thirteenth.
fn setup_countdown_series() -> Vec<Bar> {
    let mut c = vec![110.0, 111.0, 112.0, 113.0, 114.0];
    c.extend((0..9).map(|k| 109.0 - k as f64));
    c.extend((0..11).map(|k| 100.0 - k as f64));
    c.extend([100.0, 100.0, 99.5, 94.0, 94.0]);
    c.iter()
        .enumerate()
        .map(|(i, c)| {
            let high = if i == 28 { c + 1.0 } else { c + 0.5 };
            Bar::new(i as i64 * 3600, *c, high, c - 0.5, *c, 100.0)
        })
        .collect()
}

#[test]
fn n4_setup_countdown_negates_and_swaps_its_alerts() {
    check_bars_entry(
        "setup_countdown",
        &[],
        &[("value", Answer::Negated), ("countdown", Answer::Negated)],
    );
    let pairs = [
        ("buy_setup", "sell_setup"),
        ("sell_setup", "buy_setup"),
        ("buy_countdown", "sell_countdown"),
        ("sell_countdown", "buy_countdown"),
    ];
    let mut seen = check_alerts_mirrored("setup_countdown", &[], &setup_countdown_series(), &pairs);
    assert_eq!(seen, 2, "buy setup and buy countdown of the crafted series");
    for (_, bars) in series() {
        seen += check_alerts_mirrored("setup_countdown", &[], &bars, &pairs);
    }
    assert!(seen > 2, "the generated series complete setups too");
}

#[test]
fn n4_narrow_range_is_mirror_invariant() {
    check_bars_entry(
        "narrow_range",
        &[],
        &[("value", Answer::Same), ("inside", Answer::Same)],
    );
    let mut seen = 0;
    for (_, bars) in series() {
        seen += check_alerts_mirrored(
            "narrow_range",
            &[],
            &bars,
            &[("narrow_range", "narrow_range")],
        );
    }
    assert!(seen > 0);
}

#[test]
fn n4_williams_fractals_swap_their_levels_and_alerts() {
    check_bars_swapped_pair("williams_fractals", &[], "value", "down_level");
    check_bars_swapped_pair("williams_fractals", &[], "down_level", "value");
    let mut seen = 0;
    for (_, bars) in series() {
        seen += check_alerts_mirrored(
            "williams_fractals",
            &[],
            &bars,
            &[
                ("up_fractal", "down_fractal"),
                ("down_fractal", "up_fractal"),
            ],
        );
    }
    assert!(seen > 0);
}

/// The generated series are not used here: on them equally filled bins sometimes tie, and the tie
/// rule (lower bin) is not mirror symmetric by design. These windows are exact in binary floating
/// point (halves, a step of 2) and have a unique point of control.
#[test]
fn n4_tpo_profile_reflects_its_value_and_swaps_its_area_edges() {
    let hl = |h: f64, l: f64, i: i64| Bar::new(i, l, h, l, h, 0.0);
    // Counts [2, 3, 2, 1] on lo 396, hi 404; the mirror has [1, 2, 3, 2].
    let bars = [
        hl(404.0, 396.0, 0),
        hl(400.0, 398.0, 1),
        hl(399.5, 399.0, 2),
    ];
    let params = HashMap::from([("lookback".to_string(), 3.0), ("num_bins".to_string(), 4.0)]);
    let run = |bars: &[Bar]| {
        let mut engine = build_checked("tpo_profile", &params).unwrap();
        let mut last = None;
        for b in bars {
            last = engine.on_bar(b);
        }
        last.unwrap()
    };
    let (a, b) = (run(&bars), run(&mirror(&bars)));
    assert_eq!(
        (a.value, a.extra["val"], a.extra["vah"]),
        (399.0, 396.0, 402.0)
    );
    assert_eq!(b.value, 2.0 * LEVEL - a.value);
    assert_eq!(b.extra["vah"], 2.0 * LEVEL - a.extra["val"]);
    assert_eq!(b.extra["val"], 2.0 * LEVEL - a.extra["vah"]);
}
