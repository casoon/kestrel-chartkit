mod common;
use kestrel_chartkit::{
    analytics::*, indicator::smoothing::SmootherKind, Agreement, Bar, SignalDirection,
};
const BASE: &str = include_str!("fixtures/golden_analytics_components.txt");
const GOLD: &str = include_str!("fixtures/golden_analytics_composites.txt");
fn expected(key: &str) -> f64 {
    common::golden_value(GOLD, key)
}
fn base(key: &str) -> f64 {
    common::golden_value(BASE, key)
}
fn check(key: &str, value: f64) {
    common::assert_close(value, expected(key), 1e-9, key);
}
#[test]
fn persistence_and_trend_from_confirmed_components() {
    let bars: Vec<_> = (0..120)
        .map(|i| {
            let c = 100. + i as f64 * 0.5;
            Bar::new(i, c, c + 0.2, c - 0.2, c, 1.)
        })
        .collect();
    let p = trend_persistence_reading(&bars).unwrap();
    check("persistence_adx_score", p.adx_score);
    check("persistence_score", p.score);
    check("persistence_risk", p.transition_risk);
    assert_eq!(p.state, TrendPersistenceState::Strong);
    assert_eq!(p.direction, TrendPersistenceDirection::Up);
    assert_eq!(p.drag, TrendPersistenceSensor::Adx);
    // Feed confirmed sub-values, never the composite under test, into the trend calculation.
    let regime = RegimeReading {
        state: RegimeState::Trending,
        adx: base("ramp_adx"),
        choppiness: base("ramp_chop"),
        efficiency: base("ramp_er"),
        trend_votes: 3,
    };
    let trend = trend_reading(&bars, &regime, SmootherKind::Sma, 5, 0., 2., 0.01).unwrap();
    check("trend_sma_slope", trend.slope_pct);
    assert_eq!(trend.phase, MarketPhase::StrongUp);
}
#[test]
fn activity_and_sentiment_from_confirmed_components() {
    let bars: Vec<_> = [1., 4., 2., 3.]
        .iter()
        .map(|&v| Bar::new(0, 100., 101., 99., 100., v))
        .collect();
    let a = activity_reading(&bars, 0.5, true, 4);
    check("activity_score", a.score);
    let bars = vec![Bar::new(0, 109., 110., 100., 109., 10.); 40];
    let f = fear_greed_reading(&bars, None, None, None, None).unwrap();
    check("fear_greed_neutral_flow", f.score);
    assert_eq!(f.state, FearGreedState::Greed);
    assert_eq!(f.driver, FearGreedDriver::Flow);
    let fear = FearGaugeReading {
        state: FearGaugeState::FearSpike,
        wvf: base("fear_wvf"),
        bwvf: base("fear_bwvf"),
        absorbed: true,
    };
    let f = fear_greed_reading(&bars, None, None, Some(&fear), None).unwrap();
    check("fear_greed_absorbed_flow", f.score);
    assert_eq!(f.state, FearGreedState::Neutral);
}
#[test]
fn window_levels_match_analytical_prices() {
    let bars: Vec<_> = (100..120)
        .map(|i| {
            let c = i as f64;
            Bar::new(i, c, c + 1., c - 1., c, 1.)
        })
        .collect();
    let levels = cheat_sheet(&bars, 20).unwrap();
    for (label, key) in [
        ("Pivot", "level_pivot"),
        ("Pivot S1", "level_pivot_s1"),
        ("Pivot R1", "level_pivot_r1"),
        ("Pivot S2", "level_pivot_s2"),
        ("Pivot R2", "level_pivot_r2"),
        ("Fib 50.0%", "level_fib50"),
        ("SMA20", "level_sma20"),
        ("RSI 30", "level_rsi30"),
        ("RSI 50", "level_rsi50"),
        ("RSI 70", "level_rsi70"),
    ] {
        let l = levels.levels.iter().find(|l| l.label == label).unwrap();
        check(key, l.price);
        common::assert_close(
            l.distance_pct,
            100. * (expected(key) / 119. - 1.),
            1e-9,
            "distance",
        );
    }
    let support = levels.levels.iter().find(|l| l.label == "S1").unwrap();
    assert_eq!(support.price, 99.);
    assert_eq!(support.side, CheatSheetLevelSide::Below);
    let resistance = levels.levels.iter().find(|l| l.label == "R1").unwrap();
    assert_eq!(resistance.price, 120.);
    assert_eq!(resistance.side, CheatSheetLevelSide::Above);
}
#[test]
fn neutral_position_alignment_and_atr_distance() {
    use SignalDirection::*;
    let a = Agreement {
        direction: Bullish,
        agreement: 0.75,
        conflict: false,
    };
    assert_eq!(
        position_alignment(Bullish, Some(&a)),
        PositionAlignment::Aligned
    );
    assert_eq!(
        position_alignment(Bearish, Some(&a)),
        PositionAlignment::Opposed
    );
    assert_eq!(
        position_alignment(Neutral, Some(&a)),
        PositionAlignment::Neutral
    );
    assert_eq!(
        position_alignment(Bullish, None),
        PositionAlignment::Unconfigured
    );
    assert_eq!(
        position_alignment(
            Bullish,
            Some(&Agreement {
                conflict: true,
                ..a
            })
        ),
        PositionAlignment::Conflict
    );
    assert_eq!(atr_distance(100., 104., 2.), Some(2.));
    assert_eq!(atr_distance(104., 100., 2.), Some(2.));
    assert_eq!(atr_distance(100., 104., 0.), None);
}
