mod common;

use common::*;
use kestrel_chartkit::heatmap::evaluate_instrument_heatmap;
use kestrel_chartkit::model::Bar;

fn walk(n: usize) -> Vec<Bar> {
    generate_random_walk_bars(7, n, 100.0, 0.0, 0.8, 1000.0)
}

#[test]
fn one_row_per_bar_with_matching_timestamps() {
    let bars = walk(900);
    let rows = evaluate_instrument_heatmap(&bars);
    assert_eq!(rows.len(), bars.len());
    for (r, b) in rows.iter().zip(&bars) {
        assert_eq!(r.ts, b.timestamp);
    }
}

#[test]
fn empty_input_gives_empty_output() {
    assert!(evaluate_instrument_heatmap(&[]).is_empty());
}

#[test]
fn every_reported_value_lies_within_unit_interval() {
    for bars in [
        walk(1200),
        generate_sine_bars(1200, 100.0, 15.0, 40.0, 1000.0),
    ] {
        for r in evaluate_instrument_heatmap(&bars) {
            for v in r.values() {
                assert!(v.is_nan() || (-1.0..=1.0).contains(&v), "out of range: {v}");
            }
        }
    }
}

#[test]
fn all_tracks_report_after_warmup_when_volume_exists() {
    let rows = evaluate_instrument_heatmap(&walk(1200));
    let last = rows.last().unwrap();
    for (name, v) in kestrel_chartkit::heatmap::HEATMAP_TRACKS
        .iter()
        .zip(last.values())
    {
        assert!(v.is_finite(), "{name} not evaluated at the last bar");
    }
}

#[test]
fn warmup_rows_are_not_evaluated_instead_of_filled() {
    let rows = evaluate_instrument_heatmap(&walk(1200));
    // HMA(220) cannot have a slope, let alone 50 ranked slopes, in the first 250 bars.
    assert!(rows[..250].iter().all(|r| r.dir_hma.is_nan()));
    assert!(rows[0].ord_adx.is_nan());
}

#[test]
fn flow_tracks_are_nan_without_real_volume() {
    let bars = generate_random_walk_bars(7, 1200, 100.0, 0.0, 0.8, 0.0);
    let rows = evaluate_instrument_heatmap(&bars);
    assert!(rows
        .iter()
        .all(|r| r.flow_cmf.is_nan() && r.flow_rvat.is_nan()));
    assert!(rows.last().unwrap().mom_tsi.is_finite());
}

#[test]
fn result_is_deterministic() {
    let bars = walk(900);
    let a = evaluate_instrument_heatmap(&bars);
    let b = evaluate_instrument_heatmap(&bars);
    for (x, y) in a.iter().zip(&b) {
        for (p, q) in x.values().iter().zip(y.values()) {
            assert!(p.to_bits() == q.to_bits());
        }
    }
}

#[test]
fn rows_depend_only_on_bars_up_to_their_own() {
    let bars = walk(1000);
    let full = evaluate_instrument_heatmap(&bars);
    let cut = evaluate_instrument_heatmap(&bars[..700]);
    for i in 0..700 {
        for (p, q) in full[i].values().iter().zip(cut[i].values()) {
            assert!(
                p.to_bits() == q.to_bits(),
                "row {i} changed with later bars"
            );
        }
    }
}

#[test]
fn steady_uptrend_has_positive_direction_and_mirror_is_negative() {
    let up = generate_trend_bars(900, 100.0, 0.3, 1000.0);
    let rows = evaluate_instrument_heatmap(&up);
    // A constant relative slope is not constant in rank terms, so only the sign is asserted:
    // the smoother lies below the close on a rising line.
    assert!(rows.last().unwrap().dir_kas > 0.0);
    let down = generate_trend_bars(900, 400.0, -0.3, 1000.0);
    let rows = evaluate_instrument_heatmap(&down);
    assert!(rows.last().unwrap().dir_kas < 0.0);
}

#[test]
fn percentile_normalisation_is_scale_invariant() {
    let a = walk(900);
    let b: Vec<Bar> = a
        .iter()
        .map(|x| {
            Bar::new(
                x.timestamp,
                x.open * 7.0,
                x.high * 7.0,
                x.low * 7.0,
                x.close * 7.0,
                x.volume,
            )
        })
        .collect();
    let ra = evaluate_instrument_heatmap(&a);
    let rb = evaluate_instrument_heatmap(&b);
    for (x, y) in ra.iter().zip(&rb) {
        for (p, q) in x.values().iter().zip(y.values()) {
            assert!(
                (p.is_nan() && q.is_nan()) || (p - q).abs() < 1e-4,
                "{p} vs {q}"
            );
        }
    }
}
