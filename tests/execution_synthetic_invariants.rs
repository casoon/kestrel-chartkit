use kestrel_chartkit::execution::{
    submit_bracket, FillSimulator, FillSimulatorConfig, OrderKind, OrderSide,
};
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::synthetic::{random_walk_bars, trending_bars};

#[test]
fn test_trailing_stop_ratchets_in_synthetic_uptrend() {
    let mut sim = FillSimulator::new(FillSimulatorConfig::default());

    // Enter a long position of 10 units at 100.0
    sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
    sim.on_bar(&Bar::new(0, 100.0, 100.5, 99.5, 100.0, 1000.0), 0);
    assert_eq!(sim.position().quantity, 10.0);

    // Submit a trailing stop sell order trailing by 3.0 points
    let trail_amount = 3.0;
    let trail_id = sim
        .submit(OrderSide::Sell, OrderKind::Trailing { trail_amount }, 10.0)
        .expect("submit trailing order");

    // Feed a synthetic uptrend (20 bars climbing from 100.0 towards ~120.0)
    let up_bars = trending_bars(42, 20, 100.0, 1.0, 0.1, 1000.0);
    let mut highest_stop = f64::MIN;

    for qb in &up_bars {
        sim.on_bar(&qb.bar, qb.bar.timestamp);

        // Find the trailing order
        let order = sim
            .open_orders()
            .find(|o| o.id == trail_id)
            .expect("order should stay open while price rises");

        let stop_price = order
            .trailing_stop_price
            .expect("trailing stop price must be set");

        // Trailing stop on long position must strictly ratchet upwards (or stay equal), never drop
        assert!(
            stop_price >= highest_stop - 1e-9,
            "Trailing stop ratcheted downwards! previous max {highest_stop}, got {stop_price}"
        );
        highest_stop = stop_price;

        // Stop price must be exactly high - trail_amount (or higher from earlier bars)
        assert!(stop_price >= qb.bar.high - trail_amount - 1e-9);
    }

    assert!(
        highest_stop > 110.0,
        "Trailing stop should have ratcheted well above 110.0, got {highest_stop}"
    );

    // Now feed a pullback bar that dips below the ratcheted trailing stop
    let pullback_bar = Bar::new(
        100_000,
        highest_stop + 1.0,
        highest_stop + 1.0,
        highest_stop - 2.0,
        highest_stop - 1.0,
        1000.0,
    );
    let fills = sim.on_bar(&pullback_bar, pullback_bar.timestamp);

    assert_eq!(
        fills.len(),
        1,
        "Trailing stop should have filled on pullback"
    );
    assert_eq!(
        sim.position().quantity,
        0.0,
        "Position should be fully closed"
    );
    assert!(
        sim.position().realized_pnl > 0.0,
        "Trade should have closed in profit"
    );
}

#[test]
fn test_trailing_stop_ratchets_in_synthetic_downtrend() {
    let mut sim = FillSimulator::new(FillSimulatorConfig::default());

    // Enter a short position of 10 units at 200.0
    sim.submit(OrderSide::Sell, OrderKind::Market, 10.0);
    sim.on_bar(&Bar::new(0, 200.0, 200.5, 199.5, 200.0, 1000.0), 0);
    assert_eq!(sim.position().quantity, -10.0);

    // Trailing stop buy order trailing by 4.0 points above the low
    let trail_amount = 4.0;
    let trail_id = sim
        .submit(OrderSide::Buy, OrderKind::Trailing { trail_amount }, 10.0)
        .expect("submit trailing order");

    // Feed a synthetic downtrend (20 bars descending from 200.0 towards ~180.0)
    let down_bars = trending_bars(101, 20, 200.0, -1.0, 0.1, 1000.0);
    let mut lowest_stop = f64::MAX;

    for qb in &down_bars {
        sim.on_bar(&qb.bar, qb.bar.timestamp);

        let order = sim
            .open_orders()
            .find(|o| o.id == trail_id)
            .expect("order should stay open while price drops");

        let stop_price = order
            .trailing_stop_price
            .expect("trailing stop price must be set");

        // Trailing stop on short position must strictly ratchet downwards (or stay equal), never rise
        assert!(
            stop_price <= lowest_stop + 1e-9,
            "Trailing stop ratcheted upwards on short! previous min {lowest_stop}, got {stop_price}"
        );
        lowest_stop = stop_price;
    }

    assert!(
        lowest_stop < 190.0,
        "Trailing stop should have ratcheted down below 190.0, got {lowest_stop}"
    );

    // Feed a bounce bar that surges above the ratcheted trailing stop
    let bounce_bar = Bar::new(
        100_000,
        lowest_stop - 1.0,
        lowest_stop + 2.0,
        lowest_stop - 1.0,
        lowest_stop + 1.0,
        1000.0,
    );
    let fills = sim.on_bar(&bounce_bar, bounce_bar.timestamp);

    assert_eq!(fills.len(), 1, "Trailing stop should have filled on bounce");
    assert_eq!(
        sim.position().quantity,
        0.0,
        "Short position should be closed"
    );
    assert!(
        sim.position().realized_pnl > 0.0,
        "Short trade should have closed in profit"
    );
}

#[test]
fn test_bracket_orders_clean_resolution_across_random_walk_seeds() {
    // Test across 20 distinct random walk market paths
    for seed in 1..=20 {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        let entry_price = 100.0;
        let tp_price = 106.0;
        let sl_price = 94.0;
        let qty = 5.0;

        let (_entry_id, stop_id, target_id) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            qty,
            OrderKind::Market,
            sl_price,
            tp_price,
        )
        .expect("bracket submission");

        // Feed random walk bars
        let bars = random_walk_bars(seed, 60, entry_price, 0.0, 1.2, 1000.0);
        let mut exit_triggered = false;

        for qb in &bars {
            let fills = sim.on_bar(&qb.bar, qb.bar.timestamp);
            for fill in fills {
                if fill.order_id == stop_id {
                    // Stop filled: cancel target
                    sim.cancel(target_id);
                    exit_triggered = true;
                } else if fill.order_id == target_id {
                    // Target filled: cancel stop
                    sim.cancel(stop_id);
                    exit_triggered = true;
                }
            }

            if exit_triggered {
                break;
            }
        }

        // If an exit triggered, position must be completely flat
        if exit_triggered {
            assert_eq!(
                sim.position().quantity,
                0.0,
                "Position must be 0.0 after bracket exit on seed {seed}"
            );
            let open_exit_orders = sim
                .open_orders()
                .filter(|o| o.id == stop_id || o.id == target_id)
                .count();
            assert_eq!(
                open_exit_orders, 0,
                "No bracket exit orders should remain open on seed {seed}"
            );
        }
    }
}
