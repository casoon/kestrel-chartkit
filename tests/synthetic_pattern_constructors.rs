//! Scenario fixtures for the pattern constructors in `synthetic.rs`.
//!
//! Each test derives its reference independently of the code under test: the candle metrics are
//! recomputed from the produced OHLC using the documented formulas, and the harmonic prices are
//! hand-calculated from the ratio table. That is the point of these constructors — a figure built
//! from them must satisfy the condition it illustrates, and nothing but an independent
//! recomputation shows that.

use kestrel_chartkit::synthetic::{
    bar_from_shape, bars_from_pivots, xabcd_pivots, xabcd_prices, CandleShape, HarmonicRatios,
    Pivot,
};

const TOL: f64 = 1e-9;

/// The documented normalisation, recomputed from a finished bar.
fn metrics(open: f64, high: f64, low: f64, close: f64) -> (f64, f64, f64) {
    let range = high - low;
    let body = (close - open).abs() / range;
    let upper = (high - open.max(close)) / range;
    let lower = (open.min(close) - low) / range;
    (body, upper, lower)
}

#[test]
fn bar_from_shape_reproduces_the_requested_metrics() {
    let shape = CandleShape {
        body_ratio: 0.2,
        upper_wick_ratio: 0.15,
        lower_wick_ratio: 0.65,
        relative_range: 1.4,
        bullish: true,
    };
    let bar = bar_from_shape(0, &shape, 100.0, 2.5, 1000.0).bar;

    let (body, upper, lower) = metrics(bar.open, bar.high, bar.low, bar.close);
    assert!((body - 0.2).abs() < TOL, "body {body}");
    assert!((upper - 0.15).abs() < TOL, "upper {upper}");
    assert!((lower - 0.65).abs() < TOL, "lower {lower}");

    // relative_range · atr = 1.4 · 2.5 = 3.5
    assert!((bar.high - bar.low - 3.5).abs() < TOL);
    assert!((bar.open - 100.0).abs() < TOL, "opens at the anchor");
    assert!(bar.close > bar.open, "bullish");
}

#[test]
fn bar_from_shape_mirrors_for_a_bearish_candle() {
    let shape = CandleShape {
        bullish: false,
        ..CandleShape {
            body_ratio: 0.2,
            upper_wick_ratio: 0.65,
            lower_wick_ratio: 0.15,
            relative_range: 1.4,
            bullish: false,
        }
    };
    let bar = bar_from_shape(0, &shape, 100.0, 2.5, 1000.0).bar;

    let (body, upper, lower) = metrics(bar.open, bar.high, bar.low, bar.close);
    assert!((body - 0.2).abs() < TOL);
    assert!((upper - 0.65).abs() < TOL);
    assert!((lower - 0.15).abs() < TOL);
    assert!(bar.close < bar.open);
}

#[test]
fn unnormalised_ratios_describe_the_same_candle() {
    // { 2, 1, 1 } and { 0.5, 0.25, 0.25 } are the same division of the range.
    let grob = bar_from_shape(
        0,
        &CandleShape {
            body_ratio: 2.0,
            upper_wick_ratio: 1.0,
            lower_wick_ratio: 1.0,
            relative_range: 1.0,
            bullish: true,
        },
        100.0,
        3.0,
        1.0,
    )
    .bar;
    let fein = bar_from_shape(
        0,
        &CandleShape {
            body_ratio: 0.5,
            upper_wick_ratio: 0.25,
            lower_wick_ratio: 0.25,
            relative_range: 1.0,
            bullish: true,
        },
        100.0,
        3.0,
        1.0,
    )
    .bar;
    assert!((grob.high - fein.high).abs() < TOL);
    assert!((grob.low - fein.low).abs() < TOL);
    assert!((grob.close - fein.close).abs() < TOL);
}

#[test]
fn with_body_splits_the_rest_evenly() {
    let shape = CandleShape::with_body(0.4, 1.0, true);
    assert!((shape.upper_wick_ratio - 0.3).abs() < TOL);
    assert!((shape.lower_wick_ratio - 0.3).abs() < TOL);
}

/// Head and shoulders: five pivots, high · low · higher high · low · lower high.
fn head_and_shoulders() -> Vec<Pivot> {
    vec![
        (0, 100.0),
        (10, 112.0),
        (20, 104.0),
        (32, 124.0),
        (44, 103.0),
        (54, 111.0),
        (64, 96.0),
    ]
}

#[test]
fn every_pivot_bar_carries_its_price_as_an_extreme() {
    let pivots = head_and_shoulders();
    let bars = bars_from_pivots(7, &pivots, 0.8, 100.0);
    assert_eq!(bars.len(), 65);

    for (i, (index, price)) in pivots.iter().enumerate() {
        let bar = &bars[*index].bar;
        let vorher = if i == 0 { pivots[1].1 } else { pivots[i - 1].1 };
        if *price > vorher {
            assert!(
                (bar.high - price).abs() < TOL,
                "peak at {index}: high {} != {price}",
                bar.high
            );
        } else {
            assert!(
                (bar.low - price).abs() < TOL,
                "trough at {index}: low {} != {price}",
                bar.low
            );
        }
    }
}

#[test]
fn each_pivot_is_a_strict_local_extreme() {
    // Local, not global: the final trough of a head and shoulders lies below the left shoulder's
    // trough, and that is the pattern and not a defect. What has to hold is that nothing between
    // the neighbouring pivots reaches past the one in the middle — that is what a pivot detector
    // with a depth up to the pivot spacing sees.
    let pivots = head_and_shoulders();
    let bars = bars_from_pivots(7, &pivots, 0.8, 100.0);

    for (i, (index, price)) in pivots.iter().enumerate() {
        let vorher = if i == 0 { pivots[1].1 } else { pivots[i - 1].1 };
        let peak = *price > vorher;
        let von = if i == 0 { 0 } else { pivots[i - 1].0 };
        let bis = if i + 1 < pivots.len() {
            pivots[i + 1].0
        } else {
            bars.len() - 1
        };

        for j in von..=bis {
            if j == *index {
                continue;
            }
            let bar = &bars[j].bar;
            if peak {
                assert!(
                    bar.high < *price - f64::EPSILON,
                    "bar {j} reaches {} above peak {price} at {index}",
                    bar.high
                );
            } else {
                assert!(
                    bar.low > *price + f64::EPSILON,
                    "bar {j} reaches {} below trough {price} at {index}",
                    bar.low
                );
            }
        }
    }
}

#[test]
fn full_liveliness_still_leaves_the_structure_intact() {
    // The most movement the bound allows. The pattern has to survive it.
    let pivots = head_and_shoulders();
    let bars = bars_from_pivots(3, &pivots, 1.0, 100.0);
    let head = &bars[32].bar;
    assert!((head.high - 124.0).abs() < TOL);
    for (j, bar) in bars.iter().enumerate() {
        if j != 32 {
            assert!(bar.bar.high < 124.0 + TOL, "bar {j} overtops the head");
        }
    }
}

#[test]
fn a_malformed_pivot_list_yields_nothing() {
    // Not strictly increasing — an empty series is louder than a silently wrong one.
    assert!(bars_from_pivots(1, &[(10, 100.0), (10, 110.0)], 0.5, 1.0).is_empty());
    assert!(bars_from_pivots(1, &[(20, 100.0), (10, 110.0)], 0.5, 1.0).is_empty());
    assert!(bars_from_pivots(1, &[(0, 100.0)], 0.5, 1.0).is_empty());
}

#[test]
fn the_series_is_deterministic() {
    let pivots = head_and_shoulders();
    let a = bars_from_pivots(11, &pivots, 0.7, 100.0);
    let b = bars_from_pivots(11, &pivots, 0.7, 100.0);
    assert_eq!(a, b);
}

#[test]
fn gartley_prices_match_the_ratio_table() {
    // Hand-calculated from X = 100, A = 120, XA = 20:
    //   B = 120 − 0.618 · 20 = 107.64
    //   C = 107.64 + 0.5 · (120 − 107.64) = 113.82
    //   D = 120 − 0.786 · 20 = 104.28
    let [x, a, b, c, d] = xabcd_prices(100.0, 120.0, &HarmonicRatios::GARTLEY);
    assert!((x - 100.0).abs() < TOL);
    assert!((a - 120.0).abs() < TOL);
    assert!((b - 107.64).abs() < 1e-9);
    assert!((c - 113.82).abs() < 1e-9);
    assert!((d - 104.28).abs() < 1e-9);
}

#[test]
fn a_butterfly_extends_past_x_and_a_gartley_does_not() {
    // The one structural difference between the two families, and the reason `d` is not clamped.
    let [x, _, _, _, d_gartley] = xabcd_prices(100.0, 120.0, &HarmonicRatios::GARTLEY);
    let [_, _, _, _, d_butterfly] = xabcd_prices(100.0, 120.0, &HarmonicRatios::BUTTERFLY);
    assert!(d_gartley > x, "retracement stays inside XA");
    assert!(d_butterfly < x, "extension runs past X");
}

#[test]
fn the_structure_mirrors_for_a_bearish_reading() {
    let [x, a, b, c, d] = xabcd_prices(120.0, 100.0, &HarmonicRatios::GARTLEY);
    assert!(a < x, "A below X");
    assert!(b > a && b < x, "B retraces upward, short of X");
    assert!(c < b, "C turns back down");
    assert!(d > a && d < x, "D inside XA");
}

#[test]
fn xabcd_pivots_feed_the_series_constructor() {
    let pivots = xabcd_pivots(0, 12, 100.0, 120.0, &HarmonicRatios::GARTLEY);
    assert_eq!(pivots.len(), 5);
    assert_eq!(
        pivots.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 12, 24, 36, 48]
    );

    let bars = bars_from_pivots(5, &pivots, 0.3, 100.0);
    assert_eq!(bars.len(), 49);
    // A is the highest point of a bullish Gartley — the peak the whole structure hangs from.
    assert!((bars[12].bar.high - 120.0).abs() < TOL);
}
