use kestrel_chartkit::model::InstrumentMeta;
use kestrel_chartkit::risk::{position_size, AccountRisk, StopDecision, StopManager};
use kestrel_chartkit::synthetic::{random_walk_bars, trending_bars};

#[test]
fn test_stop_manager_breakeven_trigger_on_synthetic_uptrend() {
    let stop_mgr = StopManager {
        breakeven_trigger_r: Some(1.5),
        time_stop_bars: None,
    };

    let entry = 100.0;
    let initial_stop = 98.0;
    let risk_per_unit = entry - initial_stop; // 2.0
    let trigger_price = entry + 1.5 * risk_per_unit; // 103.0

    // Uptrend with gentle slope (+0.4 per bar) plus noise
    let bars = trending_bars(42, 25, entry, 0.4, 0.05, 1000.0);
    let mut triggered = false;

    for (i, qb) in bars.iter().enumerate() {
        let decision = stop_mgr.evaluate(
            entry,
            qb.bar.close,
            risk_per_unit,
            i as u32,
            true, // is_long
        );

        if qb.bar.close < trigger_price {
            assert_eq!(
                decision,
                StopDecision::Hold,
                "Should Hold while close ({}) < trigger ({}) at bar {}",
                qb.bar.close,
                trigger_price,
                i
            );
        } else {
            assert_eq!(
                decision,
                StopDecision::MoveToBreakeven(entry),
                "Should MoveToBreakeven when close ({}) >= trigger ({}) at bar {}",
                qb.bar.close,
                trigger_price,
                i
            );
            triggered = true;
        }
    }

    assert!(
        triggered,
        "Breakeven should have triggered during the uptrend"
    );
}

#[test]
fn test_stop_manager_breakeven_trigger_on_synthetic_downtrend() {
    let stop_mgr = StopManager {
        breakeven_trigger_r: Some(2.0),
        time_stop_bars: None,
    };

    let entry: f64 = 100.0;
    let initial_stop: f64 = 102.5;
    let risk_per_unit = (initial_stop - entry).abs(); // 2.5
    let trigger_price = entry - 2.0 * risk_per_unit; // 95.0

    // Downtrend (-0.5 per bar) plus noise
    let bars = trending_bars(101, 25, entry, -0.5, 0.05, 1000.0);
    let mut triggered = false;

    for (i, qb) in bars.iter().enumerate() {
        let decision = stop_mgr.evaluate(
            entry,
            qb.bar.close,
            risk_per_unit,
            i as u32,
            false, // is_short
        );

        if qb.bar.close > trigger_price {
            assert_eq!(
                decision,
                StopDecision::Hold,
                "Should Hold while close ({}) > trigger ({}) at bar {}",
                qb.bar.close,
                trigger_price,
                i
            );
        } else {
            assert_eq!(
                decision,
                StopDecision::MoveToBreakeven(entry),
                "Should MoveToBreakeven on short when close ({}) <= trigger ({}) at bar {}",
                qb.bar.close,
                trigger_price,
                i
            );
            triggered = true;
        }
    }

    assert!(
        triggered,
        "Breakeven should have triggered during the short downtrend"
    );
}

#[test]
fn test_stop_manager_time_stop_across_random_walks() {
    let time_limit = 12;
    let stop_mgr = StopManager {
        breakeven_trigger_r: None,
        time_stop_bars: Some(time_limit),
    };

    for seed in 1..=20 {
        let bars = random_walk_bars(seed, 20, 100.0, 0.0, 0.5, 1000.0);

        for (i, qb) in bars.iter().enumerate() {
            let decision = stop_mgr.evaluate(100.0, qb.bar.close, 2.0, i as u32, true);

            if (i as u32) < time_limit {
                assert_eq!(decision, StopDecision::Hold);
            } else {
                assert_eq!(decision, StopDecision::TimeStopExit);
            }
        }
    }
}

#[test]
fn test_position_sizing_invariants_across_synthetic_prices() {
    let account = AccountRisk {
        equity: 100_000.0,
        risk_pct_per_trade: 0.01, // $1,000 budget
        max_leverage: 10.0,
        max_position_notional: Some(50_000.0),
    };
    let instrument = InstrumentMeta {
        symbol: "SYNTH".to_string(),
        tick_size: 0.01,
        price_precision: 2,
        timezone: "UTC".to_string(),
    };

    // Feed a random walk and size positions at varying entry and stop distances
    let bars = random_walk_bars(42, 30, 100.0, 0.05, 1.0, 1000.0);
    for qb in &bars {
        let entry = qb.bar.close;
        let stop = entry * 0.98; // 2% stop distance
        let tick_value = 1.0;

        let result = position_size(&account, &instrument, entry, stop, tick_value);

        // Basic invariants
        assert!(result.size > 0.0, "Position size must be positive");
        assert!(
            result.risk_amount <= 1000.0 + 1e-6,
            "Risk amount must never exceed risk budget"
        );
        let notional = result.size * entry;
        assert!(
            notional <= 50_000.0 + 1e-6,
            "Notional must never exceed max_position_notional"
        );
    }
}
