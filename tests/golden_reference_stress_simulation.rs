//! Independent golden-reference and hand-calculation tests for StressScenario,
//! stop-gap slippage execution, synchronized multi-asset block bootstrapping, and deterministic
//! equity path simulations per plan/07-stress-und-ausfuehrungsunsicherheit.md and CLAUDE.md.

use kestrel_chartkit::contract::{ContractSpec, Currency, InstrumentType};
use kestrel_chartkit::execution::ExecutionCosts;
use kestrel_chartkit::portfolio::{CashLedger, PositionSide, PositionSnapshot};
use kestrel_chartkit::stress::{
    apply_portfolio_stress, multi_asset_block_bootstrap, simulate_equity_paths,
    simulate_stop_gap_execution, PathSimulationSummary, StressScenario,
};

#[test]
fn test_golden_stress_neutral_scenario_reproduces_baseline() {
    // Acceptance criterion: "alle Stressfaktoren neutral reproduzieren Basislauf"
    let ledger = CashLedger {
        cash: 100_000.0,
        ..Default::default()
    };
    let spec = ContractSpec {
        price_currency: Currency::eur(),
        settlement_currency: Currency::eur(),
        multiplier: 25.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::LinearFuture,
    };

    let pos = PositionSnapshot {
        symbol: "FDAX".to_string(),
        spec,
        side: PositionSide::Long,
        quantity: 2.0,
        entry_price: 19_000.0,
        current_price: 19_500.0,
        stop_price: Some(18_800.0),
        fx_to_account: 1.0,
    };

    let baseline = kestrel_chartkit::portfolio::evaluate_portfolio(
        Currency::eur(),
        &ledger,
        std::slice::from_ref(&pos),
    )
    .unwrap();

    let neutral_scenario = StressScenario::neutral();
    let stressed =
        apply_portfolio_stress(Currency::eur(), &ledger, &[pos], &neutral_scenario).unwrap();

    assert_eq!(stressed.snapshot.equity, baseline.equity);
    assert_eq!(stressed.snapshot.gross_exposure, baseline.gross_exposure);
    assert_eq!(stressed.snapshot.net_exposure, baseline.net_exposure);
    assert_eq!(stressed.snapshot.unrealized_pnl, baseline.unrealized_pnl);
    assert_eq!(stressed.equity_change, 0.0);
    assert_eq!(stressed.equity_change_pct, 0.0);
    assert_eq!(stressed.gross_exposure_change, 0.0);
}

#[test]
fn test_golden_portfolio_stress_market_crash_and_fx_shock() {
    // Hand calculation for market shock:
    // Long 1 contract ES: entry 5000, current 5200, mult 50.
    // Unrealized base PnL = (5200 - 5000) * 1 * 50 = $10,000.
    // Base notional = 5200 * 50 = $260,000.
    // Cash = $50,000 -> Base Equity = $60,000.
    //
    // Apply Stress: -10% price crash (5200 * 0.90 = 4680)
    // Stressed current price = 4680.
    // Stressed PnL = (4680 - 5000) * 50 = -$16,000.
    // Stressed equity = 50,000 - 16,000 = $34,000.
    // Equity change = 34,000 - 60,000 = -$26,000.
    // Equity change % = -26,000 / 60,000 = -43.333333333333%
    // Stressed notional = 4680 * 50 = $234,000.
    // Gross exposure change = 234,000 - 260,000 = -$26,000.
    let ledger = CashLedger {
        cash: 50_000.0,
        ..Default::default()
    };
    let spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 50.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::LinearFuture,
    };

    let pos = PositionSnapshot {
        symbol: "ES".to_string(),
        spec,
        side: PositionSide::Long,
        quantity: 1.0,
        entry_price: 5000.0,
        current_price: 5200.0,
        stop_price: Some(4800.0),
        fx_to_account: 1.0,
    };

    let crash_scenario = StressScenario {
        price_shock_pct: -0.10,
        spread_multiplier: 2.0,
        slippage_multiplier: 2.0,
        fx_shock_pct: 0.0,
        participation_cap_multiplier: 0.5,
    };

    let res = apply_portfolio_stress(Currency::usd(), &ledger, &[pos], &crash_scenario).unwrap();

    assert_eq!(res.snapshot.equity, 34_000.0);
    assert_eq!(res.snapshot.unrealized_pnl, -16_000.0);
    assert_eq!(res.snapshot.gross_exposure, 234_000.0);
    assert_eq!(res.equity_change, -26_000.0);
    assert!((res.equity_change_pct - (-26_000.0 / 60_000.0)).abs() < 1e-12);
    assert_eq!(res.gross_exposure_change, -26_000.0);
}

#[test]
fn test_golden_stop_gap_slippage_execution() {
    // Acceptance criterion: Overnight gap / stop execution uncertainty
    //
    // Case 1: Long position with stop at 100.
    // Normal case: next open is 102 (no gap through stop), price dips during day to 98 -> stop fills at 100.
    let fill_normal = simulate_stop_gap_execution(100.0, 102.0, true);
    assert_eq!(fill_normal, 100.0);

    // Gap case: market gaps down overnight, opening at 92.0 (below stop 100).
    // Protective sell order cannot fill at 100; fills at 92.0.
    let fill_gap_down = simulate_stop_gap_execution(100.0, 92.0, true);
    assert_eq!(fill_gap_down, 92.0);

    // Case 2: Short position with protective buy stop at 100.
    // Normal case: next open is 98 (no gap through stop) -> stop fills at 100.
    let fill_short_normal = simulate_stop_gap_execution(100.0, 98.0, false);
    assert_eq!(fill_short_normal, 100.0);

    // Gap case: market gaps up overnight, opening at 108.0 (above stop 100).
    // Protective buy order fills at 108.0.
    let fill_short_gap_up = simulate_stop_gap_execution(100.0, 108.0, false);
    assert_eq!(fill_short_gap_up, 108.0);
}

#[test]
fn test_golden_deterministic_fee_and_spread_stress_scaling() {
    // Acceptance criterion: "höhere deterministische Gebühren senken Netto-P&L"
    let base_costs = ExecutionCosts {
        fee_pct: 0.001,       // 10 bps
        spread: 0.50,         // 50 cents
        slippage_pct: 0.0005, // 5 bps
    };

    let stress = StressScenario {
        price_shock_pct: 0.0,
        spread_multiplier: 3.0,
        slippage_multiplier: 4.0,
        fx_shock_pct: 0.0,
        participation_cap_multiplier: 1.0,
    };

    let stressed_costs = stress.stressed_costs(base_costs);

    assert_eq!(stressed_costs.fee_pct, 0.001);
    assert_eq!(stressed_costs.spread, 1.50); // 0.50 * 3.0
    assert_eq!(stressed_costs.slippage_pct, 0.0020); // 0.0005 * 4.0
}

#[test]
fn test_golden_synchronized_multi_asset_block_bootstrap_preserves_pairs() {
    // Acceptance criterion: "gemeinsame Asset-Resamples erhalten Zeitpaare"
    // Asset A: [10.0, 20.0, 30.0, 40.0, 50.0]
    // Asset B: [100.0, 200.0, 300.0, 400.0, 500.0] (Asset B = 10 * Asset A at every timestamp t)
    let asset_a = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    let asset_b = vec![100.0, 200.0, 300.0, 400.0, 500.0];

    let bootstrapped = multi_asset_block_bootstrap(
        &[asset_a, asset_b],
        2,     // block_size
        10,    // path_length
        5,     // num_paths
        12345, // seed
    )
    .unwrap();

    assert_eq!(bootstrapped.len(), 5); // 5 paths

    for path in &bootstrapped {
        assert_eq!(path.len(), 2); // 2 assets
        let path_a = &path[0];
        let path_b = &path[1];
        assert_eq!(path_a.len(), 10);
        assert_eq!(path_b.len(), 10);

        // Verify contemporaneous relationship B[t] == 10 * A[t] holds for every single step
        for t in 0..10 {
            assert!(
                (path_b[t] - path_a[t] * 10.0).abs() < 1e-12,
                "Contemporaneous pair relationship violated at step {t}: A={}, B={}",
                path_a[t],
                path_b[t]
            );
        }
    }
}

#[test]
fn test_golden_path_simulation_seed_determinism() {
    // Acceptance criterion: "Gleicher Seed reproduziert Ergebnis"
    let returns = vec![0.01, -0.005, 0.02, -0.015, 0.008, 0.012, -0.02, 0.005];
    let initial_equity = 10_000.0;
    let seed = 987654321;

    let run1 = simulate_equity_paths(initial_equity, &returns, 2, 20, 100, seed).unwrap();

    let run2 = simulate_equity_paths(initial_equity, &returns, 2, 20, 100, seed).unwrap();

    // Exact determinism: same quantiles, same terminal equity
    assert_eq!(
        run1.terminal_equity_quantiles,
        run2.terminal_equity_quantiles
    );
    assert_eq!(run1.max_drawdown_quantiles, run2.max_drawdown_quantiles);
    assert_eq!(
        run1.empirical_mean_terminal_equity,
        run2.empirical_mean_terminal_equity
    );
}

#[test]
fn test_golden_drawdown_exceedance_probability_calculation() {
    // Hand calculation:
    // Simulated max drawdowns for 10 paths:
    // [0.05, 0.08, 0.10, 0.12, 0.15, 0.18, 0.20, 0.22, 0.25, 0.30]
    // Exceeding 0.20 (>= 20%): 0.20, 0.22, 0.25, 0.30 -> exactly 4 out of 10 = 40% (0.40).
    let drawdowns = [0.05, 0.08, 0.10, 0.12, 0.15, 0.18, 0.20, 0.22, 0.25, 0.30];
    let prob_20 = PathSimulationSummary::probability_drawdown_exceeds(&drawdowns, 0.20);
    assert!((prob_20 - 0.40).abs() < 1e-12);

    let prob_50 = PathSimulationSummary::probability_drawdown_exceeds(&drawdowns, 0.50);
    assert_eq!(prob_50, 0.0);
}
