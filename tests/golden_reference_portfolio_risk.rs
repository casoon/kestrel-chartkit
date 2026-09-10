//! Independent golden-reference and hand-calculation tests for PortfolioSnapshot,
//! exposures, cashflow-adjusted equity returns, drawdowns, and historical VaR/Expected Shortfall
//!.

use kestrel_chartkit::contract::{ContractSpec, Currency, InstrumentType};
use kestrel_chartkit::portfolio::{
    calculate_return_metrics, cashflow_adjusted_return, compute_drawdown, evaluate_portfolio,
    historical_var_and_es, volatility_targeting_scale, CashLedger, PositionSide, PositionSnapshot,
};

#[test]
fn test_golden_opposing_positions_net_zero_gross_positive() {
    // Acceptance criterion 1: Zwei gegenläufige Positionen mit Net=0, aber Gross>0.
    // FDAX: 20,000 pts, multiplier 25 EUR/pt -> notional 500,000 EUR per contract
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

    // Long 1 contract FDAX at 20,000 (stop 19,980, 20 pts risk = 500 EUR)
    let long_pos = PositionSnapshot {
        symbol: "FDAX".to_string(),
        spec: spec.clone(),
        side: PositionSide::Long,
        quantity: 1.0,
        entry_price: 20_000.0,
        current_price: 20_000.0,
        stop_price: Some(19_980.0),
        fx_to_account: 1.0,
    };

    // Short 1 contract FDAX at 20,000 (stop 20,020, 20 pts risk = 500 EUR)
    let short_pos = PositionSnapshot {
        symbol: "FDAX".to_string(),
        spec,
        side: PositionSide::Short,
        quantity: 1.0,
        entry_price: 20_000.0,
        current_price: 20_000.0,
        stop_price: Some(20_020.0),
        fx_to_account: 1.0,
    };

    let snapshot = evaluate_portfolio(Currency::eur(), &ledger, &[long_pos, short_pos]).unwrap();

    assert_eq!(snapshot.long_notional, 500_000.0);
    assert_eq!(snapshot.short_notional, 500_000.0);
    assert_eq!(snapshot.gross_exposure, 1_000_000.0);
    assert_eq!(snapshot.net_exposure, 0.0);
    assert_eq!(snapshot.gross_leverage, 10.0); // 1,000,000 / 100,000
    assert_eq!(snapshot.net_leverage, 0.0);
    assert_eq!(snapshot.equity, 100_000.0);
    // Combined stop risk: 500 + 500 = 1000 EUR
    assert_eq!(snapshot.total_stop_risk, 1000.0);
    // Concentration: 500k / 1M = 0.5 (50%)
    assert_eq!(snapshot.max_position_concentration, 0.5);
}

#[test]
fn test_golden_deposit_does_not_create_return() {
    // Acceptance criterion 2: Einzahlung erzeugt keinen Gewinn.
    // Starting equity 100k, deposit 50k, ending equity 150k -> 0.0% return
    let r_zero = cashflow_adjusted_return(100_000.0, 150_000.0, 50_000.0, 1.0).unwrap();
    assert_eq!(
        r_zero, 0.0,
        "Deposit with zero market move must yield 0.0% return"
    );

    // Deposit 50k and genuine 10k trading profit -> ending equity 160k
    // True return on total invested capital (100k + 50k = 150k): 10k / 150k = 6.666667%
    let r_gain = cashflow_adjusted_return(100_000.0, 160_000.0, 50_000.0, 1.0).unwrap();
    assert!((r_gain - (10_000.0 / 150_000.0)).abs() < 1e-9);
}

#[test]
fn test_golden_fx_changes_account_value() {
    // Acceptance criterion 3: FX verändert Kontowert.
    // 100 shares of US equity at 150 USD, entry at 150 USD (zero price PnL in USD)
    let spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 1.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::Equity,
    };
    let ledger = CashLedger {
        cash: 0.0,
        ..Default::default()
    };

    // Case A: EUR/USD = 1.08 -> 1 USD = (1 / 1.08) EUR
    let pos_a = PositionSnapshot {
        symbol: "AAPL".to_string(),
        spec: spec.clone(),
        side: PositionSide::Long,
        quantity: 100.0,
        entry_price: 150.0,
        current_price: 150.0,
        stop_price: None,
        fx_to_account: 1.0 / 1.08,
    };
    let snap_a = evaluate_portfolio(Currency::eur(), &ledger, &[pos_a]).unwrap();
    let expected_val_a = 15_000.0 / 1.08;
    assert!((snap_a.equity - 0.0).abs() < 1e-9); // Unrealized PnL is 0 since current == entry
    assert!((snap_a.gross_exposure - expected_val_a).abs() < 1e-6);

    // Case B: USD strengthens to EUR/USD = 1.00 (parity)
    let pos_b = PositionSnapshot {
        symbol: "AAPL".to_string(),
        spec,
        side: PositionSide::Long,
        quantity: 100.0,
        entry_price: 150.0,
        current_price: 150.0,
        stop_price: None,
        fx_to_account: 1.0,
    };
    let snap_b = evaluate_portfolio(Currency::eur(), &ledger, &[pos_b]).unwrap();
    assert_eq!(snap_b.gross_exposure, 15_000.0);
    // Notional value increased in EUR purely due to FX move:
    assert!(
        (snap_b.gross_exposure - snap_a.gross_exposure - (15_000.0 - expected_val_a)).abs() < 1e-6
    );
}

#[test]
fn test_golden_fees_reduce_equity() {
    // Acceptance criterion 4: Kosten senken Equity.
    let ledger = CashLedger {
        cash: 49_850.0, // starting 50,000 minus 150 fees
        cumulative_fees: 150.0,
        cumulative_deposits: 50_000.0,
        ..Default::default()
    };
    let snap = evaluate_portfolio(Currency::eur(), &ledger, &[]).unwrap();
    assert_eq!(snap.equity, 49_850.0);
    assert_eq!(snap.cumulative_fees, 150.0);
}

#[test]
fn test_golden_exact_small_loss_sample_var_and_es() {
    // Acceptance criterion 5: exakte kleine Verluststichprobe für VaR/ES.
    // 10 historical daily returns:
    let returns = vec![
        -0.05, -0.04, -0.03, -0.02, -0.01, 0.01, 0.02, 0.03, 0.04, 0.05,
    ];

    // 90% Confidence (alpha = 0.90, p = 0.10):
    // Tail size = ceil(0.10 * 10) = 1 item (the worst: -0.05)
    let risk_90 = historical_var_and_es(&returns, 0.90).unwrap();
    assert_eq!(risk_90.var, 0.05, "90% VaR must be 0.05 (5%)");
    assert_eq!(
        risk_90.expected_shortfall, 0.05,
        "90% Expected Shortfall must be 0.05 (5%)"
    );
    assert_eq!(risk_90.sample_count, 10);

    // 80% Confidence (alpha = 0.80, p = 0.20):
    // Tail size = ceil(0.20 * 10) = 2 items (worst: -0.05, -0.04)
    let risk_80 = historical_var_and_es(&returns, 0.80).unwrap();
    assert_eq!(risk_80.var, 0.04, "80% VaR must be 0.04 (4%)");
    // Expected shortfall is mean of losses: (0.05 + 0.04) / 2 = 0.045
    assert_eq!(
        risk_80.expected_shortfall, 0.045,
        "80% Expected Shortfall must be 0.045 (4.5%)"
    );
}

#[test]
fn test_golden_drawdown_stats_hand_calc() {
    // Hand-crafted equity series:
    // [100.0, 120.0, 110.0, 90.0, 105.0, 130.0, 117.0]
    // Peak 120 -> trough 90 -> max drawdown = 30.0 (30 / 120 = 25.0%)
    // Duration: bar 2 (110), bar 3 (90), bar 4 (105) = 3 bars
    // Peak 130 -> current 117 -> current drawdown = 13.0 (13 / 130 = 10.0%)
    let curve = vec![100.0, 120.0, 110.0, 90.0, 105.0, 130.0, 117.0];
    let dd = compute_drawdown(&curve);

    assert_eq!(dd.peak_equity, 130.0);
    assert_eq!(dd.max_drawdown, 30.0);
    assert_eq!(dd.max_drawdown_pct, 0.25);
    assert_eq!(dd.max_drawdown_duration_bars, 3);
    assert_eq!(dd.current_drawdown, 13.0);
    assert_eq!(dd.current_drawdown_pct, 0.10);
}

#[test]
fn test_golden_volatility_targeting_scale() {
    // Current vol 20%, Target vol 10%, Max leverage 3.0 -> scale = 0.50
    let scale_down = volatility_targeting_scale(0.20, 0.10, 3.0);
    assert_eq!(scale_down, 0.50);

    // Current vol 5%, Target vol 15%, Max leverage 2.0 -> raw scale 3.0, capped at 2.0
    let scale_up = volatility_targeting_scale(0.05, 0.15, 2.0);
    assert_eq!(scale_up, 2.0);
}

#[test]
fn test_golden_sharpe_and_sortino_hand_calc() {
    // Symmetrical returns: [-0.02, 0.04] (2 periods)
    // Mean = 0.01
    // Annualized with 252 periods:
    // Annual rf = 0.0
    // Sample variance ((-0.03)^2 + 0.03^2) / 1 = 0.0018, so Sharpe = 0.01 / sqrt(0.0018) * sqrt(252)
    // = sqrt(0.0252 / 0.0018) = sqrt(14). Downside variance (0.02^2 + 0) / 2 = 0.0002, so
    // Sortino = sqrt(0.0252 / 0.0002) = sqrt(126); annualized volatility sqrt(0.0018 * 252).
    let returns = vec![-0.02, 0.04];
    let metrics = calculate_return_metrics(&returns, 0.0, 252.0).unwrap();
    assert_eq!(metrics.mean_return, 0.01);
    assert_eq!(metrics.sample_count, 2);
    assert!(metrics.sharpe_ratio > 0.0);
    assert!(metrics.sortino_ratio > 0.0);
    assert!((metrics.sharpe_ratio - 14.0_f64.sqrt()).abs() < 1e-12);
    assert!((metrics.sortino_ratio - 126.0_f64.sqrt()).abs() < 1e-12);
    assert!((metrics.annualized_volatility - (0.0018_f64 * 252.0).sqrt()).abs() < 1e-12);
}
