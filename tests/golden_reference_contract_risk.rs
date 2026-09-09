//! Independent golden-reference and hand-calculation tests for ContractSpec,
//! currency/FX models, and risk position sizing per CLAUDE.md requirements.

use kestrel_chartkit::contract::{
    contract_pnl, contract_tick_value, notional_value, stop_risk_amount, ContractSpec, Currency,
    FxRate, InstrumentType, ValuationError,
};
use kestrel_chartkit::risk::{position_size_contract, AccountRisk};

#[test]
fn test_golden_hand_calc_fdax_future_leverage_capped() {
    // Exact hand calculation from plan/03-instrument-und-geldmodell.md:
    // Equity: 100,000 EUR
    // Risk: 1% (= 1,000 EUR)
    // Future price: 20,000 pts
    // Stop: 19,980 pts (20 pts risk)
    // Multiplier: 25 EUR/pt
    // Max leverage: 5x
    let account = AccountRisk {
        equity: 100_000.0,
        risk_pct_per_trade: 0.01,
        max_leverage: 5.0,
        max_position_notional: None,
    };
    let spec = ContractSpec {
        price_currency: Currency::eur(),
        settlement_currency: Currency::eur(),
        multiplier: 25.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::LinearFuture,
    };

    // 1. Single contract risk = 20 pts * 25 EUR/pt = 500 EUR
    let single_risk = stop_risk_amount(20_000.0, 19_980.0, 1.0, &spec, Some(1.0)).unwrap();
    assert_eq!(single_risk, 500.0);

    // 2. Single contract notional = 20,000 pts * 25 EUR/pt = 500,000 EUR
    let single_notional = notional_value(20_000.0, 1.0, &spec, Some(1.0)).unwrap();
    assert_eq!(single_notional, 500_000.0);

    // 3. Position sizing:
    // Risk budget = 1,000 EUR -> raw risk size = 1,000 / 500 = 2 contracts
    // Leverage cap = (100,000 * 5) / 500,000 = 1.0 contract
    // Final sized position MUST be capped to 1 contract!
    let result = position_size_contract(&account, &spec, 20_000.0, 19_980.0, Some(1.0)).unwrap();

    assert_eq!(
        result.size, 1.0,
        "Size must be capped to 1.0 by 5x leverage"
    );
    assert_eq!(result.risk_amount, 500.0, "Resulting risk must be 500 EUR");
    assert!(
        result.capped_by_leverage,
        "Must be flagged as capped by leverage"
    );
    assert!(!result.capped_by_notional);
}

#[test]
fn test_golden_hand_calc_fdax_future_without_leverage_cap() {
    // Same parameters but 10x leverage headroom (1,000,000 EUR cap)
    let account = AccountRisk {
        equity: 100_000.0,
        risk_pct_per_trade: 0.01,
        max_leverage: 10.0,
        max_position_notional: None,
    };
    let spec = ContractSpec {
        price_currency: Currency::eur(),
        settlement_currency: Currency::eur(),
        multiplier: 25.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::LinearFuture,
    };

    let result = position_size_contract(&account, &spec, 20_000.0, 19_980.0, Some(1.0)).unwrap();

    assert_eq!(
        result.size, 2.0,
        "Size must be 2.0 contracts without leverage binding"
    );
    assert_eq!(
        result.risk_amount, 1_000.0,
        "Risk budget fully utilized at 1,000 EUR"
    );
    assert!(!result.capped_by_leverage);
    assert!(!result.capped_by_notional);
}

#[test]
fn test_golden_hand_calc_us_equity_with_fx_conversion() {
    // EUR account trading US equity quoted in USD
    let account = AccountRisk {
        equity: 54_000.0,         // EUR
        risk_pct_per_trade: 0.01, // 540 EUR risk budget
        max_leverage: 5.0,
        max_position_notional: None,
    };
    let spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 1.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::Equity,
    };

    // FX: EUR / USD = 1.08 -> 1 USD = 1 / 1.08 EUR
    let fx_rate = FxRate::new("EUR", "USD", 1.08);
    let fx_usd_to_eur = fx_rate
        .convert(1.0, &Currency::usd(), &Currency::eur())
        .unwrap();

    let entry = 150.0; // USD
    let stop = 145.0; // USD (5 USD price risk)

    // Hand calculation:
    // Risk per share in USD = 5.0 USD
    // Risk per share in EUR = 5.0 / 1.08 = 4.62962962962963 EUR
    // Raw shares = 540.0 EUR / (5.0 / 1.08 EUR) = 540.0 * 1.08 / 5.0 = 583.2 / 5 = 116.64 shares
    // Rounded down to integer shares = 116 shares
    // Actual risk = 116 * (5.0 / 1.08) = 537.037037037 EUR
    let result = position_size_contract(&account, &spec, entry, stop, Some(fx_usd_to_eur)).unwrap();

    assert_eq!(result.size, 116.0);
    assert!((result.risk_amount - 537.037037037).abs() < 1e-6);
    assert!(!result.capped_by_leverage);
}

#[test]
fn test_golden_lot_size_and_min_quantity_thresholds() {
    let account = AccountRisk {
        equity: 10_000.0,
        risk_pct_per_trade: 0.01, // 100 risk budget
        max_leverage: 10.0,
        max_position_notional: None,
    };
    let spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 1.0,
        quantity_step: 10.0, // lot step 10
        min_quantity: 50.0,  // minimum 50
        instrument_type: InstrumentType::Equity,
    };

    // Case A: entry 100, stop 98 (risk 2/unit) -> raw size = 100 / 2 = 50.
    // Exactly reaches min_quantity of 50 -> size 50.
    let res_a = position_size_contract(&account, &spec, 100.0, 98.0, Some(1.0)).unwrap();
    assert_eq!(res_a.size, 50.0);

    // Case B: entry 100, stop 97.8 (risk 2.2/unit) -> raw size = 100 / 2.2 = 45.45.
    // Rounded down to 40 -> strictly below min_quantity 50 -> size must be 0.0!
    let res_b = position_size_contract(&account, &spec, 100.0, 97.8, Some(1.0)).unwrap();
    assert_eq!(
        res_b.size, 0.0,
        "Size below min_quantity must yield 0.0 (no trade)"
    );
    assert_eq!(res_b.risk_amount, 0.0);

    // Case C: entry 100, stop 98.7 (risk 1.3/unit) -> raw size = 100 / 1.3 = 76.92.
    // Rounded down to 70 -> size 70.
    let res_c = position_size_contract(&account, &spec, 100.0, 98.7, Some(1.0)).unwrap();
    assert_eq!(res_c.size, 70.0);
}

#[test]
fn test_golden_contract_pnl_long_and_short_with_fx() {
    let spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 1.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::Equity,
    };
    let fx_usd_to_eur = 1.0 / 1.08;

    // Long 100 shares from 150 to 160 USD on EUR account:
    // Profit in USD = (160 - 150) * 100 = 1,000 USD
    // Profit in EUR = 1,000 / 1.08 = 925.9259259259 EUR
    let pnl_long = contract_pnl(150.0, 160.0, 100.0, true, &spec, Some(fx_usd_to_eur)).unwrap();
    assert!((pnl_long - 925.9259259259).abs() < 1e-6);

    // Short 100 shares from 150 to 160 USD (loss):
    let pnl_short = contract_pnl(150.0, 160.0, 100.0, false, &spec, Some(fx_usd_to_eur)).unwrap();
    assert!((pnl_short - (-925.9259259259)).abs() < 1e-6);
}

#[test]
fn test_golden_fx_required_no_silent_fallback() {
    let spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::eur(),
        multiplier: 1.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::Equity,
    };
    let account = AccountRisk {
        equity: 10_000.0,
        risk_pct_per_trade: 0.01,
        max_leverage: 5.0,
        max_position_notional: None,
    };

    // Passing None must fail with FxUnavailable, never silent 1:1 replacement!
    let res = position_size_contract(&account, &spec, 100.0, 95.0, None);
    assert!(matches!(res, Err(ValuationError::FxUnavailable(_))));

    // Passing non-positive FX rate must also fail
    let res_bad = position_size_contract(&account, &spec, 100.0, 95.0, Some(0.0));
    assert!(matches!(res_bad, Err(ValuationError::FxUnavailable(_))));
}

#[test]
fn test_golden_tick_value_consistency() {
    // E-mini S&P 500 Future: tick size 0.25 pt, multiplier 50 USD/pt
    let es_spec = ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 50.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::LinearFuture,
    };

    // USD account: tick value = 0.25 * 50 = 12.50 USD
    let tick_val_usd = contract_tick_value(0.25, &es_spec, Some(1.0)).unwrap();
    assert_eq!(tick_val_usd, 12.50);

    // EUR account (EUR/USD 1.08): tick value in EUR = 12.50 / 1.08 = 11.574074 EUR
    let tick_val_eur = contract_tick_value(0.25, &es_spec, Some(1.0 / 1.08)).unwrap();
    assert!((tick_val_eur - (12.50 / 1.08)).abs() < 1e-6);
}

#[test]
fn test_golden_series_identity_provenance_and_compatibility() {
    use kestrel_chartkit::model::{ContinuityKind, SeriesIdentity, SessionKind};

    let series_rth = SeriesIdentity::new("NQ", "5m")
        .with_session(SessionKind::Regular)
        .with_continuity(ContinuityKind::SingleContract)
        .with_contract_code("2026-09");

    let series_eth = SeriesIdentity::new("NQ", "5m")
        .with_session(SessionKind::Extended)
        .with_continuity(ContinuityKind::SingleContract)
        .with_contract_code("2026-09");

    let series_rth_clone = series_rth.clone();

    assert!(series_rth.is_compatible(&series_rth_clone));
    assert!(!series_rth.is_compatible(&series_eth));

    let series_back_adjusted = series_rth
        .clone()
        .with_continuity(ContinuityKind::StitchedBackAdjusted);
    assert!(!series_rth.is_compatible(&series_back_adjusted));
}
