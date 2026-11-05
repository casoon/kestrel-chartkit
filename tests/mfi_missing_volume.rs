mod common;

use common::*;
use kestrel_chartkit::indicator::mfi::Mfi;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;

/// FNV-1a over the bit patterns of every output (`value`, `signal`), `None` marked separately.
fn checksum(ind: &mut dyn Indicator, bars: &[Bar]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |x: u64| {
        for b in x.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for out in run_indicator(ind, bars) {
        match out {
            None => feed(0xdead_beef),
            Some(o) => {
                feed(o.value.to_bits());
                feed(o.extra["signal"].to_bits());
            }
        }
    }
    h
}

/// Complete series: the values are bit-identical to those of the version that kept running sums
/// over the window (checksums taken from that version).
#[test]
fn complete_series_keeps_its_bit_exact_values() {
    let bars = generate_random_walk_bars(11, 400, 100.0, 0.05, 2.0, 1000.0);
    assert_eq!(checksum(&mut Mfi::with_defaults(), &bars), EXPECTED_DEFAULT);
    let mut short = Mfi::new(5, 3, 3, 50.0, 80.0, 20.0, 5, true);
    assert_eq!(checksum(&mut short, &bars), EXPECTED_SHORT);
}

const EXPECTED_DEFAULT: u64 = 1082238413424186946;
const EXPECTED_SHORT: u64 = 17505538802092369246;

/// `high = low = close`, so the typical price is the close. `mfi_len = 3`, `avg_len = 1` (the line
/// is the raw MFI). Volume 100 everywhere except bar 3, which is missing (NaN).
///
/// Money flow `close * 100`, signed by the close change: bar 1 +1100, bar 2 -1000, bar 3 up but
/// missing, bar 4 +1300, bar 5 -1200, bar 6 +1400, bar 7 +1500, bar 8 -1400. With
/// `raw = 100 * pos / (pos + neg)`:
///
/// - window bars 0-2: pos 1100, neg 1000
/// - windows ending at bars 3, 4, 5 contain bar 3: no output
/// - window bars 4-6: pos 2700, neg 1200
/// - window bars 5-7: pos 2900, neg 1200
/// - window bars 6-8: pos 2900, neg 1400
#[test]
fn single_missing_volume_only_voids_the_windows_that_contain_it() {
    let closes = [10.0, 11.0, 10.0, 12.0, 13.0, 12.0, 14.0, 15.0, 14.0];
    let bars: Vec<Bar> = closes
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let v = if i == 3 { f64::NAN } else { 100.0 };
            Bar::new(i as i64 * 3600, c, c, c, c, v)
        })
        .collect();
    let mut mfi = Mfi::new(3, 1, 3, 50.0, 80.0, 20.0, 5, true);
    let out = run_indicator(&mut mfi, &bars);

    let frac = |pos: f64, neg: f64| 100.0 * pos / (pos + neg);
    let expected = [
        None,
        None,
        Some(frac(1100.0, 1000.0)),
        None,
        None,
        None,
        Some(frac(2700.0, 1200.0)),
        Some(frac(2900.0, 1200.0)),
        Some(frac(2900.0, 1400.0)),
    ];
    for (i, (got, want)) in out.iter().zip(expected).enumerate() {
        match (got, want) {
            (None, None) => {}
            (Some(g), Some(w)) => assert!(
                (g.value - w).abs() < 1e-9,
                "bar {i}: {} instead of {w}",
                g.value
            ),
            _ => panic!(
                "bar {i}: {:?} against {:?}",
                got.as_ref().map(|o| o.value),
                want
            ),
        }
    }
}

/// With a longer smoothing the values after the gap are finite and stay inside 0..100: the
/// missing bar does not poison the averages.
#[test]
fn smoothed_line_recovers_after_a_missing_volume() {
    let mut bars = generate_random_walk_bars(5, 300, 100.0, 0.05, 2.0, 1000.0);
    bars[100].volume = f64::NAN;
    bars[101].volume = f64::NAN;
    let mut mfi = Mfi::with_defaults();
    let out = run_indicator(&mut mfi, &bars);
    for (i, o) in out.iter().enumerate() {
        if let Some(o) = o {
            assert!(
                o.value.is_finite() && o.extra["signal"].is_finite(),
                "bar {i}"
            );
            assert!((0.0..=100.0).contains(&o.value), "bar {i}");
            assert!(
                !(100..115).contains(&i),
                "bar {i} has a missing volume in its window"
            );
        }
    }
    assert!(
        out[115].is_some(),
        "the first window after the gap has an MFI"
    );
    assert!(out[299].is_some());
}

/// `flow_exhaustion` computes its raw MFI per window: a missing volume voids those windows only
/// and never reaches its smoothing state.
#[test]
fn flow_exhaustion_stays_finite_after_a_missing_volume() {
    let entry = kestrel_chartkit::indicator::registry::catalog()
        .into_iter()
        .find(|e| e.name == "flow_exhaustion")
        .unwrap();
    let mut bars = generate_random_walk_bars(7, 400, 100.0, 0.05, 2.0, 1000.0);
    bars[100].volume = f64::NAN;
    let mut ind =
        kestrel_chartkit::indicator::registry::build(entry.name, &entry.default_params).unwrap();
    for (i, o) in run_indicator(ind.as_mut(), &bars).iter().enumerate() {
        if let Some(o) = o {
            assert!(
                o.value.is_finite() && o.extra.values().all(|v| v.is_finite()),
                "bar {i}"
            );
        }
    }
}
