mod common;

use common::*;
use kestrel_chartkit::engine::acceptance_detector::*;
use kestrel_chartkit::engine::balance_classifier::*;
use kestrel_chartkit::engine::location_quality::*;
use kestrel_chartkit::engine::market_context::VolumeNodeKind;
use kestrel_chartkit::evaluation::*;
use kestrel_chartkit::model::MarketRegime;
use kestrel_chartkit::signal::TriggerAction;
use kestrel_chartkit::viz::*;

#[test]
fn test_m5_1_balance_imbalance_classification() {
    let bars = generate_flat_spread_bars(10, 100.0, 0.2, 1000.0);
    let out = classify_balance_imbalance(&bars, 2.0, 0.1);

    assert_eq!(out.state, MarketBalanceState::Balance);
    assert!(out.balance_confidence >= 0.50);
}

#[test]
fn test_m5_2_acceptance_and_rejection_detection() {
    let bars = generate_trend_bars(10, 100.0, 1.0, 1000.0); // Closes: 101, 102, ... 110
    let out = detect_acceptance_rejection(&bars, 105.0, 3);

    assert_eq!(
        out.acceptance_level,
        kestrel_chartkit::engine::market_context::AcceptanceLevel::High
    );
    assert!(out.consecutive_acceptance_bars >= 3);
}

#[test]
fn test_m5_3_location_quality_score() {
    let acc = AcceptanceDetectorOutput {
        acceptance_level: kestrel_chartkit::engine::market_context::AcceptanceLevel::High,
        rejection_kind: RejectionKind::None,
        consecutive_acceptance_bars: 5,
        level: 100.0,
    };

    let score = calculate_location_quality(
        101.0,
        100.0,
        2.0,
        VolumeNodeKind::Lvn,
        MarketRegime::BullishExpansion,
        &acc,
        true, // is_long
    );

    assert!(
        score.score >= 50.0,
        "High quality setup should score >= 50, got {}",
        score.score
    );
    assert!(score.distance_score > 0.0);
    assert!(score.regime_alignment_score > 0.0);
}

#[test]
fn test_m5_4_trade_stats_and_optimization_hook() {
    let records = vec![
        SignalEvaluationRecord {
            timestamp: 1000,
            trigger: TriggerAction::Buy,
            score: 0.8,
            confidence: 0.9,
            entry_price: 100.0,
            exit_price: 104.0,
            realized_r_multiple: 2.0,
            duration_bars: 5,
            outcome: TradeOutcome::Win,
        },
        SignalEvaluationRecord {
            timestamp: 2000,
            trigger: TriggerAction::Buy,
            score: 0.7,
            confidence: 0.8,
            entry_price: 104.0,
            exit_price: 102.0,
            realized_r_multiple: -1.0,
            duration_bars: 3,
            outcome: TradeOutcome::Loss,
        },
        SignalEvaluationRecord {
            timestamp: 3000,
            trigger: TriggerAction::Buy,
            score: 0.9,
            confidence: 0.95,
            entry_price: 102.0,
            exit_price: 107.0,
            realized_r_multiple: 2.5,
            duration_bars: 8,
            outcome: TradeOutcome::Win,
        },
    ];

    let stats = TradeStats::compute(&records);
    assert_eq!(stats.total_trades, 3);
    assert!((stats.winrate - 0.6666666666666666).abs() < 1e-5);
    assert!(stats.profit_factor > 1.0);
    assert!(stats.expectancy_r > 0.0);

    let mut hook = ParameterOptimizationHook::default_preset();
    hook.optimize_from_stats(&stats);
    assert!(hook.min_confidence_threshold <= 0.50);
}

#[test]
fn test_m5_5_svg_render_and_chart_dto_export() {
    let bars = generate_sine_bars(20, 100.0, 5.0, 10.0, 1000.0);
    let chart_bars: Vec<ChartBarData> = bars.iter().map(|b| b.into()).collect();

    let render_data = ChartRenderData {
        title: "Kestrel ChartKit Test Preview".to_string(),
        bars: chart_bars,
        series: vec![ChartSeries {
            name: "EMA 20".to_string(),
            color: "#29b6f6".to_string(),
            points: vec![(1000, 100.0), (1010, 102.0)],
        }],
        zones: vec![ChartZoneData {
            name: "Demand Zone".to_string(),
            price_top: 98.0,
            price_bottom: 95.0,
            color: "#00e676".to_string(),
        }],
        markers: vec![ChartMarkerData {
            timestamp: 1000,
            price: 100.0,
            label: "BUY".to_string(),
            action: TriggerAction::Buy,
        }],
    };

    let svg_str = render_chart_svg(&render_data, 800, 400);

    assert!(svg_str.contains("<svg"));
    assert!(svg_str.contains("Kestrel ChartKit Test Preview"));
    assert!(svg_str.contains("Demand Zone"));
    assert!(svg_str.contains("</svg>"));
}
