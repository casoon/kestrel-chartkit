mod common;

use kestrel_chartkit::indicator::force_index::ElderForceIndex;
use kestrel_chartkit::indicator::rvat::RelativeVolumeAtTime;
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

#[test]
fn test_golden_volume_profile_contract_boundary_reset() {
    let mut vp = VolumeProfileEngine::new(3, 10);
    let bar1 = Bar::new(1, 100.0, 102.0, 98.0, 100.0, 1000.0);
    let bar2 = Bar::new(2, 100.0, 102.0, 98.0, 100.0, 2000.0);
    let bar3 = Bar::new(3, 100.0, 102.0, 98.0, 100.0, 3000.0);

    vp.on_bar(&bar1);
    vp.on_bar(&bar2);
    let out3 = vp.on_bar(&bar3);
    assert!(out3.is_some());

    // Next bar is a contract boundary (e.g. commodity roll)
    let roll_bar = Bar::new(4, 150.0, 152.0, 148.0, 150.0, 500.0);
    let out_roll = vp.on_bar_with_boundary(&roll_bar, true);
    // Since lookback is 3, resetting state means only 1 bar is present, so None is returned until warmup finishes
    assert!(out_roll.is_none());
}

// --- Paket 21: Elder's Force Index -----------------------------------------------------------

const EFI_CLOSES: [f64; 6] = [100.0, 102.0, 101.0, 101.0, 104.0, 103.0];
const EFI_VOLUMES: [f64; 6] = [1000.0, 1500.0, 800.0, 0.0, 1200.0, 900.0];

fn efi_bars() -> Vec<Bar> {
    EFI_CLOSES
        .iter()
        .zip(EFI_VOLUMES)
        .enumerate()
        .map(|(i, (&c, v))| Bar::new(i as i64 * 60, c, c + 0.5, c - 0.5, c, v))
        .collect()
}

#[test]
fn test_golden_force_index_reference_values() {
    let mut efi = ElderForceIndex::new(3);
    let outputs: Vec<_> = efi_bars().iter().filter_map(|b| efi.on_bar(b)).collect();

    assert_eq!(
        outputs.len(),
        3,
        "EFI(3) darf erst ab der dritten Preisänderung ausgeben"
    );

    let tolerance = expected("efi_tolerance");
    for (out, key) in outputs.iter().zip(["first", "second", "third"]) {
        common::assert_close(
            out.extra["raw"],
            expected(&format!("efi3_raw_{key}")),
            tolerance,
            &format!("EFI(3) raw ({key} output)"),
        );
        common::assert_close(
            out.value,
            expected(&format!("efi3_line_{key}")),
            tolerance,
            &format!("EFI(3) line ({key} output)"),
        );
    }
}

/// Nullvolumen bzw. unveränderter Schluss ergibt Rohwert 0 — die geglättete Linie wird dadurch
/// aber nicht auf 0 gesetzt, sondern nur in Richtung 0 gezogen.
#[test]
fn test_force_index_zero_volume_bar_does_not_zero_the_line() {
    let mut efi = ElderForceIndex::new(3);
    let first = efi_bars()
        .iter()
        .filter_map(|b| efi.on_bar(b))
        .next()
        .expect("EFI gab nichts aus");

    common::assert_close(
        first.extra["raw"],
        0.0,
        0.0,
        "Rohwert der Nullvolumen-Kerze",
    );
    assert!(
        first.value.abs() > 1.0,
        "geglättete Linie wurde durch die Nullvolumen-Kerze auf {} gesetzt",
        first.value
    );
}

/// Ohne Vorgängerschluss gibt es keine Kraft: Die erste Kerze ist keine Nullkraft-Kerze.
#[test]
fn test_force_index_has_no_output_without_a_previous_close() {
    let mut efi = ElderForceIndex::new(1);
    let bars = efi_bars();
    assert!(efi.on_bar(&bars[0]).is_none());
    let second = efi
        .on_bar(&bars[1])
        .expect("EFI(1) gibt ab der ersten Änderung aus");
    common::assert_close(second.extra["raw"], 3000.0, 0.0, "erste Rohkraft");
    common::assert_close(second.value, 3000.0, 0.0, "EMA(1) entspricht dem Rohwert");
}

/// Flachmarkt: keine Preisänderung bei echtem Volumen heißt Kraft 0 auf jeder Kerze.
#[test]
fn test_force_index_flat_series_is_zero() {
    let mut efi = ElderForceIndex::new(3);
    let outputs: Vec<_> = (0..10)
        .filter_map(|i| efi.on_bar(&Bar::new(i * 60, 42.0, 42.5, 41.5, 42.0, 1000.0)))
        .collect();
    assert!(!outputs.is_empty(), "EFI gab nichts aus");
    for out in outputs {
        common::assert_close(out.value, 0.0, 0.0, "EFI im Flachmarkt");
        common::assert_close(out.extra["raw"], 0.0, 0.0, "EFI-Rohwert im Flachmarkt");
    }
}

/// Nach `reset` beginnt die Serie deterministisch neu; ein Serienwechsel führt den Durchschnitt
/// nicht über die Grenze fort.
#[test]
fn test_force_index_reset_restarts_deterministically() {
    let mut efi = ElderForceIndex::new(3);
    let bars = efi_bars();
    let run = |efi: &mut ElderForceIndex| -> Vec<f64> {
        bars.iter()
            .filter_map(|b| efi.on_bar(b))
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut efi);
    efi.reset();
    assert_eq!(first, run(&mut efi));
}

/// Kausalität: Ein bereits ausgegebener Wert ändert sich durch spätere Kerzen nicht.
#[test]
fn test_force_index_earlier_outputs_do_not_change_with_more_bars() {
    let bars = efi_bars();
    let prefix: Vec<f64> = {
        let mut efi = ElderForceIndex::new(3);
        bars[..5]
            .iter()
            .filter_map(|b| efi.on_bar(b))
            .map(|o| o.value)
            .collect()
    };
    let full: Vec<f64> = {
        let mut efi = ElderForceIndex::new(3);
        bars.iter()
            .filter_map(|b| efi.on_bar(b))
            .map(|o| o.value)
            .collect()
    };
    assert_eq!(prefix, full[..prefix.len()]);
}

// --- Paket 31: Relative Volume at Time ------------------------------------------------------

const SECONDS_PER_DAY: i64 = 86_400;

/// Drei Tage mit je drei Slots und ungleichen Tagesprofilen; an Tag 1 fehlt der Slot 01:00.
fn rvat_bars() -> Vec<Bar> {
    let profile = [
        [100.0, 200.0, 300.0],
        [150.0, 0.0, 350.0],
        [300.0, 100.0, 200.0],
    ];
    let mut bars = Vec::new();
    for (day, volumes) in profile.iter().enumerate() {
        for (index, slot) in [0i64, 3600, 7200].iter().enumerate() {
            if day == 1 && index == 1 {
                continue;
            }
            let timestamp = day as i64 * SECONDS_PER_DAY + slot;
            bars.push(Bar::new(
                timestamp,
                100.0,
                101.0,
                99.0,
                100.0,
                volumes[index],
            ));
        }
    }
    bars
}

#[test]
fn test_golden_relative_volume_at_time_reference_values() {
    let mut rvat = RelativeVolumeAtTime::new(5, 0, 3600);
    let outputs: Vec<_> = rvat_bars()
        .iter()
        .filter_map(|bar| rvat.on_bar(bar).map(|out| (bar.timestamp, out)))
        .collect();

    let tolerance = expected("rvat_tolerance");
    assert_eq!(outputs.len() as f64, expected("rvat_output_count"));

    let find = |timestamp: i64| {
        outputs
            .iter()
            .find(|(ts, _)| *ts == timestamp)
            .map(|(_, out)| out)
            .unwrap_or_else(|| panic!("keine Ausgabe für {timestamp}"))
    };

    for (timestamp, key) in [
        (SECONDS_PER_DAY, "d1_s0"),
        (SECONDS_PER_DAY + 7200, "d1_s7200"),
        (2 * SECONDS_PER_DAY, "d2_s0"),
        (2 * SECONDS_PER_DAY + 3600, "d2_s3600"),
        (2 * SECONDS_PER_DAY + 7200, "d2_s7200"),
    ] {
        let out = find(timestamp);
        common::assert_close(
            out.value,
            expected(&format!("rvat_{key}_regular")),
            tolerance,
            &format!("{key} regulär"),
        );
        common::assert_close(
            out.extra["cumulative"],
            expected(&format!("rvat_{key}_cumulative")),
            tolerance,
            &format!("{key} kumulativ"),
        );
    }
}

/// Wie viele Vergleichstage tatsächlich beigetragen haben, steht am Ergebnis — ein fehlender
/// Slot senkt die Zahl, statt still übersprungen zu werden.
#[test]
fn test_rvat_reports_how_many_comparison_days_contributed() {
    let mut rvat = RelativeVolumeAtTime::new(5, 0, 3600);
    let outputs: Vec<_> = rvat_bars()
        .iter()
        .filter_map(|bar| rvat.on_bar(bar).map(|out| (bar.timestamp, out)))
        .collect();

    for (timestamp, key) in [
        (SECONDS_PER_DAY, "d1_s0"),
        (2 * SECONDS_PER_DAY, "d2_s0"),
        (2 * SECONDS_PER_DAY + 3600, "d2_s3600"),
        (2 * SECONDS_PER_DAY + 7200, "d2_s7200"),
    ] {
        let out = &outputs.iter().find(|(ts, _)| *ts == timestamp).unwrap().1;
        let expected_samples = expected(&format!("rvat_{key}_samples"));
        common::assert_close(out.extra["samples"], expected_samples, 0.0, key);
        // Vollständig ist der Vergleich erst, wenn alle `days` Tage beigetragen haben.
        common::assert_close(out.extra["complete"], 0.0, 0.0, &format!("{key} complete"));
    }
}

/// Der erste Tag hat nichts zu vergleichen und gibt deshalb nichts aus.
#[test]
fn test_rvat_stays_silent_on_the_first_day() {
    let mut rvat = RelativeVolumeAtTime::new(5, 0, 3600);
    for bar in rvat_bars().iter().take(3) {
        assert!(
            rvat.on_bar(bar).is_none(),
            "am ersten Tag gibt es keinen Vergleichstag"
        );
    }
}

/// Kein Lookahead: Ein bereits ausgegebener Wert ändert sich durch spätere Tage nicht.
#[test]
fn test_rvat_is_causal_over_prefixes() {
    let bars = rvat_bars();
    let collect = |upto: usize| -> Vec<(i64, f64)> {
        let mut rvat = RelativeVolumeAtTime::new(5, 0, 3600);
        bars[..upto]
            .iter()
            .filter_map(|bar| rvat.on_bar(bar).map(|out| (bar.timestamp, out.value)))
            .collect()
    };
    let prefix = collect(6);
    let full = collect(bars.len());
    assert_eq!(prefix, full[..prefix.len()]);
}

/// Der Tagesbeginn ist verschiebbar, und er entscheidet, wo die kumulative Reihe neu anfängt.
/// Beginnt der Tag um 02:00, ist die 02:00-Kerze die erste ihres Tages: dort muss die kumulative
/// Quote mit der regulären zusammenfallen, während sie bei Tagesbeginn um Mitternacht die
/// bereits aufgelaufene Menge des Tages trägt.
#[test]
fn test_rvat_day_start_offset_decides_where_the_cumulative_series_restarts() {
    let bars = rvat_bars();
    let last_output = |offset: i64| {
        let mut rvat = RelativeVolumeAtTime::new(5, offset, 3600);
        bars.iter()
            .filter_map(|bar| rvat.on_bar(bar))
            .last()
            .expect("Ausgabe erwartet")
    };

    let midnight = last_output(0);
    assert!(
        (midnight.value - midnight.extra["cumulative"]).abs() > 0.1,
        "um Mitternacht beginnend trägt die letzte Kerze bereits Tagesvolumen"
    );

    let two_am = last_output(7200);
    common::assert_close(
        two_am.extra["cumulative"],
        two_am.value,
        1e-12,
        "als erste Kerze ihres Tages ist die kumulative Quote die reguläre",
    );
}

#[test]
fn test_rvat_reset_restarts_deterministically() {
    let bars = rvat_bars();
    let mut rvat = RelativeVolumeAtTime::new(5, 0, 3600);
    let run = |rvat: &mut RelativeVolumeAtTime| -> Vec<f64> {
        bars.iter()
            .filter_map(|bar| rvat.on_bar(bar))
            .map(|o| o.value)
            .collect()
    };
    let first = run(&mut rvat);
    rvat.reset();
    assert_eq!(first, run(&mut rvat));
}

/// Der deklarierte Warmup ist ein Tag in Bars — bei Stundenbars 24, bei Tagesbars einer.
#[test]
fn test_rvat_declared_warmup_is_one_day_of_bars() {
    assert_eq!(RelativeVolumeAtTime::new(5, 0, 3600).warmup_period(), 24);
    assert_eq!(RelativeVolumeAtTime::new(5, 0, 60).warmup_period(), 1440);
    assert_eq!(RelativeVolumeAtTime::new(5, 0, 86_400).warmup_period(), 1);
}
