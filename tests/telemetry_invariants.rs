//! Market telemetry (`src/telemetry.rs`): invariants (determinism, causality, bounds, missing
//! data), the long/short mirror and analytic scenarios whose expected read-out follows from the
//! construction of the series alone.

use kestrel_chartkit::model::Bar;
use kestrel_chartkit::telemetry::{
    compute_market_telemetry, compute_market_telemetry_with, BarometerStatus, EkgStatus,
    ProfileSource, TelemetryConfig, ThermometerStatus, TradableGrade, WashoutPhase,
};

const H: i64 = 3600;

/// Deterministic random walk with drift and uneven wicks (not symmetric around its mean, so a
/// mirror test cannot pass by accident).
fn walk(n: usize, seed: u64, drift: f64) -> Vec<Bar> {
    let mut s = seed;
    let mut next = || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((s >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let mut close = 100.0;
    (0..n)
        .map(|i| {
            let open = close;
            close = open * (1.0 + drift + (next() - 0.5) * 0.02);
            let up = next() * 0.006 * open;
            let down = next() * 0.004 * open;
            Bar::new(
                i as i64 * H,
                open,
                open.max(close) + up,
                open.min(close) - down,
                close,
                f64::NAN,
            )
        })
        .collect()
}

/// Arithmetic reflection `p' = 2m − p`: high and low swap, every distance keeps its size.
fn reflect(bars: &[Bar], m: f64) -> Vec<Bar> {
    bars.iter()
        .map(|b| {
            Bar::new(
                b.timestamp,
                2.0 * m - b.open,
                2.0 * m - b.low,
                2.0 * m - b.high,
                2.0 * m - b.close,
                b.volume,
            )
        })
        .collect()
}

/// Log reflection `p' = c / p`: the Vix Fix of the mirror is the inverse Vix Fix of the original.
fn log_reflect(bars: &[Bar], c: f64) -> Vec<Bar> {
    bars.iter()
        .map(|b| {
            Bar::new(
                b.timestamp,
                c / b.open,
                c / b.low,
                c / b.high,
                c / b.close,
                b.volume,
            )
        })
        .collect()
}

fn close_to(a: Option<f64>, b: Option<f64>, tol: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= tol * (1.0 + a.abs().max(b.abs())),
        (None, None) => true,
        _ => false,
    }
}

#[test]
fn deterministic() {
    let bars = walk(400, 7, 0.0004);
    assert_eq!(
        compute_market_telemetry(&bars, 0.3, 1.0),
        compute_market_telemetry(&bars, 0.3, 1.0)
    );
}

#[test]
fn causal_snapshot_ignores_everything_after_the_last_bar() {
    let bars = walk(400, 11, 0.0002);
    let k = 300;
    let before = compute_market_telemetry(&bars[..k], 0.3, 1.0);
    // Rewrite the future wildly: a crash after bar k. The snapshot of the prefix must not move.
    let mut changed = bars.clone();
    for b in &mut changed[k..] {
        b.low *= 0.5;
        b.close *= 0.6;
        b.open = b.open.min(b.close).max(b.low);
    }
    assert_eq!(before, compute_market_telemetry(&changed[..k], 0.3, 1.0));
    assert_eq!(before.ts, bars[k - 1].timestamp);
    assert_eq!(before.bars, k);
}

#[test]
fn bounds_hold_on_long_series() {
    for seed in 1..6 {
        let t = compute_market_telemetry(&walk(500, seed, 0.0003 * seed as f64 - 0.0009), 0.2, 1.0);
        let th = &t.thermometer;
        assert!(
            th.fractal_dimension
                .is_some_and(|d| (1.0..=2.0).contains(&d)),
            "{th:?}"
        );
        assert!(th.choppiness.is_some_and(|c| (0.0..=100.0).contains(&c)));
        assert!(t
            .barometer
            .wick_to_body_ratio
            .is_some_and(|w| (0.0..=1.0).contains(&w)));
        for p in [
            t.barometer.atr_percentile,
            t.seismometer.vix_fix_percentile,
            t.seismometer.blowoff_percentile,
        ] {
            assert!(p.is_some_and(|p| p > 0.0 && p <= 100.0), "{p:?}");
        }
        assert!(t.bar_atr.is_some_and(|a| a > 0.0));
        assert_ne!(th.status, ThermometerStatus::Unknown);
        assert_eq!(
            th.trend_model_active,
            th.status != ThermometerStatus::TurbulentChaos
        );
        let l = &t.landkarte;
        assert!(l.nearest_magnet_above.is_none_or(|m| m > t.close));
        assert!(l.nearest_magnet_below.is_none_or(|m| m < t.close));
        assert!(l.safe_stop_level_long.is_none_or(|s| s < t.close));
        assert!(l.safe_stop_level_short.is_none_or(|s| s > t.close));
        assert_eq!(
            t.seismometer.long_veto_active,
            t.seismometer.washout_phase == WashoutPhase::W1Panic
        );
        assert_eq!(
            t.ekg.fomo_warning,
            t.ekg.overextension_z.unwrap().abs() > 2.5
        );
    }
}

#[test]
fn short_series_reports_unknown_instead_of_guessing() {
    let t = compute_market_telemetry(&walk(5, 3, 0.0), 0.2, 1.0);
    assert_eq!(t.thermometer.status, ThermometerStatus::Unknown);
    assert!(!t.thermometer.trend_model_active);
    assert_eq!(t.seismometer.washout_phase, WashoutPhase::Unknown);
    assert!(!t.seismometer.long_veto_active && !t.seismometer.short_veto_active);
    assert_eq!(t.ekg.status, EkgStatus::Unknown);
    assert_eq!(
        t.kostengitter.tradable_grade,
        TradableGrade::Unknown,
        "no ATR yet"
    );
    assert_eq!(t.bar_atr, None);
}

#[test]
fn missing_spread_is_unknown() {
    let bars = walk(200, 5, 0.0);
    for spread in [f64::NAN, -1.0, f64::INFINITY] {
        let k = compute_market_telemetry(&bars, spread, 1.0).kostengitter;
        assert_eq!(
            (k.spread_bps, k.spread_atr_ratio, k.tradable_grade),
            (None, None, TradableGrade::Unknown)
        );
    }
}

#[test]
fn spread_load_is_spread_over_atr_and_graded_by_the_thresholds() {
    let bars = walk(200, 9, 0.0);
    let t = compute_market_telemetry(&bars, 0.0, 1.0);
    let atr = t.bar_atr.unwrap();
    let close = t.close;
    // A spread quoted in points of 0.1 price units: 4 points = 0.4 price units.
    let k = compute_market_telemetry(&bars, 4.0, 0.1).kostengitter;
    assert!(close_to(k.spread_bps, Some(0.4 / close * 1e4), 1e-12));
    assert!(close_to(k.spread_atr_ratio, Some(0.4 / atr), 1e-12));
    for (load, grade) in [
        (0.01, TradableGrade::Green),
        (0.05, TradableGrade::Yellow),
        (0.08, TradableGrade::Yellow),
        (0.2, TradableGrade::Red),
    ] {
        let g = compute_market_telemetry(&bars, load * atr, 1.0)
            .kostengitter
            .tradable_grade;
        assert_eq!(g, grade, "load {load}");
    }
}

#[test]
fn straight_trend_is_laminar_and_zigzag_is_turbulent() {
    // Every bar one step higher with the same small range: a straight line, D = 1, choppiness low.
    let line: Vec<Bar> = (0..200)
        .map(|i| {
            let c = 100.0 + i as f64;
            Bar::new(i * H, c - 0.8, c + 0.1, c - 0.9, c, f64::NAN)
        })
        .collect();
    let t = compute_market_telemetry(&line, 0.1, 1.0).thermometer;
    assert!(t.fractal_dimension.unwrap() < 1.1, "{t:?}");
    assert_eq!(t.status, ThermometerStatus::Laminar, "{t:?}");
    assert!(t.trend_model_active);

    // Alternating up and down bars of the same size around a flat level: noise, every bar overlaps.
    let zigzag: Vec<Bar> = (0..200)
        .map(|i| {
            let (o, c) = if i % 2 == 0 {
                (100.0, 101.0)
            } else {
                (101.0, 100.0)
            };
            Bar::new(i * H, o, 101.2, 99.8, c, f64::NAN)
        })
        .collect();
    let t = compute_market_telemetry(&zigzag, 0.1, 1.0).thermometer;
    assert!(
        t.fractal_dimension.unwrap() > 1.48 && t.choppiness.unwrap() > 61.8,
        "{t:?}"
    );
    assert_eq!(t.status, ThermometerStatus::TurbulentChaos);
    assert!(!t.trend_model_active);
}

#[test]
fn crash_bar_is_a_panic_then_recovery() {
    // Quiet walk, then one bar whose low falls 15 % below the recent closes.
    let mut bars = walk(150, 21, 0.0001);
    let c = bars.last().unwrap().close;
    bars.push(Bar::new(150 * H, c, c, c * 0.85, c * 0.9, f64::NAN));
    let t = compute_market_telemetry(&bars, 0.1, 1.0).seismometer;
    assert_eq!(t.washout_phase, WashoutPhase::W1Panic);
    assert!(t.long_veto_active && !t.short_veto_active);
    assert_eq!(t.vix_fix_percentile, Some(100.0));
    // Two quiet bars later (same level) it is recovery, and the veto is off.
    let l = bars.last().unwrap().close;
    for i in 0..2 {
        bars.push(Bar::new(
            (151 + i) * H,
            l,
            l * 1.001,
            l * 0.999,
            l,
            f64::NAN,
        ));
    }
    let t = compute_market_telemetry(&bars, 0.1, 1.0).seismometer;
    assert_eq!(t.washout_phase, WashoutPhase::W2Recovery);
    assert!(!t.long_veto_active);
}

#[test]
fn open_gap_below_is_the_magnet_and_the_stop_sits_behind_it() {
    // Flat base around 100, then a jump: bar k-2 high 101, bar k low 104 → bullish gap 101..104;
    // afterwards the price holds at 110, so the gap stays open below.
    let mut bars: Vec<Bar> = (0..60)
        .map(|i| {
            Bar::new(
                i * H,
                100.0,
                101.0,
                99.0,
                100.0 + 0.01 * (i % 3) as f64,
                f64::NAN,
            )
        })
        .collect();
    bars.push(Bar::new(60 * H, 100.0, 103.0, 99.5, 102.5, f64::NAN));
    bars.push(Bar::new(61 * H, 105.0, 106.0, 104.0, 105.5, f64::NAN));
    for i in 62..80 {
        bars.push(Bar::new(i * H, 110.0, 110.5, 109.5, 110.0, f64::NAN));
    }
    let t = compute_market_telemetry(&bars, 0.1, 1.0);
    let l = &t.landkarte;
    let atr = t.bar_atr.unwrap();
    // Three bullish gaps stay open: bar 59 high 101 → bar 61 low 104, bar 60 high 103 → bar 62 low
    // 109.5, bar 61 high 106 → bar 63 low 109.5.
    assert_eq!(l.open_gaps_below, 3, "{l:?}");
    assert_eq!(l.open_gaps_above, 0);
    // Nearest edge below 110 is 109.5, shared by the two newer gaps; on a tie the newest one
    // (106..109.5) is the zone, so the stop sits under 106.
    assert_eq!(l.nearest_magnet_below, Some(109.5));
    assert!(
        close_to(l.safe_stop_level_long, Some(106.0 - 0.25 * atr), 1e-12),
        "{l:?}"
    );
    assert_eq!(
        l.poc_source,
        Some(ProfileSource::Time),
        "no volume → time at price"
    );
    assert_eq!(l.nearest_magnet_above, None);
}

#[test]
fn volume_profile_only_with_volume_on_every_bar() {
    let mut bars = walk(120, 4, 0.0);
    for b in &mut bars {
        b.volume = 1000.0;
    }
    assert_eq!(
        compute_market_telemetry(&bars, 0.1, 1.0)
            .landkarte
            .poc_source,
        Some(ProfileSource::Volume)
    );
    bars[100].volume = 0.0;
    assert_eq!(
        compute_market_telemetry(&bars, 0.1, 1.0)
            .landkarte
            .poc_source,
        Some(ProfileSource::Time)
    );
}

#[test]
fn narrowing_ranges_read_as_narrow_range() {
    let mut bars = walk(100, 8, 0.0);
    let c = bars.last().unwrap().close;
    for i in 0..7 {
        let w = 0.5 - 0.06 * i as f64;
        bars.push(Bar::new((100 + i) * H, c, c + w, c - w, c, f64::NAN));
    }
    assert!(
        compute_market_telemetry(&bars, 0.1, 1.0)
            .barometer
            .narrow_range_active
    );
}

#[test]
fn wicks_raise_tension_and_widen_the_trailing_distance() {
    let mut bars = walk(100, 12, 0.0);
    let c = bars.last().unwrap().close;
    for i in 0..10 {
        // Body 0.1, range 2.0: wick share 0.95.
        bars.push(Bar::new(
            (100 + i) * H,
            c,
            c + 1.0,
            c - 1.0,
            c + 0.1 * if i % 2 == 0 { 1.0 } else { -1.0 },
            f64::NAN,
        ));
    }
    let b = compute_market_telemetry(&bars, 0.1, 1.0).barometer;
    assert!(b.wick_to_body_ratio.unwrap() > 0.9);
    assert_eq!(b.status, BarometerStatus::TensionElevated);
    assert_eq!(
        b.recommended_trailing_atr,
        TelemetryConfig::default().trailing_atr_tension
    );
}

#[test]
fn stretched_close_warns_and_a_custom_threshold_moves_the_line() {
    let mut bars = walk(120, 13, 0.0);
    let c = bars.last().unwrap().close;
    for i in 0..6 {
        let o = c * (1.0 + 0.02 * i as f64);
        bars.push(Bar::new(
            (120 + i) * H,
            o,
            o * 1.021,
            o * 0.999,
            o * 1.02,
            f64::NAN,
        ));
    }
    let t = compute_market_telemetry(&bars, 0.1, 1.0).ekg;
    let z = t.overextension_z.unwrap();
    assert!(z > 2.5 && t.fomo_warning && t.fomo_side == 1, "{t:?}");
    let cfg = TelemetryConfig {
        overextension_atr: z + 1.0,
        ..TelemetryConfig::default()
    };
    assert!(
        !compute_market_telemetry_with(&bars, 0.1, 1.0, &cfg)
            .ekg
            .fomo_warning
    );
}

/// Long and short alike (arithmetic mirror): order, tension and distances keep their size,
/// signs and sides swap.
#[test]
fn arithmetic_mirror_swaps_sides() {
    for seed in [31, 32, 33] {
        let bars = walk(400, seed, 0.0006);
        let m = 150.0;
        let a = compute_market_telemetry(&bars, 0.2, 1.0);
        let b = compute_market_telemetry(&reflect(&bars, m), 0.2, 1.0);
        let tol = 1e-9;
        assert!(close_to(a.bar_atr, b.bar_atr, tol));
        assert!(close_to(
            a.thermometer.fractal_dimension,
            b.thermometer.fractal_dimension,
            tol
        ));
        assert!(close_to(
            a.thermometer.choppiness,
            b.thermometer.choppiness,
            tol
        ));
        assert!(close_to(
            a.barometer.wick_to_body_ratio,
            b.barometer.wick_to_body_ratio,
            tol
        ));
        assert_eq!(
            a.barometer.narrow_range_active,
            b.barometer.narrow_range_active
        );
        assert!(close_to(
            a.ekg.overextension_z,
            b.ekg.overextension_z.map(|z| -z),
            tol
        ));
        assert_eq!(a.ekg.fomo_side, -b.ekg.fomo_side);
        assert_eq!(
            a.ekg.divergence_direction, -b.ekg.divergence_direction,
            "seed {seed}"
        );
        assert_eq!(a.ekg.tighten_side, -b.ekg.tighten_side);
        let la = &a.landkarte;
        let lb = &b.landkarte;
        assert_eq!(
            (la.open_gaps_above, la.open_gaps_below),
            (lb.open_gaps_below, lb.open_gaps_above)
        );
        let mir = |x: Option<f64>| x.map(|x| 2.0 * m - x);
        assert!(
            close_to(la.safe_stop_level_long, mir(lb.safe_stop_level_short), tol),
            "seed {seed}"
        );
        assert!(close_to(
            la.safe_stop_level_short,
            mir(lb.safe_stop_level_long),
            tol
        ));
        assert!(close_to(
            a.kostengitter.spread_atr_ratio,
            b.kostengitter.spread_atr_ratio,
            tol
        ));
    }
}

/// Long and short alike for the seismometer: on the log mirror the panic at lows becomes the
/// blow-off at tops.
#[test]
fn log_mirror_swaps_panic_and_blowoff() {
    let mut bars = walk(150, 41, 0.0001);
    let c = bars.last().unwrap().close;
    bars.push(Bar::new(150 * H, c, c, c * 0.85, c * 0.9, f64::NAN));
    let a = compute_market_telemetry(&bars, 0.1, 1.0).seismometer;
    let b = compute_market_telemetry(&log_reflect(&bars, 10_000.0), 0.1, 1.0).seismometer;
    assert_eq!(
        (a.washout_phase, a.blowoff_phase),
        (b.blowoff_phase, b.washout_phase)
    );
    assert_eq!(
        (a.long_veto_active, a.short_veto_active),
        (b.short_veto_active, b.long_veto_active)
    );
    assert!(a.long_veto_active);
    assert!(close_to(a.vix_fix_percentile, b.blowoff_percentile, 1e-9));
}

#[cfg(feature = "serde")]
#[test]
fn serialises_status_names_as_documented() {
    let mut bars = walk(150, 41, 0.0001);
    let c = bars.last().unwrap().close;
    bars.push(Bar::new(150 * H, c, c, c * 0.85, c * 0.9, f64::NAN));
    let t = compute_market_telemetry(&bars, 0.1, 1.0);
    // Round trip through serde's data model without a JSON dependency: the enums carry the names.
    assert_eq!(format!("{:?}", t.seismometer.washout_phase), "W1Panic");
    fn name<T: serde::Serialize>(v: &T) -> String {
        struct Name(String);
        impl serde::Serializer for &mut Name {
            type Ok = ();
            type Error = std::fmt::Error;
            type SerializeSeq = serde::ser::Impossible<(), std::fmt::Error>;
            type SerializeTuple = serde::ser::Impossible<(), std::fmt::Error>;
            type SerializeTupleStruct = serde::ser::Impossible<(), std::fmt::Error>;
            type SerializeTupleVariant = serde::ser::Impossible<(), std::fmt::Error>;
            type SerializeMap = serde::ser::Impossible<(), std::fmt::Error>;
            type SerializeStruct = serde::ser::Impossible<(), std::fmt::Error>;
            type SerializeStructVariant = serde::ser::Impossible<(), std::fmt::Error>;
            fn serialize_unit_variant(
                self,
                _: &'static str,
                _: u32,
                v: &'static str,
            ) -> Result<(), Self::Error> {
                self.0 = v.to_string();
                Ok(())
            }
            fn serialize_bool(self, _: bool) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_i8(self, _: i8) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_i16(self, _: i16) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_i32(self, _: i32) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_i64(self, _: i64) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_u8(self, _: u8) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_u16(self, _: u16) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_u32(self, _: u32) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_u64(self, _: u64) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_f32(self, _: f32) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_f64(self, _: f64) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_char(self, _: char) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_str(self, _: &str) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_bytes(self, _: &[u8]) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_none(self) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_some<T: ?Sized + serde::Serialize>(
                self,
                _: &T,
            ) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_unit(self) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_unit_struct(self, _: &'static str) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_newtype_struct<T: ?Sized + serde::Serialize>(
                self,
                _: &'static str,
                _: &T,
            ) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_newtype_variant<T: ?Sized + serde::Serialize>(
                self,
                _: &'static str,
                _: u32,
                _: &'static str,
                _: &T,
            ) -> Result<(), Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_tuple_struct(
                self,
                _: &'static str,
                _: usize,
            ) -> Result<Self::SerializeTupleStruct, Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_tuple_variant(
                self,
                _: &'static str,
                _: u32,
                _: &'static str,
                _: usize,
            ) -> Result<Self::SerializeTupleVariant, Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_struct(
                self,
                _: &'static str,
                _: usize,
            ) -> Result<Self::SerializeStruct, Self::Error> {
                Err(std::fmt::Error)
            }
            fn serialize_struct_variant(
                self,
                _: &'static str,
                _: u32,
                _: &'static str,
                _: usize,
            ) -> Result<Self::SerializeStructVariant, Self::Error> {
                Err(std::fmt::Error)
            }
        }
        let mut n = Name(String::new());
        v.serialize(&mut n).unwrap();
        n.0
    }
    assert_eq!(name(&t.seismometer.washout_phase), "W1_PANIC");
    assert_eq!(name(&ThermometerStatus::TurbulentChaos), "TURBULENT_CHAOS");
    assert_eq!(name(&BarometerStatus::CompressedCoil), "COMPRESSED_COIL");
    assert_eq!(name(&TradableGrade::Green), "GREEN");
    assert_eq!(name(&ProfileSource::Time), "time");
}
