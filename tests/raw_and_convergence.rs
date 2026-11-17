//! `extra["raw"]` of RSI / Williams %R against hand calculations, and the start-up convergence
//! contract of `Indicator::convergence_period` against a series started much earlier.

use kestrel_chartkit::indicator::atr::Atr;
use kestrel_chartkit::indicator::macd::Macd;
use kestrel_chartkit::indicator::rsi::Rsi;
use kestrel_chartkit::indicator::trix::Trix;
use kestrel_chartkit::indicator::tsi::Tsi;
use kestrel_chartkit::indicator::wavetrend::WaveTrendEngine;
use kestrel_chartkit::indicator::williams_r::WilliamsR;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;

fn bar(i: i64, high: f64, low: f64, close: f64) -> Bar {
    Bar {
        timestamp: i,
        open: close,
        high,
        low,
        close,
        volume: 1000.0,
    }
}

fn close_bars(closes: &[f64]) -> Vec<Bar> {
    closes
        .iter()
        .enumerate()
        .map(|(i, &c)| bar(i as i64, c + 0.5, c - 0.5, c))
        .collect()
}

fn run(
    ind: &mut dyn Indicator,
    bars: &[Bar],
) -> Vec<Option<(f64, std::collections::HashMap<String, f64>)>> {
    bars.iter()
        .map(|b| ind.on_bar(b).map(|o| (o.value, o.extra)))
        .collect()
}

const EPS: f64 = 1e-9;

#[test]
fn rsi_raw_equals_value_without_line_smoothing() {
    let bars = close_bars(&[100.0, 102.0, 101.0, 103.0, 102.0, 104.0, 103.5, 99.0]);
    let mut rsi = Rsi::new(2, 1, 3, 50.0, 70.0, 30.0, 5, true, 100, 4, 10.0);
    let mut seen = 0;
    for (value, extra) in run(&mut rsi, &bars).into_iter().flatten() {
        assert!((value - extra["raw"]).abs() < EPS);
        seen += 1;
    }
    assert!(seen >= 5);
}

#[test]
fn rsi_raw_is_unsmoothed_against_hand_calculation() {
    // Wilder, len 2: changes +2, -1 -> avg 1.0 / 0.5 -> raw 100 - 100/3 = 66.667;
    // +2 -> 1.5 / 0.25 -> RS 6 -> raw 100 - 100/7; -1 -> 0.75 / 0.625 -> RS 1.2 -> raw 54.5454.
    let bars = close_bars(&[100.0, 102.0, 101.0, 103.0, 102.0]);
    let mut rsi = Rsi::new(2, 3, 3, 50.0, 70.0, 30.0, 5, true, 100, 4, 10.0);
    let out = run(&mut rsi, &bars);
    let raw = [
        100.0 - 100.0 / 3.0,
        100.0 - 100.0 / 7.0,
        100.0 - 100.0 / 2.2,
    ];
    let mut line = raw[0];
    for (k, expected_raw) in raw.iter().enumerate() {
        let (value, extra) = out[2 + k].clone().expect("output");
        assert!((extra["raw"] - expected_raw).abs() < 1e-9, "raw {k}");
        if k > 0 {
            line = 0.5 * expected_raw + 0.5 * line; // Ema(3): alpha 0.5, first-sample seed
        }
        assert!((value - line).abs() < 1e-9, "value {k}");
    }
    assert!((out[3].clone().unwrap().0 - out[3].clone().unwrap().1["raw"]).abs() > 1.0);
}

#[test]
fn williams_r_raw_equals_value_without_line_smoothing() {
    let bars = close_bars(&[10.0, 12.0, 9.0, 13.0, 11.0, 8.0, 12.0]);
    let mut w = WilliamsR::new(2, 1, 3, 50.0, 80.0, 20.0, 5, true, 50, 4, 10.0);
    let mut seen = 0;
    for (value, extra) in run(&mut w, &bars).into_iter().flatten() {
        assert!((value - extra["raw"]).abs() < EPS);
        seen += 1;
    }
    assert!(seen >= 5);
}

#[test]
fn williams_r_raw_is_unsmoothed_against_hand_calculation() {
    // len 2, ascending scale: raw 75, 20, 60 (windows of two bars); Ema(3) alpha 0.5.
    let bars = vec![
        bar(0, 10.0, 8.0, 9.0),
        bar(1, 12.0, 9.0, 11.0),
        bar(2, 11.0, 7.0, 8.0),
        bar(3, 9.0, 6.0, 9.0),
    ];
    let mut w = WilliamsR::new(2, 3, 3, 50.0, 80.0, 20.0, 5, true, 50, 4, 10.0);
    let out = run(&mut w, &bars);
    let expect = [(75.0, 75.0), (20.0, 47.5), (60.0, 53.75)];
    for (k, (raw, line)) in expect.iter().enumerate() {
        let (value, extra) = out[1 + k].clone().expect("output");
        assert!((extra["raw"] - raw).abs() < 1e-9, "raw {k}");
        assert!((value - line).abs() < 1e-9, "value {k}");
    }
}

fn walk(n: usize) -> Vec<Bar> {
    let mut state: u64 = 0x2545F4914F6CDD1D;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f64) / ((1u64 << 31) as f64) - 0.5
    };
    let mut close = 100.0_f64;
    (0..n)
        .map(|i| {
            let open = close;
            close = (close + 2.0 * next()).max(1.0);
            let high = open.max(close) + next().abs();
            let low = open.min(close) - next().abs();
            Bar {
                timestamp: i as i64,
                open,
                high,
                low,
                close,
                volume: 1000.0 + 100.0 * next(),
            }
        })
        .collect()
}

/// Output of a series started `lead` bars earlier agrees with a fresh start from
/// `convergence_period()` bars on, in `value` and `extra["signal"]`.
fn assert_converges(name: &str, make: fn() -> Box<dyn Indicator>) {
    let bars = walk(3400);
    let lead = 2000;
    let conv = make().convergence_period();
    assert!(conv >= make().warmup_period(), "{name}: below warmup");
    let long = run(&mut make(), &bars);
    let short = run(&mut make(), &bars[lead..]);
    let mut worst = 0.0_f64;
    for k in conv..conv + 100 {
        let (lv, le) = long[lead + k].clone().expect("long output");
        let (sv, se) = short[k].clone().expect("short output");
        worst = worst.max((lv - sv).abs());
        if let (Some(a), Some(b)) = (le.get("signal"), se.get("signal")) {
            worst = worst.max((a - b).abs());
        }
    }
    assert!(worst < 1e-3, "{name}: diff {worst} after {conv} bars");
}

#[test]
fn convergence_period_bounds_the_startup_influence() {
    assert_converges("wavetrend", || Box::new(WaveTrendEngine::with_defaults()));
    assert_converges("tsi", || Box::new(Tsi::with_defaults()));
    assert_converges("macd", || Box::new(Macd::with_defaults()));
    assert_converges("trix", || Box::new(Trix::with_defaults()));
    assert_converges("rsi", || Box::new(Rsi::with_defaults()));
    assert_converges("atr", || Box::new(Atr::with_defaults()));
}

#[test]
fn convergence_period_exceeds_warmup_for_cascades() {
    assert!(
        Macd::with_defaults().convergence_period() > 10 * Macd::with_defaults().warmup_period()
    );
    assert!(WaveTrendEngine::with_defaults().convergence_period() > 100);
}
