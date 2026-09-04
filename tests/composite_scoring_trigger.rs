use kestrel_chartkit::engine::market_state::PlaybookState;
use kestrel_chartkit::model::{Bar, MarketRegime};
use kestrel_chartkit::scoring::composite::aggregate_subscores;
use kestrel_chartkit::signal::{PermissionGrade, SignalDirection, SubScore, TriggerAction};
use std::collections::HashMap;

fn directional_subscores(score: f64) -> Vec<SubScore> {
    vec![
        SubScore {
            indicator: "rsi".to_string(),
            score,
            raw_value: 50.0,
            reason: None,
        },
        SubScore {
            indicator: "macd".to_string(),
            score,
            raw_value: score,
            reason: None,
        },
    ]
}

#[test]
fn test_m3_1_playbook_state_permission_grade_conversion() {
    let p_pref: PermissionGrade = PlaybookState::Preferred.into();
    assert_eq!(p_pref, PermissionGrade::ClearToTrade);

    let p_neut: PermissionGrade = PlaybookState::Neutral.into();
    assert_eq!(p_neut, PermissionGrade::Caution);

    let p_supp: PermissionGrade = PlaybookState::Suppressed.into();
    assert_eq!(p_supp, PermissionGrade::Veto);

    let p_not: PermissionGrade = PlaybookState::NotAllowed.into();
    assert_eq!(p_not, PermissionGrade::Veto);

    // Inverse conversion
    let s_clear: PlaybookState = PermissionGrade::ClearToTrade.into();
    assert_eq!(s_clear, PlaybookState::Preferred);
}

#[test]
fn test_m3_2_bullish_trigger_target_invalidation_generation() {
    let subscores = vec![
        SubScore {
            indicator: "rsi".to_string(),
            score: 0.8,
            raw_value: 25.0,
            reason: Some("RSI oversold reversal".to_string()),
        },
        SubScore {
            indicator: "macd".to_string(),
            score: 0.9,
            raw_value: 1.2,
            reason: Some("MACD bullish cross".to_string()),
        },
    ];

    let bar = Bar::new(1000, 100.0, 105.0, 95.0, 100.0, 1000.0);
    let composite = aggregate_subscores(
        subscores,
        None,
        MarketRegime::BullishExpansion,
        Vec::new(),
        Some(&bar),
        2.0, // ATR = 2.0
    );

    assert_eq!(composite.direction, SignalDirection::Bullish);
    assert_eq!(composite.permission, PermissionGrade::ClearToTrade);
    assert_eq!(composite.trigger, TriggerAction::Buy);

    // TargetZone: price + 2 * ATR = 100 + 4 = 104
    let target = composite.target_zone.expect("TargetZone expected");
    assert_eq!(target.price, 104.0);
    assert_eq!(target.reward_atr, 2.0);

    // InvalidationZone: price - 1.5 * ATR = 100 - 3 = 97
    let invalidation = composite
        .invalidation_zone
        .expect("InvalidationZone expected");
    assert_eq!(invalidation.price, 97.0);
    assert_eq!(invalidation.risk_atr, 1.5);

    // SetupDuration: max_bars = 20
    let duration = composite.setup_duration.expect("SetupDuration expected");
    assert_eq!(duration.max_bars, 20);
    assert_eq!(duration.elapsed_bars, 0);
}

#[test]
fn test_m3_3_bearish_trigger_and_zones() {
    let subscores = vec![
        SubScore {
            indicator: "rsi".to_string(),
            score: -0.85,
            raw_value: 78.0,
            reason: Some("RSI overbought reversal".to_string()),
        },
        SubScore {
            indicator: "macd".to_string(),
            score: -0.90,
            raw_value: -1.5,
            reason: Some("MACD bearish cross".to_string()),
        },
    ];

    let bar = Bar::new(1000, 200.0, 205.0, 195.0, 200.0, 1000.0);
    let composite = aggregate_subscores(
        subscores,
        None,
        MarketRegime::BearishExpansion,
        Vec::new(),
        Some(&bar),
        4.0, // ATR = 4.0
    );

    assert_eq!(composite.direction, SignalDirection::Bearish);
    assert_eq!(composite.permission, PermissionGrade::ClearToTrade);
    assert_eq!(composite.trigger, TriggerAction::Sell);

    // TargetZone: price - 2 * ATR = 200 - 8 = 192
    let target = composite.target_zone.expect("TargetZone expected");
    assert_eq!(target.price, 192.0);

    // InvalidationZone: price + 1.5 * ATR = 200 + 6 = 206
    let invalidation = composite
        .invalidation_zone
        .expect("InvalidationZone expected");
    assert_eq!(invalidation.price, 206.0);
}

#[test]
fn test_regime_gate_blocks_opposed_and_transition_entries() {
    let bar = Bar::new(1000, 100.0, 105.0, 95.0, 100.0, 1000.0);

    let cases = [
        (
            0.9,
            MarketRegime::BullishExpansion,
            PermissionGrade::ClearToTrade,
            TriggerAction::Buy,
        ),
        (
            0.9,
            MarketRegime::BearishExpansion,
            PermissionGrade::Veto,
            TriggerAction::Hold,
        ),
        (
            -0.9,
            MarketRegime::BearishExpansion,
            PermissionGrade::ClearToTrade,
            TriggerAction::Sell,
        ),
        (
            -0.9,
            MarketRegime::BullishExpansion,
            PermissionGrade::Veto,
            TriggerAction::Hold,
        ),
        (
            0.9,
            MarketRegime::Transition,
            PermissionGrade::Caution,
            TriggerAction::Hold,
        ),
        (
            0.9,
            MarketRegime::Consolidation,
            PermissionGrade::ClearToTrade,
            TriggerAction::Buy,
        ),
    ];

    for (score, regime, expected_permission, expected_trigger) in cases {
        let signal = aggregate_subscores(
            directional_subscores(score),
            None,
            regime,
            Vec::new(),
            Some(&bar),
            2.0,
        );
        assert_eq!(signal.permission, expected_permission, "regime: {regime:?}");
        assert_eq!(signal.trigger, expected_trigger, "regime: {regime:?}");
    }
}

#[test]
fn test_non_finite_inputs_do_not_escape_composite_boundary() {
    let invalid_bar = Bar::new(1000, f64::NAN, 105.0, 95.0, 100.0, 1000.0);
    let weights = HashMap::from([("rsi".to_string(), f64::INFINITY)]);
    let mut subscores = directional_subscores(0.9);
    subscores.push(SubScore {
        indicator: "invalid_score".to_string(),
        score: f64::NAN,
        raw_value: 1.0,
        reason: Some("must be discarded".to_string()),
    });
    subscores.push(SubScore {
        indicator: "invalid_raw".to_string(),
        score: 1.0,
        raw_value: f64::INFINITY,
        reason: Some("must be discarded".to_string()),
    });

    let signal = aggregate_subscores(
        subscores,
        Some(&weights),
        MarketRegime::BullishExpansion,
        Vec::new(),
        Some(&invalid_bar),
        f64::INFINITY,
    );

    assert!(signal.score.is_finite());
    assert!(signal.confidence.is_finite());
    assert!(signal.heat_score.is_finite());
    assert_eq!(signal.per_indicator.len(), 2);
    assert!(signal.target_zone.is_none());
    assert!(signal.invalidation_zone.is_none());
    assert!(signal.setup_duration.is_none());
    assert!(signal.risk_plan.is_none());
}

#[test]
fn test_non_finite_atr_uses_finite_price_fallback() {
    let bar = Bar::new(1000, 100.0, 105.0, 95.0, 100.0, 1000.0);
    let signal = aggregate_subscores(
        directional_subscores(0.9),
        None,
        MarketRegime::BullishExpansion,
        Vec::new(),
        Some(&bar),
        f64::INFINITY,
    );

    let risk = signal
        .risk_plan
        .expect("aligned signal should have geometry");
    assert!(risk.stop_loss.is_finite());
    assert!(risk.target_1.is_finite());
    assert!(risk.target_2.is_finite());
}

#[test]
fn test_neutral_signal_has_no_trade_geometry() {
    let bar = Bar::new(1000, 100.0, 105.0, 95.0, 100.0, 1000.0);
    let signal = aggregate_subscores(
        directional_subscores(0.0),
        None,
        MarketRegime::Consolidation,
        Vec::new(),
        Some(&bar),
        2.0,
    );

    assert_eq!(signal.direction, SignalDirection::Neutral);
    assert_eq!(signal.trigger, TriggerAction::Hold);
    assert_eq!(signal.permission, PermissionGrade::Veto);
    assert!(signal.target_zone.is_none());
    assert!(signal.invalidation_zone.is_none());
    assert!(signal.setup_duration.is_none());
    assert!(signal.risk_plan.is_none());
}
