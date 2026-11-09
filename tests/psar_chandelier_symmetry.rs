//! Parabolic SAR follows Wilder's order (hold first, then test the reversal); Chandelier Exit
//! and Adaptive Chandelier keep their direction when both flip conditions hold on one bar, so a
//! price series and its reflection `p' = 2M - p` (highs and lows swapped) give mirrored output.

use kestrel_chartkit::indicator::adaptive_chandelier::AdaptiveChandelierEngine;
use kestrel_chartkit::indicator::chandelier_exit::ChandelierExitEngine;
use kestrel_chartkit::indicator::trend_structural::ParabolicSarEngine;
use kestrel_chartkit::indicator::{Indicator, IndicatorOutput};
use kestrel_chartkit::model::Bar;

const M: f64 = 100.0;

fn walk(n: usize) -> Vec<Bar> {
    let mut s: u64 = 12345;
    let mut next = || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((s >> 33) as f64) / ((1u64 << 31) as f64) - 0.5
    };
    let mut c = M;
    (0..n)
        .map(|i| {
            let o = c;
            c = o + next() * 3.0 + if (i / 40) % 2 == 0 { 0.15 } else { -0.15 };
            let h = o.max(c) + (next() + 0.5) * 1.2;
            let l = o.min(c) - (next() + 0.5) * 1.2;
            Bar::new(i as i64 * 3600, o, h, l, c, 100.0)
        })
        .collect()
}

fn mirror(bars: &[Bar]) -> Vec<Bar> {
    bars.iter()
        .map(|b| {
            Bar::new(
                b.timestamp,
                2.0 * M - b.open,
                2.0 * M - b.low,
                2.0 * M - b.high,
                2.0 * M - b.close,
                b.volume,
            )
        })
        .collect()
}

fn run(ind: &mut dyn Indicator, bars: &[Bar]) -> Vec<Option<IndicatorOutput>> {
    bars.iter().map(|b| ind.on_bar(b)).collect()
}

fn psar(bars: &[Bar]) -> Vec<f64> {
    let mut e = ParabolicSarEngine::new(0.02, 0.2);
    run(&mut e, bars)
        .into_iter()
        .map(|o| o.unwrap().value)
        .collect()
}

#[test]
fn psar_holds_before_reversal_test_minimal_case() {
    // Wilder: the third low (97.0) touches the held SAR (97.0) without breaking it.
    let bars = vec![
        Bar::new(0, 100.0, 100.5, 97.0, 98.0, 1.0),
        Bar::new(1, 98.0, 101.0, 98.0, 99.0, 1.0),
        Bar::new(2, 99.0, 100.0, 97.0, 100.0, 1.0),
    ];
    assert_eq!(psar(&bars), vec![97.0, 97.0, 97.0]);
}

#[test]
fn psar_reflection_is_mirrored_after_start() {
    let bars = walk(600);
    let a = psar(&bars);
    let b = psar(&mirror(&bars));
    // The long start differs from a short start only until the first reversal has happened.
    for i in 100..a.len() {
        assert!(
            (a[i] - (2.0 * M - b[i])).abs() < 1e-9,
            "bar {i}: {} vs {}",
            a[i],
            b[i]
        );
    }
}

fn check_chandelier(make: &dyn Fn() -> Box<dyn Indicator>) {
    let bars = walk(1500);
    let a = run(make().as_mut(), &bars);
    let b = run(make().as_mut(), &mirror(&bars));
    let mut crossed = 0;
    for i in 300..bars.len() {
        let (x, y) = (a[i].as_ref().unwrap(), b[i].as_ref().unwrap());
        let (l, s) = (x.extra["long_stop"], x.extra["short_stop"]);
        let prev = a[i - 1].as_ref().unwrap();
        if bars[i].close > prev.extra["short_stop"] && bars[i].close < prev.extra["long_stop"] {
            crossed += 1;
        }
        assert_eq!(
            x.state.as_deref() == Some("long"),
            y.state.as_deref() == Some("short"),
            "bar {i}: direction not mirrored"
        );
        assert!((l - (2.0 * M - y.extra["short_stop"])).abs() < 1e-9);
        assert!((s - (2.0 * M - y.extra["long_stop"])).abs() < 1e-9);
    }
    assert!(
        crossed > 10,
        "series must contain bars with both flip conditions true"
    );
}

#[test]
fn chandelier_exit_reflection_is_mirrored_with_crossed_stops() {
    check_chandelier(&|| Box::new(ChandelierExitEngine::new(10, 1.0)));
}

#[test]
fn adaptive_chandelier_reflection_is_mirrored_with_crossed_stops() {
    check_chandelier(&|| {
        Box::new(AdaptiveChandelierEngine::new(
            10, 1.0, false, true, 0.0, 0.5, 1.5,
        ))
    });
}
