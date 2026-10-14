//! Leg geometry against `reference/kestrel_reference/fixtures/leg_geometry.py`.

mod common;

use kestrel_chartkit::structure::{
    largest_pullback, path_efficiency, retracement, strict_pivots, swing_overlap, swing_sequence,
    threshold_zigzag, window_leg, RetracementZone, SwingSequence,
};

const GOLDEN: &str = include_str!("fixtures/golden_leg_geometry.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

fn close(a: f64, b: f64, key: &str) {
    let tol = value("leg_geometry_tolerance");
    assert!((a - b).abs() <= tol * b.abs().max(1.0), "{key}: {a} vs {b}");
}

#[test]
fn test_window_leg_matches_the_reference() {
    let bars = common::wave_series(400);
    for i in 0..value("meta_window_n") as usize {
        let k = |f: &str| value(&format!("window{i}_{f}"));
        let end = k("end") as usize;
        let leg = window_leg(&bars[..=end], k("len") as usize).expect("leg");
        close(leg.from, k("from"), "from");
        close(leg.to, k("to"), "to");
        assert_eq!(leg.from_index, k("from_index") as usize, "window{i}");
        assert_eq!(leg.to_index, k("to_index") as usize, "window{i}");
        assert_eq!(leg.up, k("up") == 1.0, "window{i}");
    }
}

#[test]
fn test_retracement_matches_the_reference() {
    let deep = value("meta_deep");
    for i in 0..value("meta_retr_n") as usize {
        let k = |f: &str| value(&format!("retr{i}_{f}"));
        let r = retracement(k("from"), k("to"), k("price"), deep).expect("leg has a size");
        close(r.share, k("share"), &format!("retr{i}"));
        let zone = match r.zone {
            RetracementZone::Extension => 0.0,
            RetracementZone::Correction => 1.0,
            RetracementZone::DeepCorrection => 2.0,
            RetracementZone::Reversal => 3.0,
        };
        assert_eq!(zone, k("zone"), "retr{i}");
    }
}

#[test]
fn test_largest_pullback_matches_the_reference() {
    let bars = common::wave_series(400);
    for i in 0..value("meta_pull_n") as usize {
        let k = |f: &str| value(&format!("pull{i}_{f}"));
        let got = largest_pullback(&bars, k("at") as usize, k("extreme"), k("up") == 1.0);
        close(got, k("pullback"), &format!("pull{i}"));
    }
}

#[test]
fn test_swing_sequence_matches_the_reference() {
    for i in 0..value("meta_seq_n") as usize {
        let k = |f: &str| value(&format!("seq{i}_{f}"));
        let highs: Vec<f64> = (0..k("nh") as usize).map(|j| k(&format!("h{j}"))).collect();
        let lows: Vec<f64> = (0..k("nl") as usize).map(|j| k(&format!("l{j}"))).collect();
        let got = match swing_sequence(&highs, &lows) {
            SwingSequence::Up => 0.0,
            SwingSequence::Down => 1.0,
            SwingSequence::Mixed => 2.0,
            SwingSequence::Unknown => 3.0,
        };
        assert_eq!(got, k("result"), "seq{i}");
    }
}

#[test]
fn test_strict_pivots_match_the_reference() {
    let bars = common::wave_series(400);
    for w in [2usize, 5] {
        let (lows, highs) = strict_pivots(&bars, w);
        let k = |f: &str| value(&format!("piv_w{w}_{f}"));
        let want_lows: Vec<usize> = (0..k("lows_n") as usize)
            .map(|j| k(&format!("low{j}")) as usize)
            .collect();
        let want_highs: Vec<usize> = (0..k("highs_n") as usize)
            .map(|j| k(&format!("high{j}")) as usize)
            .collect();
        let got_lows: Vec<usize> = (0..bars.len()).filter(|&j| lows[j]).collect();
        let got_highs: Vec<usize> = (0..bars.len()).filter(|&j| highs[j]).collect();
        assert_eq!(got_lows, want_lows, "lows, width {w}");
        assert_eq!(got_highs, want_highs, "highs, width {w}");
        assert!(!want_lows.is_empty() && !want_highs.is_empty());
    }
}

#[test]
fn test_threshold_zigzag_matches_the_reference() {
    let bars = common::wave_series(400);
    for c in [2u32, 4] {
        let thr: Vec<Option<f64>> = (0..bars.len())
            .map(|i| (i >= 10).then_some(c as f64))
            .collect();
        let got = threshold_zigzag(&bars, &thr);
        let k = |f: &str| value(&format!("zz_c{c}_{f}"));
        assert_eq!(got.len(), k("n") as usize, "threshold {c}");
        assert!(got.len() >= 4, "the series must swing enough");
        for (j, p) in got.iter().enumerate() {
            assert_eq!(p.index, k(&format!("p{j}_index")) as usize, "zz{c} p{j}");
            close(p.price, k(&format!("p{j}_price")), "price");
            assert_eq!(p.is_high, k(&format!("p{j}_high")) == 1.0, "zz{c} p{j}");
            assert_eq!(
                p.confirmed_index,
                k(&format!("p{j}_confirmed")) as usize,
                "zz{c} p{j}"
            );
        }
    }
}

#[test]
fn test_path_efficiency_and_overlap_match_the_reference() {
    for i in 0..value("meta_eff_n") as usize {
        let k = |f: &str| value(&format!("eff{i}_{f}"));
        let pts: Vec<f64> = (0..k("n") as usize).map(|j| k(&format!("v{j}"))).collect();
        let got = path_efficiency(&pts);
        if has(&format!("eff{i}_none")) {
            assert_eq!(got, None, "eff{i}");
        } else {
            close(got.expect("value"), k("result"), &format!("eff{i}"));
        }
    }
    for i in 0..value("meta_ovl_n") as usize {
        let k = |f: &str| value(&format!("ovl{i}_{f}"));
        let swings: Vec<(f64, bool)> = (0..k("n") as usize)
            .map(|j| (k(&format!("s{j}_price")), k(&format!("s{j}_high")) == 1.0))
            .collect();
        let got = swing_overlap(k("anchor"), &swings, k("up") == 1.0);
        if has(&format!("ovl{i}_none")) {
            assert_eq!(got, None, "ovl{i}");
        } else {
            close(got.expect("value"), k("result"), &format!("ovl{i}"));
        }
    }
}
