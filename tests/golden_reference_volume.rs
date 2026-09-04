mod common;

use kestrel_chartkit::indicator::volume_profile::VolumeProfileEngine;
use kestrel_chartkit::indicator::vwap::Vwap;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;

const GOLDEN: &str = include_str!("fixtures/golden_volume.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

#[test]
fn test_golden_vwap_reference_values() {
    let mut vwap = Vwap::new(50, 10);
    // Typical price = (H + L + C) / 3 = (105 + 95 + 100) / 3 = 100.0
    let bar1 = Bar::new(1, 100.0, 105.0, 95.0, 100.0, 1000.0);
    // Typical price = (115 + 105 + 110) / 3 = 110.0
    let bar2 = Bar::new(2, 110.0, 115.0, 105.0, 110.0, 3000.0);

    vwap.on_bar(&bar1);
    let out2 = vwap.on_bar(&bar2).unwrap();

    // Cumulative VWAP = (100*1000 + 110*3000) / (1000 + 3000) = (100,000 + 330,000) / 4000 = 430,000 / 4000 = 107.5
    common::assert_close(
        out2.value,
        expected("vwap_two_bar"),
        expected("vwap_tolerance"),
        "two-bar VWAP",
    );
}

#[test]
fn test_golden_volume_profile_reference_values() {
    let mut vp = VolumeProfileEngine::new(3, 10);
    let bars = vec![
        Bar::new(1, 100.0, 102.0, 98.0, 100.0, 1000.0),
        Bar::new(2, 100.0, 102.0, 98.0, 100.0, 2000.0),
        Bar::new(3, 100.0, 102.0, 98.0, 100.0, 3000.0),
    ];

    let mut last_out = None;
    for b in &bars {
        last_out = vp.on_bar(b);
    }

    let out = last_out.expect("Volume Profile produced no output");
    assert!(out.extra.contains_key("vpoc"));
    assert!(out.extra.contains_key("vah"));
    assert!(out.extra.contains_key("val"));

    let tolerance = expected("volume_profile_tolerance");
    common::assert_close(
        out.extra["vpoc"],
        expected("vpoc_three_bar"),
        tolerance,
        "Volume Profile VPOC",
    );
    common::assert_close(
        out.extra["vah"],
        expected("vah_three_bar"),
        tolerance,
        "Volume Profile VAH",
    );
    common::assert_close(
        out.extra["val"],
        expected("val_three_bar"),
        tolerance,
        "Volume Profile VAL",
    );
}

use kestrel_chartkit::indicator::registry::build_checked;
use std::collections::HashMap;

const VOL_PRICES: [f64; 20] = [
    44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61,
    46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64,
];

fn run_vol(
    name: &str,
    params: &HashMap<String, f64>,
) -> Option<kestrel_chartkit::indicator::IndicatorOutput> {
    let mut ind = build_checked(name, params).unwrap();
    let mut last = None;
    for (i, &p) in VOL_PRICES.iter().enumerate() {
        let bar = Bar::new(
            i as i64 * 60,
            p,
            p + 0.5,
            p - 0.5,
            p,
            (i + 1) as f64 * 100.0,
        );
        if let Some(out) = ind.on_bar(&bar) {
            last = Some(out);
        }
    }
    last
}

#[test]
fn test_golden_acc_dist_reference_values() {
    let out = run_vol("acc_dist", &HashMap::new()).expect("AccDist produced no output");
    common::assert_close(
        out.value,
        expected("acc_dist_last"),
        expected("volume_tolerance"),
        "AccDist",
    );
}

#[test]
fn test_golden_anchored_vwap_reference_values() {
    let out = run_vol("anchored_vwap", &HashMap::new()).expect("Anchored VWAP produced no output");
    common::assert_close(
        out.value,
        expected("anchored_vwap_last"),
        expected("volume_tolerance"),
        "Anchored VWAP",
    );
}

#[test]
fn test_golden_cmf_reference_values() {
    let out = run_vol("cmf", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("CMF produced no output");
    common::assert_close(
        out.value,
        expected("cmf5_last"),
        expected("volume_tolerance"),
        "CMF(5)",
    );
}

#[test]
fn test_golden_cvd_reference_values() {
    let out = run_vol("cvd", &HashMap::new()).expect("CVD produced no output");
    common::assert_close(
        out.value,
        expected("cvd_last"),
        expected("volume_tolerance"),
        "CVD",
    );
}

#[test]
fn test_golden_eom_reference_values() {
    let out = run_vol("eom", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("EOM produced no output");
    common::assert_close(
        out.value,
        expected("eom5_last"),
        expected("volume_tolerance"),
        "EOM(5)",
    );
}

#[test]
fn test_golden_extended_volume_profile_reference_values() {
    let out = run_vol(
        "extended_volume_profile",
        &HashMap::from([
            ("lookback".to_string(), 5.0),
            ("num_bins".to_string(), 10.0),
        ]),
    )
    .expect("Extended VP produced no output");
    common::assert_close(
        out.value,
        expected("ext_vpoc"),
        expected("volume_tolerance"),
        "Ext VPOC",
    );
}

#[test]
fn test_golden_hires_volume_flow_reference_values() {
    let out = run_vol(
        "hires_volume_flow",
        &HashMap::from([("window_len".to_string(), 5.0)]),
    )
    .expect("HiRes Volume Flow produced no output");
    common::assert_close(
        out.value,
        expected("hires_flow_last"),
        expected("volume_tolerance"),
        "HiRes Volume Flow",
    );
}

#[test]
fn test_golden_klinger_reference_values() {
    let out = run_vol(
        "klinger",
        &HashMap::from([
            ("fast_len".to_string(), 3.0),
            ("slow_len".to_string(), 5.0),
            ("signal_len".to_string(), 3.0),
        ]),
    )
    .expect("Klinger produced no output");
    let tol = expected("volume_tolerance");
    common::assert_close(out.value, expected("klinger_line"), tol, "Klinger line");
    common::assert_close(
        out.extra["signal"],
        expected("klinger_signal"),
        tol,
        "Klinger signal",
    );
}

#[test]
fn test_golden_nvi_reference_values() {
    let out = run_vol("nvi", &HashMap::new()).expect("NVI produced no output");
    common::assert_close(
        out.value,
        expected("nvi_last"),
        expected("volume_tolerance"),
        "NVI",
    );
}

#[test]
fn test_golden_obv_reference_values() {
    let out = run_vol("obv", &HashMap::new()).expect("OBV produced no output");
    common::assert_close(
        out.value,
        expected("obv_last"),
        expected("volume_tolerance"),
        "OBV",
    );
}

#[test]
fn test_golden_persistent_volume_profile_reference_values() {
    let out = run_vol(
        "persistent_volume_profile",
        &HashMap::from([
            ("lookback".to_string(), 5.0),
            ("bin_width".to_string(), 1.0),
        ]),
    )
    .expect("Persistent VP produced no output");
    common::assert_close(
        out.value,
        expected("pers_vpoc"),
        expected("volume_tolerance"),
        "Persistent VPOC",
    );
}

#[test]
fn test_golden_pvi_reference_values() {
    let out = run_vol("pvi", &HashMap::new()).expect("PVI produced no output");
    common::assert_close(
        out.value,
        expected("pvi_last"),
        expected("volume_tolerance"),
        "PVI",
    );
}

#[test]
fn test_golden_rvol_reference_values() {
    let out = run_vol("rvol", &HashMap::from([("period".to_string(), 5.0)]))
        .expect("RVOL produced no output");
    common::assert_close(
        out.value,
        expected("rvol5_last"),
        expected("volume_tolerance"),
        "RVOL(5)",
    );
}

#[test]
fn test_golden_volume_reference_values() {
    let out = run_vol("volume", &HashMap::from([("ma_period".to_string(), 5.0)]))
        .expect("Volume produced no output");
    let tol = expected("volume_tolerance");
    common::assert_close(out.value, expected("volume_last"), tol, "Volume");
    common::assert_close(
        out.extra["avg_volume"],
        expected("volume_avg5"),
        tol,
        "Average Volume(5)",
    );
}
