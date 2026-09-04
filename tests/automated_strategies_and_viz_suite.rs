mod common;

use common::*;
use kestrel_chartkit::model::*;
use kestrel_chartkit::scoring::*;
use kestrel_chartkit::signal::*;
use kestrel_chartkit::viz::*;

/// AUTOMATED VERIFICATION SUITE: Strategien, Playbooks, Composite Signals & Visualisierung (SVG / DTO)

#[test]
fn test_automated_strategy_playbooks_under_all_regimes() {
    let regimes = vec![
        MarketRegime::BullishExpansion,
        MarketRegime::BearishExpansion,
        MarketRegime::Consolidation,
        MarketRegime::Transition,
    ];

    for regime in regimes {
        let bars = generate_trend_bars(30, 100.0, 1.0, 1000.0);
        let last_bar = bars.last().unwrap();

        let subscores = vec![
            SubScore {
                indicator: "rsi".to_string(),
                score: 0.8,
                raw_value: 28.0,
                reason: Some("RSI Oversold Bounce".to_string()),
            },
            SubScore {
                indicator: "macd".to_string(),
                score: 0.75,
                raw_value: 1.2,
                reason: Some("MACD Bullish Cross".to_string()),
            },
        ];

        let weights = WeightPreset::TrendFollowing.weights();
        let signal = aggregate_subscores(
            subscores,
            Some(&weights),
            regime,
            vec![],
            Some(last_bar),
            2.0, // ATR
        );

        assert!((-1.0..=1.0).contains(&signal.score));
        assert!((0.0..=1.0).contains(&signal.confidence));

        match signal.trigger {
            TriggerAction::Buy | TriggerAction::Sell => {
                assert!(
                    signal.target_zone.is_some(),
                    "Trigger {:?} missing target zone for regime {:?}",
                    signal.trigger,
                    regime
                );
                assert!(
                    signal.invalidation_zone.is_some(),
                    "Trigger {:?} missing invalidation zone for regime {:?}",
                    signal.trigger,
                    regime
                );
                assert!(
                    signal.setup_duration.is_some(),
                    "Trigger {:?} missing setup duration for regime {:?}",
                    signal.trigger,
                    regime
                );
            }
            TriggerAction::Exit | TriggerAction::Hold => {}
        }
    }
}

#[test]
fn test_automated_visualization_svg_and_dto_integrity() {
    let bars = generate_sine_bars(50, 100.0, 10.0, 25.0, 5000.0);
    let chart_bars: Vec<ChartBarData> = bars.iter().map(|b| b.into()).collect();

    let render_data = ChartRenderData {
        title: "Automated Strategy Visualizer Test".to_string(),
        bars: chart_bars,
        series: vec![
            ChartSeries {
                name: "EMA 20".to_string(),
                color: "#29b6f6".to_string(),
                points: vec![(1000, 100.0), (1020, 105.0)],
            },
            ChartSeries {
                name: "VWAP".to_string(),
                color: "#ab47bc".to_string(),
                points: vec![(1000, 99.5), (1020, 104.2)],
            },
        ],
        zones: vec![
            ChartZoneData {
                name: "Value Area High".to_string(),
                price_top: 110.0,
                price_bottom: 108.0,
                color: "#ef5350".to_string(),
            },
            ChartZoneData {
                name: "Value Area Low".to_string(),
                price_top: 92.0,
                price_bottom: 90.0,
                color: "#26a69a".to_string(),
            },
        ],
        markers: vec![
            ChartMarkerData {
                timestamp: 1000,
                price: 95.0,
                label: "BUY TRIGGER".to_string(),
                action: TriggerAction::Buy,
            },
            ChartMarkerData {
                timestamp: 1020,
                price: 108.0,
                label: "TAKE PROFIT".to_string(),
                action: TriggerAction::Exit,
            },
        ],
    };

    let svg_output = render_chart_svg(&render_data, 1200, 600);

    // Validate SVG structure and elements
    assert!(svg_output.starts_with("<svg"));
    assert!(svg_output.ends_with("</svg>"));
    assert!(svg_output.contains("Automated Strategy Visualizer Test"));
    assert!(svg_output.contains("Value Area High"));
    assert!(svg_output.contains("Value Area Low"));
    assert!(svg_output.contains("fill=\"#00e676\"")); // Buy marker green
    assert!(svg_output.contains("fill=\"#ff9100\"")); // Exit marker orange
}
