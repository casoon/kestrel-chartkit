use kestrel_chartkit::{
    aggregate_subscores, build, classify_regime, find_sr_zones, score_indicator, Bar,
    SignalDirection,
};
use std::collections::HashMap;

#[test]
fn test_composite_signal_generation() {
    let mut rsi = build("rsi", &HashMap::new()).expect("RSI builder failed");
    let mut macd = build("macd", &HashMap::new()).expect("MACD builder failed");

    let mut subscores = Vec::new();
    let mut bars = Vec::new();

    // 1. Drop price to trigger oversold RSI
    for i in 0..30 {
        let price = 100.0 - (i as f64) * 2.0;
        let bar = Bar::new(
            i * 3600,
            price + 0.5,
            price + 0.5,
            price - 0.5,
            price,
            1000.0,
        );
        rsi.on_bar(&bar);
        macd.on_bar(&bar);
        bars.push(bar);
    }

    // 2. Bounce price up to trigger bullish cross out of oversold
    for i in 30..40 {
        let price = 40.0 + ((i - 30) as f64) * 3.0;
        let bar = Bar::new(
            i * 3600,
            price - 0.5,
            price + 0.5,
            price - 0.5,
            price,
            1000.0,
        );
        let rsi_out = rsi.on_bar(&bar);
        let macd_out = macd.on_bar(&bar);
        bars.push(bar.clone());

        if i == 39 {
            if let Some(out) = rsi_out {
                subscores.push(score_indicator("rsi", &out, &rsi.alerts()));
            }
            if let Some(out) = macd_out {
                subscores.push(score_indicator("macd", &out, &macd.alerts()));
            }
        }
    }

    assert_eq!(subscores.len(), 2);

    let regime = classify_regime(&bars, 25.0, 1.5);
    let sr_zones = find_sr_zones(&bars, 5);

    let composite = aggregate_subscores(subscores, None, regime, sr_zones, bars.last(), 1.5);

    assert_eq!(composite.direction, SignalDirection::Bullish);
    assert!(composite.score > 0.0);
    assert!(composite.heat_score > 0.0);
    assert!(composite.risk_plan.is_some());
}
