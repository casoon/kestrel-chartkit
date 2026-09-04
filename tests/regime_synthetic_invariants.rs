use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::model::{Bar, MarketRegime};
use kestrel_chartkit::regime::classify_regime;
use kestrel_chartkit::synthetic::{random_walk_bars, trending_bars};
use std::collections::HashMap;

#[test]
fn test_classify_regime_on_synthetic_uptrend() {
    // 35 bars of strong synthetic uptrend (+1.0 per bar)
    let qbars = trending_bars(42, 35, 100.0, 1.0, 0.05, 1000.0);
    let bars: Vec<Bar> = qbars.into_iter().map(|qb| qb.bar).collect();

    // High trend strength (ADX = 35.0)
    let regime = classify_regime(&bars, 35.0, 0.5);
    assert_eq!(
        regime,
        MarketRegime::BullishExpansion,
        "Strong synthetic uptrend must classify as BullishExpansion"
    );
}

#[test]
fn test_classify_regime_on_synthetic_downtrend() {
    // 35 bars of strong synthetic downtrend (-1.0 per bar)
    let qbars = trending_bars(101, 35, 100.0, -1.0, 0.05, 1000.0);
    let bars: Vec<Bar> = qbars.into_iter().map(|qb| qb.bar).collect();

    // High trend strength (ADX = 35.0)
    let regime = classify_regime(&bars, 35.0, 0.5);
    assert_eq!(
        regime,
        MarketRegime::BearishExpansion,
        "Strong synthetic downtrend must classify as BearishExpansion"
    );
}

#[test]
fn test_classify_regime_on_synthetic_flat_random_walk() {
    // 35 bars of very tight random walk without trend
    let qbars = random_walk_bars(777, 35, 100.0, 0.0, 0.005, 1000.0);
    let bars: Vec<Bar> = qbars.into_iter().map(|qb| qb.bar).collect();

    // Non-trending ADX (< 20.0) and low ATR (<= 0.02)
    let regime = classify_regime(&bars, 12.0, 0.01);
    assert_eq!(
        regime,
        MarketRegime::Consolidation,
        "Flat random walk with low ADX and low ATR must classify as Consolidation"
    );
}

#[test]
fn test_classify_regime_end_to_end_streaming_pipeline() {
    // Build real streaming ADX and ATR indicators from the registry
    let mut adx = build_checked("adx", &HashMap::from([("adx_len".to_string(), 14.0)])).unwrap();
    let mut atr = build_checked("atr", &HashMap::from([("atr_len".to_string(), 14.0)])).unwrap();

    // Generate 60 bars of a strong synthetic uptrend
    let qbars = trending_bars(999, 60, 50.0, 1.2, 0.1, 1000.0);
    let mut bars_history = Vec::new();
    let mut final_regime = MarketRegime::Transition;

    for qb in &qbars {
        bars_history.push(qb.bar.clone());
        let adx_out = adx.on_bar(&qb.bar);
        let atr_out = atr.on_bar(&qb.bar);

        if let (Some(adx_val), Some(atr_val)) = (adx_out, atr_out) {
            final_regime = classify_regime(&bars_history, adx_val.value, atr_val.value);
        }
    }

    assert_eq!(
        final_regime,
        MarketRegime::BullishExpansion,
        "End-to-end indicator pipeline over synthetic uptrend must converge to BullishExpansion"
    );
}
